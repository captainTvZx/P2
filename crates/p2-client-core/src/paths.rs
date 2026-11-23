//! Platforma göre kullanıcı klasörleri. Windows'ta Electron dönemindeki yerler (`%APPDATA%`,
//! `%LOCALAPPDATA%`, `%USERPROFILE%`) korunur; macOS'ta Apple'ın önerdiği yerler kullanılır:
//! - ayarlar/profil: `~/Library/Application Support`
//! - önbellek: `~/Library/Caches`
//! - indirilenler / masaüstü: `~/Downloads`, `~/Desktop`

use std::path::PathBuf;

fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// Kullanıcının ev klasörü.
pub fn home_dir() -> PathBuf {
    #[cfg(windows)]
    let home = env_dir("USERPROFILE");
    #[cfg(not(windows))]
    let home = env_dir("HOME");
    home.unwrap_or_else(std::env::temp_dir)
}

/// Dolaşan uygulama verisi: Windows `%APPDATA%`, macOS `~/Library/Application Support`,
/// diğerleri `$XDG_CONFIG_HOME` ya da `~/.config`.
pub fn config_dir() -> PathBuf {
    #[cfg(windows)]
    return env_dir("APPDATA").unwrap_or_else(|| home_dir().join("AppData").join("Roaming"));
    #[cfg(target_os = "macos")]
    return home_dir().join("Library").join("Application Support");
    #[cfg(not(any(windows, target_os = "macos")))]
    return env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home_dir().join(".config"));
}

/// Yerel önbellek: Windows `%LOCALAPPDATA%`, macOS `~/Library/Caches`, diğerleri `~/.cache`.
pub fn cache_dir() -> PathBuf {
    #[cfg(windows)]
    return env_dir("LOCALAPPDATA").unwrap_or_else(|| home_dir().join("AppData").join("Local"));
    #[cfg(target_os = "macos")]
    return home_dir().join("Library").join("Caches");
    #[cfg(not(any(windows, target_os = "macos")))]
    return env_dir("XDG_CACHE_HOME").unwrap_or_else(|| home_dir().join(".cache"));
}

/// İndirilenler klasörü.
pub fn downloads_dir() -> PathBuf {
    home_dir().join("Downloads")
}

/// Ortak uygulama verisi (sunucu kurulumu): Windows `%ProgramData%`, macOS `/Library/Application Support`.
pub fn shared_data_dir() -> PathBuf {
    #[cfg(windows)]
    return env_dir("ProgramData").unwrap_or_else(|| "C:\\ProgramData".into());
    #[cfg(target_os = "macos")]
    return PathBuf::from("/Library/Application Support");
    #[cfg(not(any(windows, target_os = "macos")))]
    return PathBuf::from("/var/lib");
}
