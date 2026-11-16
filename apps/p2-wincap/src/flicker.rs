//! Boyutlandırma titremesi ölçümü (yalnız testin kendi penceresi; masaüstüne girdi yok):
//!   p2-wincap --flicker <pid> <çıktı klasörü> <w1> <h1> <w2> <h2> <adım> <tur> <ms> <rx> <ry> <rw> <rh>
//! Pencereyi SetWindowPos ile w1×h1 ↔ w2×h2 arasında adım adım büyütüp küçültürken Windows Graphics
//! Capture ile gelen HER kareyi toplar. Her kare için kare boyutuna göre kesirli bölgede
//! (rx, ry, rw, rh) siyaha yakın piksel oranı ve ortalama renk yazılır; kareler yarım
//! çözünürlükte PNG olarak kaydedilir. Sonda "kara kare" (bölgenin %50'sinden fazlası siyah) sayısı.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::Graphics::Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem};
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::SizeInt32;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::WinRT::Direct3D11::{CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize};
use windows::Win32::UI::WindowsAndMessaging::{SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SetWindowPos};
use windows::core::Interface;

struct Frame {
    w: u32,
    h: u32,
    rgb: Vec<u8>,
}

pub fn run(args: &[String], hwnd_of: impl Fn(u32) -> Option<HWND>) -> bool {
    if args.first().map(String::as_str) != Some("--flicker") {
        return false;
    }
    let n = |i: usize| args.get(i).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.);
    let pid = n(1) as u32;
    let out = args.get(2).cloned().unwrap_or_else(|| ".".into());
    let (w1, h1, w2, h2) = (n(3) as i32, n(4) as i32, n(5) as i32, n(6) as i32);
    let (steps, cycles, ms) = (n(7).max(1.) as i32, n(8).max(1.) as i32, n(9) as u64);
    let region = (n(10), n(11), n(12), n(13));
    let Some(hwnd) = hwnd_of(pid) else {
        eprintln!("pencere bulunamadı: {pid}");
        std::process::exit(1);
    };
    std::fs::create_dir_all(&out).ok();
    match capture(hwnd, (w1, h1, w2, h2), steps, cycles, ms) {
        Ok(frames) => report(&frames, &out, region),
        Err(e) => {
            eprintln!("yakalama hatası: {e}");
            std::process::exit(1);
        }
    }
    true
}

