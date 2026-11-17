//! Dilim 12 test yardımcıları. Girdi YALNIZ testin kendi başlattığı sürecin pencerelerine
//! PostMessage ile gider (masaüstüne sentetik giriş yok); listeleme ve isabet denetimi salt okur.
//!   p2-wincap --list <pid>                              görünür pencereler: başlık, dikdörtgen, exstyle
//!   p2-wincap --hit <x> <y>                             o noktadaki pencere (tıklama geçiren katmanlar atlanır)
//!   p2-wincap --click <başlık> <pid> <x> <y>            istemci koordinatında (fiziksel px) sol tık
//!   p2-wincap --rclick <başlık> <pid> <x> <y>           sağ tık (bağlam menüleri; parite aracı)
//!   p2-wincap --move <başlık> <pid> <x> <y>             yalnız fare hareketi (hover durumları)
//!   p2-wincap --drag <başlık> <pid> x1 y1 x2 y2 [adım]  sol tuş basılı sürükleme
//!   p2-wincap --minimize <başlık> <pid>                 WM_SYSCOMMAND SC_MINIMIZE
//!   p2-wincap --dclick <başlık> <pid> <x> <y>           çift tık (iki hızlı sol tık)
//!   p2-wincap --key <başlık> <pid> <vk>                 WM_KEYDOWN/UP (ondalık sanal tuş kodu; ör. 46 = Delete).
//!                                                       Ctrl/Shift taklit EDİLEMEZ (GetKeyState gerçek klavyeyi okur)
//!   p2-wincap --wheel <başlık> <pid> <x> <y> <delta>    fare tekerleği (120 = bir adım yukarı)
//!   p2-wincap --hwheel <başlık> <pid> <x> <y> <delta>   yatay tekerlek / dokunmatik yüzey (WM_MOUSEHWHEEL; 120 = sağa)

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
        Some("--nchit") | Some("--ncclick") | Some("--setsize") | Some("--rect") | Some("--restore") => {
            // Pencere çerçevesi testleri (yalnız testin kendi penceresine; SendMessage/PostMessage):
            //   --nchit <başlık> <pid> <x> <y>      istemci koordinatındaki (fiziksel px, eksi/taşan olabilir) WM_NCHITTEST sonucu
            //   --ncclick <başlık> <pid> <x> <y>    önce WM_MOUSEMOVE, sonra isabet; sonuç küçült/büyüt/kapat düğmesiyse
            //                                       WM_NCLBUTTONDOWN/UP (başlık/kenar sonucunda düğme mesajı GÖNDERİLMEZ:
            //                                       DefWindowProc gerçek fareyi izleyen taşıma/boyutlandırma döngüsüne girerdi)
            //   --setsize <başlık> <pid> <w> <h>    SetWindowPos ile boyut (en küçük boyut sınırı denetimi)
            //   --rect <başlık> <pid>               pencere dikdörtgeni + istemci boyutu + simge durumu
            let (title, pid) = (args.get(1).cloned().unwrap_or_default(), num(2) as u32);
            let Some(h) = titled(pid, &title) else {
                eprintln!("pencere bulunamadı: {pid} «{title}»");
                std::process::exit(1);
            };
            let to_screen = |x: i32, y: i32| {
                let mut pt = POINT { x, y };
                unsafe {
                    let _ = windows::Win32::Graphics::Gdi::ClientToScreen(h, &mut pt);
                }
                pt
            };
            let name = |ht: isize| match ht as u32 {
                HTCLIENT => "HTCLIENT",
                HTCAPTION => "HTCAPTION",
                HTMINBUTTON => "HTMINBUTTON",
                HTMAXBUTTON => "HTMAXBUTTON",
                HTCLOSE => "HTCLOSE",
                HTLEFT => "HTLEFT",
                HTRIGHT => "HTRIGHT",
                HTTOP => "HTTOP",
                HTBOTTOM => "HTBOTTOM",
                HTTOPLEFT => "HTTOPLEFT",
                HTTOPRIGHT => "HTTOPRIGHT",
                HTBOTTOMLEFT => "HTBOTTOMLEFT",
                HTBOTTOMRIGHT => "HTBOTTOMRIGHT",
                HTNOWHERE => "HTNOWHERE",
                _ => "?",
            };
            match args[0].as_str() {
                // Simge durumundaki test penceresini etkinleştirmeden eski yerine (ekran dışı) getirir.
                "--restore" => unsafe {
                    let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
                    println!("geri getirildi");
                },
                "--rect" => {
                    let (mut r, mut c) = (RECT::default(), RECT::default());
                    unsafe {
                        let _ = GetWindowRect(h, &mut r);
                        let _ = GetClientRect(h, &mut c);
                    }
                    let state = if unsafe { IsIconic(h) }.as_bool() { "iconic" } else if unsafe { IsZoomed(h) }.as_bool() { "zoomed" } else { "normal" };
                    println!("rect {},{} {}x{} client {}x{} {state}", r.left, r.top, r.right - r.left, r.bottom - r.top, c.right, c.bottom);
                }
                "--setsize" => {
                    unsafe {
                        let _ = SetWindowPos(h, None, 0, 0, num(3), num(4), SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);
                    }
                    let mut r = RECT::default();
                    unsafe {
                        let _ = GetWindowRect(h, &mut r);
                    }
                    println!("size {}x{}", r.right - r.left, r.bottom - r.top);
                }
                _ => {
                    let (x, y) = (num(3), num(4));
                    let pt = to_screen(x, y);
                    let screen = lparam(pt.x, pt.y);
                    if args[0] == "--ncclick" {
                        unsafe {
                            let _ = PostMessageW(Some(h), WM_MOUSEMOVE, WPARAM(0), lparam(x, y));
                        }
                        std::thread::sleep(std::time::Duration::from_millis(150));
                    }
                    let ht = unsafe { SendMessageW(h, WM_NCHITTEST, Some(WPARAM(0)), Some(screen)) }.0;
                    println!("{} ({ht})", name(ht));
                    if args[0] == "--ncclick" && matches!(ht as u32, HTMINBUTTON | HTMAXBUTTON | HTCLOSE) {
                        unsafe {
                            let _ = PostMessageW(Some(h), WM_NCLBUTTONDOWN, WPARAM(ht as usize), screen);
                        }
                        std::thread::sleep(std::time::Duration::from_millis(60));
                        unsafe {
                            let _ = PostMessageW(Some(h), WM_NCLBUTTONUP, WPARAM(ht as usize), screen);
                        }
                        println!("gönderildi: düğme tıklaması");
                    }
                }
            }
            true
        }
        Some("--click") | Some("--rclick") | Some("--move") | Some("--drag") | Some("--minimize") | Some("--dclick") | Some("--key") | Some("--wheel") | Some("--hwheel") => {
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
                "--dclick" => {
                    let (x, y) = (num(3), num(4));
                    post(WM_MOUSEMOVE, 0, lparam(x, y));
                    std::thread::sleep(std::time::Duration::from_millis(60));
                    // GPUI tıklama sayısını ardışık WM_LBUTTONDOWN'lardan kendisi sayar (CS_DBLCLKS yok).
                    for _ in 0..2 {
                        post(WM_LBUTTONDOWN, 1, lparam(x, y));
                        std::thread::sleep(std::time::Duration::from_millis(30));
                        post(WM_LBUTTONUP, 0, lparam(x, y));
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                }
                "--key" => {
                    let vk = num(3) as usize;
                    post(WM_KEYDOWN, vk, LPARAM(1));
                    std::thread::sleep(std::time::Duration::from_millis(40));
                    post(WM_KEYUP, vk, LPARAM(1 | (1 << 30) | (1 << 31)));
                }
                "--wheel" | "--hwheel" => {
                    // WM_MOUSEWHEEL ekran koordinatı ister.
                    let mut pt = POINT { x: num(3), y: num(4) };
                    unsafe {
                        let _ = windows::Win32::Graphics::Gdi::ClientToScreen(h, &mut pt);
                    }
                    post(WM_MOUSEMOVE, 0, lparam(num(3), num(4)));
                    std::thread::sleep(std::time::Duration::from_millis(40));
                    post(if args[0] == "--hwheel" { WM_MOUSEHWHEEL } else { WM_MOUSEWHEEL }, ((num(5) as i16 as u16 as usize) << 16), lparam(pt.x, pt.y));
                }
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
