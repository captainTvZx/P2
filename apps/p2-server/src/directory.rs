//! Sunucu tarayıcısı dizini: P2 Server kurulumları (kim nereye kurduysa) kendini buraya duyurur,
//! istemciler listeyi buradan alır. Yalnız `[directory] host = true` olan sunucuda (merkezi sunucu)
//! etkindir; diğerlerinde uçlar 404 döner.
//!
//! - `POST /directory/announce` `{ id, key, name, description, url, access, codeHash, version, members, online, icon, iconType }`
//! - `POST /directory/remove` `{ id, key }`
//! - `GET  /directory/servers?q=` → `{ servers: [{ id, name, description, iconUrl, access, version, members, online, live, url? }] }`
//! - `GET  /directory/servers/:id/icon`
//! - `POST /directory/servers/:id/join` `{ code }` → `{ url }` (özel sunucu: kod doğruysa; yanlışsa 403 `BAD_CODE`)
//!
//! Özel sunucunun adresi listede verilmez; yalnız doğru kodla `join` döndürür. Kod dizine açık
//! gelmez: sunucu `sha256(id ":" kod)` gönderir. Kimlik ilk duyuruda anahtarla eşlenir (anahtarın
//! özeti saklanır); başka biri aynı kimlikle duyuramaz. Adres, o adresteki `/instance` aynı kimliği
//! döndürünce "doğrulanmış" sayılır; doğrulanmamış sunucu listelenmez.
//! Kayıtlar veri klasöründe `directory.json`'da tutulur; 14 gün duyurmayan sunucu silinir.

use std::collections::HashMap;
use std::path::PathBuf;

use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::Digest as _;

use crate::AppState;
use crate::ctx::{Body as JsonBody, Query, Reply, forbidden, not_found, ok};
use crate::error::ApiError;

/// Son duyurudan bu kadar süre geçmeyen sunucu "çevrimiçi" sayılır (duyuru 5 dakikada bir).
const LIVE_SECS: i64 = 12 * 60;
const FORGET_SECS: i64 = 14 * 24 * 3600;
const REVERIFY_SECS: i64 = 3600;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: String,
    key_hash: String,
    pub name: String,
    pub description: String,
    pub url: String,
    pub access: String,
    code_hash: Option<String>,
    pub version: String,
    pub members: i64,
    pub online: i64,
    icon: Option<String>,
    icon_type: Option<String>,
    first_seen: i64,
    last_seen: i64,
    verified: bool,
    verified_at: i64,
    verified_url: String,
}

#[derive(Default)]
pub struct Directory {
    file: Option<PathBuf>,
    entries: HashMap<String, Entry>,
}

