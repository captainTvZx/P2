//! Sunucu kimliği ve sunucu tarayıcısı (`p2-server.ini`, bkz. `p2_server_ops::settings`).
//!
//! - `GET  /instance` → `{ id, name, description, iconUrl, access, version, members, online, publicUrl }` (oturumsuz)
//! - `GET  /instance/icon` → simge dosyası (ya da adresine yönlendirme)
//! - `POST /instance/check-code { code }` → `{ ok }` (özel sunucu katılım kodu)
//! - `GET  /calls/ice-servers` → `{ iceServers, ttl }`: STUN + geçici TURN kimliği (coturn `use-auth-secret`)
//!
//! Ayar dosyası çalışırken izlenir (10 sn); değişiklik yeniden başlatmadan uygulanır. Sunucu listelenmek
//! istiyorsa (`[directory] list`, `public_url`) 5 dakikada bir ve her ayar değişikliğinde dizine kendini
//! duyurur; listelenmek istemiyorsa dizinden kaydını sildirir.

use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use base64::Engine as _;
use hmac::{KeyInit as _, Mac as _};
use p2_server_ops::settings::{Access, ServerSettings};
use serde_json::{Value, json};

use crate::AppState;
use crate::ctx::{Body as JsonBody, Me, Reply, ok};
use crate::error::ApiError;

/// Paylaşılan ayar durumu.
pub struct Instance {
    pub data_dir: PathBuf,
    pub settings: RwLock<ServerSettings>,
    /// Ayarlar değişince duyurucu hemen çalışsın.
    pub changed: tokio::sync::Notify,
}

impl Instance {
    pub fn load() -> Arc<Self> {
        let data_dir = p2_server_ops::data_dir();
        let settings = ServerSettings::load_or_create(&data_dir);
        Arc::new(Self { data_dir, settings: RwLock::new(settings), changed: tokio::sync::Notify::new() })
    }

    pub fn get(&self) -> ServerSettings {
        self.settings.read().unwrap().clone()
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/instance", get(info))
        .route("/instance/icon", get(icon))
        .route("/instance/check-code", post(check_code))
        .route("/calls/ice-servers", get(ice_servers))
}

/// Ayar izleyicisi ve dizin duyurucusu.
pub fn start(state: &AppState) {
    let st = state.clone();
    tokio::spawn(async move {
        let file = p2_server_ops::settings::path(&st.instance.data_dir);
        let mut seen = modified(&file);
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            let now = modified(&file);
            if now != seen {
                seen = now;
                let fresh = ServerSettings::load_or_create(&st.instance.data_dir);
                let changed = *st.instance.settings.read().unwrap() != fresh;
                if changed {
                    tracing::info!("p2-server.ini değişti; yeni ayarlar uygulandı");
                    *st.instance.settings.write().unwrap() = fresh;
                    st.instance.changed.notify_one();
                }
            }
        }
    });
    let st = state.clone();
    tokio::spawn(async move {
        // Açılışta sunum başlasın diye kısa bekleme (dizin doğrulaması /instance'ı çağırır).
        tokio::time::sleep(Duration::from_secs(5)).await;
        let mut was_listed = false;
        loop {
            let s = st.instance.get();
            let listed = s.listed && !s.public_url.is_empty() && !s.directory_url.is_empty();
            if listed {
                match announce(&st, &s).await {
                    Ok(()) => was_listed = true,
                    Err(e) => tracing::warn!("sunucu tarayıcısına duyurulamadı ({}): {e}", s.directory_url),
                }
            } else if was_listed || !s.listed {
                // Listeden çıkmak isteyen ya da adresi silinen sunucu dizinden kaydını sildirir.
                if remove(&s).await.is_ok() {
                    was_listed = false;
                }
            }
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(300)) => {}
                _ = st.instance.changed.notified() => {}
            }
        }
    });
}

fn modified(file: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(file).and_then(|m| m.modified()).ok()
}

async fn counts(state: &AppState) -> (i64, usize) {
    let members: i64 = sqlx::query_scalar(r#"select count(*) from "User""#).fetch_one(&state.db).await.unwrap_or(0);
    let online = state.presence.lock().unwrap().online.len();
    (members, online)
}

fn icon_type(file: &std::path::Path) -> &'static str {
    match file.extension().and_then(|e| e.to_str()).map(str::to_lowercase).as_deref() {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        _ => "image/png",
    }
}

async fn info(State(state): State<AppState>) -> Reply {
    let s = state.instance.get();
    let (members, online) = counts(&state).await;
    let has_icon = !s.icon.trim().is_empty();
    ok(json!({
        "id": s.instance_id,
        "name": s.name,
        "description": s.description,
        "iconUrl": if has_icon { Value::String("/instance/icon".into()) } else { Value::Null },
        "access": s.access.as_str(),
        "version": state.cfg.version,
        "members": members,
        "online": online,
        "publicUrl": if s.public_url.is_empty() { Value::Null } else { Value::String(s.public_url.clone()) },
    }))
}

