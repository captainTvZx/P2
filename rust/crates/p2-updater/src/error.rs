//! Güncelleyici hataları. Mesajlar electron-updater'ınkilerle aynı kalıpta tutulur; arayüz
//! sessiz olmayan hataları olduğu gibi gösterir (Electron sürümü de İngilizce metni gösteriyordu).

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    /// Bağlantı kurulamadı, zaman aşımı, ad çözümlenemedi, bağlantı koptu (Electron'da `net::…`).
    Network,
    /// Sunucu 4xx/5xx döndü.
    HttpStatus(u16),
    /// latest.yml okunamadı/çözümlenemedi.
    InvalidUpdateInfo,
    /// Sürüm metni geçerli bir semver değil.
    InvalidVersion,
    /// latest.yml'de dosya yok.
    NoFiles,
    /// Dosya için ne sha512 ne sha2 var.
    NoChecksum,
    /// İndirilen dosyanın özeti tutmadı.
    ChecksumMismatch,
    /// Disk hatası.
    Io,
    /// Kurulum başlatılamadı ya da indirilmiş güncelleme yok.
    Install,
    /// İndirme iptal edildi (Electron gibi `error` olayı olarak yayılmaz).
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateError {
    pub kind: ErrorKind,
    pub message: String,
}

impl UpdateError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into() }
    }

    pub(crate) fn io(context: &str, error: std::io::Error) -> Self {
        Self::new(ErrorKind::Io, format!("{context}: {error}"))
    }

    /// Electron tarafındaki sessizlik kuralı: henüz sürüm yayınlanmamışsa (404) ya da ağ yoksa
    /// arayüzde hata gösterilmez, durum yalnızca `idle`a döner.
    /// (updater.cjs: /404|ENOTFOUND|ECONNREFUSED|ETIMEDOUT|EAI_AGAIN|net::/i,
    ///  server-updater.cjs: /404|ECONNREFUSED|ETIMEDOUT|net::/i — Electron'un `net` modülü tüm ağ
    ///  hatalarını `net::ERR_…` olarak verdiği için ikisi pratikte aynıdır.)
    pub fn is_quiet(&self) -> bool {
        match self.kind {
            ErrorKind::Network | ErrorKind::HttpStatus(404) => true,
            _ => quiet_message(&self.message),
        }
    }
}

/// Metin tabanlı kural (JS'teki düzenli ifadenin birebir karşılığı); hook'lardan gelen serbest
/// metinler için de kullanılabilir.
pub fn quiet_message(message: &str) -> bool {
    let upper = message.to_ascii_uppercase();
    ["404", "ENOTFOUND", "ECONNREFUSED", "ETIMEDOUT", "EAI_AGAIN", "NET::"].iter().any(|needle| upper.contains(needle))
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for UpdateError {}

/// reqwest hatasını Electron'un `net::ERR_…` biçimine çevirir; böylece metin tabanlı sessizlik
/// kuralı da tutar.
pub(crate) fn from_reqwest(url: &str, error: &reqwest::Error) -> UpdateError {
    if let Some(status) = error.status() {
        return UpdateError::new(ErrorKind::HttpStatus(status.as_u16()), format!("{} {url}", status.as_u16()));
    }
    let code = if error.is_timeout() {
        "net::ERR_TIMED_OUT (ETIMEDOUT)"
    } else if error.is_connect() {
        "net::ERR_CONNECTION_REFUSED (ECONNREFUSED)"
    } else {
        "net::ERR_FAILED"
    };
    UpdateError::new(ErrorKind::Network, format!("{code}: {url}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessiz_hatalar() {
        assert!(UpdateError::new(ErrorKind::Network, "x").is_quiet());
        assert!(UpdateError::new(ErrorKind::HttpStatus(404), "x").is_quiet());
        assert!(!UpdateError::new(ErrorKind::HttpStatus(500), "500 Internal").is_quiet());
        assert!(!UpdateError::new(ErrorKind::ChecksumMismatch, "sha512 checksum mismatch").is_quiet());
        assert!(quiet_message("connect ECONNREFUSED 127.0.0.1:4000"));
        assert!(quiet_message("net::ERR_INTERNET_DISCONNECTED"));
        assert!(!quiet_message("Sunucu durdurulamadı."));
    }
}
