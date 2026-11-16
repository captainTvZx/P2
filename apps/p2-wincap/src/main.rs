//! Test aracı: verilen sürecin görünür ana penceresini Windows Graphics Capture ile PNG'ye
//! kaydeder. Yalnız o pencereyi okur (masaüstünü değil), girdi göndermez.
//!   p2-wincap <pid> <çıktı.png>

mod capture;
mod flicker;
mod testinput;

use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetClientRect, GetWindowThreadProcessId, IsWindowVisible};
use windows::core::BOOL;

/// Sürecin başlığında `title` geçen görünür penceresi (ör. açılış/başlatma kartı).
fn find_titled(pid: u32, title: &str) -> Option<HWND> {
    struct Find<'a> {
        pid: u32,
        title: &'a str,
        hwnd: Option<HWND>,
    }
    extern "system" fn each(hwnd: HWND, lp: LPARAM) -> BOOL {
        let find = unsafe { &mut *(lp.0 as *mut Find) };
        let mut owner = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut owner)) };
        if owner == find.pid && unsafe { IsWindowVisible(hwnd) }.as_bool() {
            let mut buf = [0u16; 256];
            let n = unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowTextW(hwnd, &mut buf) };
            if String::from_utf16_lossy(&buf[..n.max(0) as usize]).contains(find.title) {
                find.hwnd = Some(hwnd);
                return BOOL(0);
            }
        }
        BOOL(1)
    }
    let mut find = Find { pid, title, hwnd: None };
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut find as *mut Find as isize));
    }
    find.hwnd
}

fn find_window(pid: u32) -> Option<HWND> {
    struct Find {
        pid: u32,
        hwnd: Option<HWND>,
        area: i64,
    }
    // Sürecin görünür pencerelerinden en büyüğü: sıfır boyutlu yardımcı pencereler
    // (WGC "Parametre hatalı" verir) ana pencereden önce sıralanabiliyor.
    extern "system" fn each(hwnd: HWND, lp: LPARAM) -> BOOL {
        let find = unsafe { &mut *(lp.0 as *mut Find) };
        let mut owner = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut owner)) };
        if owner == find.pid && unsafe { IsWindowVisible(hwnd) }.as_bool() {
            let mut r = RECT::default();
            if unsafe { GetClientRect(hwnd, &mut r) }.is_ok() {
                let area = i64::from(r.right - r.left) * i64::from(r.bottom - r.top);
                if area > find.area {
                    find.area = area;
                    find.hwnd = Some(hwnd);
                }
            }
        }
        BOOL(1)
    }
    let mut find = Find { pid, hwnd: None, area: 0 };
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut find as *mut Find as isize));
    }
    find.hwnd
}

