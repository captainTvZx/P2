//! Electron → Rust taşıma köprüsü (Faz 8).
//!
//! Rust istemcisi kendi profilini kullanır (`%APPDATA%\P2-Rust`); Electron istemcisinin profili
//! `%APPDATA%\P2` (main.cjs `app.setName('P2')`). İlk açılışta Rust profili boşsa (ayar, oturum ve
//! taşıma işareti yok) Electron profilinden şunlar alınır:
//!
//! - **Ayarlar:** son Electron sürümü (0.10.x köprü sürümü, `electron/migrate-bridge.cjs`)
//!   `localStorage`'ı `%APPDATA%\P2\migrate.json` dosyasına yazar. `p2:settings` (settings.ts),
//!   `p2:apiUrl`, `p2:mode` buradan [`Settings`]'e çevrilir; tema `appearance.json`'dan. Bütün
//!   `localStorage` ham hâliyle `electron-localstorage.json` olarak saklanır (pano görünümü,
//!   proje sırası, overlay düzenleri gibi henüz Rust'ta karşılığı olmayan tercihler için).
//!   `migrate.json` yoksa (köprü sürümü atlandıysa) eski `legacy-prefs.json` denenir.
//! - **Oturum:** Electron yenileme jetonunu `safeStorage` ile `session.bin`'e yazar (Chromium
//!   OSCrypt: `v10` + AES-256-GCM; anahtar `Local State` → `os_crypt.encrypted_key`, DPAPI ile
//!   korunur). Jeton burada çözülür ve Rust biçiminde (DPAPI) yeniden yazılır; düz metin hiçbir
//!   zaman diske inmez. Kullanıcı yeniden giriş yapmaz.
//! - **Kütüphane:** `library/library.json` (biçim aynı, `p2-patcher::library`), `hash-cache.json`
//!   ve yarım kalan yüklemeler (`uploads/`). Oyun dosyaları kütüphane kökünde durduğu için
//!   taşınmaz; kök yolu library.json'da olduğundan kurulu oyunlar olduğu gibi görünür.
//! - **Arama balonu konumu** (`call-widget.json`, aynı biçim).
//!
//! Electron profili değiştirilmez (geri dönüş mümkün kalsın). Taşıma bir kez yapılır:
//! `electron-import.json` işareti yazılır.
//!
//! Testler: `P2_PROFILE_DIR` verilmişse gerçek `%APPDATA%\P2` **asla** okunmaz; kaynak yalnızca
//! `P2_ELECTRON_PROFILE_DIR` ile verilebilir. `P2_NO_ELECTRON_IMPORT=1` taşımayı kapatır.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::store::{InstallMode, Settings};

/// Rust profilinde taşımanın yapıldığını gösteren işaret.
pub const MARKER: &str = "electron-import.json";

#[derive(Clone, Debug, Default)]
pub struct ImportReport {
    pub source: PathBuf,
    /// migrate.json (ya da legacy-prefs.json) bulundu ve ayarlar çevrildi.
    pub settings: bool,
    /// Ayarların kaynağı: "migrate.json" | "legacy-prefs.json" | "".
    pub settings_source: String,
    pub api_url: String,
    pub session: bool,
    pub library: bool,
    pub files: Vec<String>,
    pub notes: Vec<String>,
}

impl ImportReport {
    pub fn to_json(&self) -> Value {
        json!({
            "source": self.source.display().to_string(),
            "settings": self.settings,
            "settingsSource": self.settings_source,
            "apiUrl": self.api_url,
            "session": self.session,
            "library": self.library,
            "files": self.files,
            "notes": self.notes,
        })
    }
}

fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// Electron istemcisinin profil klasörü. `P2_ELECTRON_PROFILE_DIR` önceliklidir; ayrı bir Rust
/// profili (`P2_PROFILE_DIR`) kullanılıyorsa gerçek Electron profiline bakılmaz.
pub fn electron_profile_dir() -> Option<PathBuf> {
    if let Some(dir) = env_dir("P2_ELECTRON_PROFILE_DIR") {
        return Some(dir);
    }
    if env_dir("P2_PROFILE_DIR").is_some() {
        return None;
    }
    env_dir("APPDATA").map(|d| d.join("P2"))
}

