//! electron-builder'ın ürettiği `latest.yml` dosyası. Biçim sabittir (js-yaml dump çıktısı):
//!
//! ```yaml
//! version: 0.9.9
//! files:
//!   - url: P2-Server-Setup-0.9.9.exe
//!     sha512: fqBY…==
//!     size: 187314464
//! path: P2-Server-Setup-0.9.9.exe
//! sha512: fqBY…==
//! releaseDate: '2026-11-03T21:19:48.580Z'
//! ```
//!
//! Tam bir YAML ayrıştırıcısı yerine bu alt küme okunur: üst düzey skalerler, `files` listesi
//! (blok biçimi), tek/çift tırnaklı ve düz skalerler. Bilinmeyen anahtarlar ve iç içe bloklar
//! (`releaseNotes: |`, `packages:` …) atlanır.

use serde::Serialize;

use crate::error::{ErrorKind, UpdateError};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub url: String,
    pub sha512: Option<String>,
    pub sha2: Option<String>,
    pub size: Option<u64>,
    pub block_map_size: Option<u64>,
    pub is_admin_rights_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub files: Vec<FileInfo>,
    /// Eski biçim: tek dosya (files boşsa kullanılır).
    pub path: Option<String>,
    pub sha512: Option<String>,
    pub sha2: Option<String>,
    pub release_date: Option<String>,
    pub release_name: Option<String>,
    /// Kademeli yayın yüzdesi (ham metin; electron-updater gibi parseInt ile okunur).
    pub staging_percentage: Option<String>,
    pub minimum_system_version: Option<String>,
}

impl UpdateInfo {
    /// electron-updater `getFileList`: files doluysa o, değilse `path`+`sha512`'den tek dosya.
    pub fn file_list(&self) -> Result<Vec<FileInfo>, UpdateError> {
        if !self.files.is_empty() {
            return Ok(self.files.clone());
        }
        if let Some(path) = &self.path {
            return Ok(vec![FileInfo { url: path.clone(), sha512: self.sha512.clone(), sha2: self.sha2.clone(), ..Default::default() }]);
        }
        Err(UpdateError::new(ErrorKind::NoFiles, format!("No files provided: {{\"version\":\"{}\"}}", self.version)))
    }
}

/// `latest.yml` metnini çözer.
pub fn parse_update_info(text: &str) -> Result<UpdateInfo, UpdateError> {
    let text = text.trim_start_matches('\u{feff}');
    let mut info = UpdateInfo::default();
    let mut section: Option<String> = None;
    let mut current: Option<FileInfo> = None;
    let mut saw_key = false;

    for raw in text.lines() {
        let line = raw.trim_end();
        let content = line.trim_start();
        if content.is_empty() || content.starts_with('#') || content == "---" {
            continue;
        }
        let indent = line.len() - content.len();
        if indent == 0 {
            if let Some(file) = current.take() {
                info.files.push(file);
            }
            let Some((key, value)) = split_key(content) else {
                return Err(invalid(&format!("geçersiz satır: {content}")));
            };
            saw_key = true;
            section = None;
            match value {
                None => section = Some(key.to_string()),
                Some(v) if v.starts_with('|') || v.starts_with('>') => section = Some(key.to_string()),
                Some(v) => set_top(&mut info, key, &scalar(v)),
            }
            continue;
        }
        if section.as_deref() != Some("files") {
            continue;
        }
        let mut item = content;
        if content == "-" || content.starts_with("- ") {
            if let Some(file) = current.take() {
                info.files.push(file);
            }
            current = Some(FileInfo::default());
            item = content[1..].trim_start();
            if item.is_empty() {
                continue;
            }
        }
        if let (Some(file), Some((key, Some(value)))) = (current.as_mut(), split_key(item)) {
            set_file(file, key, &scalar(value));
        }
    }
    if let Some(file) = current.take() {
        info.files.push(file);
    }
    if !saw_key {
        return Err(invalid("boş içerik"));
    }
    Ok(info)
}

fn invalid(detail: &str) -> UpdateError {
    UpdateError::new(ErrorKind::InvalidUpdateInfo, format!("Cannot parse update info from latest.yml: {detail}"))
}

/// `anahtar: değer` ya da `anahtar:` — iki nokta, ardından boşluk ya da satır sonu gelmeli.
fn split_key(text: &str) -> Option<(&str, Option<&str>)> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b':' && (i + 1 == bytes.len() || bytes[i + 1] == b' ' || bytes[i + 1] == b'\t') {
            let key = text[..i].trim().trim_matches(|c| c == '\'' || c == '"');
            let value = text[i + 1..].trim();
            return Some((key, if value.is_empty() { None } else { Some(value) }));
        }
        i += 1;
    }
    None
}

