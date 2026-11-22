//! Oyun içi tarayıcı: WebView2 (Edge) ekran dışındaki gizli bir pencerede çalışır; görüntüsü
//! `CapturePreview` ile alınıp overlay penceresine çizilir, fare ve klavye o pencereye iletilir.
//! Kendi iş parçacığında (COM STA + Win32 mesaj döngüsü) yaşar; host ile kanallarla konuşur.

use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::time::{Duration, Instant};

use webview2_com::Microsoft::Web::WebView2::Win32::*;
use webview2_com::*;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, IStream, STATFLAG_NONAME, STREAM_SEEK_SET};
use windows::Win32::UI::Shell::SHCreateMemStream;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, HSTRING, PCWSTR, PWSTR, w};

/// Host → tarayıcı.
#[derive(Clone, Debug)]
pub enum BrowserCmd {
    Navigate(String),
    Resize(u32, u32),
    Back,
    Forward,
    Reload,
    /// Fare: `msg` WM_MOUSEMOVE/WM_LBUTTONDOWN/…; x, y görünüm içi piksel; `wheel` WM_MOUSEWHEEL için.
    Mouse { msg: u32, x: i32, y: i32, wheel: i32 },
    Key { vk: u32, down: bool },
    Char(u32),
    /// Görüntü yakalama açık/kapalı (pencere görünmüyorsa boşuna çekilmesin).
    Capture(bool),
}

/// Tarayıcı → host.
pub enum BrowserEvent {
    Frame { w: u32, h: u32, bgra: Vec<u8> },
    Url(String),
    Error(String),
}

pub struct Browser {
    tx: Sender<BrowserCmd>,
    pub events: Receiver<BrowserEvent>,
    thread: Option<std::thread::JoinHandle<()>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Browser {
    pub fn start(url: String, w: u32, h: u32) -> Self {
        let (tx, rx) = channel();
        let (etx, events) = channel();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop2 = stop.clone();
        let thread = std::thread::Builder::new()
            .name("p2-browser".into())
            .spawn(move || {
                if let Err(e) = run(url, w.max(64), h.max(64), rx, &etx, &stop2) {
                    let _ = etx.send(BrowserEvent::Error(format!("{e}")));
                }
            })
            .ok();
        Self { tx, events, thread, stop }
    }

    pub fn send(&self, cmd: BrowserCmd) {
        let _ = self.tx.send(cmd);
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        // Beklenmez: WebView2 kurulumu takılırsa overlay donmasın; iş parçacığı bayrağı görünce kendisi biter.
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        drop(self.thread.take());
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

/// Ekran dışı konum (-32000 simge durumundaki pencerelerin konumu; ondan uzak durulur).
const OFF_X: i32 = -24000;
const OFF_Y: i32 = -24000;

fn run(url: String, w: u32, h: u32, rx: Receiver<BrowserCmd>, etx: &Sender<BrowserEvent>, stop: &std::sync::atomic::AtomicBool) -> windows::core::Result<()> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let instance = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)?;
        let class = w!("P2OverlayBrowser");
        let wc = WNDCLASSW { lpfnWndProc: Some(wndproc), hInstance: instance.into(), lpszClassName: class, ..Default::default() };
        RegisterClassW(&wc);
        // Görünür ama ekran dışında: Chromium görünmez pencerede çizimi durdurur.
        let hwnd = CreateWindowExW(WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE, class, w!("P2 Browser"), WS_POPUP, OFF_X, OFF_Y, w as i32, h as i32, None, None, Some(instance.into()), None)?;
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);

        let data_dir = std::env::var("LOCALAPPDATA").map(|d| format!("{d}\\P2\\overlay-browser")).unwrap_or_default();
        let options = CoreWebView2EnvironmentOptions::default();
        // Ekran dışı pencere "örtülü" sayılıp çizim kısılmasın.
        options.set_additional_browser_arguments("--disable-features=CalculateNativeWinOcclusion --disable-backgrounding-occluded-windows --disable-renderer-backgrounding".to_string());
        let (env_tx, env_rx) = channel();
        CreateCoreWebView2EnvironmentCompletedHandler::wait_for_async_operation(
            Box::new(move |handler| {
                CreateCoreWebView2EnvironmentWithOptions(PCWSTR::null(), &HSTRING::from(data_dir), &ICoreWebView2EnvironmentOptions::from(options), &handler).map_err(webview2_com::Error::WindowsError)
            }),
            Box::new(move |error_code, environment| {
                error_code?;
                let _ = env_tx.send(environment);
                Ok(())
            }),
        )
        .map_err(to_win)?;
        let env = env_rx.recv().ok().flatten().ok_or_else(|| windows::core::Error::from_hresult(windows::Win32::Foundation::E_POINTER))?;