/// Rust profili boş mu (ilk açılış).
pub fn needs_import(profile: &Path) -> bool {
    !profile.join("settings.json").exists() && !profile.join("session.bin").exists() && !profile.join(MARKER).exists()
}

/// Kaynakta taşınacak bir şey var mı.
pub fn has_electron_data(source: &Path) -> bool {
    ["migrate.json", "session.bin", "legacy-prefs.json", "library/library.json"].iter().any(|f| source.join(f).is_file())
}

/// İlk açılışta (profil boş ve Electron verisi varsa) taşır. Açılışta `load_settings()`'ten önce
/// çağrılır.
pub fn import_if_needed() -> Option<ImportReport> {
    if std::env::var_os("P2_NO_ELECTRON_IMPORT").is_some() {
        return None;
    }
    let source = electron_profile_dir()?;
    let target = crate::store::profile_dir();
    if !needs_import(&target) || !source.is_dir() || !has_electron_data(&source) {
        return None;
    }
    Some(import_from(&source, &target))
}

fn read_json(path: &Path) -> Option<Value> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes)).ok()
}

/// `localStorage` anahtar → değer (değerler her zaman metin).
fn storage_map(value: &Value) -> Map<String, Value> {
    value.as_object().map(|o| o.iter().filter(|(_, v)| v.is_string()).map(|(k, v)| (k.clone(), v.clone())).collect()).unwrap_or_default()
}

/// Taşımayı yapar. Hedefte var olan dosyaların üzerine yazılmaz.
pub fn import_from(source: &Path, target: &Path) -> ImportReport {
    let _ = std::fs::create_dir_all(target);
    let mut report = ImportReport { source: source.to_path_buf(), ..Default::default() };

    // ---- localStorage (migrate.json; yoksa eski legacy-prefs.json)
    let migrate = read_json(&source.join("migrate.json"));
    let (storage, from) = match &migrate {
        Some(doc) if doc.get("localStorage").is_some_and(Value::is_object) => (storage_map(&doc["localStorage"]), "migrate.json"),
        _ => match read_json(&source.join("legacy-prefs.json")) {
            Some(doc) if doc.is_object() => (storage_map(&doc), "legacy-prefs.json"),
            _ => (Map::new(), ""),
        },
    };
    if from == "legacy-prefs.json" {
        report.notes.push("migrate.json yok (0.10.x köprü sürümü çalışmamış); eski legacy-prefs.json kullanıldı".into());
    }
    let mut settings = Settings::default();
    if !from.is_empty() {
        report.settings = true;
        report.settings_source = from.into();
        apply_storage(&mut settings, &storage);
        let raw = target.join("electron-localstorage.json");
        if !raw.exists() && serde_json::to_vec_pretty(&Value::Object(storage.clone())).map(|b| std::fs::write(&raw, b)).is_ok_and(|r| r.is_ok()) {
            report.files.push("electron-localstorage.json".into());
        }
    }
    // Tema: ana sürecin yazdığı appearance.json (etkin temanın açık/koyu kipi).
    if let Some(mode) = read_json(&source.join("appearance.json")).and_then(|a| a["mode"].as_str().map(str::to_string)) {
        settings.theme = Some(if mode == "light" { "light" } else { "dark" }.into());
    } else if let Some(mode) = migrate.as_ref().and_then(|m| m["appearanceMode"].as_str()) {
        settings.theme = Some(if mode == "light" { "light" } else { "dark" }.into());
    }
    report.api_url = settings.api_url.clone();
    let settings_file = target.join("settings.json");
    if !settings_file.exists() && (report.settings || settings.theme.is_some()) {
        if let Ok(json) = serde_json::to_vec_pretty(&settings)
            && std::fs::write(&settings_file, json).is_ok()
        {
            report.files.push("settings.json".into());
        }
    }

    // ---- Oturum (safeStorage → DPAPI)
    let session_target = target.join("session.bin");
    if !session_target.exists() && source.join("session.bin").is_file() {
        match electron_session(source) {
            Ok(token) if !token.is_empty() => match crate::store::protect(token.as_bytes()) {
                Some(sealed) if std::fs::write(&session_target, &sealed).is_ok() => {
                    report.session = true;
                    report.files.push("session.bin".into());
                }
                _ => report.notes.push("oturum DPAPI ile yazılamadı".into()),
            },
            Ok(_) => report.notes.push("Electron oturumu boş".into()),
            Err(why) => report.notes.push(format!("Electron oturumu çözülemedi: {why}")),
        }
    }

    // ---- Kütüphane durumu (oyunlar kökte kalır)
    let lib_src = source.join("library");
    let lib_dst = target.join("library");
    if lib_src.join("library.json").is_file() {
        let _ = std::fs::create_dir_all(&lib_dst);
        for name in ["library.json", "hash-cache.json"] {
            if copy_new(&lib_src.join(name), &lib_dst.join(name)) {
                report.files.push(format!("library/{name}"));
                report.library |= name == "library.json";
            }
        }
        let uploads = lib_src.join("uploads");
        if uploads.is_dir() && !lib_dst.join("uploads").exists() {
            let copied = copy_tree(&uploads, &lib_dst.join("uploads"));
            if copied > 0 {
                report.files.push(format!("library/uploads ({copied} dosya)"));
            }
        }
    }
    if copy_new(&source.join("call-widget.json"), &target.join("call-widget.json")) {
        report.files.push("call-widget.json".into());
    }

    let marker = json!({
        "source": source.display().to_string(),
        "importedAt": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        "electronVersion": migrate.as_ref().and_then(|m| m["app"].as_str()).unwrap_or(""),
        "report": report.to_json(),
    });
    let _ = std::fs::write(target.join(MARKER), serde_json::to_vec_pretty(&marker).unwrap_or_default());
    report
}

