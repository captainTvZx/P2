//! `server.log`: `[2026-11-13T10:00:00.123Z] ileti` satırları. Konsolun kendi satırları ve
//! (dosyaya yönlendirilen) API süreci çıktısı aynı dosyada toplanır.

use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use regex::Regex;

fn ansi() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\x1b\[[0-9;]*m").unwrap())
}

fn error_like() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)error|hata|failed|refused|authentication|does not exist").unwrap())
}

/// `writeLog` için temizlenmiş satır; boşsa `None`.
pub fn clean_line(value: &str) -> Option<String> {
    let clean = ansi().replace_all(value, "");
    let clean = clean.trim();
    (!clean.is_empty()).then(|| clean.to_string())
}

/// Satır hata gibi görünüyorsa (Electron'daki desen) `lastError` için ilk 600 karakter.
pub fn error_excerpt(clean: &str) -> Option<String> {
    error_like().is_match(clean).then(|| js_slice(clean, 600))
}

/// JS `String.prototype.slice(0, n)` (UTF-16 birimleri; vekil çifti bölünmez).
fn js_slice(value: &str, n: usize) -> String {
    let mut units = 0;
    let mut out = String::new();
    for ch in value.chars() {
        units += ch.len_utf16();
        if units > n {
            break;
        }
        out.push(ch);
    }
    out
}

pub struct Log {
    file: PathBuf,
    last_error: Mutex<String>,
    write_lock: Mutex<()>,
}

impl Log {
    pub fn new(file: PathBuf) -> Self {
        Self { file, last_error: Mutex::new(String::new()), write_lock: Mutex::new(()) }
    }

    pub fn path(&self) -> &Path {
        &self.file
    }

    /// `writeLog(value)`: boş satır yazılmaz; hata deseni `lastError`'u günceller. Yazıldıysa `true`.
    pub fn write(&self, value: &str) -> bool {
        let Some(clean) = clean_line(value) else { return false };
        let line = format!("[{}] {clean}\n", crate::iso_now());
        if let Some(excerpt) = error_excerpt(&clean) {
            *self.last_error.lock().unwrap() = excerpt;
        }
        let _guard = self.write_lock.lock().unwrap();
        if let Some(dir) = self.file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&self.file) {
            let _ = file.write_all(line.as_bytes());
        }
        true
    }

    pub fn last_error(&self) -> String {
        self.last_error.lock().unwrap().clone()
    }

    /// `recentLog()`: son 120 boş olmayan satır, `\n` ile. Büyük dosyada yalnız son 1 MB okunur.
    pub fn recent(&self) -> String {
        recent_lines(&self.file, 120)
    }
}

pub fn recent_lines(file: &Path, count: usize) -> String {
    const TAIL: u64 = 1024 * 1024;
    let Ok(mut handle) = std::fs::File::open(file) else { return String::new() };
    let len = handle.metadata().map(|m| m.len()).unwrap_or(0);
    let start = len.saturating_sub(TAIL);
    if handle.seek(SeekFrom::Start(start)).is_err() {
        return String::new();
    }
    let mut bytes = Vec::new();
    if handle.read_to_end(&mut bytes).is_err() {
        return String::new();
    }
    let text = String::from_utf8_lossy(&bytes);
    let mut lines: Vec<&str> = text.split('\n').collect();
    if start > 0 && !lines.is_empty() {
        lines.remove(0); // yarım kalan ilk satır
    }
    let lines: Vec<&str> = lines.into_iter().filter(|l| !l.is_empty()).collect();
    let from = lines.len().saturating_sub(count);
    lines[from..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_format_and_last_error() {
        let dir = std::env::temp_dir().join(format!("p2-ops-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let log = Log::new(dir.join("server.log"));
        assert!(!log.write("   \x1b[32m  \x1b[0m "));
        assert!(log.write("\x1b[32mP2 API başlatılıyor\x1b[0m"));
        assert_eq!(log.last_error(), "");
        assert!(log.write("Veritabanı HATASI oluştu"));
        assert_eq!(log.last_error(), "Veritabanı HATASI oluştu");
        log.write("password authentication failed for user \"x\"");
        let text = std::fs::read_to_string(dir.join("server.log")).unwrap();
        let first = text.lines().next().unwrap();
        let re = Regex::new(r"^\[\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z\] P2 API başlatılıyor$").unwrap();
        assert!(re.is_match(first), "{first}");
        assert_eq!(log.recent().lines().count(), 3);
        for i in 0..200 {
            log.write(&format!("satır {i}"));
        }
        let recent = log.recent();
        assert_eq!(recent.lines().count(), 120);
        assert!(recent.ends_with("satır 199"));
        let long = "error ".repeat(200);
        log.write(&long);
        assert_eq!(log.last_error().encode_utf16().count(), 600);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
