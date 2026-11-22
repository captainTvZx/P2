//! Ekran üstü çizim kaplaması — main.cjs `showOverlay` / `OVERLAY_HTML` karşılığı.
//! Paylaşım sürerken şeffaf, tıklamayı geçiren, her zaman üstte bir pencere paylaşılan ekranı
//! kaplar; izleyenlerin çizdikleri ve kalem imleçleri yayıncının gerçek masaüstünde görünür
//! (ekran yakalandığı için izleyenler de aynı çizimi görür). Yalnız ekran paylaşımında açılır;
//! pencere paylaşımında konum hizalanamaz (Electron ile aynı kural).
//!
//! Testler: `P2_TEST_OVERLAY=1` test deseni paylaşılırken de açar ve kaplamayı istemcinin
//! kendi ana penceresinin üstüne koyar (kullanıcının masaüstünün geri kalanına taşmaz).

use std::time::Instant;

use gpui_kit::prelude::*;
use gpui_kit::{App, AnyWindowHandle, Context, Entity, Subscription, TitlebarOptions, Window, WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, div, point, px, size};
use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{DISPLAY_DEVICEW, EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFOEXW, MonitorFromWindow, MONITOR_DEFAULTTOPRIMARY};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::BOOL;

use crate::annotate::{self, DESKTOP_LOOK};
use crate::call::CallController;

pub const TITLE: &str = "P2 çizim kaplaması";

pub struct DrawOverlay {
    calls: Entity<CallController>,
    me: String,
    /// Kaplamanın mantıksal boyutu (px).
    size: (f32, f32),
    traced: Option<Instant>,
    _observe: Subscription,
}

impl DrawOverlay {
    fn new(calls: Entity<CallController>, me: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let observe = cx.observe(&calls, |_, _, cx| cx.notify());
        let b = window.bounds();
        Self { calls, me, size: (f32::from(b.size.width), f32::from(b.size.height)), traced: None, _observe: observe }
    }
}

impl Render for DrawOverlay {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let b = window.viewport_size();
        self.size = (f32::from(b.width), f32::from(b.height));
        let board = self.calls.read(cx).boards.get(&self.me).cloned().unwrap_or_default();
        if !board.is_empty() && self.traced.is_none_or(|t| t.elapsed().as_millis() >= 1000) {
            self.traced = Some(Instant::now());
            crate::trace(&format!("kaplama: {} çizgi, {} imleç çiziliyor", board.strokes.len(), board.cursors.len()));
        }
        let cursors = annotate::cursor_layer(&board, (0.0, 0.0, self.size.0, self.size.1), DESKTOP_LOOK);
        div().size_full().relative().font_family("Segoe UI").child(annotate::strokes_canvas(board, None)).children(cursors)
    }
}

/// Paylaşılan ekranın fiziksel dikdörtgeni. libwebrtc ekran kimliği = EnumDisplayDevices sırası
/// (`\\.\DISPLAYn`); eşleşmezse birincil ekran (main.cjs `overlayDisplay`).
pub fn screen_rect(id: u64) -> RECT {
    unsafe {
        let mut dev = DISPLAY_DEVICEW { cb: size_of::<DISPLAY_DEVICEW>() as u32, ..Default::default() };
        let name = EnumDisplayDevicesW(None, id as u32, &mut dev, 0).as_bool().then(|| String::from_utf16_lossy(&dev.DeviceName).trim_end_matches('\0').to_string());
        struct Find {
            name: Option<String>,
            found: Option<RECT>,
        }
        extern "system" fn each(m: HMONITOR, _: HDC, _: *mut RECT, lp: LPARAM) -> BOOL {
            let f = unsafe { &mut *(lp.0 as *mut Find) };
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
            if unsafe { GetMonitorInfoW(m, &mut info as *mut MONITORINFOEXW as *mut windows::Win32::Graphics::Gdi::MONITORINFO) }.as_bool() {
                let dev = String::from_utf16_lossy(&info.szDevice).trim_end_matches('\0').to_string();
                if f.name.as_deref() == Some(dev.as_str()) {
                    f.found = Some(info.monitorInfo.rcMonitor);
                    return BOOL(0);
                }
            }
            BOOL(1)
        }
        let mut find = Find { name, found: None };
        let _ = EnumDisplayMonitors(None, None, Some(each), LPARAM(&mut find as *mut Find as isize));
        find.found.unwrap_or_else(|| {
            let m = MonitorFromWindow(HWND::default(), MONITOR_DEFAULTTOPRIMARY);
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
            let _ = GetMonitorInfoW(m, &mut info as *mut MONITORINFOEXW as *mut windows::Win32::Graphics::Gdi::MONITORINFO);
            info.monitorInfo.rcMonitor
        })
    }
}