fn copy_new(from: &Path, to: &Path) -> bool {
    from.is_file() && !to.exists() && std::fs::copy(from, to).is_ok()
}

fn copy_tree(from: &Path, to: &Path) -> usize {
    let mut count = 0;
    let _ = std::fs::create_dir_all(to);
    for entry in std::fs::read_dir(from).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            count += copy_tree(&path, &to.join(entry.file_name()));
        } else if copy_new(&path, &to.join(entry.file_name())) {
            count += 1;
        }
    }
    count
}

// ------------------------------------------------------------------ ayarlar

/// `localStorage` → [`Settings`]. Eksik alanlar Rust varsayılanında kalır (settings.ts ile aynı).
pub fn apply_storage(settings: &mut Settings, storage: &Map<String, Value>) {
    let text = |key: &str| storage.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    if let Some(url) = text("p2:apiUrl") {
        settings.api_url = url.trim_end_matches('/').to_string();
    }
    // Electron'da "host" (eski birleşik kurulum) ya da "client"; Rust istemcisi yalnız istemci
    // kipidir. Adres biliniyorsa doğrudan bağlanılır, kurulum türü ekranı atlanır.
    if text("p2:mode").is_some() && !settings.api_url.is_empty() {
        settings.mode = InstallMode::Client;
    }
    if let Some(raw) = text("p2:settings")
        && let Ok(Value::Object(e)) = serde_json::from_str::<Value>(&raw)
    {
        apply_electron_settings(settings, &e);
    }
}

