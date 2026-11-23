//! P2 masaüstü istemcisi (Rust + GPUI).
//!
//! Geçiş süresince kendi profilini kullanır (`%APPDATA%\P2-Rust`); kurulu Electron
//! istemcisine dokunmaz. Bugünkü P2 sunucusuna (NestJS ya da Rust) bağlanır.
//!
//! Sürüm derlemesi Windows alt sistemindedir (kurulu sürümde konsol penceresi açılmaz); izler
//! yine stdout'a yazılır, testler süreci borularla başlatıp okur (sunucu konsolu ile aynı).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod avatars;
mod annotate;
mod call;
mod call_widget;
mod exe_icon;
mod overlay_draw;
mod resize;
mod rewind;
mod screens;
#[cfg(windows)]
mod shell;
#[cfg(target_os = "macos")]
#[path = "shell_mac.rs"]
mod shell;
mod splash;
mod splash_view;
#[cfg(windows)]
mod store_update;
#[cfg(not(windows))]
#[path = "store_update_other.rs"]
mod store_update;
mod titlebar;
pub mod toasts;
mod update_banner;
mod updates;
mod task;

use gpui_kit::component::TitleBar;
use gpui_kit::{AppContext as _, Bounds, WindowBounds, WindowOptions, px, size};

pub const CLIENT_VERSION: &str = concat!("rust-", env!("CARGO_PKG_VERSION"));

/// Tanılama izi (konsola): ekran geçişleri, gerçek zamanlı olaylar. Otomatik pilot
/// testleri bu satırları okur.
pub fn trace(line: &str) {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    let t = START.get_or_init(std::time::Instant::now).elapsed().as_millis();
    println!("[+{t} ms] {line}");
    diag_log(t, line);
}

/// Tanı kaydı: arama/ses/donma/ekran/pencere satırları profil klasöründe logs/client.log'a da
/// yazılır (mesaj içerikleri gibi kişisel satırlar yazılmaz). 2 MB'ı geçince client.old.log olur.
fn diag_log(t: u128, line: &str) {
    use std::io::Write;
    const KINDS: [&str; 11] = ["arama:", "ses:", "donma:", "ekran:", "pencere:", "balon:", "overlay", "kütüphane: oyun", "başlatma kartı:", "kaplama:", "PANİK"];
    if !KINDS.iter().any(|k| line.starts_with(k)) {
        return;
    }
    static FILE: std::sync::Mutex<Option<std::fs::File>> = std::sync::Mutex::new(None);
    let mut guard = FILE.lock().unwrap_or_else(|e| e.into_inner());
    if guard.is_none() {
        let dir = p2_client_core::store::profile_dir().join("logs");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("client.log");
        if std::fs::metadata(&path).is_ok_and(|m| m.len() > 2 * 1024 * 1024) {
            let _ = std::fs::rename(&path, dir.join("client.old.log"));
        }
        *guard = std::fs::OpenOptions::new().create(true).append(true).open(&path).ok();
        if let Some(f) = guard.as_mut() {
            let _ = writeln!(f, "---- açılış {} (sürüm {})", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), env!("CARGO_PKG_VERSION"));
        }
    }
    if let Some(f) = guard.as_mut() {
        let _ = writeln!(f, "[+{t} ms] {line}");
    }
}

/// Ana pencere (bildirimde yanıp sönme, tepsiden gösterme).
pub static MAIN_HWND: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static DESKTOP_NOTIFY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
/// Kabuk olayları (tepsi, ikinci örnek): ilk pencere kurulurken alınır.
pub static SHELL: std::sync::Mutex<Option<(std::sync::mpsc::Sender<shell::ShellEvent>, std::sync::mpsc::Receiver<shell::ShellEvent>)>> = std::sync::Mutex::new(None);

