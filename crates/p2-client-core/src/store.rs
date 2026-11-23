//! Yerel ayarlar ve oturum.
//!
//! Geçiş boyunca Rust istemcisi kendi profilini kullanır (`%APPDATA%\P2-Rust`), böylece
//! kurulu Electron istemcisinin `%APPDATA%\P2` profiline ve oturumuna dokunmaz. Faz 8'deki
//! taşıma köprüsü (`migrate.rs`) ilk açılışta Electron ayarlarını, oturumunu ve kütüphanesini buraya aktarır.
//!
//! Yenileme jetonu Windows DPAPI (CryptProtectData, kullanıcı kapsamı) ile şifrelenir —
//! Electron'un `safeStorage`'ıyla aynı güvence düzeyi.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallMode {
    #[default]
    Unset,
    Client,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub mode: InstallMode,
    pub api_url: String,
    pub language: Option<String>,
    /// "dark" (varsayılan), "light" ya da tema kimliği (`p2-mavi`, `gece`, … `custom`).
    pub theme: Option<String>,
    pub media: MediaSettings,
    pub app: AppSettings,
    pub library: LibraryPrefs,
    pub overlay: OverlayPrefs,
    pub availability: Availability,
    /// Ayarlar > Görünüm (theme.ts `Appearance`: özel tema, yazı tipi, yazı boyutu).
    pub appearance: AppearancePrefs,
    /// Ayarlar > Dil ve Bölge (settings.ts `autoTimeZone`, `timeZone`, `hour24`, `weekStart`).
    pub region: RegionPrefs,
}

/// theme.ts `ThemeSpec` (özel tema).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct CustomTheme {
    pub name: String,
    /// "dark" | "light"
    pub mode: String,
    pub accent: String,
    pub surface: String,
    pub tint: f32,
    pub contrast: f32,
    pub saturation: f32,
}