/// settings.ts `Settings` nesnesi → Rust ayarları.
pub fn apply_electron_settings(s: &mut Settings, e: &Map<String, Value>) {
    let b = |k: &str| e.get(k).and_then(Value::as_bool);
    let n = |k: &str| e.get(k).and_then(Value::as_f64);
    let t = |k: &str| e.get(k).and_then(Value::as_str).map(str::to_string);
    let pct = |k: &str| n(k).map(|v| v.round().clamp(0.0, 100.0) as u32);

    if let Some(lang) = t("language").filter(|l| !l.is_empty()) {
        s.language = Some(lang);
    }
    // Uygulama ve bildirimler
    if let Some(v) = t("closeAction").filter(|v| ["ask", "tray", "minimize", "quit"].contains(&v.as_str())) {
        s.app.close_action = v;
    }
    if let Some(v) = b("desktopNotifications") {
        s.app.desktop_notifications = v;
    }
    if let Some(v) = b("notificationSound") {
        s.app.notification_sound = v;
    }
    // Ses ve görüntü. Aygıt kimlikleri Chromium'a özgü (karma) olduğu için taşınmaz: Windows
    // varsayılanı kullanılır.
    let m = &mut s.media;
    if let Some(v) = b("echoCancellation") {
        m.echo_cancellation = v;
    }
    if let Some(v) = b("noiseSuppression") {
        m.noise_suppression = v;
    }
    if let Some(v) = b("autoGainControl") {
        m.auto_gain = v;
    }
    if let Some(v) = t("voiceMode") {
        // "ptm" (bas-sustur) Rust'ta yok: açık mikrofon.
        m.voice_mode = if v == "ptt" { "ptt".into() } else { "open".into() };
    }
    if let Some(v) = t("voiceKey").filter(|v| !v.is_empty()) {
        m.voice_key = voice_key(&v);
    }
    if let Some(v) = n("voiceBitrate") {
        m.voice_bitrate = v.round().clamp(8.0, 510.0) as u32;
    }
    if let Some(v) = pct("inputVolume") {
        m.input_volume = v;
    }
    if let Some(v) = pct("outputVolume") {
        m.output_volume = v;
    }
    if let Some(v) = pct("ringtoneVolume") {
        m.ringtone_volume = v;
    }
    if let Some(v) = t("stunUrl") {
        m.stun_url = v;
    }
    if let Some(v) = t("turnUrl") {
        m.turn_url = v;
    }
    if let Some(v) = t("turnUsername") {
        m.turn_username = v;
    }
    if let Some(v) = t("turnPassword") {
        m.turn_password = v;
    }
    if let Some(v) = b("forceRelay") {
        m.force_relay = v;
    }
    if let Some(v) = b("shareAudio") {
        m.share_audio = v;
    }
    if let Some(v) = t("penInk") {
        m.pen_ink = v;
    }
    if let Some(v) = n("penWidth") {
        m.pen_width = v.round().clamp(2.0, 14.0) as u32;
    }
    // Kütüphane
    let l = &mut s.library;
    if let Some(v) = b("libraryAutoDownload") {
        l.auto_download = v;
    }
    if let Some(v) = n("libraryBandwidthMbps") {
        l.bandwidth_mbps = v.max(0.0).round() as u64;
    }
    if let Some(v) = b("shareActivity") {
        l.share_activity = v;
    }
    if let Some(v) = b("minimizeOnLaunch") {
        l.minimize_on_launch = v;
    }
    if let Some(v) = b("announcePlay") {
        l.announce_play = v;
    }
    if let Some(v) = b("playToasts") {
        l.play_toasts = v;
    }
    if let Some(v) = b("libraryOverlay") {
        l.overlay = v;
    }
    // Oyun içi overlay
    let o = &mut s.overlay;
    if let Some(Value::Object(h)) = e.get("overlayHotkeys") {
        let hk = |k: &str| h.get(k).and_then(Value::as_str).map(str::to_string);
        if let Some(v) = hk("panel") {
            o.hotkeys.panel = v;
        }
        if let Some(v) = hk("screenshot") {
            o.hotkeys.screenshot = v;
        }
        if let Some(v) = hk("mute") {
            o.hotkeys.mute = v;
        }
        if let Some(v) = hk("deafen") {
            o.hotkeys.deafen = v;
        }
        if let Some(v) = hk("pushToTalk") {
            o.hotkeys.push_to_talk = v;
        }
    }
    if let Some(v) = t("overlayToastCorner").filter(|v| ["top-right", "top-left", "bottom-right", "bottom-left"].contains(&v.as_str())) {
        o.toast_corner = v;
    }
    if let Some(v) = n("overlayOpacity") {
        o.opacity = (v as f32).clamp(0.35, 1.0);
    }
    if let Some(v) = n("overlayScale") {
        o.scale = (v as f32).clamp(0.8, 1.5);
    }
    if let Some(v) = b("overlayReduceMotion") {
        o.reduce_motion = v;
    }
    if let Some(v) = b("overlayHud") {
        o.hud = v;
    }
    if let Some(v) = t("overlayHudPosition").filter(|v| ["top-left", "top-center", "top-right"].contains(&v.as_str())) {
        o.hud_position = v;
    }
    if let Some(v) = b("overlayHudDetails") {
        o.hud_details = v;
    }
    if let Some(v) = t("overlayScreenshotDir") {
        o.screenshot_dir = v;
    }
    if let Some(Value::Object(nt)) = e.get("overlayNotifications") {
        let f = |k: &str, d: bool| nt.get(k).and_then(Value::as_bool).unwrap_or(d);
        let d = o.notifications.clone();
        o.notifications.messages = f("messages", d.messages);
        o.notifications.calls = f("calls", d.calls);
        o.notifications.play = f("play", d.play);
        o.notifications.tasks = f("tasks", d.tasks);
        o.notifications.releases = f("releases", d.releases);
        o.notifications.downloads = f("downloads", d.downloads);
    }
    // Uygunluk
    let a = &mut s.availability;
    if let Some(Value::Array(days)) = e.get("workDays") {
        let list: Vec<u8> = days.iter().filter_map(Value::as_u64).filter(|d| *d <= 6).map(|d| d as u8).collect();
        a.work_days = list;
    }
    if let Some(v) = t("workStart").filter(|v| v.contains(':')) {
        a.work_start = v;
    }
    if let Some(v) = t("workEnd").filter(|v| v.contains(':')) {
        a.work_end = v;
    }
    if let Some(v) = t("outsideHoursStatus").filter(|v| ["none", "AWAY", "BUSY"].contains(&v.as_str())) {
        a.outside_hours_status = v;
    }
}