        let (ctl_tx, ctl_rx) = channel();
        let env2 = env.clone();
        CreateCoreWebView2ControllerCompletedHandler::wait_for_async_operation(
            Box::new(move |handler| env2.CreateCoreWebView2Controller(hwnd, &handler).map_err(webview2_com::Error::WindowsError)),
            Box::new(move |error_code, controller| {
                error_code?;
                let _ = ctl_tx.send(controller);
                Ok(())
            }),
        )
        .map_err(to_win)?;
        let controller = ctl_rx.recv().ok().flatten().ok_or_else(|| windows::core::Error::from_hresult(windows::Win32::Foundation::E_POINTER))?;
        controller.SetBounds(RECT { left: 0, top: 0, right: w as i32, bottom: h as i32 })?;
        controller.SetIsVisible(true)?;
        let webview = controller.CoreWebView2()?;
        if let Ok(settings) = webview.Settings() {
            let _ = settings.SetAreDevToolsEnabled(false);
            let _ = settings.SetIsStatusBarEnabled(false);
        }
        // Yeni sekme açmak isteyen bağlantılar aynı görünümde açılır.
        let wv = webview.clone();
        let mut token = Default::default();
        let _ = webview.add_NewWindowRequested(
            &NewWindowRequestedEventHandler::create(Box::new(move |_, args| {
                if let Some(args) = args {
                    let mut uri = PWSTR::null();
                    if args.Uri(&mut uri).is_ok() {
                        let u = take_pwstr(uri);
                        let _ = args.SetHandled(true);
                        let _ = wv.Navigate(&HSTRING::from(u));
                    }
                }
                Ok(())
            })),
            &mut token,
        );
        webview.Navigate(&HSTRING::from(url.as_str()))?;

        let (mut size, mut capture, mut busy) = ((w, h), true, false);
        let (frame_tx, frame_rx) = channel::<Option<Vec<u8>>>();
        let mut last_capture = Instant::now() - Duration::from_secs(1);
        let mut last_url = String::new();
        // Girdi hedefi her seferinde yeniden bulunur: Chromium siteler arası geçişte giriş penceresini
        // yeniden oluşturur; sayfa yüklenmeden gelen ilk tıklama da yanlış pencereyi saklıyordu.
        let mut msg = MSG::default();
        while !stop.load(std::sync::atomic::Ordering::SeqCst) {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            loop {
                let cmd = match rx.try_recv() {
                    Ok(c) => c,
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return Ok(()),
                };
                match cmd {
                    BrowserCmd::Navigate(u) => {
                        let _ = webview.Navigate(&HSTRING::from(u));
                    }
                    BrowserCmd::Resize(nw, nh) if (nw, nh) != size && nw >= 64 && nh >= 64 => {
                        size = (nw, nh);
                        let _ = SetWindowPos(hwnd, None, OFF_X, OFF_Y, nw as i32, nh as i32, SWP_NOACTIVATE | SWP_NOZORDER);
                        let _ = controller.SetBounds(RECT { left: 0, top: 0, right: nw as i32, bottom: nh as i32 });
                    }
                    BrowserCmd::Resize(..) => {}
                    BrowserCmd::Back => {
                        let _ = webview.GoBack();
                    }
                    BrowserCmd::Forward => {
                        let _ = webview.GoForward();
                    }
                    BrowserCmd::Reload => {
                        let _ = webview.Reload();
                    }
                    BrowserCmd::Capture(on) => capture = on,
                    BrowserCmd::Mouse { msg: m, x, y, wheel } => {
                        let t = input_hwnd(hwnd);
                        if m == WM_LBUTTONDOWN || m == WM_RBUTTONDOWN {
                            let _ = controller.MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC);
                        }
                        let keys: usize = match m {
                            WM_LBUTTONDOWN => 0x1,
                            WM_RBUTTONDOWN => 0x2,
                            _ => 0,
                        };
                        if m == WM_MOUSEWHEEL {
                            // Tekerlek: lParam ekran koordinatı, wParam yüksek sözcük delta.
                            let lp = pack(OFF_X + x, OFF_Y + y);
                            let wp = ((wheel as i16 as u16 as usize) << 16) | keys;
                            let _ = PostMessageW(Some(t), WM_MOUSEWHEEL, WPARAM(wp), LPARAM(lp));
                        } else {
                            let _ = PostMessageW(Some(t), m, WPARAM(keys), LPARAM(pack(x, y)));
                        }
                    }
                    BrowserCmd::Key { vk, down } => {
                        let t = input_hwnd(hwnd);
                        let scan = windows::Win32::UI::Input::KeyboardAndMouse::MapVirtualKeyW(vk, windows::Win32::UI::Input::KeyboardAndMouse::MAPVK_VK_TO_VSC);
                        let mut lp = 1 | (scan << 16) as isize;
                        if !down {
                            lp |= (0b11 << 30) as isize;
                        }
                        let _ = PostMessageW(Some(t), if down { WM_KEYDOWN } else { WM_KEYUP }, WPARAM(vk as usize), LPARAM(lp));
                    }
                    BrowserCmd::Char(c) => {
                        let t = input_hwnd(hwnd);
                        let _ = PostMessageW(Some(t), WM_CHAR, WPARAM(c as usize), LPARAM(1));
                    }
                }
            }
            // Yakalama tamamlandı mı?
            while let Ok(png) = frame_rx.try_recv() {
                busy = false;
                if let Some(png) = png {
                    if let Ok(img) = image::load_from_memory(&png) {
                        let mut rgba = img.to_rgba8();
                        let (fw, fh) = rgba.dimensions();
                        for px in rgba.pixels_mut() {
                            px.0.swap(0, 2);
                        }
                        let _ = etx.send(BrowserEvent::Frame { w: fw, h: fh, bgra: rgba.into_raw() });
                    }
                }
            }
            // ~15 kare/sn; önceki kare bitmeden yenisi istenmez.
            if capture && !busy && last_capture.elapsed() >= Duration::from_millis(66) {
                last_capture = Instant::now();
                if let Some(stream) = SHCreateMemStream(None) {
                    let (ftx, s2) = (frame_tx.clone(), stream.clone());
                    let handler = CapturePreviewCompletedHandler::create(Box::new(move |result| {
                        let _ = ftx.send(result.ok().and_then(|_| read_stream(&s2)));
                        Ok(())
                    }));
                    if webview.CapturePreview(COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG, &stream, &handler).is_ok() {
                        busy = true;
                    }
                }
                let mut src = PWSTR::null();
                if webview.Source(&mut src).is_ok() {
                    let u = take_pwstr(src);
                    if u != last_url {
                        last_url = u.clone();
                        let _ = etx.send(BrowserEvent::Url(u));
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(8));
        }
        let _ = controller.Close();
        let _ = DestroyWindow(hwnd);
        Ok(())
    }
}

