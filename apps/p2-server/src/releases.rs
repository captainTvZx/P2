//! Kütüphane sürümleri — `server/src/releases.ts` karşılığı: parça parça (içerik adresli)
//! build yükleme, manifest, yayınlama/canlıya alma, parça indirme (Range) ve kurulum kayıtları.
//! Disk düzeni, ortam değişkeni (P2_RELEASE_DIR) ve özetleme releaseStore.ts ile aynıdır.

#[path = "release_manifest.rs"]
pub mod release_manifest;
#[path = "release_store.rs"]
pub mod release_store;

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use axum::Router;
use axum::body::Body as RawBody;
use axum::extract::{DefaultBodyLimit, Path, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use chrono::NaiveDateTime;
use serde_json::{Map, Value, json};
use sqlx::types::Json;

use crate::AppState;
use crate::activity::{self, ActivityInput};
use crate::ctx::{Body, Me, Query, Reply, created, not_found, ok, sql, uuid};
use crate::error::{ApiError, ApiResult};
use crate::perm;
use crate::tasks::{absent, collapse_js_space, crash, is_int, js_trim, str_len, truthy};
use crate::validate::{Checker, js_slice};
use release_manifest::{CHUNK_SIZE, DEFAULT_LIMITS, DIFF_LIST_LIMIT, chunk_length, diff_manifests, is_hash, same_manifest, stored_files, validate_manifest_input};
use release_store::{chunk_exists, chunk_path, missing_chunks, write_chunk};

// ------------------------------------------------------------------ açılış işleri

/// onModuleInit: dizinleri hazırla, yarım yazımları süpür, GC'yi 30 sn sonra ve 6 saatte bir
/// çalıştır. main.rs'nin açılışta çağırması için `pub`; çağrılmazsa ilk sürüm isteğinde kendiliğinden başlar.
pub fn start(state: &AppState) {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }
    let _ = release_store::ensure_dirs().and_then(|_| release_store::sweep_tmp());
    let db = state.db.clone();
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;
        loop {
            release_store::gc_chunks(&db, true).await;
            tokio::time::sleep(std::time::Duration::from_secs(6 * 3600)).await;
        }
    });
}

// ------------------------------------------------------------------ doğrulama ve biçimler

/// Kategori (kanal) kullanıcı tanımlıdır: harf/rakamla başlar, en çok 24 karakter.
pub fn channel_ok(value: &str) -> bool {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^[\p{L}\p{N}][\p{L}\p{N} ._-]{0,23}$").unwrap()).is_match(value)
}

pub fn normalize_channel(value: &str) -> String {
    collapse_js_space(js_trim(value))
}

fn version_ok(value: &str) -> bool {
    (1..=40).contains(&value.len()) && value.bytes().all(|b| b.is_ascii_alphanumeric() || b".+-".contains(&b))
}

fn matches(v: Option<&Value>, f: fn(&str) -> bool) -> bool {
    v.and_then(Value::as_str).is_some_and(f)
}

fn is_bool(v: Option<&Value>) -> bool {
    v.is_some_and(Value::is_boolean)
}

