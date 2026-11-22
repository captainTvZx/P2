//! Windows uygulama kabuğu (electron/main.cjs): tek örnek (ikinci açılış ilkini öne getirir,
//! `p2://play/<proje>` iletir), sistem tepsisi simgesi ve menüsü, masaüstü bildirimi (tepsi
//! balonu — kayıtlı AppUserModelID gerektirmez), pencereyi gizle/göster/yanıp söndür,
//! `p2://` protokol kaydı (yalnız kurulu sürümde).

use std::io::{BufRead as _, BufReader, Write as _};
use std::sync::OnceLock;
use std::sync::mpsc::{Receiver, Sender, channel};

use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Storage::FileSystem::{CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_NONE, OPEN_EXISTING, PIPE_ACCESS_INBOUND, ReadFile};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Pipes::{ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE, PIPE_WAIT};
use windows::Win32::UI::Shell::{NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIIF_NOSOUND, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW, Shell_NotifyIconW};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{HSTRING, PCWSTR, w};

/// Tepsi ve ikinci örnekten arayüze.
#[derive(Clone, Debug)]
pub enum ShellEvent {
    Show,
    Quit,
    /// İkinci açılışın argümanları (`p2://play/...`).
    Args(Vec<String>),
    NotificationClicked,
}

fn profile_key() -> String {
    let dir = p2_client_core::store::profile_dir().to_string_lossy().to_lowercase();
    // FNV-1a: profil başına ayrı kilit (testler farklı profillerle aynı anda çalışır).
    let mut h: u64 = 0xcbf29ce484222325;
    for b in dir.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

fn pipe_name() -> HSTRING {
    HSTRING::from(format!(r"\\.\pipe\P2Rust-{}", profile_key()))
}

/// Tek örnek: başka bir örnek açıksa argümanlar ona iletilir ve `false` döner (çık).
pub fn single_instance(tx: Sender<ShellEvent>) -> bool {
    let name = pipe_name();
    // İlk örnek mi? Boru zaten varsa bağlan ve argümanları yaz.
    unsafe {
        if let Ok(handle) = CreateFileW(&name, windows::Win32::Foundation::GENERIC_WRITE.0, FILE_SHARE_NONE, None, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, None) {
            let args: Vec<String> = std::env::args().skip(1).collect();
            let line = format!("{}\n", serde_json::to_string(&args).unwrap_or_default());
            let mut file = std::fs::File::from(std::os::windows::io::OwnedHandle::from_raw_handle(handle.0 as _));
            let _ = file.write_all(line.as_bytes());
            return false;
        }
    }
    std::thread::Builder::new()
        .name("p2-tek-ornek".into())
        .spawn(move || loop {
            let pipe = unsafe { CreateNamedPipeW(&name, PIPE_ACCESS_INBOUND, PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT, 4, 0, 4096, 0, None) };
            if pipe.is_invalid() {
                return;
            }
            let _ = unsafe { ConnectNamedPipe(pipe, None) };
            let reader = PipeReader(pipe);
            let mut line = String::new();
            let _ = BufReader::new(reader).read_line(&mut line);
            let args: Vec<String> = serde_json::from_str(line.trim()).unwrap_or_default();
            unsafe {
                let _ = DisconnectNamedPipe(pipe);
                let _ = windows::Win32::Foundation::CloseHandle(pipe);
            }
            if tx.send(ShellEvent::Args(args)).is_err() {
                return;
            }
        })
        .ok();
    true
}

use std::os::windows::io::FromRawHandle as _;

struct PipeReader(HANDLE);
impl std::io::Read for PipeReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let mut n = 0u32;
        unsafe { ReadFile(self.0, Some(buf), Some(&mut n), None) }.map(|_| n as usize).or(Ok(0))
    }
}

// ---------------------------------------------------------------- tepsi

const WM_TRAY: u32 = WM_APP + 1;
const WM_NOTIFY_SHOW: u32 = WM_APP + 2;
const NIN_BALLOONUSERCLICK: u32 = WM_USER + 5;
const MENU_OPEN: usize = 1;
const MENU_QUIT: usize = 2;