/// Bas-konuş tuşu: Electron `KeyboardEvent.code` ("Space", "KeyV", "Digit1", "F8") → GPUI tuş adı
/// ("space", "v", "1", "f8").
pub fn voice_key(code: &str) -> String {
    let lower = code.to_lowercase();
    if let Some(rest) = code.strip_prefix("Key").filter(|r| r.len() == 1) {
        return rest.to_lowercase();
    }
    if let Some(rest) = code.strip_prefix("Digit").filter(|r| r.len() == 1) {
        return rest.into();
    }
    if let Some(rest) = code.strip_prefix("Numpad").filter(|r| r.len() == 1 && r.chars().all(|c| c.is_ascii_digit())) {
        return rest.into();
    }
    if let Some(rest) = code.strip_prefix("Arrow") {
        return rest.to_lowercase();
    }
    match code {
        "Backquote" => "`".into(),
        "Escape" => "escape".into(),
        "Enter" => "enter".into(),
        "Tab" => "tab".into(),
        "ControlLeft" | "ControlRight" => "control".into(),
        "ShiftLeft" | "ShiftRight" => "shift".into(),
        "AltLeft" | "AltRight" => "alt".into(),
        _ => lower,
    }
}

// ------------------------------------------------------------------ Electron safeStorage

/// Electron `safeStorage.encryptString` ile yazılmış `session.bin`'i çözer.
pub fn electron_session(source: &Path) -> Result<String, String> {
    let blob = std::fs::read(source.join("session.bin")).map_err(|e| format!("session.bin okunamadı: {e}"))?;
    let key = os_crypt_key(source)?;
    let plain = decrypt_v10(&key, &blob)?;
    String::from_utf8(plain).map_err(|_| "jeton UTF-8 değil".to_string())
}