/// Skaler değer: tırnaklıysa çözülür, düzse satır içi yorum atılır.
fn scalar(value: &str) -> String {
    let value = value.trim();
    if let Some(rest) = value.strip_prefix('\'') {
        let mut out = String::new();
        let mut chars = rest.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\'' {
                if chars.peek() == Some(&'\'') {
                    chars.next();
                    out.push('\'');
                    continue;
                }
                break;
            }
            out.push(c);
        }
        return out;
    }
    if let Some(rest) = value.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = rest.chars();
        while let Some(c) = chars.next() {
            match c {
                '"' => break,
                '\\' => match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('r') => out.push('\r'),
                    Some('0') => out.push('\0'),
                    Some(other) => out.push(other),
                    None => {}
                },
                c => out.push(c),
            }
        }
        return out;
    }
    let plain = match value.find(" #") {
        Some(at) => &value[..at],
        None => value,
    };
    let plain = plain.trim();
    if plain == "~" || plain == "null" { String::new() } else { plain.to_string() }
}

fn opt(value: &str) -> Option<String> {
    if value.is_empty() { None } else { Some(value.to_string()) }
}

fn set_top(info: &mut UpdateInfo, key: &str, value: &str) {
    match key {
        "version" => info.version = value.to_string(),
        "path" => info.path = opt(value),
        "sha512" => info.sha512 = opt(value),
        "sha2" => info.sha2 = opt(value),
        "releaseDate" => info.release_date = opt(value),
        "releaseName" => info.release_name = opt(value),
        "stagingPercentage" => info.staging_percentage = opt(value),
        "minimumSystemVersion" => info.minimum_system_version = opt(value),
        _ => {}
    }
}

fn set_file(file: &mut FileInfo, key: &str, value: &str) {
    match key {
        "url" => file.url = value.to_string(),
        "sha512" => file.sha512 = opt(value),
        "sha2" => file.sha2 = opt(value),
        "size" => file.size = value.parse().ok(),
        "blockMapSize" => file.block_map_size = value.parse().ok(),
        "isAdminRightsRequired" => file.is_admin_rights_required = value == "true",
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GERCEK: &str = "version: 0.9.9\r\nfiles:\r\n  - url: P2-Server-Setup-0.9.9.exe\r\n    sha512: fqBYxzelNgI6zAOgm9vMrOS1O587NaoRQHqQ3xJxcSxHFIwHzwhJqDJG+GJdnEu00++IE8iy+F5Cpt5GaRMgjQ==\r\n    size: 187314464\r\npath: P2-Server-Setup-0.9.9.exe\r\nsha512: fqBYxzelNgI6zAOgm9vMrOS1O587NaoRQHqQ3xJxcSxHFIwHzwhJqDJG+GJdnEu00++IE8iy+F5Cpt5GaRMgjQ==\r\nreleaseDate: '2026-11-03T21:19:48.580Z'\r\n";

    #[test]
    fn electron_builder_ciktisi() {
        let info = parse_update_info(GERCEK).unwrap();
        assert_eq!(info.version, "0.9.9");
        assert_eq!(info.files.len(), 1);
        assert_eq!(info.files[0].url, "P2-Server-Setup-0.9.9.exe");
        assert_eq!(info.files[0].size, Some(187314464));
        assert!(info.files[0].sha512.as_deref().unwrap().ends_with("gjQ=="));
        assert_eq!(info.path.as_deref(), Some("P2-Server-Setup-0.9.9.exe"));
        assert_eq!(info.release_date.as_deref(), Some("2026-11-03T21:19:48.580Z"));
        assert!(!info.files[0].is_admin_rights_required);
    }

    #[test]
    fn coklu_dosya_bom_ve_bloklar() {
        let text = "\u{feff}version: \"1.2.0-beta.1\"\nfiles:\n  -\n    url: 'a b''s.exe'\n    sha512: AAA\n    isAdminRightsRequired: true\n    blockMapSize: 12\n  - url: x64/P2.exe # yorum\n    sha2: abcd\nreleaseNotes: |\n  satır: bir\n  - liste değil\nstagingPercentage: 40\nreleaseDate: 2026-01-01\n";
        let info = parse_update_info(text).unwrap();
        assert_eq!(info.version, "1.2.0-beta.1");
        assert_eq!(info.files.len(), 2);
        assert_eq!(info.files[0].url, "a b's.exe");
        assert!(info.files[0].is_admin_rights_required);
        assert_eq!(info.files[0].block_map_size, Some(12));
        assert_eq!(info.files[1].url, "x64/P2.exe");
        assert_eq!(info.files[1].sha2.as_deref(), Some("abcd"));
        assert_eq!(info.staging_percentage.as_deref(), Some("40"));
        assert_eq!(info.release_date.as_deref(), Some("2026-01-01"));
    }

    #[test]
    fn eski_bicim_path() {
        let info = parse_update_info("version: 1.0.0\npath: Setup.exe\nsha512: QQ==\n").unwrap();
        let files = info.file_list().unwrap();
        assert_eq!(files[0].url, "Setup.exe");
        assert_eq!(files[0].sha512.as_deref(), Some("QQ=="));
        let bos = parse_update_info("version: 1.0.0\n").unwrap();
        assert_eq!(bos.file_list().unwrap_err().kind, ErrorKind::NoFiles);
    }

    #[test]
    fn bozuk_icerik() {
        assert_eq!(parse_update_info("").unwrap_err().kind, ErrorKind::InvalidUpdateInfo);
        assert_eq!(parse_update_info("<html>").unwrap_err().kind, ErrorKind::InvalidUpdateInfo);
    }
}