struct TrayState {
    tx: Sender<ShellEvent>,
    pending: std::sync::Mutex<Option<(String, String)>>,
}

static TRAY: OnceLock<TrayState> = OnceLock::new();
static TRAY_HWND: OnceLock<usize> = OnceLock::new();

fn wide_into(dst: &mut [u16], s: &str) {
    for (d, c) in dst.iter_mut().zip(s.encode_utf16().chain(std::iter::once(0))) {
        *d = c;
    }
    if let Some(last) = dst.last_mut() {
        *last = 0;
    }
}

fn icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut data = NOTIFYICONDATAW { cbSize: size_of::<NOTIFYICONDATAW>() as u32, hWnd: hwnd, uID: 1, ..Default::default() };
    data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    data.uCallbackMessage = WM_TRAY;
    // Uygulama simgesi (exe kaynağı yoksa Windows varsayılanı).
    data.hIcon = unsafe { LoadIconW(GetModuleHandleW(None).ok().map(|m| m.into()), PCWSTR(1 as _)).or_else(|_| LoadIconW(None, IDI_APPLICATION)).unwrap_or_default() };
    wide_into(&mut data.szTip, "P2");
    data
}

extern "system" fn tray_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let Some(state) = TRAY.get() else { return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) } };
    match msg {
        WM_TRAY => {
            let event = (lparam.0 & 0xffff) as u32;
            match event {
                WM_LBUTTONUP | WM_LBUTTONDBLCLK => {
                    let _ = state.tx.send(ShellEvent::Show);
                }
                NIN_BALLOONUSERCLICK => {
                    let _ = state.tx.send(ShellEvent::NotificationClicked);
                }
                WM_RBUTTONUP | WM_CONTEXTMENU => unsafe {
                    let menu = CreatePopupMenu().unwrap_or_default();
                    let (open_label, quit_label) = (HSTRING::from(p2_i18n::t("appx.tray.open")), HSTRING::from(p2_i18n::t("app.quit")));
                    let _ = AppendMenuW(menu, MF_STRING, MENU_OPEN, PCWSTR(open_label.as_ptr()));
                    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                    let _ = AppendMenuW(menu, MF_STRING, MENU_QUIT, PCWSTR(quit_label.as_ptr()));
                    let mut pt = POINT::default();
                    let _ = GetCursorPos(&mut pt);
                    let _ = SetForegroundWindow(hwnd);
                    let picked = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, pt.x, pt.y, Some(0), hwnd, None);
                    let _ = DestroyMenu(menu);
                    match picked.0 as usize {
                        MENU_OPEN => {
                            let _ = state.tx.send(ShellEvent::Show);
                        }
                        MENU_QUIT => {
                            let _ = state.tx.send(ShellEvent::Quit);
                        }
                        _ => {}
                    }
                },
                _ => {}
            }
            LRESULT(0)
        }
        WM_NOTIFY_SHOW => {
            if let Some((title, body)) = state.pending.lock().unwrap().take() {
                let mut data = icon_data(hwnd);
                data.uFlags = NIF_INFO;
                wide_into(&mut data.szInfoTitle, &title);
                wide_into(&mut data.szInfo, &body);
                // Sesi uygulama kendisi çalar (Electron: silent: true).
                data.dwInfoFlags = NIIF_INFO | NIIF_NOSOUND;
                unsafe {
                    let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
                }
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            let data = icon_data(hwnd);
            unsafe {
                let _ = Shell_NotifyIconW(NIM_DELETE, &data);
                PostQuitMessage(0);
            }
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// Tepsi simgesini kendi iş parçacığında kurar (gizli mesaj penceresi). İkinci çağrı etkisiz.
pub fn start_tray(tx: Sender<ShellEvent>) -> bool {
    if TRAY.set(TrayState { tx, pending: std::sync::Mutex::new(None) }).is_err() {
        return true;
    }
    let (ok_tx, ok_rx) = channel::<bool>();
    std::thread::Builder::new()
        .name("p2-tepsi".into())
        .spawn(move || unsafe {
            let instance = GetModuleHandleW(None).unwrap_or_default();
            let class = w!("P2RustTray");
            let wc = WNDCLASSW { lpfnWndProc: Some(tray_proc), hInstance: instance.into(), lpszClassName: class, ..Default::default() };
            RegisterClassW(&wc);
            let Ok(hwnd) = CreateWindowExW(WINDOW_EX_STYLE(0), class, w!("P2 tepsi"), WINDOW_STYLE(0), 0, 0, 0, 0, Some(HWND_MESSAGE), None, Some(instance.into()), None) else {
                let _ = ok_tx.send(false);
                return;
            };
            let _ = TRAY_HWND.set(hwnd.0 as usize);
            let data = icon_data(hwnd);
            let added = Shell_NotifyIconW(NIM_ADD, &data).as_bool();
            let _ = ok_tx.send(added);
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        })
        .ok();
    ok_rx.recv_timeout(std::time::Duration::from_secs(3)).unwrap_or(false)
}

/// Otomatik pilot (testler) masaüstünde iz bırakmaz: balon ve yanıp sönme yok
/// (`P2_REAL_NOTIFY` ile açılır).
pub fn quiet() -> bool {
    (std::env::var_os("P2_SILENT_NOTIFY").is_some() || std::env::var_os("P2_AUTOPILOT_SERVER").is_some()) && std::env::var_os("P2_REAL_NOTIFY").is_none()
}

/// Masaüstü bildirimi (tepsi balonu; Windows 10/11'de bildirim olarak görünür).
/// `P2_SILENT_NOTIFY` (testler): masaüstünde gösterilmez, yalnız iz düşer.
pub fn notify(title: &str, body: &str) -> bool {
    if quiet() {
        return true;
    }
    let (Some(state), Some(hwnd)) = (TRAY.get(), TRAY_HWND.get()) else { return false };
    *state.pending.lock().unwrap() = Some((title.chars().take(63).collect(), body.chars().take(240).collect()));
    unsafe { PostMessageW(Some(HWND(*hwnd as _)), WM_NOTIFY_SHOW, WPARAM(0), LPARAM(0)).is_ok() }
}

pub fn remove_tray() {
    if let Some(hwnd) = TRAY_HWND.get() {
        unsafe {
            let _ = PostMessageW(Some(HWND(*hwnd as _)), WM_CLOSE, WPARAM(0), LPARAM(0));
            let _ = DestroyWindow(HWND(*hwnd as _));
        }
        let data = icon_data(HWND(*hwnd as _));
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &data);
        }
    }
}