pub fn set_desktop_notifications(on: bool) {
    DESKTOP_NOTIFY.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Uygulama arka plandaysa masaüstü bildirimi ve görev çubuğunda yanıp sönme
/// (main.cjs `notification:show`). `P2_FORCE_NOTIFY` (testler): ön planda da gösterir.
pub fn desktop_notify(title: &str, body: &str) {
    if !DESKTOP_NOTIFY.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let hwnd = shell::HWND(MAIN_HWND.load(std::sync::atomic::Ordering::Relaxed) as _);
    if !hwnd.is_invalid() && shell::is_foreground(hwnd) && std::env::var_os("P2_FORCE_NOTIFY").is_none() {
        return;
    }
    let shown = shell::notify(title, body);
    if !hwnd.is_invalid() {
        shell::flash(hwnd, true);
    }
    trace(&format!("bildirim: {title} — {} ({})", body.chars().take(80).collect::<String>(), if shown { "gösterildi" } else { "gösterilemedi" }));
}

fn main() {
    // Panik: sebebi ve yeri client.log'a yazılır (pencere işleyicisinde panik süreci sessizce kapatır).
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let bt = std::backtrace::Backtrace::force_capture().to_string();
        let bt: String = bt.lines().filter(|l| l.contains("p2_") || l.contains("gpui")).take(24).collect::<Vec<_>>().join(" | ");
        trace(&format!("PANİK: {info} ;; {bt}"));
        default_hook(info);
    }));
    // Tanılama/test: yalnız Electron → Rust taşımasını yap, raporu yaz ve çık (pencere açmaz).
    if std::env::args().any(|a| a == "--p2-import-electron") {
        let report = p2_client_core::migrate::import_if_needed();
        println!("{}", report.map(|r| r.to_json()).unwrap_or(serde_json::Value::Null));
        return;
    }
    shell::set_app_user_model_id();
    let (tx, rx) = shell::events();
    if !shell::single_instance(tx.clone()) {
        println!("[tek örnek] P2 zaten açık; argümanlar açık örneğe iletildi");
        return;
    }
    // macOS: `p2://` bağlantıları ve Dock'tan yeniden açma argüman değil uygulama olayı olarak gelir.
    let url_tx = tx.clone();
    *SHELL.lock().unwrap() = Some((tx, rx));
    if shell::register_protocol() {
        trace("kabuk: p2:// protokolü kaydedildi");
    }
    store_update::ensure_desktop_shortcut();
    // Faz 8: ilk açılışta Electron profilinden ayar, oturum ve kütüphane taşınır (profil boşsa).
    if let Some(report) = p2_client_core::migrate::import_if_needed() {
        trace(&format!("taşıma: Electron profilinden alındı {}", report.to_json()));
    }
    let settings = p2_client_core::store::load_settings();
    set_desktop_notifications(settings.app.desktop_notifications);
    if let Some(lang) = &settings.language {
        p2_i18n::set_language(lang);
    }
    // "dark"/"light" (eski) ya da Electron tema kimliği (p2-mavi, kagit, …).
    let start_palette = screens::settings::settings_look::startup_palette(&settings);
    let application = gpui_kit::application().with_assets(gpui_kit::assets::Assets);
    {
        let (urls, reopen) = (url_tx.clone(), url_tx);
        application.on_open_urls(move |list| {
            let _ = urls.send(shell::ShellEvent::Args(list));
        });
        application.on_reopen(move |_| {
            let _ = reopen.send(shell::ShellEvent::Show);
        });
    }
    application.run(move |cx| {
        gpui_kit::init(cx);
        p2_ui::theme::apply(start_palette.clone(), cx);
        // Ayarlar > Görünüm yazı boyutu (Electron --app-font-scale; varsayılan 85).
        p2_ui::fx::set_font_scale_percent(settings.appearance.font_scale as f32);
        // Electron ui.css: 140ms etkileşim geçişleri + ::-webkit-scrollbar (yalnız ana istemci).
        p2_ui::fx::install_client_defaults();
        let open_main = |cx: &mut gpui_kit::App| {
            // Test/parite: P2_TEST_WINDOW_POS="x,y" → pencere bu konumda (ör. ekran dışı) ve odak çalmadan açılır.
            let test_pos = std::env::var("P2_TEST_WINDOW_POS").ok().and_then(|v| {
                let (x, y) = v.split_once(',')?;
                Some((x.trim().parse::<f32>().ok()?, y.trim().parse::<f32>().ok()?))
            });
            let bounds = match test_pos {
                Some((x, y)) => {
                    // Test: P2_TEST_WINDOW_SIZE="g,y" → pencere boyutu (varsayılan 1280×800).
                    let (w, h) = std::env::var("P2_TEST_WINDOW_SIZE").ok().and_then(|v| v.split_once(',').and_then(|(w, h)| Some((w.trim().parse::<f32>().ok()?, h.trim().parse::<f32>().ok()?)))).unwrap_or((1280., 800.));
                    Bounds::new(gpui_kit::point(px(x), px(y)), size(px(w), px(h)))
                }
                None => Bounds::centered(None, size(px(1280.), px(800.)), cx),
            };
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(900.), px(600.))),
                focus: test_pos.is_none(),
                // macOS: trafik ışıkları 38 px başlık çubuğunun ortasında.
                titlebar: Some(gpui_kit::TitlebarOptions { traffic_light_position: Some(gpui_kit::point(px(13.), px(12.))), ..TitleBar::title_bar_options() }),
                ..TitleBar::window_options()
            };
            gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| app::ClientApp::new(window, cx))).expect("pencere açılamadı");
            if test_pos.is_none() {
                cx.activate(true);
            }
        };
        // Kapanışta hazır güncelleme sessizce kurulur (main.cjs `before-quit`).
        cx.on_app_quit(|_| {
            updates::install_if_ready();
            async {}
        })
        .detach();
        // Açılış penceresi (güncelleme denetimi) bitmeden ana pencere açılmaz (main.cjs).
        match splash::wanted() {
            Some(base) => splash::run(cx, base, open_main),
            None => open_main(cx),
        }
    });
}

/// `P2_TRACE_RENDER` açıksa görünümlerin çizim sayısını saniyede bir yazar (boşta CPU tanısı).
pub fn count_render(view: &'static str) {
    use std::collections::HashMap;
    use std::sync::Mutex;
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if !*ON.get_or_init(|| std::env::var_os("P2_TRACE_RENDER").is_some()) {
        return;
    }
    static COUNTS: Mutex<Option<(std::time::Instant, HashMap<&'static str, u32>)>> = Mutex::new(None);
    let mut guard = COUNTS.lock().unwrap();
    let (since, counts) = guard.get_or_insert_with(|| (std::time::Instant::now(), HashMap::new()));
    *counts.entry(view).or_default() += 1;
    if since.elapsed().as_secs() >= 1 {
        trace(&format!("çizim/sn: {counts:?}"));
        counts.clear();
        *since = std::time::Instant::now();
    }
}
