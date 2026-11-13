//! Sohbet bölmesinin ekleri: dosya ekleri (ayrıştırma, resim önizleme, indirme), emoji paneli,
//! @bahsetme önerileri (EmojiPanel.tsx, EmojiSuggestions.tsx, mentions.ts, attachments.tsx).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use gpui_kit::RenderImage;
use p2_client_core::UploadedFile;

/// Mesaj içeriğindeki dosya işareti (`[[p2-file|ad|tür|boyut|yol]]`).
#[derive(Clone, Debug, PartialEq)]
pub struct FileRef {
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub url: String,
}

impl FileRef {
    pub fn is_image(&self) -> bool {
        matches!(self.mime.as_str(), "image/png" | "image/jpeg" | "image/gif" | "image/webp" | "image/bmp")
    }


    /// Sunucuya göre yol (`/uploads/...`); eski mesajlarda tam adres olabilir.
    pub fn path(&self) -> String {
        match self.url.find("/uploads/") {
            Some(i) => self.url[i..].to_string(),
            None => self.url.clone(),
        }
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn marker(line: &str) -> Option<FileRef> {
    let inner = line.trim().strip_prefix("[[p2-file|")?.strip_suffix("]]")?;
    let mut parts = inner.splitn(4, '|');
    let (name, mime, size, url) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    if !(url.starts_with('/') || url.starts_with("http://") || url.starts_with("https://")) {
        return None;
    }
    Some(FileRef { name: percent_decode(name), mime: percent_decode(mime), size: size.parse().ok()?, url: url.to_string() })
}

/// İçeriği metin ve eklere ayırır (dosya işaretleri kendi satırındadır).
pub fn split_content(content: &str) -> (String, Vec<FileRef>) {
    let mut text = Vec::new();
    let mut files = Vec::new();
    for line in content.split('\n') {
        match marker(line) {
            Some(f) => files.push(f),
            None => text.push(line),
        }
    }
    (text.join("\n").trim_end().to_string(), files)
}

/// Gönderilecek içerik: metin + her ek kendi satırında.
pub fn compose_content(text: &str, files: &[UploadedFile]) -> String {
    let mut out = text.trim().to_string();
    for f in files {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&p2_client_core::attachment_marker(f));
    }
    out
}

pub enum Slot {
    Loading,
    Ready(Arc<RenderImage>, u32, u32),
    /// Metin önizlemesi (attachments `TextPreview`: ilk 100 000 karakter).
    Text(String),
    Failed,
}

/// attachments.tsx `PreviewKind`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Image,
    Svg,
    Audio,
    Video,
    Pdf,
    Text,
    Docx,
    Sheet,
    Slides,
    Archive,
    Epub,
    Font,
    Binary,
}

/// attachments.tsx `attachmentPreviewKind`.
pub fn preview_kind(name: &str, mime: &str) -> Kind {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    let t = mime.to_ascii_lowercase();
    let e = ext.as_str();
    if t == "image/svg+xml" || e == "svg" {
        Kind::Svg
    } else if t.starts_with("image/") {
        Kind::Image
    } else if t.starts_with("audio/") || ["mp3", "wav", "ogg", "oga", "opus", "m4a", "aac", "flac"].contains(&e) {
        Kind::Audio
    } else if t.starts_with("video/") || ["mp4", "m4v", "webm", "ogv", "mov"].contains(&e) {
        Kind::Video
    } else if t == "application/pdf" || e == "pdf" {
        Kind::Pdf
    } else if t.contains("wordprocessingml") || e == "docx" || e == "odt" {
        Kind::Docx
    } else if t.contains("spreadsheet") || t.contains("ms-excel") || ["xlsx", "xls", "ods"].contains(&e) {
        Kind::Sheet
    } else if t.contains("presentation") || ["pptx", "odp"].contains(&e) {
        Kind::Slides
    } else if t == "application/epub+zip" || e == "epub" {
        Kind::Epub
    } else if t.starts_with("font/") || ["ttf", "otf", "woff", "woff2"].contains(&e) {
        Kind::Font
    } else if ["zip", "rar", "7z", "tar", "gzip", "compressed"].iter().any(|k| t.contains(k)) || ["zip", "rar", "7z", "tar", "gz", "tgz", "bz2"].contains(&e) {
        Kind::Archive
    } else if t.starts_with("text/")
        || [
            "txt", "md", "log", "csv", "tsv", "json", "xml", "yaml", "yml", "js", "jsx", "ts", "tsx", "py", "rb", "rs", "go", "java", "kt", "cs", "cpp", "c", "h", "hpp", "php", "swift", "sql", "sh", "ps1", "bat", "ini", "toml", "conf", "css",
            "scss", "html", "vue", "svelte",
        ]
        .contains(&e)
    {
        Kind::Text
    } else {
        Kind::Binary
    }
}

