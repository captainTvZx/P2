//! Faz 0 / Spike 3 — Rust overlay host.
//!
//! Bugünkü C++ overlay DLL'i (native/overlay/dist/p2overlay64.dll) + enjektörüyle çalışır;
//! Electron'daki `overlayHost.cjs` + `host.node`'un yerini alır:
//!   1. Test oyununu (p2-d3dtest, D3D11 ya da D3D12) enjektörle askıya alınmış başlatır.
//!   2. Paylaşımlı bellek oturumunu kurar (p2-overlay-ipc, şema 4).
//!   3. Overlay arayüzünü ana uygulamayla aynı GPUI + p2-ui ile EKRAN DIŞINDA çizer
//!      (wgpu headless çizici), önceden çarpılmış BGRA'ya çevirip kirli dikdörtgenle yazar.
//!   4. DLL'den gelen fare olaylarını GPUI penceresine iletir.
//! Ölçer: overlay kapalı/açık oyun FPS'i, kare başına çizim süresi, host belleği; ve
//! oyun penceresinin görüntüsünü kaydeder (yalnız o pencere, PrintWindow).
//!
//!   p2-overlay-host d3d11|d3d12 [çıktı klasörü]

mod capture;
mod view;

use std::io::BufRead as _;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::{
    AppContext as _, HeadlessAppContext, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    PlatformHeadlessRenderer, PlatformInput, point, px, size,
};
use p2_overlay_ipc::{DRAW_PINNED, InputMsg, Session, input};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
};
use windows::core::BOOL;

use view::OverlayView;

const FRAME: Duration = Duration::from_millis(33); // ~30 fps overlay

fn log(msg: impl AsRef<str>) {
    println!("[host] {}", msg.as_ref());
}

fn repo_root() -> std::path::PathBuf {
    std::env::var("P2_REPO")
        .map(Into::into)
        .unwrap_or_else(|_| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

fn find_window(pid: u32) -> Option<HWND> {
    struct Find {
        pid: u32,
        hwnd: Option<HWND>,
    }
    extern "system" fn each(hwnd: HWND, lp: LPARAM) -> BOOL {
        let find = unsafe { &mut *(lp.0 as *mut Find) };
        let mut owner = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut owner)) };
        if owner == find.pid && unsafe { IsWindowVisible(hwnd) }.as_bool() {
            find.hwnd = Some(hwnd);
            return BOOL(0);
        }
        BOOL(1)
    }
    let mut find = Find { pid, hwnd: None };
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut find as *mut Find as isize));
    }
    find.hwnd
}

fn save_rgba(path: &std::path::Path, w: u32, h: u32, rgba: Vec<u8>) {
    match image::RgbaImage::from_raw(w, h, rgba).map(|img| img.save(path)) {
        Some(Ok(())) => log(format!("görüntü: {}", path.display())),
        _ => log(format!("görüntü kaydedilemedi: {}", path.display())),
    }
}

/// GPUI çıktısını (2× ölçekli, `2w × 2h` RGBA) DLL'in beklediği `w × h` önceden çarpılmış
/// BGRA'ya çevirir: 2×2 kutu filtresiyle küçültür (süper örnekleme). Düz alfa algılanırsa
/// (bir pikselde renk > alfa) ortalamadan önce çarpar. Dönüş: düz alfa mıydı.
///
/// Neden 2×: GPUI'nin ekran dışı test penceresi sabit 2.0 ölçekle çalışıyor ve ölçeği
/// değiştiren işlev crate içine kapalı (Faz 0 bulgusu).
fn downsample_premultiplied_bgra(rgba: &[u8], w: u32, h: u32, out: &mut [u8]) -> bool {
    let straight = rgba.chunks_exact(4).any(|p| p[3] < 255 && (p[0] > p[3] || p[1] > p[3] || p[2] > p[3]));
    let src_stride = (w * 2 * 4) as usize;
    for y in 0..h as usize {
        for x in 0..w as usize {
            let mut acc = [0u32; 4];
            for (dy, dx) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                let s = &rgba[(y * 2 + dy) * src_stride + (x * 2 + dx) * 4..][..4];
                let a = s[3] as u32;
                let mul = |c: u8| if straight { (c as u32 * a + 127) / 255 } else { c as u32 };
                acc[0] += mul(s[2]);
                acc[1] += mul(s[1]);
                acc[2] += mul(s[0]);
                acc[3] += a;
            }
            let d = &mut out[(y * w as usize + x) * 4..][..4];
            for i in 0..4 {
                d[i] = ((acc[i] + 2) / 4) as u8;
            }
        }
    }
    straight
}