fn to_win(e: webview2_com::Error) -> windows::core::Error {
    match e {
        webview2_com::Error::WindowsError(e) => e,
        other => windows::core::Error::new(windows::Win32::Foundation::E_FAIL, format!("{other:?}")),
    }
}

fn pack(x: i32, y: i32) -> isize {
    ((y as i16 as u16 as u32) << 16 | (x as i16 as u16 as u32)) as i32 as isize
}

fn take_pwstr(p: PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    let s = unsafe { p.to_string().unwrap_or_default() };
    unsafe { windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as *const _)) };
    s
}

unsafe fn read_stream(stream: &IStream) -> Option<Vec<u8>> {
    unsafe {
        let mut stat = Default::default();
        stream.Stat(&mut stat, STATFLAG_NONAME).ok()?;
        let len = stat.cbSize as usize;
        stream.Seek(0, STREAM_SEEK_SET, None).ok()?;
        let mut buf = vec![0u8; len];
        let mut read = 0u32;
        stream.Read(buf.as_mut_ptr().cast(), len as u32, Some(&mut read)).ok().ok()?;
        buf.truncate(read as usize);
        Some(buf)
    }
}

/// Girdiyi alan Chromium penceresi (`Chrome_RenderWidgetHostHWND`); bulunamazsa en derin çocuk.
fn input_hwnd(root: HWND) -> HWND {
    unsafe extern "system" fn visit(h: HWND, lp: LPARAM) -> BOOL {
        unsafe {
            let out = &mut *(lp.0 as *mut (Option<HWND>, Option<HWND>));
            let mut name = [0u16; 64];
            let n = GetClassNameW(h, &mut name);
            let class = String::from_utf16_lossy(&name[..n as usize]);
            if class == "Chrome_RenderWidgetHostHWND" {
                out.0 = Some(h);
                return BOOL(0);
            }
            out.1 = Some(h);
            BOOL(1)
        }
    }
    let mut found: (Option<HWND>, Option<HWND>) = (None, None);
    unsafe {
        let _ = EnumChildWindows(Some(root), Some(visit), LPARAM(&mut found as *mut _ as isize));
    }
    found.0.or(found.1).unwrap_or(root)
}