// ---------------------------------------------------------------- ana pencere

pub fn hwnd_of(window: &gpui_kit::Window) -> Option<HWND> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(HWND(h.hwnd.get() as _)),
        _ => None,
    }
}

/// Saydam açılır pencere (başlatma kartı, çizim katmanı): Windows 11'in kendi kenarlığı, 8 px
/// köşe yuvarlaması, gölgesi ve açılış animasyonu kapatılır. Aksi halde kartın 18 px köşesinin
/// dışında ince bir çerçeve ve aradan masaüstü görünür.
pub fn plain_popup(hwnd: HWND) {
    use windows::Win32::Graphics::Dwm::*;
    let set = |attr: DWMWINDOWATTRIBUTE, v: u32| unsafe {
        let _ = DwmSetWindowAttribute(hwnd, attr, &v as *const u32 as *const _, 4);
    };
    set(DWMWA_TRANSITIONS_FORCEDISABLED, 1);
    set(DWMWA_NCRENDERING_POLICY, DWMNCRP_DISABLED.0 as u32);
    set(DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND.0 as u32);
    set(DWMWA_BORDER_COLOR, 0xFFFF_FFFE);
}

/// GPUI açılır pencereyi stil 0 (WS_OVERLAPPED) ile kurar; Windows buna sağ/sol/alt 8 px çerçeve
/// ekliyordu (kartın dışında kare çizgi, sağ altta düz köşe). Stil WS_POPUP yapılır ve pencere, önceki
/// istemci alanının tam yerine oturtulur. Pencere açılışının İÇİNDE çağrılmamalı (WM_SIZE/DPI iç içe).
pub fn strip_frame(hwnd: HWND) {
    unsafe {
        let mut client = windows::Win32::Foundation::RECT::default();
        let _ = GetClientRect(hwnd, &mut client);
        let mut origin = windows::Win32::Foundation::POINT { x: 0, y: 0 };
        let _ = windows::Win32::Graphics::Gdi::ClientToScreen(hwnd, &mut origin);
        let visible = IsWindowVisible(hwnd).as_bool();
        let style = WS_POPUP.0 | if visible { WS_VISIBLE.0 } else { 0 };
        SetWindowLongPtrW(hwnd, GWL_STYLE, style as isize);
        let _ = SetWindowPos(hwnd, None, origin.x, origin.y, client.right - client.left, client.bottom - client.top, SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
    }
}

pub fn hide(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
}

/// Pencereyi her zaman üstte tutar (açılış penceresi, Electron `alwaysOnTop`); etkinleştirmez.
pub fn set_topmost(hwnd: HWND, on: bool) {
    unsafe {
        let _ = SetWindowPos(hwnd, Some(if on { HWND_TOPMOST } else { HWND_NOTOPMOST }), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

pub fn show(hwnd: HWND) {
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        } else {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
        let _ = SetForegroundWindow(hwnd);
    }
}

pub fn is_visible(hwnd: HWND) -> bool {
    unsafe { IsWindowVisible(hwnd).as_bool() && !IsIconic(hwnd).as_bool() }
}

/// Görev çubuğunda yanıp söndür (pencere öndeyse etkisiz).
pub fn flash(hwnd: HWND, on: bool) {
    if quiet() {
        return;
    }
    use windows::Win32::UI::WindowsAndMessaging::{FLASHW_STOP, FLASHW_TIMERNOFG, FLASHW_TRAY, FLASHWINFO, FlashWindowEx};
    let info = FLASHWINFO { cbSize: size_of::<FLASHWINFO>() as u32, hwnd, dwFlags: if on { FLASHW_TRAY | FLASHW_TIMERNOFG } else { FLASHW_STOP }, uCount: 0, dwTimeout: 0 };
    unsafe {
        let _ = FlashWindowEx(&info);
    }
}

pub fn is_foreground(hwnd: HWND) -> bool {
    unsafe { GetForegroundWindow() == hwnd }
}

/// Kurulu sürümün AppUserModelID'si — Electron `app.setAppUserModelId('com.p2.desktop')` ve
/// kurulumun kısayollara yazdığı kimlik (`WinShell::SetLnkAUMI`). Aynı kimlik sayesinde görev
/// çubuğuna sabitlenmiş P2 simgesi Rust sürümünde de aynı düğmede açılır. Geliştirme derlemesi
/// Electron'daki gibi `.dev` ekini kullanır (kurulu sürümle aynı düğmede toplanmasın).
pub const APP_USER_MODEL_ID: &str = "com.p2.desktop";

fn app_user_model_id() -> String {
    let dev = std::env::current_exe().map(|e| e.to_string_lossy().to_lowercase().contains("p2-rust-target")).unwrap_or(true);
    if dev { format!("{APP_USER_MODEL_ID}.dev") } else { APP_USER_MODEL_ID.to_string() }
}

pub fn set_app_user_model_id() {
    let id = app_user_model_id();
    unsafe {
        let _ = windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(id.as_str()));
    }
}

/// main.cjs `pinWindowIcon` → `window.setAppDetails({appId, appIconPath, relaunchCommand,
/// relaunchDisplayName: 'P2'})`: pencerenin kendi özellik deposuna AppUserModelID ve yeniden
/// başlatma simgesi/komutu yazılır. Böylece görev çubuğu düğmesinin simgesi bu exe'den gelir;
/// aynı kimliği taşıyan başka (ör. hedefi silinmiş eski kurulumdan kalma, simge yolu boş) bir
/// Başlat menüsü kısayolu düğmeyi boş beyaz sayfa simgesine çeviremez. Simgenin kendisi
/// (WM_SETICON büyük/küçük) GPUI penceresi açılırken verilir (gpui-pre-windows P2 yaması).
pub fn apply_app_details(hwnd: HWND) {
    use windows::Win32::Foundation::PROPERTYKEY;
    use windows::Win32::System::Com::CoTaskMemAlloc;
    use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
    use windows::Win32::System::Variant::VT_LPWSTR;
    use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
    use windows::core::{GUID, PWSTR};
    // PKEY_AppUserModel_* ({9F4C2855-9F79-4B39-A8D0-E1D42DE1D5F3}): 2 RelaunchCommand,
    // 3 RelaunchIconResource, 4 RelaunchDisplayNameResource, 5 ID.
    const FMTID: GUID = GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3);
    let key = |pid: u32| PROPERTYKEY { fmtid: FMTID, pid };
    // VT_LPWSTR (kabuk VT_BSTR kabul etmiyor); dize CoTaskMem'de, PROPVARIANT düşünce PropVariantClear serbest bırakır.
    let lpwstr = |s: &str| -> Option<PROPVARIANT> {
        let wide: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            let buf = CoTaskMemAlloc(wide.len() * 2) as *mut u16;
            if buf.is_null() {
                return None;
            }
            std::ptr::copy_nonoverlapping(wide.as_ptr(), buf, wide.len());
            let mut v = PROPVARIANT::default();
            (*v.Anonymous.Anonymous).vt = VT_LPWSTR;
            (*v.Anonymous.Anonymous).Anonymous.pwszVal = PWSTR(buf);
            Some(v)
        }
    };
    let Ok(exe) = std::env::current_exe() else { return };
    let exe = exe.to_string_lossy().to_string();
    let values = [(2, format!("\"{exe}\"")), (3, format!("{exe},0")), (4, "P2".to_string()), (5, app_user_model_id())];
    unsafe {
        let Ok(store) = SHGetPropertyStoreForWindow::<IPropertyStore>(hwnd) else { return };
        for (pid, value) in values {
            if let Some(v) = lpwstr(&value) {
                let _ = store.SetValue(&key(pid), &v);
            }
        }
        let _ = store.Commit();
    }
}

