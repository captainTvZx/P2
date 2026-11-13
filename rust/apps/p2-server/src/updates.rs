//! Otomatik güncelleme dosyaları — `server/src/updates.ts` karşılığı. Sunucuyu barındıran kişi
//! yeni kurulum dosyasını P2_UPDATE_DIR'e koyar; ekipteki uygulamalar açılışta buraya bakıp
//! kendini günceller. Uçlar bilerek public: istemci giriş yapmadan da güncelleyebilmeli.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use axum::Router;
use axum::body::Bytes;
use axum::extract::{ConnectInfo, Path, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{Value, json};
use sha2::Sha256;

use crate::AppState;
use crate::ctx::{Reply, created};
use crate::error::ApiError;

/// Güncelleme dosyalarının yeri: normalde uygulama süreci P2_UPDATE_DIR'i verir; verilmezse
/// yayınlama dizinleri sırayla denenir (aksi hâlde latest.yml sessizce 404 döner).
fn roots() -> &'static [PathBuf] {
    static ROOTS: OnceLock<Vec<PathBuf>> = OnceLock::new();
    ROOTS.get_or_init(|| match std::env::var("P2_UPDATE_DIR") {
        Ok(dir) if !dir.is_empty() => vec![PathBuf::from(dir)],
        _ => {
            let mut list = Vec::new();
            if cfg!(windows) {
                let data = std::env::var("ProgramData").ok().filter(|d| !d.is_empty()).unwrap_or_else(|| "C:\\ProgramData".into());
                list.push(PathBuf::from(data).join("P2").join("updates"));
            }
            list.push(std::env::current_dir().unwrap_or_default().join(".p2-data").join("updates"));
            list
        }
    })
}

/// İlk bulunan kopya sunulur; hiçbirinde yoksa None.
async fn locate(parts: &[&str]) -> Option<(PathBuf, u64)> {
    for dir in roots() {
        let file = parts.iter().fold(dir.clone(), |path, part| path.join(part));
        if let Ok(info) = tokio::fs::metadata(&file).await
            && info.is_file()
        {
            return Some((file, info.len()));
        }
    }
    None
}

/// Dizin dışına çıkma denemeleri: yalnızca düz dosya adları kabul edilir.
fn safe_file(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)) && !value.starts_with('.')
}

fn safe_channel(value: &str) -> bool {
    matches!(value, "client" | "server")
}

fn set(headers: &mut HeaderMap, name: &'static str, value: String) {
    if let Ok(v) = HeaderValue::from_str(&value) {
        headers.insert(name, v);
    }
}

fn json_response(status: StatusCode, body: Value) -> Response {
    (status, axum::Json(body)).into_response()
}

/// Kurulum dosyası 150 MB'ın üzerinde; belleğe alınmaz, akıtılır. Kısmi indirme (Range)
/// desteklenir ki güncelleyici kesilen indirmeyi sürdürebilsin.
async fn stream_file(file: &std::path::Path, size: u64, safe: &str, request: &HeaderMap) -> Response {
    let mut headers = HeaderMap::new();
    set(&mut headers, "content-type", if safe.ends_with(".yml") { "text/yaml; charset=utf-8" } else { "application/octet-stream" }.into());
    set(&mut headers, "accept-ranges", "bytes".into());
    set(&mut headers, "cache-control", "no-store".into());
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"^bytes=(\d*)-(\d*)$").unwrap());
    let raw = request.get(header::RANGE).and_then(|v| v.to_str().ok()).unwrap_or_default();
    let body = |start: u64, len: u64| crate::releases::file_body(file, start, len);
    if let Some(m) = re.captures(raw) {
        let num = |i: usize| m.get(i).map(|g| g.as_str()).filter(|g| !g.is_empty()).map(|g| g.parse::<f64>().unwrap_or(f64::INFINITY));
        let total = size as f64;
        let start = match num(1) {
            Some(s) => s,
            None => (total - num(2).unwrap_or(0.0)).max(0.0),
        };
        let end = match (num(1), num(2)) {
            (Some(_), Some(e)) => e,
            _ => total - 1.0,
        };
        if !start.is_finite() || start >= total || end < start {
            set(&mut headers, "content-range", format!("bytes */{size}"));
            return (StatusCode::RANGE_NOT_SATISFIABLE, headers).into_response();
        }
        let last = end.min(total - 1.0) as u64;
        let start = start as u64;
        set(&mut headers, "content-range", format!("bytes {start}-{last}/{size}"));
        set(&mut headers, "content-length", (last - start + 1).to_string());
        return match body(start, last - start + 1).await {
            Ok(b) => (StatusCode::PARTIAL_CONTENT, headers, b).into_response(),
            Err(error) => ApiError::internal(error).into_response(),
        };
    }
    set(&mut headers, "content-length", size.to_string());
    match body(0, size).await {
        Ok(b) => (StatusCode::OK, headers, b).into_response(),
        Err(error) => ApiError::internal(error).into_response(),
    }
}

/// Yayınlanan sürümü okur: latest.yml'in ilk satırı "version: 0.5.8".
async fn published_version() -> String {
    let Some((file, _)) = locate(&["latest.yml"]).await else { return String::new() };
    let Ok(text) = tokio::fs::read(&file).await else { return String::new() };
    let text = String::from_utf8_lossy(&text);
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"(?mR)^version:\s*([^\n\r\u{2028}\u{2029}]+)$").unwrap());
    re.captures(&text).map(|c| crate::tasks::js_trim(&c[1]).to_string()).unwrap_or_default()
}