/// `Local State` → `os_crypt.encrypted_key` (base64, "DPAPI" önekli) → DPAPI ile çözülmüş
/// 32 baytlık AES anahtarı.
fn os_crypt_key(source: &Path) -> Result<Vec<u8>, String> {
    let state = read_json(&source.join("Local State")).ok_or("Local State okunamadı")?;
    let encoded = state["os_crypt"]["encrypted_key"].as_str().ok_or("os_crypt.encrypted_key yok")?;
    let raw = base64_decode(encoded).ok_or("encrypted_key base64 değil")?;
    let sealed = raw.strip_prefix(b"DPAPI").ok_or("encrypted_key DPAPI önekli değil")?;
    let key = crate::store::unprotect(sealed).ok_or("anahtar DPAPI ile çözülemedi (başka kullanıcı/bilgisayar)")?;
    if key.len() != 32 {
        return Err(format!("anahtar uzunluğu {} (32 beklenir)", key.len()));
    }
    Ok(key)
}

/// Chromium OSCrypt `v10`: "v10" + 12 bayt nonce + şifreli metin + 16 bayt etiket (AES-256-GCM).
fn decrypt_v10(key: &[u8], blob: &[u8]) -> Result<Vec<u8>, String> {
    let body = blob.strip_prefix(b"v10").ok_or_else(|| {
        if blob.starts_with(b"v20") { "v20 (uygulamaya bağlı şifreleme) desteklenmiyor".to_string() } else { "v10 öneki yok".to_string() }
    })?;
    if body.len() < 12 + 16 {
        return Err("şifreli oturum çok kısa".into());
    }
    let (nonce, rest) = body.split_at(12);
    let (cipher, tag) = rest.split_at(rest.len() - 16);
    aes_gcm::decrypt(key, nonce, cipher, tag).ok_or_else(|| "AES-GCM doğrulaması başarısız".to_string())
}

fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut acc = 0u32;
    let mut bits = 0;
    for c in text.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b'\r' | b'\n' | b' ' => continue,
            _ => return None,
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(windows)]
mod aes_gcm {
    //! Windows CNG (BCrypt) ile AES-256-GCM çözme — ek bağımlılık gerekmez.
    use windows::Win32::Security::Cryptography::{
        BCRYPT_AES_ALGORITHM, BCRYPT_ALG_HANDLE, BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO, BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO_VERSION, BCRYPT_CHAIN_MODE_GCM, BCRYPT_CHAINING_MODE, BCRYPT_FLAGS, BCRYPT_KEY_HANDLE,
        BCRYPT_OPEN_ALGORITHM_PROVIDER_FLAGS, BCryptCloseAlgorithmProvider, BCryptDecrypt, BCryptDestroyKey, BCryptGenerateSymmetricKey, BCryptOpenAlgorithmProvider, BCryptSetProperty,
    };

    pub fn decrypt(key: &[u8], nonce: &[u8], cipher: &[u8], tag: &[u8]) -> Option<Vec<u8>> {
        unsafe {
            let mut alg = BCRYPT_ALG_HANDLE::default();
            BCryptOpenAlgorithmProvider(&mut alg, BCRYPT_AES_ALGORITHM, None, BCRYPT_OPEN_ALGORITHM_PROVIDER_FLAGS(0)).ok().ok()?;
            let result = (|| {
                let mode: Vec<u16> = BCRYPT_CHAIN_MODE_GCM.as_wide().iter().copied().chain(std::iter::once(0)).collect();
                let mode_bytes = std::slice::from_raw_parts(mode.as_ptr() as *const u8, mode.len() * 2);
                BCryptSetProperty(alg.into(), BCRYPT_CHAINING_MODE, mode_bytes, 0).ok().ok()?;
                let mut handle = BCRYPT_KEY_HANDLE::default();
                BCryptGenerateSymmetricKey(alg, &mut handle, None, key, 0).ok().ok()?;
                let mut info = BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO {
                    cbSize: std::mem::size_of::<BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO>() as u32,
                    dwInfoVersion: BCRYPT_AUTHENTICATED_CIPHER_MODE_INFO_VERSION,
                    pbNonce: nonce.as_ptr() as *mut u8,
                    cbNonce: nonce.len() as u32,
                    pbTag: tag.as_ptr() as *mut u8,
                    cbTag: tag.len() as u32,
                    ..Default::default()
                };
                let mut out = vec![0u8; cipher.len()];
                let mut written = 0u32;
                let status = BCryptDecrypt(handle, Some(cipher), Some(&mut info as *mut _ as *const _), None, Some(&mut out), &mut written, BCRYPT_FLAGS(0));
                let _ = BCryptDestroyKey(handle);
                status.ok().ok()?;
                out.truncate(written as usize);
                Some(out)
            })();
            let _ = BCryptCloseAlgorithmProvider(alg, 0);
            result
        }
    }
}