impl Default for CustomTheme {
    /// theme.ts `DEFAULT_CUSTOM`.
    fn default() -> Self {
        Self { name: "Özel tema".into(), mode: "dark".into(), accent: "#2c7bff".into(), surface: "#0b1120".into(), tint: 0.5, contrast: 1., saturation: 1. }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AppearancePrefs {
    pub custom: Option<CustomTheme>,
    /// theme.ts `FONTS` kimliği: inter | system | segoe | georgia | mono.
    pub font: String,
    /// Yazı boyutu yüzdesi (70–125, 5'er; varsayılan 85).
    pub font_scale: u32,
}

impl Default for AppearancePrefs {
    fn default() -> Self {
        Self { custom: None, font: "inter".into(), font_scale: 85 }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct RegionPrefs {
    pub auto_time_zone: bool,
    /// IANA adı (boş = sistem).
    pub time_zone: String,
    pub hour24: bool,
    /// "auto" | "monday" | "sunday"
    pub week_start: String,
}

impl Default for RegionPrefs {
    fn default() -> Self {
        Self { auto_time_zone: true, time_zone: String::new(), hour24: true, week_start: "auto".into() }
    }
}

/// Ayarlar > Kütüphane (settings.ts `library*`, `shareActivity`, `minimizeOnLaunch`…).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct LibraryPrefs {
    pub auto_download: bool,
    /// 0: sınırsız (MB/s).
    pub bandwidth_mbps: u64,
    pub share_activity: bool,
    pub minimize_on_launch: bool,
    pub announce_play: bool,
    pub play_toasts: bool,
    /// Oyun içi overlay enjekte edilsin.
    pub overlay: bool,
}

impl Default for LibraryPrefs {
    fn default() -> Self {
        Self { auto_download: true, bandwidth_mbps: 0, share_activity: true, minimize_on_launch: true, announce_play: true, play_toasts: true, overlay: true }
    }
}

/// "Shift+2" biçimi; boş = kapalı (hotkeys.ts).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct OverlayHotkeys {
    pub panel: String,
    pub screenshot: String,
    pub mute: String,
    pub deafen: String,
    pub push_to_talk: String,
}

impl Default for OverlayHotkeys {
    fn default() -> Self {
        Self { panel: "Shift+2".into(), screenshot: "F12".into(), mute: "Shift+M".into(), deafen: String::new(), push_to_talk: String::new() }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct OverlayNotifications {
    pub messages: bool,
    pub calls: bool,
    pub play: bool,
    pub tasks: bool,
    pub releases: bool,
    pub downloads: bool,
}

impl Default for OverlayNotifications {
    fn default() -> Self {
        Self { messages: true, calls: true, play: true, tasks: true, releases: true, downloads: true }
    }
}

/// Ayarlar > Kütüphane > Oyun içi overlay (settings.ts `overlay*`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct OverlayPrefs {
    pub hotkeys: OverlayHotkeys,
    /// top-right | top-left | bottom-right | bottom-left
    pub toast_corner: String,
    pub opacity: f32,
    pub scale: f32,
    pub reduce_motion: bool,
    pub hud: bool,
    /// top-left | top-center | top-right
    pub hud_position: String,
    pub hud_details: bool,
    pub screenshot_dir: String,
    pub notifications: OverlayNotifications,
}

impl Default for OverlayPrefs {
    fn default() -> Self {
        Self {
            hotkeys: OverlayHotkeys::default(),
            toast_corner: "top-right".into(),
            opacity: 0.92,
            scale: 1.0,
            reduce_motion: false,
            hud: false,
            hud_position: "top-left".into(),
            hud_details: true,
            screenshot_dir: String::new(),
            notifications: OverlayNotifications::default(),
        }
    }
}

/// Ayarlar > Uygunluk (settings.ts `workDays`, `workStart`, `workEnd`, `outsideHoursStatus`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Availability {
    /// 0 = Pazar … 6 = Cumartesi.
    pub work_days: Vec<u8>,
    pub work_start: String,
    pub work_end: String,
    /// none | AWAY | BUSY
    pub outside_hours_status: String,
}

impl Default for Availability {
    fn default() -> Self {
        Self { work_days: vec![1, 2, 3, 4, 5], work_start: "09:00".into(), work_end: "18:00".into(), outside_hours_status: "AWAY".into() }
    }
}

impl Availability {
    /// Verilen gün (0=Pazar) ve dakika (gece yarısından) çalışma saati içinde mi.
    pub fn working(&self, weekday: u8, minutes: u32) -> bool {
        let parse = |s: &str| s.split_once(':').and_then(|(h, m)| Some(h.parse::<u32>().ok()? * 60 + m.parse::<u32>().ok()?));
        let (Some(start), Some(end)) = (parse(&self.work_start), parse(&self.work_end)) else { return true };
        self.work_days.contains(&weekday) && if start <= end { minutes >= start && minutes < end } else { minutes >= start || minutes < end }
    }
}

/// Ayarlar > Uygulama ve Bildirimler (settings.ts `closeAction`, `desktopNotifications`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    /// Kapat düğmesi: "ask" | "tray" | "minimize" | "quit".
    pub close_action: String,
    /// Uygulama arka plandayken masaüstü bildirimi.
    pub desktop_notifications: bool,
    /// Bildirimde ses.
    pub notification_sound: bool,
    /// Görev ayrıntısı: "panel" (sağda) | "center" (ortada geniş).
    pub task_view: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self { close_action: "ask".into(), desktop_notifications: true, notification_sound: true, task_view: "panel".into() }
    }
}

/// settings.ts ses ayarları (aynı varsayılanlar).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct MediaSettings {
    /// Aygıt adı; `None` = Windows varsayılanı.
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    pub echo_cancellation: bool,
    pub noise_suppression: bool,
    pub auto_gain: bool,
    /// "open" (açık mikrofon) ya da "ptt" (bas-konuş).
    pub voice_mode: String,
    /// Bas-konuş tuşu (GPUI tuş adı: "space", "v", "f8"…).
    pub voice_key: String,
    /// Opus hedef bit hızı (kbps).
    pub voice_bitrate: u32,
    /// 0–100 (yüzde; 100 = olduğu gibi).
    pub input_volume: u32,
    pub output_volume: u32,
    pub ringtone_volume: u32,
    pub stun_url: String,
    pub turn_url: String,
    pub turn_username: String,
    pub turn_password: String,
    /// Yalnız TURN röle (settings.ts `forceRelay`).
    pub force_relay: bool,
    /// Kamera aygıtı (Media Foundation sembolik bağlantısı); `None` = ilk kamera.
    pub camera_device: Option<String>,
    /// Ekran paylaşımında bilgisayar sesi (settings.ts `shareAudio`, varsayılan açık).
    pub share_audio: bool,
    /// Son seçilen yayın kalitesi (`1080p`…), kare hızı ve izleyicilere açılan geçmiş (sn, 0 = kapalı).
    pub share_quality: String,
    pub share_fps: u32,
    pub share_rewind: u32,
    /// Kalem rengi (`penInk`; boş = kişiye özgü renk) ve kalınlığı (`penWidth`, 2–14).
    pub pen_ink: String,
    pub pen_width: u32,
    /// Kişi başına dinleme sesi (kullanıcı kimliği → 0–2; 1 = olduğu gibi). Aramalar arasında korunur.
    pub user_volumes: std::collections::HashMap<String, f32>,
}

