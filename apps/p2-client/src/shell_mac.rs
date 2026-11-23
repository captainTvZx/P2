//! macOS uygulama kabuğu — `shell.rs` (Windows) ile aynı arayüz.
//!
//! - Tek örnek: profil klasöründe Unix soketi; ikinci açılış argümanlarını ilkine iletir.
//! - Pencere: `HWND` burada `NSWindow` işaretçisidir; göster/gizle/öne al AppKit ile.
//! - Yanıp sönme: Dock simgesi zıplar (`requestUserAttention`).
//! - Tepsi (menü çubuğu simgesi) ve masaüstü bildirimi henüz yok: `start_tray`/`notify` `false`
//!   döner; çağıranlar pencereyi kapatınca çıkar, bildirim yerine Dock'ta dikkat ister.
//! - `p2://` bağlantıları macOS'ta Info.plist `CFBundleURLTypes` ile kaydolur; argüman olarak
//!   değil, uygulamaya "URL aç" olayı olarak gelir (`app.rs` `on_open_urls`).

use std::io::{BufRead as _, BufReader, Write as _};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::mpsc::{Receiver, Sender, channel};

use objc2::rc::Retained;
use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSRequestUserAttentionType, NSView, NSWindow};

/// Tepsi ve ikinci örnekten arayüze.
#[derive(Clone, Debug)]
pub enum ShellEvent {
    Show,
    Quit,
    /// İkinci açılışın argümanları (`p2://play/...`).
    Args(Vec<String>),
    #[allow(dead_code)]
    NotificationClicked,
}

/// AppKit penceresi (`NSWindow *`). Windows'taki `HWND` ile aynı biçimde taşınır.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HWND(pub *mut core::ffi::c_void);

unsafe impl Send for HWND {}
unsafe impl Sync for HWND {}

impl HWND {
    pub fn is_invalid(&self) -> bool {
        self.0.is_null()
    }

    fn window(&self) -> Option<&NSWindow> {
        // Yalnız ana iş parçacığında (AppKit kuralı).
        MainThreadMarker::new()?;
        unsafe { (self.0 as *const NSWindow).as_ref() }
    }
}

fn socket_path() -> std::path::PathBuf {
    p2_client_core::store::profile_dir().join("p2.sock")
}

/// Tek örnek: başka bir örnek açıksa argümanlar ona iletilir ve `false` döner (çık).
pub fn single_instance(tx: Sender<ShellEvent>) -> bool {
    let path = socket_path();
    if let Ok(mut stream) = UnixStream::connect(&path) {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let _ = stream.write_all(format!("{}\n", serde_json::to_string(&args).unwrap_or_default()).as_bytes());
        return false;
    }
    // Önceki çökmeden kalan soket dosyası.
    let _ = std::fs::remove_file(&path);
    let Ok(listener) = UnixListener::bind(&path) else { return true };
    std::thread::Builder::new()
        .name("p2-tek-ornek".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut line = String::new();
                if BufReader::new(stream).read_line(&mut line).is_ok() {
                    let args: Vec<String> = serde_json::from_str(line.trim()).unwrap_or_default();
                    let _ = tx.send(ShellEvent::Args(args));
                }
            }
        })
        .ok();
    true
}

/// Menü çubuğu simgesi henüz yok.
pub fn start_tray(_tx: Sender<ShellEvent>) -> bool {
    false
}

/// Otomatik pilot (testler) masaüstünde iz bırakmaz (`P2_REAL_NOTIFY` ile açılır).
pub fn quiet() -> bool {
    (std::env::var_os("P2_SILENT_NOTIFY").is_some() || std::env::var_os("P2_AUTOPILOT_SERVER").is_some()) && std::env::var_os("P2_REAL_NOTIFY").is_none()
}

/// Masaüstü bildirimi henüz yok (`false`: çağıran Dock'ta dikkat ister).
pub fn notify(_title: &str, _body: &str) -> bool {
    quiet()
}

pub fn remove_tray() {}

// ---------------------------------------------------------------- ana pencere

pub fn hwnd_of(window: &gpui_kit::Window) -> Option<HWND> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::AppKit(h) => {
            let view = unsafe { (h.ns_view.as_ptr() as *const NSView).as_ref()? };
            let window: Option<Retained<NSWindow>> = view.window();
            window.map(|w| HWND(Retained::as_ptr(&w) as *mut _))
        }
        _ => None,
    }
}

/// Windows'taki DWM çerçeve ayarları: macOS'ta karşılığı yok (GPUI pencereyi kendisi kurar).
pub fn plain_popup(_hwnd: HWND) {}

pub fn strip_frame(_hwnd: HWND) {}

pub fn hide(hwnd: HWND) {
    if let Some(w) = hwnd.window() {
        w.orderOut(None);
    }
}

/// Pencereyi her zaman üstte tutar (açılış kartı); etkinleştirmez.
pub fn set_topmost(hwnd: HWND, on: bool) {
    if let Some(w) = hwnd.window() {
        // NSFloatingWindowLevel = 3, NSNormalWindowLevel = 0.
        w.setLevel(if on { 3 } else { 0 });
    }
}

pub fn show(hwnd: HWND) {
    let Some(w) = hwnd.window() else { return };
    if w.isMiniaturized() {
        w.deminiaturize(None);
    }
    w.makeKeyAndOrderFront(None);
    if let Some(mtm) = MainThreadMarker::new() {
        #[allow(deprecated)]
        NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
    }
}

pub fn is_visible(hwnd: HWND) -> bool {
    hwnd.window().is_some_and(|w| w.isVisible() && !w.isMiniaturized())
}

/// Dock simgesini zıplat (uygulama öndeyse etkisiz).
pub fn flash(_hwnd: HWND, on: bool) {
    if quiet() || !on {
        return;
    }
    if let Some(mtm) = MainThreadMarker::new() {
        let app = NSApplication::sharedApplication(mtm);
        app.requestUserAttention(NSRequestUserAttentionType::InformationalRequest);
    }
}

pub fn is_foreground(hwnd: HWND) -> bool {
    let Some(w) = hwnd.window() else { return false };
    let Some(mtm) = MainThreadMarker::new() else { return false };
    NSApplication::sharedApplication(mtm).isActive() && w.isKeyWindow()
}

pub fn set_app_user_model_id() {}

pub fn apply_app_details(_hwnd: HWND) {}

/// `p2://` macOS'ta paketin Info.plist'inde kayıtlı; çalışma anında yazılacak bir şey yok.
pub fn register_protocol() -> bool {
    false
}

/// `p2://play/<proje>` ya da `--p2-play=<proje>` (main.cjs `playFromArgs`).
pub fn play_target(args: &[String]) -> Option<String> {
    let found = args.iter().find(|a| a.starts_with("--p2-play=") || a.starts_with("p2://play/"))?;
    let id = found.strip_prefix("p2://play/").or_else(|| found.strip_prefix("--p2-play="))?;
    let id = id.trim_end_matches('/');
    (!id.is_empty()).then(|| id.to_string())
}

pub fn events() -> (Sender<ShellEvent>, Receiver<ShellEvent>) {
    channel()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn play_args() {
        assert_eq!(play_target(&["p2://play/abc/".into()]), Some("abc".into()));
        assert_eq!(play_target(&["--p2-play=xyz".into()]), Some("xyz".into()));
        assert_eq!(play_target(&["--başka".into()]), None);
    }
}