/// Değişen piksellerin sınır kutusu (x, y, w, h); değişiklik yoksa None.
fn dirty_rect(prev: &[u8], next: &[u8], w: u32, h: u32) -> Option<(i32, i32, i32, i32)> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0u32, 0u32);
    let (pa, na): (&[u32], &[u32]) = (bytemuck_u32(prev), bytemuck_u32(next));
    for y in 0..h {
        let row = (y * w) as usize;
        let (pr, nr) = (&pa[row..row + w as usize], &na[row..row + w as usize]);
        if pr == nr {
            continue;
        }
        let first = pr.iter().zip(nr).position(|(a, b)| a != b).unwrap() as u32;
        let last = w - 1 - pr.iter().rev().zip(nr.iter().rev()).position(|(a, b)| a != b).unwrap() as u32;
        x0 = x0.min(first);
        x1 = x1.max(last);
        y0 = y0.min(y);
        y1 = y1.max(y);
    }
    (x0 != u32::MAX).then(|| (x0 as i32, y0 as i32, (x1 - x0 + 1) as i32, (y1 - y0 + 1) as i32))
}

fn bytemuck_u32(bytes: &[u8]) -> &[u32] {
    assert!(bytes.len() % 4 == 0 && (bytes.as_ptr() as usize) % 4 == 0);
    unsafe { std::slice::from_raw_parts(bytes.as_ptr() as *const u32, bytes.len() / 4) }
}

#[derive(Default)]
struct Timing {
    render: Vec<f64>,
    write: Vec<f64>,
    dirty_px: Vec<f64>,
}
impl Timing {
    fn summary(v: &mut [f64]) -> String {
        if v.is_empty() {
            return "-".into();
        }
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        format!("ort {:.2} / p95 {:.2}", v.iter().sum::<f64>() / v.len() as f64, v[(v.len() * 95 / 100).min(v.len() - 1)])
    }
}