/// attachments.tsx `formatSize`: `845 B`, `12.4 KB`, `3.1 MB`.
pub fn format_size(size: u64) -> String {
    if size < 1024 {
        format!("{size} B")
    } else if size < 1024 * 1024 {
        format!("{:.1} KB", size as f64 / 1024.)
    } else {
        format!("{:.1} MB", size as f64 / 1024. / 1024.)
    }
}

pub type ImageCache = HashMap<String, Slot>;

/// Kimlikli indirip çözer (GPUI'de uzak resim yükleyici yok); en çok 720 px.
pub fn decode_image(bytes: &[u8]) -> Option<(Arc<RenderImage>, u32, u32)> {
    let decoded = image::load_from_memory(bytes).ok()?;
    let decoded = if decoded.width() > 720 || decoded.height() > 720 { decoded.thumbnail(720, 720) } else { decoded };
    let mut bgra = decoded.to_rgba8();
    let (w, h) = (bgra.width(), bgra.height());
    for px in bgra.pixels_mut() {
        px.0.swap(0, 2);
    }
    Some((Arc::new(RenderImage::new(vec![image::Frame::new(bgra)])), w, h))
}

/// İndirilenler klasöründe çakışmayan dosya adı.
pub fn download_path(name: &str) -> PathBuf {
    let dir = std::env::var("P2_DOWNLOAD_DIR").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(std::env::var("USERPROFILE").unwrap_or_default()).join("Downloads"));
    let safe: String = name.chars().map(|c| if "\\/:*?\"<>|".contains(c) { '-' } else { c }).collect();
    let mut path = dir.join(&safe);
    let (stem, ext) = match safe.rsplit_once('.') {
        Some((s, e)) => (s.to_string(), format!(".{e}")),
        None => (safe.clone(), String::new()),
    };
    let mut n = 1;
    while path.exists() {
        path = dir.join(format!("{stem} ({n}){ext}"));
        n += 1;
    }
    path
}


/// Kurs sonundaki `@sorgu` (bahsetme önerisi için); boşluk ya da satır başından sonra gelmeli.
pub fn mention_query(text: &str) -> Option<String> {
    let at = text.rfind('@')?;
    let before = text[..at].chars().last();
    if before.is_some_and(|c| !c.is_whitespace()) {
        return None;
    }
    let query = &text[at + 1..];
    if query.chars().any(char::is_whitespace) || query.chars().count() > 24 {
        return None;
    }
    Some(query.to_string())
}

/// `@sorgu`yu `@kullanıcı ` ile değiştirir.
pub fn apply_mention(text: &str, username: &str) -> String {
    match text.rfind('@') {
        Some(at) => format!("{}@{username} ", &text[..at]),
        None => format!("{text}@{username} "),
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_and_compose() {
        let (text, files) = split_content("bak:\n[[p2-file|rapor%20son.pdf|application%2Fpdf|12700|/uploads/a.pdf]]");
        assert_eq!(text, "bak:");
        assert_eq!(files[0].name, "rapor son.pdf");
        let f = UploadedFile { name: "a b.png".into(), mime: "image/png".into(), size: 3, url: "/uploads/x.png".into() };
        assert_eq!(compose_content("selam", &[f]), "selam\n[[p2-file|a%20b.png|image%2Fpng|3|/uploads/x.png]]");
    }

    #[test]
    fn mentions() {
        assert_eq!(mention_query("selam @ca"), Some("ca".into()));
        assert_eq!(mention_query("@"), Some(String::new()));
        assert_eq!(mention_query("mail@x"), None);
        assert_eq!(mention_query("@cem nasılsın"), None);
        assert_eq!(apply_mention("selam @ca", "cansu"), "selam @cansu ");
    }
}