#[cfg(not(windows))]
mod aes_gcm {
    pub fn decrypt(_key: &[u8], _nonce: &[u8], _cipher: &[u8], _tag: &[u8]) -> Option<Vec<u8>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("p2-migrate-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn maps_electron_settings() {
        let mut s = Settings::default();
        let electron = json!({
            "language": "en", "closeAction": "tray", "desktopNotifications": false, "notificationSound": false,
            "echoCancellation": false, "voiceMode": "ptt", "voiceKey": "KeyV", "voiceBitrate": 64, "inputVolume": 80,
            "outputVolume": 150, "ringtoneVolume": 40, "turnUrl": "turn:x:3478", "forceRelay": true, "shareAudio": false,
            "penInk": "#ff0000", "penWidth": 30, "libraryAutoDownload": false, "libraryBandwidthMbps": 5,
            "shareActivity": false, "minimizeOnLaunch": false, "announcePlay": false, "playToasts": false, "libraryOverlay": false,
            "overlayHotkeys": {"panel": "Shift+3", "screenshot": "", "mute": "Shift+M", "deafen": "Shift+D", "pushToTalk": "F9"},
            "overlayToastCorner": "bottom-left", "overlayOpacity": 0.6, "overlayScale": 1.25, "overlayReduceMotion": true,
            "overlayHud": true, "overlayHudPosition": "top-right", "overlayHudDetails": false, "overlayScreenshotDir": "D:\\Shots",
            "overlayNotifications": {"messages": false, "calls": true, "play": false},
            "workDays": [0, 6], "workStart": "10:00", "workEnd": "02:00", "outsideHoursStatus": "BUSY",
            "inputDeviceId": "8f1c…", "appearance": {"themeId": "p2-mavi"}
        });
        let storage: Map<String, Value> = [
            ("p2:settings".to_string(), Value::String(electron.to_string())),
            ("p2:apiUrl".to_string(), Value::String("http://10.0.0.5:4000/".into())),
            ("p2:mode".to_string(), Value::String("client".into())),
        ]
        .into_iter()
        .collect();
        apply_storage(&mut s, &storage);
        assert_eq!(s.api_url, "http://10.0.0.5:4000");
        assert_eq!(s.mode, InstallMode::Client);
        assert_eq!(s.language.as_deref(), Some("en"));
        assert_eq!(s.app.close_action, "tray");
        assert!(!s.app.desktop_notifications && !s.app.notification_sound);
        assert!(!s.media.echo_cancellation && s.media.noise_suppression);
        assert_eq!((s.media.voice_mode.as_str(), s.media.voice_key.as_str(), s.media.voice_bitrate), ("ptt", "v", 64));
        assert_eq!((s.media.input_volume, s.media.output_volume, s.media.ringtone_volume), (80, 100, 40));
        assert!(s.media.force_relay && !s.media.share_audio);
        assert_eq!(s.media.pen_width, 14);
        assert_eq!(s.media.input_device, None);
        assert!(!s.library.auto_download && !s.library.overlay && !s.library.play_toasts);
        assert_eq!(s.library.bandwidth_mbps, 5);
        assert_eq!(s.overlay.hotkeys.panel, "Shift+3");
        assert_eq!(s.overlay.hotkeys.screenshot, "");
        assert_eq!(s.overlay.hotkeys.push_to_talk, "F9");
        assert_eq!(s.overlay.toast_corner, "bottom-left");
        assert!((s.overlay.opacity - 0.6).abs() < 1e-6 && (s.overlay.scale - 1.25).abs() < 1e-6);
        assert!(s.overlay.reduce_motion && s.overlay.hud && !s.overlay.hud_details);
        assert_eq!(s.overlay.hud_position, "top-right");
        assert_eq!(s.overlay.screenshot_dir, "D:\\Shots");
        assert!(!s.overlay.notifications.messages && s.overlay.notifications.calls && !s.overlay.notifications.play && s.overlay.notifications.tasks);
        assert_eq!(s.availability.work_days, vec![0, 6]);
        assert_eq!((s.availability.work_start.as_str(), s.availability.work_end.as_str()), ("10:00", "02:00"));
        assert_eq!(s.availability.outside_hours_status, "BUSY");
    }

