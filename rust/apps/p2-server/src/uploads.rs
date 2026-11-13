//! Dosya yükleme ve sunma — `server/src/uploads.ts` karşılığı. Dosya JSON içinde base64 veriyle
//! gelir, `P2_UPLOAD_DIR` (yoksa `<cwd>/.p2-data/uploads`) altına `<uuid><uzantı>` + `.json` meta
//! olarak yazılır, `/uploads/:id` adresinden byte aralığı desteğiyle sunulur.

use std::path::PathBuf;

use axum::Router;
use axum::body::Body as HttpBody;
use axum::extract::{DefaultBodyLimit, Path};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::AppState;
use crate::ctx::{Body, Me, Reply, created};
use crate::error::ApiError;
use crate::validate::js_slice;

/// Yükleme kökü (screenshots sahibi/türü buradan doğrular).
pub fn upload_root() -> PathBuf {
    match std::env::var("P2_UPLOAD_DIR") {
        Ok(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => std::env::current_dir().unwrap_or_default().join(".p2-data").join("uploads"),
    }
}

/// Meta dosyası biçimi: `{name, type, declaredType?, size, userId}`.
#[allow(dead_code)]
#[derive(serde::Deserialize, Debug, Clone)]
pub struct UploadMeta {
    #[serde(default)]
    pub name: Value,
    #[serde(default, rename = "type")]
    pub kind: Value,
    #[serde(default, rename = "declaredType")]
    pub declared_type: Value,
    #[serde(default)]
    pub size: Value,
    #[serde(default, rename = "userId")]
    pub user_id: Value,
}

/// JS `String(value)`.
pub fn js_string(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => match n.as_f64() {
            Some(f) if n.is_f64() && f.fract() == 0.0 && f.abs() < 1e21 => format!("{f:.0}"),
            _ => n.to_string(),
        },
        Value::String(s) => s.clone(),
        Value::Array(items) => items.iter().map(|v| if v.is_null() { String::new() } else { js_string(v) }).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".into(),
    }
}

/// JS doğruluk değeri (falsy: null, false, 0, "").
pub fn js_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

/// `String(x || fallback)`; alan yoksa da fallback.
fn js_or(value: Option<&Value>, fallback: &str) -> String {
    match value {
        Some(v) if js_truthy(v) => js_string(v),
        _ => fallback.to_string(),
    }
}

fn safe_name(value: &str) -> String {
    let replaced: String = value.chars().map(|c| if (c as u32) < 0x20 || "\\/:*?\"<>|".contains(c) { '_' } else { c }).collect();
    let sliced = js_slice(&replaced, 180);
    if sliced.is_empty() { "dosya".into() } else { sliced }
}

/// Node `path.extname` (ayraçsız ad için).
pub fn extname(path: &str) -> String {
    let chars: Vec<char> = path.chars().collect();
    let mut start_dot: Option<usize> = None;
    let mut end: Option<usize> = None;
    let mut start_part = 0usize;
    let mut matched_slash = true;
    let mut pre_dot_state = 0i32;
    for i in (0..chars.len()).rev() {
        let c = chars[i];
        if c == '/' || c == '\\' {
            if !matched_slash {
                start_part = i + 1;
                break;
            }
            continue;
        }
        if end.is_none() {
            matched_slash = false;
            end = Some(i + 1);
        }
        if c == '.' {
            if start_dot.is_none() {
                start_dot = Some(i);
            } else if pre_dot_state != 1 {
                pre_dot_state = 1;
            }
        } else if start_dot.is_some() {
            pre_dot_state = -1;
        }
    }
    let (Some(start_dot), Some(end)) = (start_dot, end) else { return String::new() };
    if pre_dot_state == 0 || (pre_dot_state == 1 && start_dot == end - 1 && start_dot == start_part + 1) {
        return String::new();
    }
    chars[start_dot..end].iter().collect()
}

fn allowed_ext(name: &str) -> String {
    let cleaned: String = extname(name).chars().filter(|c| c.is_ascii_alphanumeric() || *c == '.').collect();
    js_slice(&cleaned, 12)
}