fn main() -> anyhow::Result<()> {
    let api = std::env::args().nth(1).unwrap_or_else(|| "d3d11".into()).to_lowercase();
    let out = std::path::PathBuf::from(std::env::args().nth(2).unwrap_or_else(|| "C:\\p2-pg\\spike3".into()));
    std::fs::create_dir_all(&out)?;
    let repo = repo_root();
    let dll = repo.join("native/overlay/dist/p2overlay64.dll").canonicalize()?;
    let injector = repo.join("native/overlay/dist/p2inject64.exe").canonicalize()?;
    let game = std::env::current_exe()?.with_file_name("p2-d3dtest.exe");

    // Kontrol modu: oyunu enjeksiyonsuz aç, PrintWindow'un DirectX içeriğini yakalayıp
    // yakalamadığını gör (overlay'siz referans).
    if std::env::var_os("P2_CAPTURE_TEST").is_some() {
        let mut game_proc = Command::new(&game).arg(&api).spawn()?;
        std::thread::sleep(Duration::from_secs(2));
        let hwnd = find_window(game_proc.id()).ok_or_else(|| anyhow::anyhow!("oyun penceresi yok"))?;
        if let Some((cw, ch, rgba)) = capture::window_rgba(hwnd) {
            save_rgba(&out.join(format!("{api}-enjeksiyonsuz.png")), cw, ch, rgba);
        }
        let _ = game_proc.kill();
        return Ok(());
    }

    // 1) Oyunu enjektörle başlat (bugünkü Electron akışıyla aynı ikili ve argümanlar).
    let mut child = Command::new(&injector)
        .arg(&dll)
        .arg(&game)
        .arg(game.parent().unwrap())
        .arg(&api)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    // Yalnız ilk satır (pid): oyun süreci borunun yazma ucunu devralabildiği için akış sonu
    // (EOF) hiç gelmeyebilir — Electron tarafı da pid'i ilk satırdan okur.
    let mut stdout = String::new();
    std::io::BufReader::new(child.stdout.take().unwrap()).read_line(&mut stdout)?;
    let status = child.wait()?;
    let pid: u32 = stdout.trim().parse().map_err(|_| anyhow::anyhow!("enjektör pid vermedi: {stdout:?} ({status})"))?;
    log(format!("oyun başlatıldı ve DLL enjekte edildi: {} {api}, pid {pid} (enjektör çıkış kodu {status})", game.display()));

    // 2) Oturum: DLL bağlanana kadar yeniden dener.
    let session = Session::create(pid, &[])?;
    let started = Instant::now();
    let hwnd = loop {
        if let Some(h) = find_window(pid).filter(|_| session.stats().frames > 0) {
            break h;
        }
        if started.elapsed() > Duration::from_secs(15) {
            anyhow::bail!("DLL bağlanmadı ya da oyun çizmeye başlamadı (frames=0)");
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    log(format!("DLL bağlandı: oyun Present sayıyor (frames={})", session.stats().frames));
    // Overlay yüzeyi = takas zinciri = istemci alanı (pencere çerçevesi hariç).
    let (w, h) = unsafe {
        let mut r = windows::Win32::Foundation::RECT::default();
        let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut r);
        ((r.right - r.left).max(1) as u32, (r.bottom - r.top).max(1) as u32)
    };

    // 3) Ekran dışı GPUI (wgpu headless çizici + DirectWrite metin sistemi).
    let t_init = Instant::now();
    // DirectWrite (WindowsPlatform::text_system) ekran dışı wgpu çizicisinde glif üretmedi
    // (Faz 0 bulgusu: kart çizildi, metinler boş). cosmic-text sistem fontlarını kendisi yükler.
    let text_system: Arc<dyn gpui_kit::PlatformTextSystem> = if std::env::var_os("P2_DIRECTWRITE").is_some() {
        gpui_kit::platform::current_platform(true).text_system()
    } else {
        Arc::new(gpui_wgpu::CosmicTextSystem::new(p2_ui::tokens::FONT))
    };
    let mut cx = HeadlessAppContext::with_platform(text_system, Arc::new(gpui_kit::assets::Assets), || {
        Ok(Some(Box::new(gpui_wgpu::WgpuHeadlessRenderer::new()?) as Box<dyn PlatformHeadlessRenderer>))
    });
    cx.update(|cx| {
        gpui_kit::init(cx);
        p2_ui::theme::apply(p2_ui::Palette::p2_dark(), cx);
    });
    let window = cx.open_window(size(px(w as f32), px(h as f32)), |_, cx| cx.new(|_| OverlayView::new(w as f32)))?;
    cx.run_until_parked();
    log(format!("ekran dışı GPUI hazır: {w}x{h}, {} ms", t_init.elapsed().as_millis()));

    let mut prev = vec![0u8; (w * h * 4) as usize];
    let mut next = vec![0u8; (w * h * 4) as usize];
    let mut timing = Timing::default();
    let mut straight_logged = false;
    let mut frames_written = 0u64;
    let mut inputs: Vec<InputMsg> = Vec::new();

    let mut fps_window = (Instant::now(), session.stats().frames);
    let mut game_fps = 0f32;
    let mut phase_fps: Vec<(String, f32)> = Vec::new();
    let mut fps_samples: Vec<f32> = Vec::new();

    let run_started = Instant::now();
    let mut phase = 0;
    let mut shot_requested = false;
    let mut shot_saved = false;
    let mut clicked_at: Option<Instant> = None;

    loop {
        let t = run_started.elapsed().as_secs_f32();
        // Aşamalar: 0) 0–4 sn overlay kapalı  1) 4–10 sn overlay açık (girdi oyunda)  2) 10–14 sn panel (girdi overlay'de)
        let want = if t < 4. { 0 } else if t < 10. { 1 } else if t < 14. { 2 } else { 3 };
        if want != phase {
            let label = ["overlay kapalı", "overlay açık (çizim)", "panel açık (girdi)"][phase];
            if !fps_samples.is_empty() {
                let skip = fps_samples.len().min(1);
                let v = &fps_samples[skip..];
                phase_fps.push((label.into(), v.iter().sum::<f32>() / v.len().max(1) as f32));
            }
            fps_samples.clear();
            phase = want;
            match phase {
                1 => {
                    session.set_draw_mask(DRAW_PINNED);
                    log("aşama: overlay açık (drawMask=PINNED, girdi oyunda)");
                }
                2 => {
                    session.set_visible(true);
                    log("aşama: panel açık (visible=1, girdi overlay'e)");
                }
                _ => break,
            }
        }

        // Oyun FPS'i: DLL'in Present sayacı.
        if fps_window.0.elapsed() >= Duration::from_millis(1000) {
            let frames = session.stats().frames;
            game_fps = (frames - fps_window.1) as f32 / fps_window.0.elapsed().as_secs_f32();
            fps_samples.push(game_fps);
            fps_window = (Instant::now(), frames);
        }

        // DLL'den gelen girdi → GPUI.
        while let Ok(msg) = session.input.try_recv() {
            inputs.push(msg);
            let pos = point(px(msg.x as f32), px(msg.y as f32));
            let event = match msg.kind {
                input::MOUSE_MOVE => Some(PlatformInput::MouseMove(MouseMoveEvent { position: pos, pressed_button: None, modifiers: Modifiers::default() })),
                input::MOUSE_BTN if msg.flags & input::FLAG_DOWN != 0 => Some(PlatformInput::MouseDown(MouseDownEvent {
                    button: MouseButton::Left,
                    position: pos,
                    modifiers: Modifiers::default(),
                    click_count: 1,
                    first_mouse: false,
                })),
                input::MOUSE_BTN => Some(PlatformInput::MouseUp(MouseUpEvent { button: MouseButton::Left, position: pos, modifiers: Modifiers::default(), click_count: 1 })),
                _ => None,
            };
            if let Some(event) = event {
                // Pencere düzeyinde gönderilir: görünümü kiralayan `window.update` içinden
                // gönderilince düğmenin dinleyicisi aynı görünümü güncelleyemiyordu (yeniden giriş).
                let _ = cx.update_window(window.into(), |_, window, cx| {
                    window.dispatch_event(event, cx);
                });
            }
        }

        // Arayüz durumunu ilerlet, ekran dışında çiz, yaz.
        let trace = frames_written < 2 && std::env::var_os("P2_TRACE").is_some();
        if trace { eprintln!("iz: advance_clock"); }
        cx.advance_clock(FRAME);
        if trace { eprintln!("iz: update"); }
        let _ = window.update(&mut cx, |view, _, cx| {
            view.frame += 1;
            view.game_fps = game_fps;
            cx.notify();
        });
        if trace { eprintln!("iz: run_until_parked"); }
        cx.run_until_parked();
        if trace { eprintln!("iz: capture_screenshot"); }
        let t0 = Instant::now();
        let image = cx.capture_screenshot(window.into())?;
        if trace { eprintln!("iz: görüntü {}x{} ({} ms)", image.width(), image.height(), t0.elapsed().as_millis()); }
        timing.render.push(t0.elapsed().as_secs_f64() * 1000.);
        if image.width() != w * 2 || image.height() != h * 2 {
            anyhow::bail!("beklenmeyen overlay boyutu {}x{} (2× bekleniyordu)", image.width(), image.height());
        }
        let t1 = Instant::now();
        let straight = downsample_premultiplied_bgra(image.as_raw(), w, h, &mut next);
        if !straight_logged {
            straight_logged = true;
            log(format!("GPUI çıktısı {} alfa", if straight { "düz (çarpılıyor)" } else { "önceden çarpılmış" }));
        }
        let dirty = if frames_written == 0 { None } else { dirty_rect(&prev, &next, w, h) };
        if frames_written == 0 || dirty.is_some() {
            session.write_frame(&next, w, h, dirty);
            frames_written += 1;
            if let Some((_, _, dw, dh)) = dirty {
                timing.dirty_px.push((dw * dh) as f64 / (w * h) as f64 * 100.);
            }
            std::mem::swap(&mut prev, &mut next);
        }
        timing.write.push(t1.elapsed().as_secs_f64() * 1000.);

        // Aşama 1'in ortasında: oyun penceresi görüntüsü (overlay dahil) + DLL'in overlay'siz karesi.
        if phase == 1 && t > 7. && !shot_requested {
            shot_requested = true;
            if let Some((cw, ch, rgba)) = capture::window_rgba(hwnd) {
                save_rgba(&out.join(format!("{api}-overlay.png")), cw, ch, rgba);
            }
            session.request_shot();
        }
        if shot_requested && !shot_saved {
            if let Some(shot) = session.read_shot() {
                let mut rgba = shot.bgra;
                for px in rgba.chunks_exact_mut(4) {
                    px.swap(0, 2);
                }
                save_rgba(&out.join(format!("{api}-dll-ekran-goruntusu.png")), shot.width, shot.height, rgba);
                shot_saved = true;
            }
        }

        // Aşama 2: yalnız oyun penceresinin kuyruğuna fare mesajı (masaüstüne değil).
        if phase == 2 && clicked_at.is_none() && t > 11. {
            let (bx, by) = view::button_center(w as f32);
            let lp = LPARAM(((by as u32) << 16 | (bx as u32 & 0xffff)) as isize);
            unsafe {
                let _ = PostMessageW(Some(hwnd), WM_MOUSEMOVE, WPARAM(0), lp);
                let _ = PostMessageW(Some(hwnd), WM_LBUTTONDOWN, WPARAM(1), lp);
                let _ = PostMessageW(Some(hwnd), WM_LBUTTONUP, WPARAM(0), lp);
            }
            clicked_at = Some(Instant::now());
            log(format!("oyun penceresine tıklama mesajı gönderildi ({bx},{by})"));
        }
        if let Some(at) = clicked_at {
            if at.elapsed() > Duration::from_millis(1500) && at.elapsed() < Duration::from_millis(1540) {
                if let Some((cw, ch, rgba)) = capture::window_rgba(hwnd) {
                    save_rgba(&out.join(format!("{api}-girdi.png")), cw, ch, rgba);
                }
            }
        }

        let spent = t0.elapsed();
        if spent < FRAME {
            std::thread::sleep(FRAME - spent);
        }
    }

    // Rapor
    let clicks = window.update(&mut cx, |view, _, _| view.clicks).unwrap_or(0);
    let kinds = |k: u16| inputs.iter().filter(|m| m.kind == k).count();
    println!("\n=== Spike 3 sonuç ({api}) ===");
    for (label, fps) in &phase_fps {
        println!("  oyun FPS · {label}: {fps:.1}");
    }
    println!("  overlay kare süresi (GPUI çizim + GPU→CPU okuma): {} ms", Timing::summary(&mut timing.render));
    println!("  dönüştürme + kirli dikdörtgen + yazma: {} ms", Timing::summary(&mut timing.write));
    println!("  yazılan overlay karesi: {frames_written}, kirli alan (ekranın %'si): {}", Timing::summary(&mut timing.dirty_px));
    let st = session.stats();
    println!("  DLL: son yüklenen kare {} / yazılan {}, frames(Present)={}", st.ack_frame_id, st.frame_id, st.frames);
    println!(
        "  DLL→host girdi: fare hareketi {}, fare düğmesi {}, boyut {}, toplam {} — düğme tıklaması GPUI'ye ulaştı: {}",
        kinds(input::MOUSE_MOVE),
        kinds(input::MOUSE_BTN),
        kinds(input::RESIZE),
        inputs.len(),
        if clicks > 0 { "EVET" } else { "HAYIR" }
    );

    // Oyunu kapat.
    let _ = Command::new("taskkill").args(["/PID", &pid.to_string(), "/F"]).stdout(Stdio::null()).stderr(Stdio::null()).status();
    drop(session);
    Ok(())
}