impl Default for MediaSettings {
    fn default() -> Self {
        Self {
            input_device: None,
            output_device: None,
            echo_cancellation: true,
            noise_suppression: true,
            auto_gain: true,
            voice_mode: "open".into(),
            voice_key: "space".into(),
            voice_bitrate: 96,
            input_volume: 100,
            output_volume: 100,
            ringtone_volume: 70,
            stun_url: "stun:stun.l.google.com:19302".into(),
            turn_url: String::new(),
            turn_username: String::new(),
            turn_password: String::new(),
            force_relay: false,
            camera_device: None,
            share_audio: true,
            share_quality: "1080p".into(),
            share_fps: 30,
            share_rewind: 0,
            pen_ink: String::new(),
            pen_width: 4,
            user_volumes: std::collections::HashMap::new(),
        }
    }
}

pub fn profile_dir() -> PathBuf {
    let base = std::env::var_os("P2_PROFILE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::paths::config_dir().join("P2-Rust"));
    let _ = std::fs::create_dir_all(&base);
    base
}

pub fn load_settings() -> Settings {
    std::fs::read(profile_dir().join("settings.json"))
        .ok()
        // UTF-8 BOM'u atla: dosya Not Defteri/PowerShell ile düzenlenmiş olabilir
        // (Dilim 1 duman testinde BOM'lu dosya ayarları sessizce sıfırlıyordu).
        .and_then(|b| serde_json::from_slice(b.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&b)).ok())
        .unwrap_or_default()
}

pub fn save_settings(settings: &Settings) {
    if let Ok(json) = serde_json::to_vec_pretty(settings) {
        let _ = std::fs::write(profile_dir().join("settings.json"), json);
    }
}

/// Sürüklenebilir panel boyutları (Electron `useResizable` / `localStorage` anahtarlarıyla aynı:
/// `p2:sidebar-width`, `p2:thread-width`, `p2-documents-sidebar-width`, `p2.library.side`…).
/// `settings.json`'dan ayrı dosyada (`layout.json`) tutulur: ayarları bellekte tutup sonradan
/// kaydeden ekranlar sürükleme sonucunu ezmesin. Dosyada olmayan anahtar, Electron'dan taşınan
/// ham `localStorage`'tan (`electron-localstorage.json`) okunur.
pub fn load_layout() -> std::collections::HashMap<String, f32> {
    let dir = profile_dir();
    let read = |name: &str| -> serde_json::Map<String, serde_json::Value> {
        std::fs::read(dir.join(name))
            .ok()
            .and_then(|b| serde_json::from_slice::<serde_json::Value>(b.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&b)).ok())
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default()
    };
    let number = |v: &serde_json::Value| v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().parse::<f64>().ok())).map(|n| n as f32);
    let mut out = std::collections::HashMap::new();
    for (k, v) in read("electron-localstorage.json") {
        if let Some(n) = number(&v).filter(|n| n.is_finite()) {
            out.insert(k, n);
        }
    }
    for (k, v) in read("layout.json") {
        if let Some(n) = number(&v).filter(|n| n.is_finite()) {
            out.insert(k, n);
        }
    }
    out
}

/// `layout.json`'a tek anahtar yazar (diğer anahtarlar korunur).
pub fn save_layout_value(key: &str, value: f32) {
    let path = profile_dir().join("layout.json");
    let mut map = std::fs::read(&path)
        .ok()
        .and_then(|b| serde_json::from_slice::<serde_json::Map<String, serde_json::Value>>(&b).ok())
        .unwrap_or_default();
    map.insert(key.to_string(), serde_json::json!((value * 100.).round() / 100.));
    if let Ok(json) = serde_json::to_vec_pretty(&map) {
        let _ = std::fs::write(path, json);
    }
}