/// Pencereyi tıklamayı geçiren, odak almayan, en üstte bir katman yapar ve fiziksel
/// dikdörtgene yerleştirir (setIgnoreMouseEvents + setAlwaysOnTop('screen-saver') + showInactive).
pub fn make_click_through(hwnd: HWND, rect: RECT) {
    unsafe {
        // Çerçevesiz: stil 0 (WS_OVERLAPPED) ekrana 8 px çerçeve ekliyordu; katman ekranı tam kaplamıyordu.
        SetWindowLongPtrW(hwnd, GWL_STYLE, (WS_POPUP.0 | WS_VISIBLE.0) as isize);
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let ex = ex | WS_EX_LAYERED.0 | WS_EX_TRANSPARENT.0 | WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0 | WS_EX_TOPMOST.0;
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex as isize);
        let _ = SetLayeredWindowAttributes(hwnd, windows::Win32::Foundation::COLORREF(0), 255, LWA_ALPHA);
        // Windows 11 kenarlığı/köşesi/gölgesi yok: paylaşılan ekranda saydam çerçeve belirmesin.
        crate::shell::plain_popup(hwnd);
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top, SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_FRAMECHANGED);
    }
}

/// Kaplamayı aç. `screen`: paylaşılan ekranın libwebrtc kimliği; `None` = test (ana pencerenin üstü).
pub fn open(calls: Entity<CallController>, me: String, screen: Option<u64>, cx: &mut App) -> Option<AnyWindowHandle> {
    let rect = match screen {
        Some(id) => screen_rect(id),
        None => {
            let main = HWND(crate::MAIN_HWND.load(std::sync::atomic::Ordering::Relaxed) as _);
            let mut r = RECT::default();
            unsafe {
                let _ = GetWindowRect(main, &mut r);
            }
            r
        }
    };
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(gpui_kit::Bounds { origin: point(px(0.), px(0.)), size: size(px(320.), px(200.)) })),
        titlebar: Some(TitlebarOptions { title: Some(TITLE.into()), appears_transparent: true, traffic_light_position: None }),
        focus: false,
        show: false,
        kind: WindowKind::PopUp,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        window_background: WindowBackgroundAppearance::Transparent,
        ..Default::default()
    };
    // Taşıma/boyutlama pencere açma işleminin İÇİNDE yapılmaz: başka ölçekli bir ekrana taşımak
    // WM_DPICHANGED'i eşzamanlı gönderir, GPUI uygulamayı ikinci kez ödünç almaya çalışıp panikler
    // (1.0.4: ikinci ekranı paylaşınca istemci kapanıyordu). Uygulama serbestken ayrı görevde yapılır.
    let mut hwnd = None;
    let handle = cx
        .open_window(options, |window, cx| {
            hwnd = crate::shell::hwnd_of(window);
            cx.new(|cx| DrawOverlay::new(calls, me, window, cx))
        })
        .ok()?;
    if let Some(hwnd) = hwnd {
        let raw = hwnd.0 as isize;
        cx.spawn(async move |_| make_click_through(HWND(raw as _), rect)).detach();
    }
    crate::trace(&format!("kaplama: açıldı {}x{} @ {},{} (tıklama geçirir, en üstte)", rect.right - rect.left, rect.bottom - rect.top, rect.left, rect.top));
    Some(handle.into())
}

pub fn close(handle: AnyWindowHandle, cx: &mut App) {
    let _ = handle.update(cx, |_, window, _| window.remove_window());
    crate::trace("kaplama: kapandı");
}