    #[test]
    fn voice_keys() {
        assert_eq!(voice_key("Space"), "space");
        assert_eq!(voice_key("KeyB"), "b");
        assert_eq!(voice_key("Digit7"), "7");
        assert_eq!(voice_key("F8"), "f8");
        assert_eq!(voice_key("ArrowUp"), "up");
    }

    #[test]
    fn base64() {
        assert_eq!(base64_decode("RFBBUEk=").unwrap(), b"DPAPI");
        assert_eq!(base64_decode("YWJj").unwrap(), b"abc");
    }

    /// Sentetik Electron profili: Local State (DPAPI korumalı anahtar) + v10 session.bin. Şifreleme
    /// burada BCrypt ile değil, bağımsız bir referansla doğrulanamayacağı için uçtan uca test
    /// `rust/tools/migrate-test.mjs`'te gerçek Electron `safeStorage` çıktısıyla yapılır; bu test
    /// dosya taşıma kurallarını sınar.
    #[test]
    fn imports_files_once_without_overwriting() {
        let src = temp("src");
        let dst = temp("dst");
        std::fs::create_dir_all(src.join("library/uploads/job1")).unwrap();
        std::fs::write(src.join("library/library.json"), r#"{"version":1,"deviceId":"d1","root":"D:\\P2ProjectLibrary","projects":{}}"#).unwrap();
        std::fs::write(src.join("library/hash-cache.json"), "{}").unwrap();
        std::fs::write(src.join("library/uploads/job1/state.json"), "{}").unwrap();
        std::fs::write(src.join("call-widget.json"), r#"{"x":10,"y":20}"#).unwrap();
        std::fs::write(src.join("appearance.json"), r#"{"vars":{},"mode":"light"}"#).unwrap();
        let settings = json!({"language": "de", "closeAction": "quit"}).to_string();
        std::fs::write(src.join("migrate.json"), serde_json::to_vec(&json!({"format": "p2-migrate", "version": 1, "app": "0.10.0", "localStorage": {"p2:settings": settings, "p2:apiUrl": "http://h:4000", "p2:mode": "client", "p2:board-view": "list"}})).unwrap()).unwrap();
        assert!(needs_import(&dst) && has_electron_data(&src));
        let r = import_from(&src, &dst);
        assert!(r.settings && r.library && !r.session);
        let saved: Settings = serde_json::from_slice(&std::fs::read(dst.join("settings.json")).unwrap()).unwrap();
        assert_eq!(saved.language.as_deref(), Some("de"));
        assert_eq!(saved.theme.as_deref(), Some("light"));
        assert_eq!(saved.api_url, "http://h:4000");
        assert_eq!(saved.mode, InstallMode::Client);
        assert_eq!(saved.app.close_action, "quit");
        assert!(dst.join("library/library.json").is_file() && dst.join("library/uploads/job1/state.json").is_file());
        assert!(dst.join("call-widget.json").is_file() && dst.join("electron-localstorage.json").is_file());
        assert!(!needs_import(&dst), "ikinci açılışta taşıma tekrarlanmaz");
        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&dst);
    }

    #[test]
    fn rejects_bad_session_blob() {
        let src = temp("bad");
        std::fs::write(src.join("session.bin"), b"v10short").unwrap();
        std::fs::write(src.join("Local State"), r#"{"os_crypt":{"encrypted_key":"AAAA"}}"#).unwrap();
        assert!(electron_session(&src).is_err());
        let _ = std::fs::remove_dir_all(&src);
    }
}