fn mime_by_ext(ext: &str) -> Option<&'static str> {
    Some(match ext {
        ".svg" => "image/svg+xml",
        ".png" => "image/png",
        ".jpg" | ".jpeg" => "image/jpeg",
        ".gif" => "image/gif",
        ".webp" => "image/webp",
        ".avif" => "image/avif",
        ".bmp" => "image/bmp",
        ".ico" => "image/x-icon",
        ".mp3" => "audio/mpeg",
        ".wav" => "audio/wav",
        ".ogg" | ".oga" | ".opus" => "audio/ogg",
        ".m4a" => "audio/mp4",
        ".aac" => "audio/aac",
        ".flac" => "audio/flac",
        ".mp4" | ".m4v" => "video/mp4",
        ".webm" => "video/webm",
        ".ogv" => "video/ogg",
        ".mov" => "video/quicktime",
        ".pdf" => "application/pdf",
        ".json" => "application/json",
        ".xml" => "application/xml",
        ".csv" => "text/csv",
        ".tsv" => "text/tab-separated-values",
        ".md" => "text/markdown",
        ".txt" | ".log" => "text/plain",
        ".yaml" | ".yml" => "text/yaml",
        ".docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        ".xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        ".xls" => "application/vnd.ms-excel",
        ".ods" => "application/vnd.oasis.opendocument.spreadsheet",
        ".pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        ".odp" => "application/vnd.oasis.opendocument.presentation",
        ".odt" => "application/vnd.oasis.opendocument.text",
        ".zip" => "application/zip",
        ".rar" => "application/vnd.rar",
        ".7z" => "application/x-7z-compressed",
        ".tar" => "application/x-tar",
        ".gz" => "application/gzip",
        ".epub" => "application/epub+zip",
        ".ttf" => "font/ttf",
        ".otf" => "font/otf",
        ".woff" => "font/woff",
        ".woff2" => "font/woff2",
        _ => return None,
    })
}