/// Yayın klasörü sürekli izlenir: yeni sürüm kopyalandığı anda bağlı olan herkese haber gider.
/// main.rs açılışta çağırabilir; çağrılmazsa ilk güncelleme isteğinde kendiliğinden başlar.
pub fn start(state: &AppState) {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }
    let state = state.clone();
    tokio::spawn(async move {
        static CURRENT: Mutex<String> = Mutex::new(String::new());
        *CURRENT.lock().unwrap() = published_version().await;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            let version = published_version().await;
            let changed = {
                let mut current = CURRENT.lock().unwrap();
                let changed = !version.is_empty() && *current != version;
                if changed {
                    *current = version.clone();
                }
                changed
            };
            if changed {
                crate::realtime::broadcast(&state, "update:published", &json!({ "version": version })).await;
            }
        }
    });
}

fn is_local(addr: Option<SocketAddr>) -> bool {
    match addr.map(|a| a.ip()) {
        Some(IpAddr::V4(ip)) => ip == Ipv4Addr::LOCALHOST,
        Some(IpAddr::V6(ip)) => ip.is_loopback() || ip.to_ipv4_mapped() == Some(Ipv4Addr::LOCALHOST),
        None => false,
    }
}

/// Sunucu, aynı makineden gelen imzalı bir istekle kendini kapatır (kurulum çalışan exe'yi değiştirebilsin).
async fn stop(State(s): State<AppState>, request: Request) -> Reply {
    start(&s);
    let peer = request.extensions().get::<ConnectInfo<SocketAddr>>().map(|c| c.0);
    let bytes: Bytes = axum::body::to_bytes(request.into_body(), 1024 * 1024).await.map_err(|_| ApiError::bad_request("INVALID_BODY", "Gövde okunamadı."))?;
    let body = crate::validate::body(&bytes)?;
    if !is_local(peer) {
        return Err(crate::ctx::forbidden("LOCAL_ONLY", "Bu istek yalnızca bu bilgisayardan yapılabilir."));
    }
    let secret = &s.cfg.access_secret;
    let expected = if secret.is_empty() {
        String::new()
    } else {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("HMAC her anahtar uzunluğunu kabul eder");
        mac.update(b"p2-shutdown");
        hex::encode(mac.finalize().into_bytes())
    };
    let given = match body.get("token") {
        None | Some(Value::Null) => String::new(),
        Some(v) if !crate::tasks::truthy(Some(v)) => String::new(),
        Some(v) => crate::tasks::js_string(v),
    };
    let valid = !expected.is_empty()
        && given.len() == expected.len()
        && given.bytes().zip(expected.bytes()).fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0;
    if !valid {
        return Err(crate::ctx::forbidden("BAD_TOKEN", "Geçersiz kapatma anahtarı."));
    }
    // Yanıt gitsin, sonra süreç kapansın.
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        std::process::exit(0);
    });
    created(json!({ "ok": true }))
}

async fn latest(State(s): State<AppState>) -> Response {
    start(&s);
    let Some((file, size)) = locate(&["latest.yml"]).await else {
        // Henüz sürüm yayınlanmamış: istemci bunu sessizce geçer.
        return json_response(StatusCode::NOT_FOUND, json!({ "code": "NO_UPDATE", "message": "Yayınlanmış sürüm yok." }));
    };
    let mut headers = HeaderMap::new();
    set(&mut headers, "content-type", "text/yaml; charset=utf-8".into());
    set(&mut headers, "content-length", size.to_string());
    set(&mut headers, "cache-control", "no-store".into());
    match crate::releases::file_body(&file, 0, size).await {
        Ok(body) => (StatusCode::OK, headers, body).into_response(),
        Err(_) => json_response(StatusCode::NOT_FOUND, json!({ "code": "NO_UPDATE", "message": "Yayınlanmış sürüm yok." })),
    }
}

/// Ayrı ürün kanalları: kök kanal eski istemciler için; /server bağımsız sunucu kurulumu.
async fn channel_download(State(s): State<AppState>, Path((channel, name)): Path<(String, String)>, headers: HeaderMap) -> Response {
    start(&s);
    let safe = safe_file(&name);
    if !safe_channel(&channel) || !safe {
        return json_response(StatusCode::BAD_REQUEST, json!({ "code": "INVALID_FILE", "message": "Kanal veya dosya adı geçersiz." }));
    }
    let Some((file, size)) = locate(&[&channel, &name]).await else {
        return json_response(StatusCode::NOT_FOUND, json!({ "code": "NOT_FOUND", "message": "Dosya bulunamadı." }));
    };
    stream_file(&file, size, &name, &headers).await
}

async fn download_named(name: &str, headers: &HeaderMap) -> Response {
    if !safe_file(name) {
        return json_response(StatusCode::BAD_REQUEST, json!({ "code": "INVALID_FILE", "message": "Dosya adı geçersiz." }));
    }
    let Some((file, size)) = locate(&[name]).await else {
        return json_response(StatusCode::NOT_FOUND, json!({ "code": "NOT_FOUND", "message": "Dosya bulunamadı." }));
    };
    stream_file(&file, size, name, headers).await
}

async fn download(State(s): State<AppState>, Path(name): Path<String>, headers: HeaderMap) -> Response {
    start(&s);
    download_named(&name, &headers).await
}

/// `GET /updates/stop` Express'te `:file` yoluna düşer.
async fn download_stop(State(s): State<AppState>, headers: HeaderMap) -> Response {
    start(&s);
    download_named("stop", &headers).await
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/updates/stop", post(stop).get(download_stop))
        .route("/updates/latest.yml", get(latest))
        .route("/updates/{channel}/{file}", get(channel_download))
        .route("/updates/{file}", get(download))
}