impl Directory {
    pub fn load(data_dir: &std::path::Path) -> Self {
        let file = data_dir.join("directory.json");
        let entries: Vec<Entry> = std::fs::read(&file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Self { file: Some(file), entries: entries.into_iter().map(|e| (e.id.clone(), e)).collect() }
    }

    fn save(&self) {
        let Some(file) = &self.file else { return };
        let list: Vec<&Entry> = self.entries.values().collect();
        if let Ok(text) = serde_json::to_vec_pretty(&list) {
            let tmp = file.with_extension("json.tmp");
            if std::fs::write(&tmp, text).is_ok() {
                let _ = std::fs::rename(&tmp, file);
            }
        }
    }
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/directory/announce", post(announce))
        .route("/directory/remove", post(remove))
        .route("/directory/servers", get(list))
        .route("/directory/servers/{id}/icon", get(icon))
        .route("/directory/servers/{id}/join", post(join))
}

fn enabled(state: &AppState) -> Result<(), ApiError> {
    if state.instance.get().directory_host { Ok(()) } else { Err(not_found("DIRECTORY_DISABLED", "Bu sunucu sunucu tarayıcısı dizini barındırmıyor.")) }
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

fn key_hash(key: &str) -> String {
    hex::encode(sha2::Sha256::digest(key.as_bytes()))
}

fn clip(s: &str, n: usize) -> String {
    s.trim().chars().take(n).collect()
}

async fn announce(State(state): State<AppState>, body: JsonBody) -> Reply {
    enabled(&state)?;
    let id = clip(body.str("id").unwrap_or_default(), 64);
    let key = body.str("key").unwrap_or_default();
    let url = clip(body.str("url").unwrap_or_default(), 300).trim_end_matches('/').to_string();
    if id.len() < 8 || key.len() < 16 || !(url.starts_with("https://") || url.starts_with("http://")) {
        return Err(ApiError::bad_request("BAD_ANNOUNCE", "Eksik ya da geçersiz duyuru (id, key, url)."));
    }
    let icon = body.str("icon").filter(|i| i.len() <= 360 * 1024 && base64::engine::general_purpose::STANDARD.decode(i).is_ok()).map(str::to_string);
    let icon_type = body.str("iconType").filter(|t| ["image/png", "image/jpeg", "image/webp", "image/gif"].contains(t)).map(str::to_string);
    let n = |k: &str| body.get(k).and_then(serde_json::Value::as_i64).unwrap_or(0).max(0);
    let verify;
    {
        let mut dir = state.directory.lock().unwrap();
        let t = now();
        let entry = dir.entries.entry(id.clone()).or_insert_with(|| Entry { id: id.clone(), key_hash: key_hash(key), first_seen: t, ..Default::default() });
        if entry.key_hash != key_hash(key) {
            return Err(forbidden("BAD_KEY", "Bu sunucu kimliği başka bir anahtarla kayıtlı."));
        }
        entry.name = clip(body.str("name").unwrap_or("P2 Server"), 60);
        entry.description = clip(body.str("description").unwrap_or_default(), 400);
        entry.access = if body.str("access") == Some("private") { "private".into() } else { "public".into() };
        entry.code_hash = body.str("codeHash").filter(|h| h.len() == 64).map(str::to_string);
        entry.version = clip(body.str("version").unwrap_or_default(), 32);
        entry.members = n("members");
        entry.online = n("online");
        entry.icon = icon;
        entry.icon_type = icon_type;
        entry.last_seen = t;
        if entry.url != url {
            entry.url = url.clone();
            entry.verified = false;
        }
        verify = !entry.verified || entry.verified_url != url || t - entry.verified_at > REVERIFY_SECS;
        dir.save();
    }
    if verify {
        // Adres gerçekten bu sunucuya mı ait: o adresteki /instance aynı kimliği döndürmeli.
        let st = state.clone();
        let (id2, url2) = (id.clone(), url.clone());
        tokio::spawn(async move {
            let good = check_url(&url2, &id2).await;
            let mut dir = st.directory.lock().unwrap();
            if let Some(e) = dir.entries.get_mut(&id2) {
                if e.url == url2 {
                    e.verified = good;
                    e.verified_at = now();
                    e.verified_url = url2.clone();
                    dir.save();
                }
            }
            if !good {
                tracing::warn!("dizin: {url2} doğrulanamadı (kimlik {id2})");
            }
        });
    }
    ok(json!({ "ok": true }))
}

async fn check_url(url: &str, id: &str) -> bool {
    let client = reqwest::Client::builder().timeout(Duration::from_secs(10)).build();
    let Ok(client) = client else { return false };
    match client.get(format!("{url}/instance")).send().await {
        Ok(r) if r.status().is_success() => r.json::<serde_json::Value>().await.ok().is_some_and(|v| v["id"].as_str() == Some(id)),
        _ => false,
    }
}

async fn remove(State(state): State<AppState>, body: JsonBody) -> Reply {
    enabled(&state)?;
    let id = body.str("id").unwrap_or_default().to_string();
    let key = body.str("key").unwrap_or_default();
    let mut dir = state.directory.lock().unwrap();
    if dir.entries.get(&id).is_some_and(|e| e.key_hash == key_hash(key)) {
        dir.entries.remove(&id);
        dir.save();
    }
    ok(json!({ "ok": true }))
}

async fn list(State(state): State<AppState>, query: Query) -> Reply {
    enabled(&state)?;
    let q = query.get("q").map(|s| s.trim().to_lowercase()).unwrap_or_default();
    let t = now();
    let mut dir = state.directory.lock().unwrap();
    let before = dir.entries.len();
    dir.entries.retain(|_, e| t - e.last_seen < FORGET_SECS);
    if dir.entries.len() != before {
        dir.save();
    }
    let mut rows: Vec<&Entry> = dir
        .entries
        .values()
        .filter(|e| e.verified)
        .filter(|e| q.is_empty() || e.name.to_lowercase().contains(&q) || e.description.to_lowercase().contains(&q))
        .collect();
    rows.sort_by(|a, b| {
        let live = |e: &Entry| t - e.last_seen < LIVE_SECS;
        live(b).cmp(&live(a)).then(b.online.cmp(&a.online)).then(b.members.cmp(&a.members)).then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    let servers: Vec<serde_json::Value> = rows
        .into_iter()
        .map(|e| {
            json!({
                "id": e.id,
                "name": e.name,
                "description": e.description,
                "iconUrl": e.icon.as_ref().map(|_| format!("/directory/servers/{}/icon", e.id)),
                "access": e.access,
                "version": e.version,
                "members": e.members,
                "online": e.online,
                "live": t - e.last_seen < LIVE_SECS,
                // Özel sunucunun adresi listede yok: yalnız doğru kodla `join` verir.
                "url": if e.access == "public" { serde_json::Value::String(e.url.clone()) } else { serde_json::Value::Null },
            })
        })
        .collect();
    ok(json!({ "servers": servers }))
}

async fn icon(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if enabled(&state).is_err() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let found = state.directory.lock().unwrap().entries.get(&id).and_then(|e| Some((e.icon.clone()?, e.icon_type.clone().unwrap_or_else(|| "image/png".into()))));
    let Some((data, kind)) = found else { return StatusCode::NOT_FOUND.into_response() };
    match base64::engine::general_purpose::STANDARD.decode(data) {
        Ok(bytes) => ([(header::CONTENT_TYPE, kind), (header::CACHE_CONTROL, "public, max-age=600".to_string())], Body::from(bytes)).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn join(State(state): State<AppState>, Path(id): Path<String>, body: JsonBody) -> Reply {
    enabled(&state)?;
    let code = body.str("code").unwrap_or_default().to_string();
    let found = state.directory.lock().unwrap().entries.get(&id).filter(|e| e.verified).cloned();
    let Some(e) = found else { return Err(not_found("SERVER_NOT_FOUND", "Sunucu bulunamadı.")) };
    if e.access == "private" {
        let good = e.code_hash.as_deref().is_some_and(|h| h == p2_server_ops::settings::code_hash(&e.id, &code));
        if !good {
            tokio::time::sleep(Duration::from_millis(500)).await;
            return Err(forbidden("BAD_CODE", "Katılım kodu yanlış.").safe());
        }
    }
    ok(json!({ "url": e.url, "name": e.name }))
}