fn re(cell: &'static std::sync::OnceLock<regex::Regex>, pattern: &str) -> &'static regex::Regex {
    cell.get_or_init(|| regex::Regex::new(pattern).unwrap())
}

/// İçerik türü: önce imza, sonra uzantı, sonra metin örneği; istemcinin bildirdiği türe en son bakılır.
pub fn sniff_type(buffer: &[u8], name: &str, declared: &str) -> String {
    static SOURCE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static AUDIO_MP4: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    static MOV: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let at = |from: usize, to: usize| buffer.get(from..to.min(buffer.len())).unwrap_or_default();
    let byte = |i: usize| buffer.get(i).copied();
    if at(0, 8) == [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a] {
        return "image/png".into();
    }
    if byte(0) == Some(0xff) && byte(1) == Some(0xd8) && byte(2) == Some(0xff) {
        return "image/jpeg".into();
    }
    if at(0, 4) == b"%PDF" {
        return "application/pdf".into();
    }
    if at(0, 4) == b"OggS" {
        return if declared.starts_with("video/") { "video/ogg" } else { "audio/ogg" }.into();
    }
    if at(0, 4) == b"fLaC" {
        return "audio/flac".into();
    }
    if at(0, 3) == b"ID3" || (byte(0) == Some(0xff) && byte(1).unwrap_or(0) & 0xe0 == 0xe0) {
        return "audio/mpeg".into();
    }
    if at(0, 4) == b"RIFF" {
        return if at(8, 12) == b"WAVE" { "audio/wav".into() } else { declared.into() };
    }
    if at(4, 8) == b"ftyp" {
        return if re(&AUDIO_MP4, r"(?i)\.(?:m4a|aac)$").is_match(name) {
            "audio/mp4"
        } else if re(&MOV, r"(?i)\.mov$").is_match(name) {
            "video/quicktime"
        } else {
            "video/mp4"
        }
        .into();
    }
    if let Some(mapped) = mime_by_ext(&extname(name).to_lowercase()) {
        return mapped.into();
    }
    let source = re(
        &SOURCE,
        r"(?i)\.(?:[cm]?[jt]sx?|tsx?|py|rb|rs|go|java|kt|kts|cs|cpp|cxx|cc|c|h|hpp|php|swift|sql|sh|ps1|bat|ini|toml|conf|properties|gradle|vue|svelte|css|scss|sass|less|html?)$",
    );
    if source.is_match(name) {
        return "text/plain".into();
    }
    let sample = at(0, 4096);
    if !sample.is_empty() && !sample.contains(&0) {
        // [\t\n\r\x20-\x7e -￿] dışında kalan karakterler (denetim karakterleri) sayılır.
        let odd = String::from_utf8_lossy(sample)
            .chars()
            .filter(|&c| {
                let n = c as u32;
                !(c == '\t' || c == '\n' || c == '\r' || (0x20..=0x7e).contains(&n) || n >= 0xa0)
            })
            .count();
        if odd < 8 {
            return if declared.starts_with("text/") { declared.into() } else { "text/plain".into() };
        }
    }
    "application/octet-stream".into()
}

fn inline_type(kind: &str) -> bool {
    static INLINE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    re(&INLINE, r"^(?:image/(?:png|jpeg|gif|webp|avif|bmp|x-icon)|audio/|video/|application/pdf|text/|application/(?:json|xml))").is_match(kind)
}

#[derive(Debug, PartialEq)]
pub enum ByteRange {
    /// Başlık yok: tüm dosya.
    Full,
    /// Geçersiz/karşılanamaz: 416.
    Invalid,
    Part(f64, f64),
}

/// HTTP `Range: bytes=a-b` çözümü (uploads.ts `rangeOf`).
pub fn range_of(value: Option<&str>, size: f64) -> ByteRange {
    static RANGE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let Some(value) = value.filter(|v| !v.is_empty()) else { return ByteRange::Full };
    let Some(m) = re(&RANGE, r"^bytes=(\d*)-(\d*)$").captures(value.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')) else {
        return ByteRange::Invalid;
    };
    let num = |s: &str| if s.is_empty() { f64::NAN } else { s.parse::<f64>().unwrap_or(f64::INFINITY) };
    let (mut start, mut end) = (num(&m[1]), num(&m[2]));
    if start.is_nan() {
        let suffix = end;
        if !suffix.is_finite() || suffix <= 0.0 {
            return ByteRange::Invalid;
        }
        start = (size - suffix).max(0.0);
        end = size - 1.0;
    } else {
        if !end.is_finite() {
            end = size - 1.0;
        }
        if start < 0.0 || start >= size || end < start {
            return ByteRange::Invalid;
        }
        end = end.min(size - 1.0);
    }
    ByteRange::Part(start, end)
}

/// JS `encodeURIComponent`.
pub fn encode_uri_component(value: &str) -> String {
    let mut out = String::new();
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Node `Buffer.from(x, 'base64')`: alfabe dışı karakterler atlanır, ilk '=' durdurur, URL-güvenli harfler de geçer.
fn decode_base64(input: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len() / 4 * 3 + 3);
    let (mut acc, mut bits) = (0u32, 0u32);
    for b in input.bytes() {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            _ => continue,
        };
        acc = (acc << 6) | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

const MAX_SIZE: usize = 250 * 1024 * 1024;

async fn upload(me: Me, Body(body): Body) -> Reply {
    static DATA: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let name = safe_name(&js_or(body.get("name"), "dosya"));
    let declared = js_slice(&js_or(body.get("type"), "application/octet-stream"), 120);
    let encoded = js_or(body.get("data"), "");
    let Some(m) = re(&DATA, r"^data:[^;]*;base64,([^\n\r\u{2028}\u{2029}]+)$").captures(&encoded) else {
        return Err(ApiError::bad_request("INVALID_FILE", "Dosya verisi geçersiz."));
    };
    let buffer = decode_base64(m.get(1).map(|g| g.as_str()).unwrap_or_default());
    drop(encoded);
    if buffer.is_empty() || buffer.len() > MAX_SIZE {
        return Err(ApiError::bad_request("FILE_SIZE", "Dosya 250 MB sınırını aşıyor."));
    }
    let kind = sniff_type(&buffer, &name, &declared);
    let root = upload_root();
    tokio::fs::create_dir_all(&root).await.map_err(ApiError::internal)?;
    let id = format!("{}{}", crate::ctx::uuid(), allowed_ext(&name));
    {
        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::OpenOptions::new().write(true).create_new(true).open(root.join(&id)).await.map_err(ApiError::internal)?;
        file.write_all(&buffer).await.map_err(ApiError::internal)?;
        file.flush().await.map_err(ApiError::internal)?;
    }
    let meta = json!({ "name": name, "type": kind, "declaredType": declared, "size": buffer.len(), "userId": me.id });
    tokio::fs::write(root.join(format!("{id}.json")), serde_json::to_string(&meta).unwrap_or_default()).await.map_err(ApiError::internal)?;
    created(json!({ "name": name, "type": kind, "size": buffer.len(), "url": format!("/uploads/{id}") }))
}

fn status_only(status: StatusCode) -> Response {
    let mut res = Response::new(HttpBody::empty());
    *res.status_mut() = status;
    res
}

fn num(n: f64) -> String {
    format!("{n:.0}")
}

async fn serve(id: &str, headers: &HeaderMap, head_only: bool) -> Response {
    static ID: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    if !re(&ID, r"^[a-f0-9-]{36}(\.[a-zA-Z0-9]{1,10})?$").is_match(id) {
        return status_only(StatusCode::NOT_FOUND);
    }
    match try_serve(id, headers, head_only).await {
        Some(res) => res,
        None => status_only(StatusCode::NOT_FOUND),
    }
}

async fn try_serve(id: &str, headers: &HeaderMap, head_only: bool) -> Option<Response> {
    let path = upload_root().join(id);
    let info = tokio::fs::metadata(&path).await.ok()?;
    let raw = tokio::fs::read_to_string(path.with_file_name(format!("{id}.json"))).await.ok()?;
    let meta: Value = serde_json::from_str(&raw).ok()?;
    let mut file = tokio::fs::File::open(&path).await.ok()?;
    let size = info.len();
    let mut sample = vec![0u8; size.min(8192) as usize];
    let mut filled = 0;
    while filled < sample.len() {
        let n = file.read(&mut sample[filled..]).await.ok()?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    let field = |k: &str| meta.get(k).filter(|v| js_truthy(v));
    let name = field("name").map(js_string).unwrap_or_else(|| id.to_string());
    let declared = field("type").or_else(|| field("declaredType")).map(js_string).unwrap_or_default();
    let kind = sniff_type(&sample, &name, &declared);
    let range = range_of(headers.get(header::RANGE).and_then(|v| v.to_str().ok()), size as f64);

    let content_type = if kind.starts_with("text/") { format!("{kind}; charset=utf-8") } else { kind.clone() };
    let file_name = field("name").map(js_string).unwrap_or_else(|| "file".into());
    let disposition = format!("{}; filename*=UTF-8''{}", if inline_type(&kind) { "inline" } else { "attachment" }, encode_uri_component(&file_name));
    let mut res = Response::new(HttpBody::empty());
    let h = res.headers_mut();
    h.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    h.insert(header::CONTENT_TYPE, HeaderValue::from_str(&content_type).ok()?);
    h.insert(header::CONTENT_DISPOSITION, HeaderValue::from_str(&disposition).ok()?);
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=31536000, immutable"));
    let mut partial = false;
    let (start, length) = match range {
        ByteRange::Invalid => {
            h.insert(header::CONTENT_RANGE, HeaderValue::from_str(&format!("bytes */{size}")).ok()?);
            *res.status_mut() = StatusCode::RANGE_NOT_SATISFIABLE;
            if head_only {
                // Express HEAD + end(): Content-Length yok. Boyutu bilinmeyen boş gövde hyper'ın 0 eklemesini önler.
                *res.body_mut() = HttpBody::from_stream(tokio_util::io::ReaderStream::new(tokio::io::empty()));
            }
            return Some(res);
        }
        ByteRange::Part(start, end) => {
            h.insert(header::CONTENT_RANGE, HeaderValue::from_str(&format!("bytes {}-{}/{size}", num(start), num(end))).ok()?);
            let length = (end - start + 1.0).max(0.0);
            h.insert(header::CONTENT_LENGTH, HeaderValue::from_str(&num(length)).ok()?);
            partial = true;
            (start as u64, length as u64)
        }
        ByteRange::Full => {
            h.insert(header::CONTENT_LENGTH, HeaderValue::from(size));
            (0, size)
        }
    };
    if partial {
        *res.status_mut() = StatusCode::PARTIAL_CONTENT;
    }
    if head_only {
        return Some(res);
    }
    file.seek(std::io::SeekFrom::Start(start)).await.ok()?;
    let stream = tokio_util::io::ReaderStream::new(file.take(length));
    *res.body_mut() = HttpBody::from_stream(stream);
    Some(res)
}

async fn file(Path(id): Path<String>, headers: HeaderMap) -> Response {
    serve(&id, &headers, false).await.into_response()
}

async fn head(Path(id): Path<String>, headers: HeaderMap) -> Response {
    serve(&id, &headers, true).await.into_response()
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/uploads", post(upload).layer(DefaultBodyLimit::max(crate::direct::BODY_LIMIT)))
        .route("/uploads/{id}", get(file).head(head))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tur_tespiti() {
        assert_eq!(sniff_type(b"%PDF-1.7", "yanlış.bin", "text/plain"), "application/pdf");
        assert_eq!(sniff_type(b"ID3\x04\0\0", "ses.bin", "application/octet-stream"), "audio/mpeg");
        assert_eq!(sniff_type(b"<svg/>", "çizim.svg", "application/octet-stream"), "image/svg+xml");
    }

    #[test]
    fn aralik() {
        assert_eq!(range_of(Some("bytes=3-8"), 20.0), ByteRange::Part(3.0, 8.0));
        assert_eq!(range_of(Some("bytes=5-"), 20.0), ByteRange::Part(5.0, 19.0));
        assert_eq!(range_of(Some("bytes=-4"), 20.0), ByteRange::Part(16.0, 19.0));
        assert_eq!(range_of(Some("bytes=30-40"), 20.0), ByteRange::Invalid);
        assert_eq!(range_of(Some("items=0-2"), 20.0), ByteRange::Invalid);
    }

    #[test]
    fn base64_gevsek() {
        assert_eq!(decode_base64("QQ==QQ=="), b"A");
        assert_eq!(decode_base64("QUJD!!REVG"), b"ABCDEF");
        assert_eq!(decode_base64("-_-_"), [251, 255, 191]);
        assert_eq!(decode_base64("Q"), b"");
        assert_eq!(extname(".bashrc"), "");
        assert_eq!(extname("a.tar.gz"), ".gz");
        assert_eq!(extname("a."), ".");
    }
}