/// `releaseSelect` + `view()`: totalSize sayıya çevrilip sona, ardından installCount.
const RELEASE_JSON: &str = concat!(
    r#"json_build_object('id', r.id, 'projectId', r."projectId", 'channel', r.channel, 'version', r.version, 'notes', r.notes, 'status', r.status,
    'executable', r.executable, 'launchArgs', r."launchArgs", 'platform', r.platform, 'overlay', r.overlay, 'fileCount', r."fileCount",
    'chunkCount', r."chunkCount", 'createdById', r."createdById", 'createdAt', "#,
    crate::iso!(r#"r."createdAt""#),
    ", 'updatedAt', ",
    crate::iso!(r#"r."updatedAt""#),
    ", 'publishedAt', ",
    crate::iso!(r#"r."publishedAt""#),
    ", 'archivedAt', ",
    crate::iso!(r#"r."archivedAt""#),
    ", 'liveAt', ",
    crate::iso!(r#"r."liveAt""#),
    r#", 'createdBy', (select json_build_object('id', u.id, 'username', u.username, 'displayName', u."displayName", 'avatarUrl', u."avatarUrl") from "User" u where u.id = r."createdById"),
    'totalSize', r."totalSize",
    'installCount', (select count(*) from "ReleaseInstall" i where i."releaseId" = r.id and i.status = 'INSTALLED'))"#
);

const AUTHOR_JSON: &str = r#"(select json_build_object('id', u.id, 'username', u.username, 'displayName', u."displayName", 'avatarUrl', u."avatarUrl") from "User" u where u.id = i."userId")"#;

#[derive(Clone, Copy)]
struct Perms {
    publish: bool,
    download: bool,
}

impl Perms {
    fn any(self) -> bool {
        self.publish || self.download
    }
}

/// Taslaklar yalnız yayınlayabilenlere görünür; yayınlanmış her kategori herkese açık.
fn visible(status: &str, perms: Perms) -> bool {
    status != "DRAFT" || perms.publish
}

async fn project(db: &sqlx::PgPool, organization_id: &str, project_id: &str, user_id: &str) -> ApiResult<(String, String)> {
    perm::require_project_access(db, organization_id, user_id, project_id).await?;
    let row: Option<(String, String)> = sqlx::query_as(r#"select id, name from "Project" where id = $1 and "organizationId" = $2 and "deletedAt" is null"#)
        .bind(project_id)
        .bind(organization_id)
        .fetch_optional(db)
        .await?;
    row.ok_or_else(|| not_found("PROJECT_NOT_FOUND", "Proje bulunamadı."))
}

async fn perms(db: &sqlx::PgPool, organization_id: &str, user_id: &str) -> ApiResult<Perms> {
    let eff = perm::effective(db, organization_id, user_id).await?;
    Ok(Perms { publish: eff.has("release.publish"), download: eff.has("release.download") })
}

fn release_missing() -> ApiError {
    not_found("RELEASE_NOT_FOUND", "Sürüm bulunamadı.")
}

async fn release_view(db: &sqlx::PgPool, project_id: &str, release_id: &str) -> ApiResult<Value> {
    let row: Option<Json<Value>> = sqlx::query_scalar(sql(format!(r#"select {RELEASE_JSON} from "Release" r where r.id = $1 and r."projectId" = $2"#)))
        .bind(release_id)
        .bind(project_id)
        .fetch_optional(db)
        .await?;
    row.map(|j| j.0).ok_or_else(release_missing)
}

fn status_of(row: &Value) -> &str {
    row.get("status").and_then(Value::as_str).unwrap_or_default()
}

/// JS nesne anahtar sırası: dizi indisi gibi görünen anahtarlar önce, sayısal sırayla.
fn js_key_order(map: Map<String, Value>) -> Map<String, Value> {
    let index = |k: &str| -> Option<u64> {
        if k == "0" || (!k.is_empty() && !k.starts_with('0') && k.bytes().all(|b| b.is_ascii_digit()) && k.len() <= 10) {
            k.parse::<u64>().ok().filter(|n| *n < 4_294_967_295)
        } else {
            None
        }
    };
    let mut numeric: Vec<(u64, String, Value)> = Vec::new();
    let mut rest: Vec<(String, Value)> = Vec::new();
    for (k, v) in map {
        match index(&k) {
            Some(n) => numeric.push((n, k, v)),
            None => rest.push((k, v)),
        }
    }
    numeric.sort_by_key(|(n, _, _)| *n);
    numeric.into_iter().map(|(_, k, v)| (k, v)).chain(rest).collect()
}

/// Uçların bir kısmı JSON dışı (boş gövde, akış) cevap verir.
fn respond(result: Result<Response, ApiError>) -> Response {
    result.unwrap_or_else(IntoResponse::into_response)
}

fn json_response(status: StatusCode, body: Value) -> Response {
    (status, axum::Json(body)).into_response()
}

fn empty_status(status: StatusCode) -> Response {
    status.into_response()
}

// ------------------------------------------------------------------ liste / ayrıntı

async fn list(State(s): State<AppState>, me: Me, Path((org, project_id)): Path<(String, String)>, q: Query) -> Reply {
    start(&s);
    project(&s.db, &org, &project_id, &me.id).await?;
    let perms = perms(&s.db, &org, &me.id).await?;
    if !perms.any() {
        return Err(not_found("PERMISSION_DENIED", "Kütüphaneyi görme yetkin yok."));
    }
    let channel = q.get("channel").filter(|c| !c.is_empty()).map(|c| normalize_channel(c));
    let rows: Vec<Json<Value>> = sqlx::query_scalar(sql(format!(
        r#"select {RELEASE_JSON} from "Release" r where r."projectId" = $1 and r.status <> 'ARCHIVED' and ($2::text is null or r.channel = $2)
           order by r."publishedAt" desc nulls first, r."createdAt" desc"#
    )))
    .bind(&project_id)
    .bind(&channel)
    .fetch_all(&s.db)
    .await?;
    let releases: Vec<Value> = rows.into_iter().map(|j| j.0).filter(|r| visible(status_of(r), perms)).collect();
    // Kategori başına canlı sürüm (yoksa en son yayın).
    let mut latest = Map::new();
    let chan = |r: &Value| r["channel"].as_str().unwrap_or_default().to_string();
    for row in &releases {
        if status_of(row) == "PUBLISHED" && !row["liveAt"].is_null() {
            latest.insert(chan(row), row.clone());
        }
    }
    for row in &releases {
        if status_of(row) == "PUBLISHED" && !latest.contains_key(&chan(row)) {
            latest.insert(chan(row), row.clone());
        }
    }
    let mut channels: Vec<String> = Vec::new();
    for row in releases.iter().filter(|r| status_of(r) == "PUBLISHED").chain(releases.iter()) {
        let c = chan(row);
        if !channels.contains(&c) {
            channels.push(c);
        }
    }
    ok(json!({
        "releases": releases,
        "latest": Value::Object(js_key_order(latest)),
        "channels": channels,
        "permissions": { "publish": perms.publish, "download": perms.download },
    }))
}

async fn latest(State(s): State<AppState>, me: Me, Path((org, project_id)): Path<(String, String)>, q: Query) -> Reply {
    start(&s);
    project(&s.db, &org, &project_id, &me.id).await?;
    let perms = perms(&s.db, &org, &me.id).await?;
    if !perms.any() {
        return ok(json!({ "release": null }));
    }
    let channel = q.get("channel").filter(|c| !c.is_empty()).map(|c| normalize_channel(c));
    let live: Option<Json<Value>> = sqlx::query_scalar(sql(format!(
        r#"select {RELEASE_JSON} from "Release" r where r."projectId" = $1 and r.status = 'PUBLISHED' and ($2::text is null or r.channel = $2)
           and r."liveAt" is not null order by r."liveAt" desc limit 1"#
    )))
    .bind(&project_id)
    .bind(&channel)
    .fetch_optional(&s.db)
    .await?;
    let row = match live {
        Some(row) => Some(row),
        None => {
            sqlx::query_scalar(sql(format!(
                r#"select {RELEASE_JSON} from "Release" r where r."projectId" = $1 and r.status = 'PUBLISHED' and ($2::text is null or r.channel = $2)
                   order by r."publishedAt" desc limit 1"#
            )))
            .bind(&project_id)
            .bind(&channel)
            .fetch_optional(&s.db)
            .await?
        }
    };
    ok(json!({ "release": row.map(|j| j.0) }))
}

async fn installs(State(s): State<AppState>, me: Me, Path((org, project_id)): Path<(String, String)>) -> Reply {
    start(&s);
    project(&s.db, &org, &project_id, &me.id).await?;
    perm::require(&s.db, &org, &me.id, "release.publish").await?;
    let rows: Vec<Json<Value>> = sqlx::query_scalar(sql(format!(
        r#"select json_build_object('deviceId', i."deviceId", 'releaseId', i."releaseId", 'channel', i.channel, 'version', i.version, 'status', i.status,
             'pinned', i.pinned, 'clientVersion', i."clientVersion", 'installedAt', {}, 'lastLaunchedAt', {}, 'playSeconds', i."playSeconds",
             'updatedAt', {}, 'user', {AUTHOR_JSON})
           from "ReleaseInstall" i where i."projectId" = $1 and i.status <> 'REMOVED' order by i."updatedAt" desc"#,
        crate::iso!(r#"i."installedAt""#),
        crate::iso!(r#"i."lastLaunchedAt""#),
        crate::iso!(r#"i."updatedAt""#),
    )))
    .bind(&project_id)
    .fetch_all(&s.db)
    .await?;
    ok(json!({ "installs": rows.into_iter().map(|j| j.0).collect::<Vec<_>>() }))
}

/// Parça indirme yetkisi: aynı kişi aynı projeden binlerce parça çeker; olumlu yanıtlar 5 dk tutulur.
fn chunk_access() -> &'static Mutex<HashMap<String, i64>> {
    static MAP: OnceLock<Mutex<HashMap<String, i64>>> = OnceLock::new();
    MAP.get_or_init(Default::default)
}

/// uploads.ts `rangeOf`: None → tüm dosya, Some(None) → 416, Some(Some((başla, bitir))).
fn range_of(value: Option<&str>, size: u64) -> Option<Option<(u64, u64)>> {
    let value = value.filter(|v| !v.is_empty())?;
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"^bytes=(\d*)-(\d*)$").unwrap());
    let Some(m) = re.captures(js_trim(value)) else { return Some(None) };
    let num = |i: usize| m.get(i).map(|g| g.as_str()).filter(|g| !g.is_empty()).map(|g| g.parse::<f64>().unwrap_or(f64::INFINITY));
    let size = size as f64;
    let (start, end) = match (num(1), num(2)) {
        (None, suffix) => {
            let Some(suffix) = suffix.filter(|s| s.is_finite() && *s > 0.0) else { return Some(None) };
            ((size - suffix).max(0.0), size - 1.0)
        }
        (Some(start), end) => {
            let end = end.filter(|e| e.is_finite()).unwrap_or(size - 1.0);
            if start < 0.0 || start >= size || end < start {
                return Some(None);
            }
            (start, end.min(size - 1.0))
        }
    };
    // Boş dosyada sonek aralığı: Node akışı geçersiz aralıkta düşer; parçalar hiç boş olmaz.
    if end < 0.0 {
        return Some(None);
    }
    Some(Some((start as u64, end as u64)))
}

/// Dosyanın [start, end] aralığını akıtan gövde.
pub async fn file_body(path: &std::path::Path, start: u64, len: u64) -> std::io::Result<RawBody> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    let mut file = tokio::fs::File::open(path).await?;
    if start > 0 {
        file.seek(std::io::SeekFrom::Start(start)).await?;
    }
    Ok(RawBody::from_stream(tokio_util::io::ReaderStream::with_capacity(file.take(len), 64 * 1024)))
}

fn set(headers: &mut HeaderMap, name: &'static str, value: String) {
    if let Ok(v) = HeaderValue::from_str(&value) {
        headers.insert(name, v);
    }
}

/// Parça indirme: bu projede yayınlanmış (ya da kendi taslağı olan) bir sürüm başvuruyorsa verilir.
async fn chunk(State(s): State<AppState>, me: Me, Path((org, project_id, hash)): Path<(String, String, String)>, headers: HeaderMap) -> Response {
    respond(chunk_inner(s, me, org, project_id, hash, headers).await)
}

async fn chunk_inner(s: AppState, me: Me, org: String, project_id: String, hash: String, headers: HeaderMap) -> Result<Response, ApiError> {
    start(&s);
    if !is_hash(&hash) {
        return Ok(empty_status(StatusCode::NOT_FOUND));
    }
    let key = format!("{project_id}|{}|{hash}", me.id);
    let now = chrono::Utc::now().timestamp_millis();
    let until = chunk_access().lock().unwrap().get(&key).copied().unwrap_or(0);
    if until < now {
        project(&s.db, &org, &project_id, &me.id).await?;
        let perms = perms(&s.db, &org, &me.id).await?;
        if !perms.any() {
            return Ok(json_response(StatusCode::FORBIDDEN, json!({ "code": "PERMISSION_DENIED", "message": "Sürüm indirme yetkin yok." })));
        }
        let hit: Option<String> = sqlx::query_scalar(
            r#"select f.hash from "ReleaseChunkRef" f join "Release" r on r.id = f."releaseId"
               where f.hash = $1 and r."projectId" = $2 and (r.status = 'PUBLISHED' or r."createdById" = $3) limit 1"#,
        )
        .bind(&hash)
        .bind(&project_id)
        .bind(&me.id)
        .fetch_optional(&s.db)
        .await?;
        if hit.is_none() {
            return Ok(empty_status(StatusCode::NOT_FOUND));
        }
        let mut map = chunk_access().lock().unwrap();
        if map.len() > 50_000 {
            map.clear();
        }
        map.insert(key, chrono::Utc::now().timestamp_millis() + 5 * 60_000);
    }
    let path = chunk_path(&hash);
    let Ok(info) = tokio::fs::metadata(&path).await else { return Ok(empty_status(StatusCode::NOT_FOUND)) };
    let size = info.len();
    let range = range_of(headers.get(header::RANGE).and_then(|v| v.to_str().ok()), size);
    let mut out = HeaderMap::new();
    set(&mut out, "accept-ranges", "bytes".into());
    set(&mut out, "content-type", "application/octet-stream".into());
    set(&mut out, "x-content-type-options", "nosniff".into());
    set(&mut out, "cache-control", "private, max-age=31536000, immutable".into());
    match range {
        Some(None) => {
            set(&mut out, "content-range", format!("bytes */{size}"));
            Ok((StatusCode::RANGE_NOT_SATISFIABLE, out).into_response())
        }
        Some(Some((start, end))) => {
            set(&mut out, "content-range", format!("bytes {start}-{end}/{size}"));
            set(&mut out, "content-length", (end - start + 1).to_string());
            let body = file_body(&path, start, end - start + 1).await.map_err(ApiError::internal)?;
            Ok((StatusCode::PARTIAL_CONTENT, out, body).into_response())
        }
        None => {
            set(&mut out, "content-length", size.to_string());
            let body = file_body(&path, 0, size).await.map_err(ApiError::internal)?;
            Ok((StatusCode::OK, out, body).into_response())
        }
    }
}

async fn detail(State(s): State<AppState>, me: Me, Path((org, project_id, release_id)): Path<(String, String, String)>) -> Reply {
    start(&s);
    project(&s.db, &org, &project_id, &me.id).await?;
    let perms = perms(&s.db, &org, &me.id).await?;
    let row = release_view(&s.db, &project_id, &release_id).await?;
    if !visible(status_of(&row), perms) || !perms.any() {
        return Err(release_missing());
    }
    ok(row)
}

async fn manifest(State(s): State<AppState>, me: Me, Path((org, project_id, release_id)): Path<(String, String, String)>) -> Response {
    respond(manifest_inner(s, me, org, project_id, release_id).await)
}

async fn manifest_inner(s: AppState, me: Me, org: String, project_id: String, release_id: String) -> Result<Response, ApiError> {
    start(&s);
    project(&s.db, &org, &project_id, &me.id).await?;
    let perms = perms(&s.db, &org, &me.id).await?;
    let row: Option<(String, String, String)> = sqlx::query_as(r#"select status, "createdById", manifest::text from "Release" where id = $1 and "projectId" = $2"#)
        .bind(&release_id)
        .bind(&project_id)
        .fetch_optional(&s.db)
        .await?;
    let missing = || Ok(json_response(StatusCode::NOT_FOUND, json!({ "code": "RELEASE_NOT_FOUND", "message": "Sürüm bulunamadı." })));
    let Some((status, created_by, text)) = row else { return missing() };
    if !visible(&status, perms) || !perms.any() {
        return missing();
    }
    if status == "DRAFT" && created_by != me.id && !perms.publish {
        return missing();
    }
    // jsonb metni boşluklu; Express res.json sıkışık yazar.
    let manifest: Value = serde_json::from_str(&text).map_err(ApiError::internal)?;
    let mut response = json_response(StatusCode::OK, manifest);
    set(response.headers_mut(), "cache-control", "private, max-age=31536000, immutable".into());
    Ok(response)
}

/// Önceki paketle fark: `against` verilmezse aynı kategoride bir önceki yayın, yoksa
/// projedeki son yayın temel alınır.
async fn diff(State(s): State<AppState>, me: Me, Path((org, project_id, release_id)): Path<(String, String, String)>, q: Query) -> Reply {
    start(&s);
    project(&s.db, &org, &project_id, &me.id).await?;
    let perms = perms(&s.db, &org, &me.id).await?;
    if !perms.any() {
        return Err(release_missing());
    }
    let row: Option<(String, String, String, Option<NaiveDateTime>, Json<Value>)> =
        sqlx::query_as(r#"select id, status, channel, "publishedAt", manifest from "Release" where id = $1 and "projectId" = $2"#)
            .bind(&release_id)
            .bind(&project_id)
            .fetch_optional(&s.db)
            .await?;
    let Some((id, status, channel, published_at, manifest)) = row else { return Err(release_missing()) };
    if !visible(&status, perms) {
        return Err(release_missing());
    }
    type Base = (String, String, String, String, Option<String>, Json<Value>);
    let base_cols = concat!(r#"id, version, channel, status, "#, crate::iso!(r#""publishedAt""#), ", manifest");
    let against = q.get("against").filter(|a| !a.is_empty());
    let mut base: Option<Base> = match against {
        Some(against) => {
            sqlx::query_as(sql(format!(r#"select {base_cols} from "Release" where id = $1 and "projectId" = $2 and status <> 'ARCHIVED'"#)))
                .bind(against)
                .bind(&project_id)
                .fetch_optional(&s.db)
                .await?
        }
        None => {
            let previous = |same_channel: bool| {
                sql(format!(
                    r#"select {base_cols} from "Release" where "projectId" = $1 and status = 'PUBLISHED' {} and id <> $3 and ($4::timestamp is null or "publishedAt" < $4)
                       order by "publishedAt" desc limit 1"#,
                    if same_channel { "and channel = $2" } else { "and $2::text is not null" }
                ))
            };
            let first: Option<Base> = sqlx::query_as(previous(true)).bind(&project_id).bind(&channel).bind(&id).bind(published_at).fetch_optional(&s.db).await?;
            match first {
                Some(b) => Some(b),
                None => sqlx::query_as(previous(false)).bind(&project_id).bind(&channel).bind(&id).bind(published_at).fetch_optional(&s.db).await?,
            }
        }
    };
    if base.as_ref().is_some_and(|b| b.0 == id || !visible(&b.3, perms)) {
        base = None;
    }
    let next = stored_files(&manifest.0).ok_or_else(|| crash("next.files"))?;
    let previous = base.as_ref().map(|b| stored_files(&b.5.0).unwrap_or_default());
    let result = diff_manifests(previous.as_deref(), &next, DIFF_LIST_LIMIT);
    let mut out = Map::new();
    out.insert("against".into(), base.map_or(Value::Null, |b| json!({ "id": b.0, "version": b.1, "channel": b.2, "publishedAt": b.4 })));
    if let Value::Object(fields) = result {
        out.extend(fields);
    }
    ok(Value::Object(out))
}

// ------------------------------------------------------------------ yükleme

fn check_create(body: &Map<String, Value>) -> ApiResult<()> {
    let mut c = Checker::new(body);
    let g = |k: &str| body.get(k);
    c.field("channel", &[(matches(g("channel"), channel_ok), None)]);
    c.field("version", &[(matches(g("version"), version_ok), None)]);
    c.field("notes", &[(absent(g("notes")) || str_len(g("notes"), 0, 4000), None)]);
    c.field("executable", &[(g("executable").is_some_and(Value::is_string), None)]);
    c.field("launchArgs", &[(absent(g("launchArgs")) || str_len(g("launchArgs"), 0, 500), None)]);
    c.field("overlay", &[(absent(g("overlay")) || is_bool(g("overlay")), None)]);
    c.field("files", &[(g("files").is_some_and(Value::is_array), None)]);
    c.finish()
}

async fn create(State(s): State<AppState>, me: Me, Path((org, project_id)): Path<(String, String)>, Body(body): Body) -> Reply {
    check_create(&body)?;
    start(&s);
    project(&s.db, &org, &project_id, &me.id).await?;
    perm::require(&s.db, &org, &me.id, "release.publish").await?;
    let validated = validate_manifest_input(body.get("executable"), body.get("launchArgs"), None, body.get("files"), &DEFAULT_LIMITS)
        .map_err(|(code, message)| ApiError::bad_request(code, &message))?;
    let (manifest, unique) = (validated.manifest, validated.unique_hashes);
    let channel = normalize_channel(body["channel"].as_str().unwrap_or_default());
    let version = body["version"].as_str().unwrap_or_default().to_string();
    let existing: Option<(String, String, String, Json<Value>)> =
        sqlx::query_as(r#"select id, status, "createdById", manifest from "Release" where "projectId" = $1 and channel = $2 and version = $3"#)
            .bind(&project_id)
            .bind(&channel)
            .bind(&version)
            .fetch_optional(&s.db)
            .await?;
    if let Some((existing_id, status, created_by, stored)) = existing {
        // Aynı kişi aynı taslağı yeniden gönderiyorsa yüklemeye kaldığı yerden devam eder.
        if status == "DRAFT" && created_by == me.id && same_manifest(&stored.0, &manifest) {
            let row = release_view(&s.db, &project_id, &existing_id).await?;
            return created(json!({ "release": row, "missing": missing_chunks(&unique).await, "resumed": true }));
        }
        return Err(ApiError::conflict("RELEASE_VERSION_EXISTS", &format!("{channel} kategorisinde {version} sürümü zaten var. Yeni bir sürüm numarası ver.")).field("version"));
    }
    let mut sizes: HashMap<&str, i64> = HashMap::new();
    for file in &manifest.files {
        for (index, hash) in file.chunks.iter().enumerate() {
            sizes.entry(hash.as_str()).or_insert_with(|| chunk_length(file, index));
        }
    }
    let notes = body.get("notes").and_then(Value::as_str).map(js_trim).filter(|n| !n.is_empty()).map(str::to_string);
    let overlay = body.get("overlay") != Some(&Value::Bool(false));
    let now = crate::ctx::now();
    let release_id = uuid();
    let mut tx = s.db.begin().await?;
    sqlx::query(
        r#"insert into "Release" (id, "projectId", channel, version, notes, status, executable, "launchArgs", platform, overlay, "totalSize", "fileCount",
             "chunkCount", manifest, "createdById", "createdAt", "updatedAt")
           values ($1, $2, $3, $4, $5, 'DRAFT', $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $15)"#,
    )
    .bind(&release_id)
    .bind(&project_id)
    .bind(&channel)
    .bind(&version)
    .bind(&notes)
    .bind(&manifest.executable)
    .bind(&manifest.launch_args)
    .bind(&manifest.platform)
    .bind(overlay)
    .bind(manifest.total_size)
    .bind(manifest.file_count as i32)
    .bind(manifest.chunk_count as i32)
    .bind(Json(manifest.to_json()))
    .bind(&me.id)
    .bind(now)
    .execute(&mut *tx)
    .await?;
    // Parça satırları önceden "bekleniyor" olarak açılır: ref'ler sayesinde GC yükleme sürerken
    // parçaları silmez. Diskteki varlık "yüklendi" demektir.
    for batch in unique.chunks(5000) {
        let lens: Vec<i32> = batch.iter().map(|h| sizes.get(h.as_str()).copied().filter(|n| *n != 0).unwrap_or(CHUNK_SIZE) as i32).collect();
        sqlx::query(r#"insert into "ReleaseChunk" (hash, size, "createdAt") select h, n, $3 from unnest($1::text[], $2::int[]) as t(h, n) on conflict do nothing"#)
            .bind(batch)
            .bind(&lens)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        sqlx::query(r#"insert into "ReleaseChunkRef" ("releaseId", hash) select $1, h from unnest($2::text[]) as t(h) on conflict do nothing"#)
            .bind(&release_id)
            .bind(batch)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    let row = release_view(&s.db, &project_id, &release_id).await?;
    created(json!({ "release": row, "missing": missing_chunks(&unique).await, "resumed": false }))
}

async fn missing(State(s): State<AppState>, me: Me, Path((org, project_id, release_id)): Path<(String, String, String)>, Body(body): Body) -> Reply {
    let mut c = Checker::new(&body);
    c.field("hashes", &[(absent(body.get("hashes")) || body.get("hashes").is_some_and(Value::is_array), None)]);
    c.finish()?;
    start(&s);
    project(&s.db, &org, &project_id, &me.id).await?;
    perm::require(&s.db, &org, &me.id, "release.publish").await?;
    let hit: Option<String> = sqlx::query_scalar(r#"select id from "Release" where id = $1 and "projectId" = $2"#)
        .bind(&release_id)
        .bind(&project_id)
        .fetch_optional(&s.db)
        .await?;
    if hit.is_none() {
        return Err(release_missing());
    }
    let all: Vec<String> = sqlx::query_scalar(r#"select hash from "ReleaseChunkRef" where "releaseId" = $1 order by ctid"#).bind(&release_id).fetch_all(&s.db).await?;
    let wanted: Vec<String> = match body.get("hashes") {
        Some(Value::Array(list)) if !list.is_empty() => all.iter().filter(|h| list.iter().any(|v| v.as_str() == Some(h.as_str()))).cloned().collect(),
        _ => all.clone(),
    };
    created(json!({ "missing": missing_chunks(&wanted).await, "total": all.len() }))
}

/// Gövde ham ikili veri; doğrudan diske akıtılır.
async fn put_chunk(State(s): State<AppState>, me: Me, Path((org, project_id, release_id, hash)): Path<(String, String, String, String)>, request: Request) -> Response {
    respond(put_chunk_inner(s, me, org, project_id, release_id, hash, request).await.map(|v| json_response(StatusCode::OK, v)))
}

async fn put_chunk_inner(s: AppState, me: Me, org: String, project_id: String, release_id: String, hash: String, request: Request) -> Result<Value, ApiError> {
    start(&s);
    if !is_hash(&hash) {
        return Err(ApiError::bad_request("CHUNK_HASH", "Parça özeti geçersiz."));
    }
    let headers = request.headers();
    let content_type = headers.get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or_default();
    if !content_type.starts_with("application/octet-stream") {
        return Err(ApiError::bad_request("CHUNK_CONTENT_TYPE", "Parça ham ikili veri olarak gönderilmeli."));
    }
    let declared = headers.get(header::CONTENT_LENGTH).and_then(|v| v.to_str().ok()).and_then(crate::validate::js_number);
    if declared.is_some_and(|d| d.is_finite() && d > CHUNK_SIZE as f64) {
        return Err(ApiError::new(StatusCode::PAYLOAD_TOO_LARGE, "CHUNK_TOO_LARGE", "Parça 4 MB sınırını aşıyor."));
    }
    project(&s.db, &org, &project_id, &me.id).await?;
    perm::require(&s.db, &org, &me.id, "release.publish").await?;
    let status: Option<String> = sqlx::query_scalar(r#"select status from "Release" where id = $1 and "projectId" = $2"#)
        .bind(&release_id)
        .bind(&project_id)
        .fetch_optional(&s.db)
        .await?;
    let Some(status) = status else { return Err(release_missing()) };
    if status != "DRAFT" {
        return Err(ApiError::conflict("RELEASE_NOT_DRAFT", "Yayınlanmış sürüme parça eklenemez."));
    }
    let hit: Option<String> = sqlx::query_scalar(r#"select hash from "ReleaseChunkRef" where "releaseId" = $1 and hash = $2"#)
        .bind(&release_id)
        .bind(&hash)
        .fetch_optional(&s.db)
        .await?;
    if hit.is_none() {
        return Err(not_found("CHUNK_NOT_IN_MANIFEST", "Bu parça sürümün manifestinde yok."));
    }
    if chunk_exists(&hash).await {
        touch(&s, &release_id);
        return Ok(json!({ "ok": true, "hash": hash, "existed": true }));
    }
    let size = write_chunk(request.into_body(), &hash).await?;
    touch(&s, &release_id);
    Ok(json!({ "ok": true, "hash": hash, "size": size }))
}

/// Yükleme sürerken taslağın updatedAt'i en çok dakikada bir tazelenir (GC terk edilmiş saymasın).
fn touch(s: &AppState, release_id: &str) {
    static TOUCHED: OnceLock<Mutex<HashMap<String, i64>>> = OnceLock::new();
    let now = chrono::Utc::now().timestamp_millis();
    {
        let mut map = TOUCHED.get_or_init(Default::default).lock().unwrap();
        if now - map.get(release_id).copied().unwrap_or(0) < 60_000 {
            return;
        }
        map.insert(release_id.to_string(), now);
    }
    let db = s.db.clone();
    let id = release_id.to_string();
    tokio::spawn(async move {
        let _ = sqlx::query(r#"update "Release" set "updatedAt" = $2 where id = $1 and status = 'DRAFT'"#).bind(&id).bind(crate::ctx::now()).execute(&db).await;
    });
}

// ------------------------------------------------------------------ yayınlama

async fn publish(State(s): State<AppState>, me: Me, Path((org, project_id, release_id)): Path<(String, String, String)>) -> Reply {
    start(&s);
    let (_, project_name) = project(&s.db, &org, &project_id, &me.id).await?;
    perm::require(&s.db, &org, &me.id, "release.publish").await?;
    let refs: Vec<String> = sqlx::query_scalar(
        r#"select f.hash from "ReleaseChunkRef" f join "Release" r on r.id = f."releaseId" where f."releaseId" = $1 and r."projectId" = $2"#,
    )
    .bind(&release_id)
    .bind(&project_id)
    .fetch_all(&s.db)
    .await?;
    let missing = missing_chunks(&refs).await;
    if !missing.is_empty() {
        let mut error = ApiError::conflict("RELEASE_INCOMPLETE", &format!("{} parça henüz yüklenmedi.", missing.len()));
        error.body.as_object_mut().unwrap().insert("missing".into(), json!(missing.len()));
        return Err(error);
    }
    let now = crate::ctx::now();
    let updated = sqlx::query(r#"update "Release" set status = 'PUBLISHED', "publishedAt" = $3, "updatedAt" = $3 where id = $1 and "projectId" = $2 and status = 'DRAFT'"#)
        .bind(&release_id)
        .bind(&project_id)
        .bind(now)
        .execute(&s.db)
        .await?
        .rows_affected();
    // Yayınlanan sürümü olan proje kütüphanede uygulama olarak görünür.
    if updated > 0 {
        sqlx::query(r#"update "Project" set "libraryEnabled" = true, "updatedAt" = $2 where id = $1 and "libraryEnabled" = false"#)
            .bind(&project_id)
            .bind(now)
            .execute(&s.db)
            .await?;
    }
    if updated == 0 {
        return Err(ApiError::conflict("RELEASE_NOT_DRAFT", "Sürüm zaten yayınlanmış."));
    }
    let mut row = release_view(&s.db, &project_id, &release_id).await?;
    let channel = row["channel"].as_str().unwrap_or_default().to_string();
    // Kategorinin canlı sürümü yoksa ilk yayın kendiliğinden canlıya alınır.
    let has_live: Option<String> = sqlx::query_scalar(r#"select id from "Release" where "projectId" = $1 and channel = $2 and status = 'PUBLISHED' and "liveAt" is not null limit 1"#)
        .bind(&project_id)
        .bind(&channel)
        .fetch_optional(&s.db)
        .await?;
    crate::realtime::emit_project(&s, &org, &project_id, "release:published", &json!({ "projectId": project_id, "projectName": project_name, "release": row })).await;
    let version = row["version"].as_str().unwrap_or_default().to_string();
    if has_live.is_none() {
        row = make_live(&s, &org, &project_id, &project_name, &release_id, &me.id).await?;
    } else {
        notify(&s, &org, &project_id, &me.id, &format!("{project_name} {version} ({channel}) yayınlandı")).await;
    }
    let notes = row["notes"].as_str().filter(|n| !n.is_empty()).map(|n| format!("\n\n{n}")).unwrap_or_default();
    let state = if has_live.is_some() { "yayınlandı" } else { "yayınlandı ve canlıya alındı" };
    let version = row["version"].as_str().unwrap_or_default().to_string();
    let channel = row["channel"].as_str().unwrap_or_default().to_string();
    release_post(&s, &org, &project_id, &me.id, &release_id, &format!("🚀 **{version}** · {channel} {state}{notes}")).await;
    created(row)
}

/// Sürüm notları Kütüphane aktivitesine kendiliğinden düşer.
async fn release_post(s: &AppState, org: &str, project_id: &str, author: &str, release_id: &str, content: &str) {
    let now = crate::ctx::now();
    let saved = sqlx::query(
        r#"insert into "ProjectPost" (id, "projectId", "authorId", content, kind, "releaseId", "createdAt", "updatedAt") values ($1, $2, $3, $4, 'RELEASE', $5, $6, $6)"#,
    )
    .bind(uuid())
    .bind(project_id)
    .bind(author)
    .bind(js_slice(content, 4000))
    .bind(release_id)
    .bind(now)
    .execute(&s.db)
    .await;
    if saved.is_ok() {
        crate::realtime::emit_project(s, org, project_id, "hub:changed", &json!({ "projectId": project_id, "what": "posts" })).await;
    }
}

/// Canlı sürüm: kategoride herkese giden (otomatik güncellenen) sürüm. Eskiye dönüş de aynı yoldan.
async fn live(State(s): State<AppState>, me: Me, Path((org, project_id, release_id)): Path<(String, String, String)>) -> Reply {
    start(&s);
    let (_, project_name) = project(&s.db, &org, &project_id, &me.id).await?;
    perm::require(&s.db, &org, &me.id, "release.publish").await?;
    let row: Option<(String, Option<NaiveDateTime>)> = sqlx::query_as(r#"select status, "liveAt" from "Release" where id = $1 and "projectId" = $2"#)
        .bind(&release_id)
        .bind(&project_id)
        .fetch_optional(&s.db)
        .await?;
    let Some((status, live_at)) = row else { return Err(release_missing()) };
    if status != "PUBLISHED" {
        return Err(ApiError::conflict("RELEASE_NOT_PUBLISHED", "Önce sürümü yayınla."));
    }
    if live_at.is_some() {
        return created(release_view(&s.db, &project_id, &release_id).await?);
    }
    let made = make_live(&s, &org, &project_id, &project_name, &release_id, &me.id).await?;
    let version = made["version"].as_str().unwrap_or_default();
    let channel = made["channel"].as_str().unwrap_or_default();
    release_post(&s, &org, &project_id, &me.id, &release_id, &format!("📡 **{version}** · {channel} canlıya alındı — kurulu oyunlar bu sürüme güncelleniyor.")).await;
    created(made)
}

async fn make_live(s: &AppState, org: &str, project_id: &str, project_name: &str, release_id: &str, actor: &str) -> ApiResult<Value> {
    let channel: Option<String> = sqlx::query_scalar(r#"select channel from "Release" where id = $1 and "projectId" = $2"#)
        .bind(release_id)
        .bind(project_id)
        .fetch_optional(&s.db)
        .await?;
    let Some(channel) = channel else { return Err(release_missing()) };
    let now = crate::ctx::now();
    let mut tx = s.db.begin().await?;
    sqlx::query(r#"update "Release" set "liveAt" = null, "updatedAt" = $3 where "projectId" = $1 and channel = $2 and "liveAt" is not null"#)
        .bind(project_id)
        .bind(&channel)
        .bind(now)
        .execute(&mut *tx)
        .await?;
    let n = sqlx::query(r#"update "Release" set "liveAt" = $2, "updatedAt" = $2 where id = $1"#).bind(release_id).bind(now).execute(&mut *tx).await?;
    if n.rows_affected() == 0 {
        return Err(crash("release update"));
    }
    tx.commit().await?;
    let row = release_view(&s.db, project_id, release_id).await?;
    crate::realtime::emit_project(s, org, project_id, "release:live", &json!({ "projectId": project_id, "projectName": project_name, "release": row })).await;
    let version = row["version"].as_str().unwrap_or_default();
    notify(s, org, project_id, actor, &format!("{project_name} {version} ({}) canlıya alındı", row["channel"].as_str().unwrap_or_default())).await;
    Ok(row)
}

/// Projeyi görebilen herkese zil kaydı düşer (yapan hariç). Hata yutulur.
async fn notify(s: &AppState, org: &str, project_id: &str, actor: &str, preview: &str) {
    let Ok(members) = sqlx::query_scalar::<_, String>(r#"select "userId" from "OrganizationMember" where "organizationId" = $1"#).bind(org).fetch_all(&s.db).await else {
        return;
    };
    let Ok(audience) = perm::project_audience(&s.db, org, project_id, &members).await else { return };
    let rows = audience
        .into_iter()
        .map(|user_id| ActivityInput {
            organization_id: org.into(),
            user_id,
            actor_id: actor.into(),
            kind: "RELEASE",
            preview: preview.into(),
            project_id: Some(project_id.into()),
            ..Default::default()
        })
        .collect();
    activity::record(s, rows).await;
}

// ------------------------------------------------------------------ düzenleme / silme

/// Taslakta kategori ve sürüm numarası da değişir; yayınlanmış sürümde yalnız notlar,
/// başlatma argümanları ve overlay.
async fn update(State(s): State<AppState>, me: Me, Path((org, project_id, release_id)): Path<(String, String, String)>, Body(body): Body) -> Reply {
    let mut c = Checker::new(&body);
    let g = |k: &str| body.get(k);
    c.field("channel", &[(absent(g("channel")) || matches(g("channel"), channel_ok), None)]);
    c.field("version", &[(absent(g("version")) || matches(g("version"), version_ok), None)]);
    c.field("notes", &[(absent(g("notes")) || str_len(g("notes"), 0, 4000), None)]);
    c.field("launchArgs", &[(absent(g("launchArgs")) || str_len(g("launchArgs"), 0, 500), None)]);
    c.field("overlay", &[(absent(g("overlay")) || is_bool(g("overlay")), None)]);
    c.finish()?;
    start(&s);
    project(&s.db, &org, &project_id, &me.id).await?;
    perm::require(&s.db, &org, &me.id, "release.publish").await?;
    let current: Option<(String, String, String)> = sqlx::query_as(r#"select status, channel, version from "Release" where id = $1 and "projectId" = $2"#)
        .bind(&release_id)
        .bind(&project_id)
        .fetch_optional(&s.db)
        .await?;
    let Some((status, current_channel, current_version)) = current else { return Err(release_missing()) };
    // null alanlar TS'te `.trim()` ya da zorunlu sütun yüzünden 500 verir.
    let notes = match g("notes") {
        None => None,
        Some(Value::String(n)) => Some(Some(js_trim(n).to_string()).filter(|n| !n.is_empty())),
        Some(_) => return Err(crash("notes.trim")),
    };
    let overlay = match g("overlay") {
        None => None,
        Some(Value::Bool(b)) => Some(*b),
        Some(_) => return Err(crash("overlay null")),
    };
    let launch_args = match g("launchArgs") {
        None => None,
        Some(Value::String(a)) => Some(Some(js_trim(a).to_string()).filter(|a| !a.is_empty())),
        Some(_) => return Err(crash("launchArgs.trim")),
    };
    let next_channel = match g("channel") {
        None => current_channel.clone(),
        Some(Value::String(ch)) => normalize_channel(ch),
        Some(_) => return Err(crash("channel.trim")),
    };
    let next_version = match g("version") {
        None => Some(current_version.clone()),
        Some(v) => v.as_str().map(str::to_string),
    };
    let rename = next_channel != current_channel || next_version.as_deref() != Some(current_version.as_str());
    if rename {
        if status != "DRAFT" {
            return Err(ApiError::conflict("RELEASE_NOT_DRAFT", "Yayınlanmış sürümün numarası ve kategorisi değiştirilemez."));
        }
        let Some(next_version) = &next_version else { return Err(crash("version null")) };
        let taken: Option<String> = sqlx::query_scalar(r#"select id from "Release" where "projectId" = $1 and channel = $2 and version = $3"#)
            .bind(&project_id)
            .bind(&next_channel)
            .bind(next_version)
            .fetch_optional(&s.db)
            .await?;
        if taken.is_some() {
            return Err(ApiError::conflict("RELEASE_VERSION_EXISTS", "Bu kategoride bu sürüm numarası kullanılıyor.").field("version"));
        }
    }
    sqlx::query(
        r#"update "Release" set
             notes = case when $2 then $3 else notes end,
             overlay = case when $4 then $5 else overlay end,
             "launchArgs" = case when $6 then $7 else "launchArgs" end,
             manifest = case when not $6 then manifest when $7::text is null then manifest - 'launchArgs' else manifest || jsonb_build_object('launchArgs', $7::text) end,
             channel = case when $8 then $9 else channel end,
             version = case when $8 then $10 else version end,
             "updatedAt" = $11
           where id = $1"#,
    )
    .bind(&release_id)
    .bind(notes.is_some())
    .bind(notes.flatten())
    .bind(overlay.is_some())
    .bind(overlay)
    .bind(launch_args.is_some())
    .bind(launch_args.flatten())
    .bind(rename)
    .bind(&next_channel)
    .bind(&next_version)
    .bind(crate::ctx::now())
    .execute(&s.db)
    .await?;
    let row = release_view(&s.db, &project_id, &release_id).await?;
    if status_of(&row) == "PUBLISHED" {
        crate::realtime::emit_project(&s, &org, &project_id, "release:updated", &json!({ "projectId": project_id, "release": row })).await;
    }
    ok(row)
}

async fn remove(State(s): State<AppState>, me: Me, Path((org, project_id, release_id)): Path<(String, String, String)>) -> Reply {
    start(&s);
    project(&s.db, &org, &project_id, &me.id).await?;
    perm::require(&s.db, &org, &me.id, "release.publish").await?;
    let row: Option<(String, String, String, Option<NaiveDateTime>)> =
        sqlx::query_as(r#"select channel, version, status, "liveAt" from "Release" where id = $1 and "projectId" = $2"#)
            .bind(&release_id)
            .bind(&project_id)
            .fetch_optional(&s.db)
            .await?;
    let Some((channel, version, status, live_at)) = row else { return Err(release_missing()) };
    sqlx::query(r#"delete from "Release" where id = $1"#).bind(&release_id).execute(&s.db).await?;
    if status == "PUBLISHED" {
        crate::realtime::emit_project(&s, &org, &project_id, "release:deleted", &json!({ "projectId": project_id, "releaseId": release_id, "channel": channel, "version": version }))
            .await;
    }
    // Canlı sürüm silindiyse kategorideki en yeni yayın canlıya geçer.
    if live_at.is_some() {
        let next: Option<String> = sqlx::query_scalar(r#"select id from "Release" where "projectId" = $1 and channel = $2 and status = 'PUBLISHED' order by "publishedAt" desc limit 1"#)
            .bind(&project_id)
            .bind(&channel)
            .fetch_optional(&s.db)
            .await?;
        if let Some(next) = next {
            let name: Option<String> = sqlx::query_scalar(r#"select name from "Project" where id = $1"#).bind(&project_id).fetch_optional(&s.db).await?;
            if let Some(name) = name {
                make_live(&s, &org, &project_id, &name, &next, &me.id).await?;
            }
        }
    }
    release_store::schedule_gc(s.db.clone());
    ok(json!({ "ok": true }))
}

// ------------------------------------------------------------------ kurulum kayıtları

async fn installed(State(s): State<AppState>, me: Me, Path((org, project_id, release_id)): Path<(String, String, String)>, Body(body): Body) -> Reply {
    let mut c = Checker::new(&body);
    let g = |k: &str| body.get(k);
    let device_ok = g("deviceId").and_then(Value::as_str).is_some_and(|d| str_len(g("deviceId"), 1, 64) && !d.is_empty() && d.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'));
    c.field("deviceId", &[(device_ok, None)]);
    c.field("status", &[(crate::tasks::is_in(g("status"), &["INSTALLED", "UPDATING", "FAILED", "REMOVED"]), None)]);
    c.field("pinned", &[(absent(g("pinned")) || is_bool(g("pinned")), None)]);
    c.field("clientVersion", &[(absent(g("clientVersion")) || str_len(g("clientVersion"), 0, 40), None)]);
    c.field("launched", &[(absent(g("launched")) || is_bool(g("launched")), None)]);
    let seconds_ok = is_int(g("playSeconds")) && g("playSeconds").and_then(Value::as_f64).is_some_and(|n| (0.0..=(7 * 24 * 3600) as f64).contains(&n));
    c.field("playSeconds", &[(absent(g("playSeconds")) || seconds_ok, None)]);
    c.finish()?;
    start(&s);
    project(&s.db, &org, &project_id, &me.id).await?;
    let perms = perms(&s.db, &org, &me.id).await?;
    if !perms.any() {
        return Err(not_found("PERMISSION_DENIED", "Sürüm indirme yetkin yok."));
    }
    let row: Option<(String, String, String)> = sqlx::query_as(r#"select id, channel, version from "Release" where id = $1 and "projectId" = $2"#)
        .bind(&release_id)
        .bind(&project_id)
        .fetch_optional(&s.db)
        .await?;
    let Some((id, channel, version)) = row else { return Err(release_missing()) };
    let pinned = match g("pinned") {
        None => None,
        Some(Value::Bool(b)) => Some(*b),
        Some(_) => return Err(crash("pinned null")),
    };
    let status = g("status").and_then(Value::as_str).unwrap_or_default().to_string();
    let client_version = g("clientVersion").and_then(Value::as_str).filter(|v| !v.is_empty()).map(str::to_string);
    let launched = truthy(g("launched"));
    let played = g("playSeconds").and_then(Value::as_f64).unwrap_or(0.0) as i32;
    let now = crate::ctx::now();
    let saved: Json<Value> = sqlx::query_scalar(sql(format!(
        r#"insert into "ReleaseInstall" as i (id, "projectId", "userId", "deviceId", pinned, "installedAt", "playSeconds", "releaseId", channel, version, status,
             "clientVersion", "lastLaunchedAt", "updatedAt")
           values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, case when $13 then $6 else null end, $6)
           on conflict ("projectId", "userId", "deviceId") do update set
             "releaseId" = excluded."releaseId", channel = excluded.channel, version = excluded.version, status = excluded.status,
             pinned = case when $14 then excluded.pinned else i.pinned end,
             "clientVersion" = case when $12::text is not null then excluded."clientVersion" else i."clientVersion" end,
             "lastLaunchedAt" = case when $13 then excluded."lastLaunchedAt" else i."lastLaunchedAt" end,
             "installedAt" = case when $11 = 'INSTALLED' then excluded."installedAt" else i."installedAt" end,
             "playSeconds" = i."playSeconds" + $7,
             "updatedAt" = excluded."updatedAt"
           returning json_build_object('deviceId', i."deviceId", 'releaseId', i."releaseId", 'channel', i.channel, 'version', i.version, 'status', i.status,
             'pinned', i.pinned, 'installedAt', {}, 'lastLaunchedAt', {}, 'playSeconds', i."playSeconds")"#,
        crate::iso!(r#"i."installedAt""#),
        crate::iso!(r#"i."lastLaunchedAt""#),
    )))
    .bind(uuid())
    .bind(&project_id)
    .bind(&me.id)
    .bind(g("deviceId").and_then(Value::as_str).unwrap_or_default())
    .bind(pinned.unwrap_or(false))
    .bind(now)
    .bind(played)
    .bind(&id)
    .bind(&channel)
    .bind(&version)
    .bind(&status)
    .bind(&client_version)
    .bind(launched)
    .bind(pinned.is_some())
    .fetch_one(&s.db)
    .await?;
    if played != 0 || launched {
        crate::realtime::emit_project(&s, &org, &project_id, "hub:changed", &json!({ "projectId": project_id, "what": "playtime" })).await;
    }
    created(saved.0)
}

pub fn routes() -> Router<AppState> {
    let base = "/organizations/{organizationId}/projects/{projectId}/releases";
    Router::new()
        .route(base, get(list).post(create))
        .route(&format!("{base}/latest"), get(latest))
        .route(&format!("{base}/installs"), get(installs))
        .route(&format!("{base}/chunks/{{hash}}"), get(chunk))
        .route(&format!("{base}/{{releaseId}}"), get(detail).patch(update).delete(remove))
        .route(&format!("{base}/{{releaseId}}/manifest"), get(manifest))
        .route(&format!("{base}/{{releaseId}}/diff"), get(diff))
        .route(&format!("{base}/{{releaseId}}/missing"), post(missing))
        .route(&format!("{base}/{{releaseId}}/chunks/{{hash}}"), axum::routing::put(put_chunk))
        .route(&format!("{base}/{{releaseId}}/publish"), post(publish))
        .route(&format!("{base}/{{releaseId}}/live"), post(live))
        .route(&format!("{base}/{{releaseId}}/installed"), post(installed))
        // Express json(): 340 MB (büyük build'lerin manifesti birkaç MB olabilir).
        .layer(DefaultBodyLimit::max(340 * 1024 * 1024))
}
