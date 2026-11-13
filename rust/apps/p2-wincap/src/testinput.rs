//! Dilim 12 test yardımcıları. Girdi YALNIZ testin kendi başlattığı sürecin pencerelerine
//! PostMessage ile gider (masaüstüne sentetik giriş yok); listeleme ve isabet denetimi salt okur.
//!   p2-wincap --list <pid>                              görünür pencereler: başlık, dikdörtgen, exstyle
//!   p2-wincap --hit <x> <y>                             o noktadaki pencere (tıklama geçiren katmanlar atlanır)
//!   p2-wincap --click <başlık> <pid> <x> <y>            istemci koordinatında (fiziksel px) sol tık
//!   p2-wincap --rclick <başlık> <pid> <x> <y>           sağ tık (bağlam menüleri; parite aracı)
//!   p2-wincap --move <başlık> <pid> <x> <y>             yalnız fare hareketi (hover durumları)
//!   p2-wincap --drag <başlık> <pid> x1 y1 x2 y2 [adım]  sol tuş basılı sürükleme
//!   p2-wincap --minimize <başlık> <pid>                 WM_SYSCOMMAND SC_MINIMIZE

use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::BOOL;

fn title_of(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn windows_of(pid: u32) -> Vec<HWND> {
    struct L {
        pid: u32,
        out: Vec<HWND>,
    }
    extern "system" fn each(hwnd: HWND, lp: LPARAM) -> BOOL {
        let l = unsafe { &mut *(lp.0 as *mut L) };
        let mut owner = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut owner)) };
        if owner == l.pid && unsafe { IsWindowVisible(hwnd) }.as_bool() {
            l.out.push(hwnd);
        }
        BOOL(1)
    }
    let mut l = L { pid, out: Vec::new() };
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut l as *mut L as isize));
    }
    l.out
}

/// Başlık parçası ya da `--list`'in yazdığı tutamaç ("0x1a2b"); yalnız o sürecin pencereleri.
fn titled(pid: u32, title: &str) -> Option<HWND> {
    if let Some(hex) = title.strip_prefix("0x") {
        let want = usize::from_str_radix(hex, 16).ok()?;
        return windows_of(pid).into_iter().find(|h| h.0 as usize == want);
    }
    windows_of(pid).into_iter().find(|h| title_of(*h).contains(title))
}

fn lparam(x: i32, y: i32) -> LPARAM {
    LPARAM(((y as u32 as isize & 0xffff) << 16) | (x as u32 as isize & 0xffff))
}

pub fn run(args: &[String]) -> bool {
    let num = |i: usize| args.get(i).and_then(|s| s.parse::<i32>().ok()).unwrap_or(0);
    match args.first().map(String::as_str) {
        Some("--list") => {
            for h in windows_of(num(1) as u32) {
                let mut r = RECT::default();
                unsafe {
                    let _ = GetWindowRect(h, &mut r);
                }
                let ex = unsafe { GetWindowLongPtrW(h, GWL_EXSTYLE) } as u32;
                println!("{:#x}\t{}\t{},{},{},{}\tex={:#x}", h.0 as usize, title_of(h), r.left, r.top, r.right - r.left, r.bottom - r.top, ex);
            }
            true
        }
        Some("--hit") => {
            let h = unsafe { WindowFromPoint(POINT { x: num(1), y: num(2) }) };
            let top = unsafe { GetAncestor(h, GA_ROOT) };
            let mut owner = 0u32;
            unsafe { GetWindowThreadProcessId(top, Some(&mut owner)) };
            println!("{:#x}\t{}\tpid={owner}", top.0 as usize, title_of(top));
            true
        }
        Some("--click") | Some("--rclick") | Some("--move") | Some("--drag") | Some("--minimize") => {
            let (title, pid) = (args.get(1).cloned().unwrap_or_default(), num(2) as u32);
            let Some(h) = titled(pid, &title) else {
                eprintln!("pencere bulunamadı: {pid} «{title}»");
                std::process::exit(1);
            };
            let post = |msg: u32, w: usize, l: LPARAM| unsafe {
                let _ = PostMessageW(Some(h), msg, WPARAM(w), l);
            };
            match args[0].as_str() {
                "--minimize" => post(WM_SYSCOMMAND, SC_MINIMIZE as usize, LPARAM(0)),
                "--move" => post(WM_MOUSEMOVE, 0, lparam(num(3), num(4))),
                "--rclick" => {
                    let (x, y) = (num(3), num(4));
                    post(WM_MOUSEMOVE, 0, lparam(x, y));
                    std::thread::sleep(std::time::Duration::from_millis(60));
                    post(WM_RBUTTONDOWN, 2, lparam(x, y));
                    std::thread::sleep(std::time::Duration::from_millis(60));
                    post(WM_RBUTTONUP, 0, lparam(x, y));
                }
                "--click" => {
                    let (x, y) = (num(3), num(4));
                    post(WM_MOUSEMOVE, 0, lparam(x, y));
                    std::thread::sleep(std::time::Duration::from_millis(60));
                    post(WM_LBUTTONDOWN, 1, lparam(x, y));
                    std::thread::sleep(std::time::Duration::from_millis(60));
                    post(WM_LBUTTONUP, 0, lparam(x, y));
                }
                _ => {
                    let (x1, y1, x2, y2) = (num(3), num(4), num(5), num(6));
                    let steps = args.get(7).and_then(|s| s.parse::<i32>().ok()).unwrap_or(20).max(1);
                    post(WM_MOUSEMOVE, 0, lparam(x1, y1));
                    std::thread::sleep(std::time::Duration::from_millis(40));
                    post(WM_LBUTTONDOWN, 1, lparam(x1, y1));
                    for i in 1..=steps {
                        std::thread::sleep(std::time::Duration::from_millis(25));
                        post(WM_MOUSEMOVE, 1, lparam(x1 + (x2 - x1) * i / steps, y1 + (y2 - y1) * i / steps));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(40));
                    post(WM_LBUTTONUP, 0, lparam(x2, y2));
                }
            }
            println!("gönderildi: {} → {:#x}", args[0], h.0 as usize);
            true
        }
        _ => false,
    }
}

/// `--shot <başlık|0xtutamaç> <pid> <çıktı.png>`: o sürecin belirli penceresinin görüntüsü.
pub fn shot(args: &[String]) -> bool {
    if args.first().map(String::as_str) != Some("--shot") {
        return false;
    }
    let pid = args.get(2).and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
    let (Some(h), Some(out)) = (titled(pid, args.get(1).map(String::as_str).unwrap_or("")), args.get(3)) else {
        eprintln!("pencere bulunamadı");
        std::process::exit(1);
    };
    let Some((w, hgt, rgba)) = crate::capture::window_rgba(h) else { std::process::exit(1) };
    image::RgbaImage::from_raw(w, hgt, rgba).expect("boyut").save(out).expect("kaydet");
    println!("{out} {w}x{hgt}");
    true
}