fn capture(hwnd: HWND, sizes: (i32, i32, i32, i32), steps: i32, cycles: i32, ms: u64) -> windows::core::Result<Vec<Frame>> {
    unsafe {
        let _ = RoInitialize(RO_INIT_MULTITHREADED);
        let mut device: Option<ID3D11Device> = None;
        let mut context: Option<ID3D11DeviceContext> = None;
        D3D11CreateDevice(None, D3D_DRIVER_TYPE_HARDWARE, Default::default(), D3D11_CREATE_DEVICE_BGRA_SUPPORT, None, D3D11_SDK_VERSION, Some(&mut device), None, Some(&mut context))?;
        let (device, context) = (device.unwrap(), context.unwrap());
        let dxgi: IDXGIDevice = device.cast()?;
        let d3d: IDirect3DDevice = CreateDirect3D11DeviceFromDXGIDevice(&dxgi)?.cast()?;
        let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
        let item: GraphicsCaptureItem = interop.CreateForWindow(hwnd)?;
        let (w1, h1, w2, h2) = sizes;
        // Arabellek en büyük boyuttan geniş: içerik sol üstte, ContentSize kadar.
        let big = SizeInt32 { Width: w1.max(w2) + 64, Height: h1.max(h2) + 64 };
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(&d3d, DirectXPixelFormat::B8G8R8A8UIntNormalized, 3, big)?;
        let session = pool.CreateCaptureSession(&item)?;
        let _ = session.SetIsBorderRequired(false);
        let _ = session.SetIsCursorCaptureEnabled(false);
        session.StartCapture()?;

        let done = Arc::new(AtomicBool::new(false));
        let raw = hwnd.0 as isize;
        let resizer = {
            let done = done.clone();
            std::thread::spawn(move || {
                let h = HWND(raw as *mut _);
                std::thread::sleep(Duration::from_millis(300));
                for _ in 0..cycles {
                    for i in (0..=steps).chain((0..steps).rev()) {
                        let w = w1 + (w2 - w1) * i / steps;
                        let hh = h1 + (h2 - h1) * i / steps;
                        let _ = SetWindowPos(h, None, 0, 0, w, hh, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
                        std::thread::sleep(Duration::from_millis(ms));
                    }
                }
                std::thread::sleep(Duration::from_millis(400));
                done.store(true, Ordering::SeqCst);
            })
        };

        let mut frames = Vec::new();
        let mut staging: Option<(ID3D11Texture2D, u32, u32)> = None;
        let started = Instant::now();
        while !done.load(Ordering::SeqCst) && started.elapsed() < Duration::from_secs(120) {
            let Ok(f) = pool.TryGetNextFrame() else {
                std::thread::sleep(Duration::from_millis(1));
                continue;
            };
            let cs = f.ContentSize()?;
            let access: IDirect3DDxgiInterfaceAccess = f.Surface()?.cast()?;
            let texture: ID3D11Texture2D = access.GetInterface()?;
            let mut desc = D3D11_TEXTURE2D_DESC::default();
            texture.GetDesc(&mut desc);
            if staging.as_ref().is_none_or(|(_, w, h)| *w != desc.Width || *h != desc.Height) {
                let mut sd = desc;
                sd.Usage = D3D11_USAGE_STAGING;
                sd.BindFlags = 0;
                sd.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
                sd.MiscFlags = 0;
                let mut t: Option<ID3D11Texture2D> = None;
                device.CreateTexture2D(&sd, None, Some(&mut t))?;
                staging = Some((t.unwrap(), desc.Width, desc.Height));
            }
            let st = &staging.as_ref().unwrap().0;
            context.CopyResource(st, &texture);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            context.Map(st, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            let w = (cs.Width.max(0) as u32).min(desc.Width);
            let h = (cs.Height.max(0) as u32).min(desc.Height);
            // Yarım çözünürlük (2×2 bloktan sol üst piksel): bellek dostu, siyah alan tespiti için yeter.
            let (hw, hh) = (w / 2, h / 2);
            let mut rgb = vec![0u8; (hw * hh * 3) as usize];
            for y in 0..hh as usize {
                let row = (mapped.pData as *const u8).add(y * 2 * mapped.RowPitch as usize);
                for x in 0..hw as usize {
                    let s = row.add(x * 8);
                    let d = (y * hw as usize + x) * 3;
                    rgb[d] = *s.add(2);
                    rgb[d + 1] = *s.add(1);
                    rgb[d + 2] = *s;
                }
            }
            context.Unmap(st, 0);
            f.Close()?;
            frames.push(Frame { w: hw, h: hh, rgb });
        }
        let _ = resizer.join();
        session.Close()?;
        pool.Close()?;
        Ok(frames)
    }
}

fn report(frames: &[Frame], out: &str, (rx, ry, rw, rh): (f64, f64, f64, f64)) {
    let mut black = 0;
    for (i, f) in frames.iter().enumerate() {
        let (x0, y0) = ((f.w as f64 * rx) as u32, (f.h as f64 * ry) as u32);
        let (x1, y1) = (((f.w as f64 * (rx + rw)) as u32).min(f.w), ((f.h as f64 * (ry + rh)) as u32).min(f.h));
        let (mut n, mut dark, mut sum) = (0u64, 0u64, [0u64; 3]);
        for y in y0..y1 {
            for x in x0..x1 {
                let d = ((y * f.w + x) * 3) as usize;
                let p = &f.rgb[d..d + 3];
                n += 1;
                if p.iter().all(|c| *c < 10) {
                    dark += 1;
                }
                for k in 0..3 {
                    sum[k] += p[k] as u64;
                }
            }
        }
        let n = n.max(1);
        let frac = dark as f64 / n as f64;
        let is_black = frac > 0.5;
        if is_black {
            black += 1;
        }
        println!(
            "kare {i:4} {}x{} siyah {:5.1}% ort {:3} {:3} {:3}{}",
            f.w * 2,
            f.h * 2,
            frac * 100.,
            sum[0] / n,
            sum[1] / n,
            sum[2] / n,
            if is_black { "  << KARA" } else { "" }
        );
        if !out.is_empty() && let Some(img) = image::RgbImage::from_raw(f.w, f.h, f.rgb.clone()) {
            let _ = img.save(format!("{out}/f{i:04}.png"));
        }
    }
    println!("toplam {} kare, kara kare {}", frames.len(), black);
}

/// `--analyze <klasör> <rx> <ry> <rw> <rh>`: `--flicker`ın kaydettiği karelerde aynı bölge ölçümü.
pub fn analyze(args: &[String]) -> bool {
    if args.first().map(String::as_str) != Some("--analyze") {
        return false;
    }
    let n = |i: usize| args.get(i).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.);
    let dir = args.get(1).cloned().unwrap_or_default();
    let mut paths: Vec<_> = std::fs::read_dir(&dir).map(|r| r.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "png")).collect()).unwrap_or_default();
    paths.sort();
    let frames: Vec<Frame> = paths
        .iter()
        .filter_map(|p| image::open(p).ok())
        .map(|i| {
            let i = i.to_rgb8();
            Frame { w: i.width(), h: i.height(), rgb: i.into_raw() }
        })
        .collect();
    report(&frames, "", (n(2), n(3), n(4), n(5)));
    true
}