async fn icon(State(state): State<AppState>) -> Response {
    let s = state.instance.get();
    let raw = s.icon.trim();
    if raw.starts_with("https://") || raw.starts_with("http://") {
        return Redirect::temporary(raw).into_response();
    }
    let Some(file) = s.icon_file(&state.instance.data_dir) else { return StatusCode::NOT_FOUND.into_response() };
    match tokio::fs::read(&file).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, icon_type(&file)), (header::CACHE_CONTROL, "public, max-age=300")], Body::from(bytes)).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn check_code(State(state): State<AppState>, body: JsonBody) -> Reply {
    let s = state.instance.get();
    let code = body.str("code").unwrap_or_default().trim().to_string();
    let good = s.access == Access::Public || (!s.access_code.trim().is_empty() && code == s.access_code.trim());
    if !good {
        // Kaba deneme yavaşlatılır.
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
    ok(json!({ "ok": good }))
}

/// Özel sunucuda yeni üye kaydı katılım kodu ister (`accessCode`). Davet koduyla gelen kayıt da
/// geçerlidir: proje daveti zaten bir üyenin verdiği izindir.
pub fn register_allowed(state: &AppState, body: &serde_json::Map<String, Value>) -> Result<(), ApiError> {
    let s = state.instance.get();
    if s.access != Access::Private || s.access_code.trim().is_empty() {
        return Ok(());
    }
    let invite = body.get("inviteCode").and_then(Value::as_str).map(str::trim).unwrap_or_default();
    let code = body.get("accessCode").and_then(Value::as_str).unwrap_or_default().trim();
    if !invite.is_empty() || code == s.access_code.trim() {
        return Ok(());
    }
    Err(crate::ctx::forbidden("ACCESS_CODE_REQUIRED", "Bu sunucu özel: kayıt olmak için katılım kodunu gir.").field("accessCode").safe())
}

/// Özel sunucunun katılım kodu mu (kayıt formunda davet kodu alanına yazılmış olabilir).
pub fn is_access_code(state: &AppState, code: &str) -> bool {
    let s = state.instance.get();
    s.access == Access::Private && !s.access_code.trim().is_empty() && code.trim() == s.access_code.trim()
}

/// STUN + (ayarlıysa) TURN. TURN kimliği coturn REST biçimi: kullanıcı `"<bitiş>:<kullanıcı>"`,
/// parola `base64(HMAC-SHA1(gizli, kullanıcı))`.
async fn ice_servers(State(state): State<AppState>, me: Me) -> Reply {
    let s = state.instance.get();
    let mut list = vec![json!({ "urls": ["stun:stun.l.google.com:19302"] })];
    if !s.turn_urls.is_empty() && !s.turn_secret.is_empty() {
        let expires = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) + s.turn_ttl;
        let username = format!("{expires}:{}", me.id);
        let mut mac = <hmac::Hmac<sha1::Sha1>>::new_from_slice(s.turn_secret.as_bytes()).map_err(ApiError::internal)?;
        mac.update(username.as_bytes());
        let credential = base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());
        list.push(json!({ "urls": s.turn_urls, "username": username, "credential": credential }));
    }
    ok(json!({ "iceServers": list, "ttl": s.turn_ttl }))
}

fn http() -> &'static reqwest::Client {
    static C: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    C.get_or_init(|| reqwest::Client::builder().timeout(Duration::from_secs(15)).user_agent(concat!("P2-Server/", env!("CARGO_PKG_VERSION"))).build().expect("http"))
}

/// Dizine giden simge: 256×256 PNG'ye küçültülür (büyük dosya da gönderilebilsin, dizin hafif kalsın).
async fn icon_bytes(state: &AppState, s: &ServerSettings) -> Option<(String, &'static str)> {
    const MAX: usize = 4 * 1024 * 1024;
    let raw = s.icon.trim();
    let (bytes, kind) = if raw.starts_with("https://") || raw.starts_with("http://") {
        let r = http().get(raw).send().await.ok()?;
        let kind = match r.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("") {
            t if t.contains("jpeg") => "image/jpeg",
            t if t.contains("webp") => "image/webp",
            _ => "image/png",
        };
        (r.bytes().await.ok()?.to_vec(), kind)
    } else {
        let file = s.icon_file(&state.instance.data_dir)?;
        (tokio::fs::read(&file).await.ok()?, icon_type(&file))
    };
    if bytes.len() > MAX {
        return None;
    }
    let small = tokio::task::spawn_blocking(move || -> Option<Vec<u8>> {
        let image = image::load_from_memory(&bytes).ok()?;
        let image = if image.width() > 256 || image.height() > 256 { image.resize(256, 256, image::imageops::FilterType::Lanczos3) } else { image };
        let mut out = std::io::Cursor::new(Vec::new());
        image.write_to(&mut out, image::ImageFormat::Png).ok()?;
        Some(out.into_inner())
    })
    .await
    .ok()
    .flatten();
    let _ = kind;
    small.map(|b| (base64::engine::general_purpose::STANDARD.encode(b), "image/png"))
}

async fn announce(state: &AppState, s: &ServerSettings) -> anyhow::Result<()> {
    let (members, online) = counts(state).await;
    let icon = icon_bytes(state, s).await;
    let body = json!({
        "id": s.instance_id,
        "key": s.instance_key,
        "name": s.name,
        "description": s.description,
        "url": s.public_url,
        "access": s.access.as_str(),
        "codeHash": s.code_hash(),
        "version": state.cfg.version,
        "members": members,
        "online": online,
        "icon": icon.as_ref().map(|i| i.0.clone()),
        "iconType": icon.as_ref().map(|i| i.1),
    });
    let r = http().post(format!("{}/directory/announce", s.directory_url)).json(&body).send().await?;
    if !r.status().is_success() {
        anyhow::bail!("HTTP {}: {}", r.status(), r.text().await.unwrap_or_default().chars().take(200).collect::<String>());
    }
    Ok(())
}

async fn remove(s: &ServerSettings) -> anyhow::Result<()> {
    if s.directory_url.is_empty() {
        return Ok(());
    }
    http().post(format!("{}/directory/remove", s.directory_url)).json(&json!({ "id": s.instance_id, "key": s.instance_key })).send().await?;
    Ok(())
}