fn session_path() -> PathBuf {
    profile_dir().join("session.bin")
}

pub fn save_session(refresh_token: &str) {
    #[cfg(target_os = "macos")]
    {
        keychain::save(refresh_token);
        return;
    }
    #[allow(unreachable_code)]
    if let Some(sealed) = dpapi::protect(refresh_token.as_bytes()) {
        let _ = std::fs::write(session_path(), sealed);
    }
}

pub fn load_session() -> Option<String> {
    #[cfg(target_os = "macos")]
    return keychain::load();
    #[allow(unreachable_code)]
    let sealed = std::fs::read(session_path()).ok()?;
    String::from_utf8(dpapi::unprotect(&sealed)?).ok().filter(|t| !t.is_empty())
}

/// DPAPI (kullanıcı kapsamı) ile korur — oturum ve Electron anahtarı taşıması (migrate.rs).
pub fn protect(data: &[u8]) -> Option<Vec<u8>> {
    dpapi::protect(data)
}

pub fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
    dpapi::unprotect(data)
}

pub fn clear_session() {
    #[cfg(target_os = "macos")]
    keychain::clear();
    let _ = std::fs::remove_file(session_path());
}

/// macOS: oturum jetonu Anahtar Zinciri'nde (login keychain) tutulur; DPAPI karşılığı.
/// Hesap adı profil klasörüdür: test profilleri (`P2_PROFILE_DIR`) birbirinin oturumunu ezmez.
#[cfg(target_os = "macos")]
mod keychain {
    use security_framework::passwords::{delete_generic_password, get_generic_password, set_generic_password};
    const SERVICE: &str = "P2 oturum";

    fn account() -> String {
        super::profile_dir().to_string_lossy().into_owned()
    }
    pub fn save(token: &str) {
        let _ = set_generic_password(SERVICE, &account(), token.as_bytes());
    }
    pub fn load() -> Option<String> {
        get_generic_password(SERVICE, &account()).ok().and_then(|b| String::from_utf8(b).ok()).filter(|t| !t.is_empty())
    }
    pub fn clear() {
        let _ = delete_generic_password(SERVICE, &account());
    }
}

#[cfg(windows)]
mod dpapi {
    use windows::Win32::Foundation::{HLOCAL, LocalFree};
    use windows::Win32::Security::Cryptography::{CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData};

    fn run(input: &[u8], protect: bool) -> Option<Vec<u8>> {
        unsafe {
            let blob = CRYPT_INTEGER_BLOB { cbData: input.len() as u32, pbData: input.as_ptr() as *mut u8 };
            let mut out = CRYPT_INTEGER_BLOB::default();
            let ok = if protect {
                CryptProtectData(&blob, None, None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out)
            } else {
                CryptUnprotectData(&blob, None, None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out)
            };
            ok.ok()?;
            let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
            let _ = LocalFree(Some(HLOCAL(out.pbData as _)));
            Some(bytes)
        }
    }
    pub fn protect(data: &[u8]) -> Option<Vec<u8>> {
        run(data, true)
    }
    pub fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
        run(data, false)
    }
}

#[cfg(not(windows))]
mod dpapi {
    pub fn protect(data: &[u8]) -> Option<Vec<u8>> {
        Some(data.to_vec())
    }
    pub fn unprotect(data: &[u8]) -> Option<Vec<u8>> {
        Some(data.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_roundtrip_is_encrypted() {
        let dir = std::env::temp_dir().join(format!("p2-store-test-{}", std::process::id()));
        unsafe { std::env::set_var("P2_PROFILE_DIR", &dir) };
        save_session("gizli-jeton-123");
        let raw = std::fs::read(dir.join("session.bin")).unwrap();
        assert!(!raw.windows(5).any(|w| w == b"gizli"), "jeton düz metin yazılmamalı");
        assert_eq!(load_session().as_deref(), Some("gizli-jeton-123"));
        clear_session();
        assert!(load_session().is_none());
        let mut s = Settings::default();
        s.mode = InstallMode::Client;
        s.api_url = "http://127.0.0.1:4000".into();
        save_settings(&s);
        assert_eq!(load_settings().api_url, "http://127.0.0.1:4000");
        let _ = std::fs::remove_dir_all(dir);
    }
}