/// WGC karesi DWM'nin "genişletilmiş çerçeve" dikdörtgenini kapsar; istemci alanının o karedeki
/// yeri ve boyutu (fiziksel px). Hesaplanamazsa kare olduğu gibi döner.
fn client_crop(hwnd: HWND, frame_w: u32, frame_h: u32) -> (u32, u32, u32, u32) {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
    use windows::Win32::Graphics::Gdi::ClientToScreen;
    unsafe {
        let mut frame = RECT::default();
        let mut client = RECT::default();
        let mut origin = POINT::default();
        if DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, &mut frame as *mut RECT as *mut _, std::mem::size_of::<RECT>() as u32).is_err()
            || GetClientRect(hwnd, &mut client).is_err()
            || !ClientToScreen(hwnd, &mut origin).as_bool()
        {
            return (0, 0, frame_w, frame_h);
        }
        let x = (origin.x - frame.left).clamp(0, frame_w as i32) as u32;
        let y = (origin.y - frame.top).clamp(0, frame_h as i32) as u32;
        let w = (client.right - client.left).max(0) as u32;
        let h = (client.bottom - client.top).max(0) as u32;
        (x, y, w.min(frame_w - x), h.min(frame_h - y))
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    // Test yardımcıları: yalnız verilen sürecin (testin kendi açtığı istemcinin) penceresine.
    //   p2-wincap --close <pid>    WM_CLOSE gönderir (kapat düğmesi gibi)
    //   p2-wincap --visible <pid>  görünür penceresi var mı (1/0)
    let first = std::env::args().nth(1).unwrap_or_default();
    if flicker::analyze(&std::env::args().skip(1).collect::<Vec<_>>()) {
        return;
    }
    if flicker::run(&std::env::args().skip(1).collect::<Vec<_>>(), find_window) {
        return;
    }
    if testinput::run(&std::env::args().skip(1).collect::<Vec<_>>()) || testinput::shot(&std::env::args().skip(1).collect::<Vec<_>>()) {
        return;
    }
    if first == "--title" {
        // p2-wincap --title <başlık parçası> <pid> <çıktı.png>: o sürecin başlığı eşleşen penceresi
        let a: Vec<String> = std::env::args().skip(2).collect();
        let (Some(title), Some(pid), Some(out)) = (a.first(), a.get(1).and_then(|p| p.parse::<u32>().ok()), a.get(2)) else { std::process::exit(2) };
        let Some(hwnd) = find_titled(pid, title) else {
            eprintln!("pencere bulunamadı: {pid} «{title}»");
            std::process::exit(1);
        };
        let Some((w, h, rgba)) = capture::window_rgba(hwnd) else { std::process::exit(1) };
        image::RgbaImage::from_raw(w, h, rgba).expect("boyut").save(out).expect("kaydet");
        println!("{out} {w}x{h}");
        return;
    }
    if first == "--client" {
        // p2-wincap --client <pid> <çıktı.png>: yalnız istemci alanı (çerçeve/gölge kırpılır).
        // Eski parite aracı (Electron ↔ Rust; kaldırıldı) Electron'un CDP görüntüsüyle aynı piksel boyutunu ister.
        let a: Vec<String> = std::env::args().skip(2).collect();
        let (Some(pid), Some(out)) = (a.first().and_then(|p| p.parse::<u32>().ok()), a.get(1)) else { std::process::exit(2) };
        let Some(hwnd) = find_window(pid) else {
            eprintln!("pencere bulunamadı: {pid}");
            std::process::exit(1);
        };
        let Some((w, h, rgba)) = capture::window_rgba(hwnd) else { std::process::exit(1) };
        let img = image::RgbaImage::from_raw(w, h, rgba).expect("boyut");
        let (x, y, cw, ch) = client_crop(hwnd, w, h);
        let cropped = image::imageops::crop_imm(&img, x, y, cw, ch).to_image();
        cropped.save(out).expect("kaydet");
        println!("{out} {cw}x{ch}");
        return;
    }
    if first == "--key" {
        // p2-wincap --key <pid> <vk hex>: yalnız o sürecin penceresine WM_KEYDOWN/UP (kendi test penceremiz)
        let pid = std::env::args().nth(2).and_then(|p| p.parse::<u32>().ok()).unwrap_or(0);
        let vk = std::env::args().nth(3).and_then(|v| u32::from_str_radix(v.trim_start_matches("0x"), 16).ok()).unwrap_or(0);
        let Some(hwnd) = find_window(pid) else { std::process::exit(1) };
        unsafe {
            use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_KEYDOWN, WM_KEYUP};
            let _ = PostMessageW(Some(hwnd), WM_KEYDOWN, windows::Win32::Foundation::WPARAM(vk as usize), windows::Win32::Foundation::LPARAM(1));
            let _ = PostMessageW(Some(hwnd), WM_KEYUP, windows::Win32::Foundation::WPARAM(vk as usize), windows::Win32::Foundation::LPARAM(0xC000_0001u32 as isize));
        }
        println!("tuş gönderildi {vk:#x}");
        return;
    }
    if first == "--close" || first == "--visible" {
        let pid = std::env::args().nth(2).and_then(|p| p.parse::<u32>().ok()).unwrap_or(0);
        let hwnd = find_window(pid);
        if first == "--visible" {
            println!("{}", if hwnd.is_some() { 1 } else { 0 });
            return;
        }
        let Some(hwnd) = hwnd else {
            eprintln!("pencere bulunamadı: {pid}");
            std::process::exit(1);
        };
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(Some(hwnd), windows::Win32::UI::WindowsAndMessaging::WM_CLOSE, windows::Win32::Foundation::WPARAM(0), windows::Win32::Foundation::LPARAM(0));
        }
        println!("WM_CLOSE gönderildi");
        return;
    }
    let (Some(pid), Some(out)) = (args.next().and_then(|p| p.parse::<u32>().ok()), args.next()) else {
        eprintln!("kullanım: p2-wincap <pid> <çıktı.png>");
        std::process::exit(2);
    };
    let Some(hwnd) = find_window(pid) else {
        eprintln!("pencere bulunamadı: {pid}");
        std::process::exit(1);
    };
    let Some((w, h, rgba)) = capture::window_rgba(hwnd) else { std::process::exit(1) };
    image::RgbaImage::from_raw(w, h, rgba).expect("boyut").save(&out).expect("kaydet");
    println!("{out} {w}x{h}");
}