/// `p2://` protokolünü bu exe'ye kaydeder (HKCU). Geliştirme derlemesinde (hedef klasörü)
/// kurulu Electron istemcisinin kaydını ezmemek için yapılmaz.
pub fn register_protocol() -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    let path = exe.to_string_lossy().to_string();
    if path.to_lowercase().contains("p2-rust-target") && std::env::var_os("P2_REGISTER_PROTOCOL").is_none() {
        return false;
    }
    use windows::Win32::System::Registry::{HKEY, HKEY_CURRENT_USER, REG_SZ, RegCloseKey, RegCreateKeyW, RegSetValueExW};
    let set = |key: &str, name: Option<&str>, value: &str| unsafe {
        let mut h = HKEY::default();
        if RegCreateKeyW(HKEY_CURRENT_USER, &HSTRING::from(key), &mut h).is_err() {
            return false;
        }
        let data: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
        let bytes = std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 2);
        let name = name.map(HSTRING::from);
        let ok = RegSetValueExW(h, name.as_ref().map(|n| PCWSTR(n.as_ptr())).unwrap_or(PCWSTR::null()), Some(0), REG_SZ, Some(bytes)).is_ok();
        let _ = RegCloseKey(h);
        ok
    };
    set(r"Software\Classes\p2", None, "URL:p2") && set(r"Software\Classes\p2", Some("URL Protocol"), "") && set(r"Software\Classes\p2\shell\open\command", None, &format!("\"{path}\" \"%1\""))
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
