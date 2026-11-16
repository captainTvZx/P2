//! Proje ekran görüntüsü galerisi — `server/src/screenshots.ts` + `screenshotRules.ts` karşılığı:
//! overlay "Ekran görüntüsü" penceresi ve Kütüphane "Ekran görüntüleri" sekmesi.
//! Yükleme iki adımlı: istemci POST /uploads (JPEG, base64) → buraya yalnız uploadPath + boyutlar.
//! Sunucu yolu doğrular, "<id>.json" meta dosyasından sahibi (ben) ve türü (görsel) denetler.
//! Proje izleyicisi listeler; sahibi ya da projeyi yöneten (channel.manage vekili) düzenler/siler (yalnız satır).

use axum::Router;
use axum::extract::{Path, State};
use axum::routing::{MethodRouter, get};
use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};
use serde_json::{Value, json};

use crate::AppState;
use crate::ctx::{Body, Me, Query, Reply, created, many_json, not_found, now, ok, one_json, page_args, sql, uuid};
use crate::error::{ApiError, ApiResult};
use crate::perm;
use crate::projects::{dto, is_int, is_str, iso, js_space, js_string, js_trim, min, missing, optional, str_len, user4};
use crate::realtime::emit_project;
use crate::validate::{js_number, js_slice};

// ------------------------------------------------------------------ screenshotRules.ts

pub const MAX_DIMENSION: i64 = 7680;
pub const MAX_CAPTION: usize = 500;
const IMAGE_TYPES: &[&str] = &["image/png", "image/jpeg", "image/webp"];

/// `/^\/uploads\/[a-f0-9-]{36}\.(png|jpe?g|webp)$/i`
pub fn valid_upload_path(value: &str) -> bool {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"(?i)^/uploads/[a-f0-9-]{36}\.(png|jpe?g|webp)$").unwrap()).is_match(value)
}

/// "/uploads/<id>.jpg" → "<id>.jpg".
pub fn upload_file_id(upload_path: &str) -> &str {
    upload_path.get("/uploads/".len()..).unwrap_or_default()
}

/// JS `||` için doğruluk.
fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

/// Meta dosyası: yükleyen bu kullanıcı mı ve içerik gerçekten görsel mi.
pub fn upload_owned_image(meta: &Value, user_id: &str) -> bool {
    if !meta.is_object() && !meta.is_array() {
        return false;
    }
    let owner = if truthy(meta.get("userId")) { js_string(&meta["userId"]) } else { String::new() };
    if owner != user_id {
        return false;
    }
    let kind = if truthy(meta.get("type")) { js_string(&meta["type"]) } else { String::new() };
    IMAGE_TYPES.contains(&kind.to_lowercase().as_str())
}

pub fn can_manage_screenshot(owner_id: &str, user_id: &str, can_manage: bool) -> bool {
    owner_id == user_id || can_manage
}

/// Başlık: boşluklar tek boşluğa iner, kırpılır, MAX_CAPTION ile sınırlanır; boş → null.
pub fn normalize_caption(value: Option<&Value>) -> Option<String> {
    let text = match value {
        None | Some(Value::Null) => return None,
        Some(v) => js_string(v),
    };
    let mut collapsed = String::new();
    let mut in_space = false;
    for c in text.chars() {
        if js_space(c) {
            if !in_space {
                collapsed.push(' ');
            }
            in_space = true;
        } else {
            collapsed.push(c);
            in_space = false;
        }
    }
    let out = js_slice(js_trim(&collapsed), MAX_CAPTION);
    if out.is_empty() { None } else { Some(out) }
}

/// Genişlik/yükseklik: 1..MAX_DIMENSION tam sayı; aksi hâlde None.
pub fn clamp_dimension(value: Option<&Value>) -> Option<i64> {
    let n = match value {
        Some(Value::Number(n)) => n.as_f64()?,
        Some(Value::String(s)) => js_number(s)?,
        _ => return None,
    }
    .floor();
    if !n.is_finite() || n < 1.0 || n > MAX_DIMENSION as f64 {
        return None;
    }
    Some(n as i64)
}

/// JS `new Date(text)` (ISO biçimleri): saat dilimsiz tarih-saat yerel saattir, yalnız tarih UTC.
fn parse_js_date(text: &str) -> Option<DateTime<Utc>> {
    if let Ok(d) = DateTime::parse_from_rfc3339(text) {
        return Some(d.with_timezone(&Utc));
    }
    if let Ok(d) = NaiveDate::parse_from_str(text, "%Y-%m-%d") {
        return Some(Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0)?));
    }
    for format in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M"] {
        if let Ok(d) = NaiveDateTime::parse_from_str(text, format) {
            return chrono::Local.from_local_datetime(&d).single().map(|l| l.with_timezone(&Utc));
        }
    }
    None
}

/// Çekim zamanı: geçerli ve geleceğe/çok eskiye kaçmıyorsa o; değilse şimdi.
pub fn normalize_taken_at(value: Option<&Value>, now_ms: i64) -> NaiveDateTime {
    let fallback = || DateTime::from_timestamp_millis(now_ms).unwrap_or_default().naive_utc();
    let Some(text) = value.and_then(Value::as_str).filter(|t| !t.is_empty()) else { return fallback() };
    let Some(at) = parse_js_date(text).map(|d| d.timestamp_millis()) else { return fallback() };
    if at > now_ms + 5 * 60_000 || at < now_ms - 365 * 86_400_000 {
        return fallback();
    }
    DateTime::from_timestamp_millis(at).unwrap_or_default().naive_utc()
}

/// JS `Number(x)` (JSON değeri).
fn js_num_value(v: Option<&Value>) -> f64 {
    match v {
        None => f64::NAN,
        Some(Value::Null) => 0.0,
        Some(Value::Bool(b)) => f64::from(u8::from(*b)),
        Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
        Some(Value::String(s)) => js_number(s).unwrap_or(f64::NAN),
        Some(Value::Array(a)) if a.is_empty() => 0.0,
        Some(Value::Array(a)) if a.len() == 1 => js_num_value(Some(&Value::String(js_string(&a[0])))),
        Some(_) => f64::NAN,
    }
}

fn or_num(n: f64) -> Option<f64> {
    (n != 0.0 && !n.is_nan()).then_some(n)
}

/// Yükleme kökü (uploads.ts ile aynı): P2_UPLOAD_DIR ya da <çalışma dizini>/.p2-data/uploads.
fn upload_root() -> std::path::PathBuf {
    std::env::var_os("P2_UPLOAD_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default().join(".p2-data").join("uploads"))
}

async fn read_meta(upload_path: &str) -> Option<Value> {
    let file = upload_root().join(format!("{}.json", upload_file_id(upload_path)));
    let text = tokio::fs::read_to_string(file).await.ok()?;
    serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()
}

// ------------------------------------------------------------------ uçlar

fn shot_json() -> String {
    format!(
        r#"json_build_object('id', x.id, 'projectId', x."projectId", 'userId', x."userId", 'uploadPath', x."uploadPath", 'width', x.width, 'height', x.height,
           'size', x.size, 'caption', x.caption, 'releaseId', x."releaseId", 'takenAt', {}, 'createdAt', {}, 'user', {})"#,
        iso(r#"x."takenAt""#),
        iso(r#"x."createdAt""#),
        user4(r#"x."userId""#)
    )
}

async fn access(s: &AppState, org: &str, project_id: &str, user_id: &str) -> ApiResult<bool> {
    perm::require_project_access(&s.db, org, user_id, project_id).await?;
    let found: Option<i32> = sqlx::query_scalar(r#"select 1 from "Project" where id = $1 and "organizationId" = $2 and "deletedAt" is null"#)
        .bind(project_id)
        .bind(org)
        .fetch_optional(&s.db)
        .await?;
    if found.is_none() {
        return Err(not_found("PROJECT_NOT_FOUND", "Proje bulunamadı."));
    }
    Ok(perm::effective_in(&s.db, project_id, user_id).await?.has("channel.manage"))
}

async fn find(s: &AppState, project_id: &str, id: &str) -> ApiResult<Value> {
    one_json(sqlx::query_scalar(sql(format!(r#"select {} from "ProjectScreenshot" x where x.id = $1 and x."projectId" = $2 limit 1"#, shot_json()))).bind(id).bind(project_id), &s.db)
        .await?
        .ok_or_else(|| not_found("SCREENSHOT_NOT_FOUND", "Ekran görüntüsü bulunamadı."))
}

fn r(m: MethodRouter<AppState>) -> MethodRouter<AppState> {
    m.fallback(crate::error::not_found)
}

async fn list(State(s): State<AppState>, me: Me, Path((org, project_id)): Path<(String, String)>, q: Query) -> Reply {
    access(&s, &org, &project_id, &me.id).await?;
    let (take, cursor) = page_args(q.get("before").map(String::as_str), q.get("limit").map(String::as_str), 60, 200);
    let user = q.get("userId").filter(|u| !u.is_empty());
    let rows = many_json(
        sqlx::query_scalar(sql(format!(
            r#"select {} from "ProjectScreenshot" x where x."projectId" = $1 and ($2::text is null or x."userId" = $2) and ($3::timestamp is null or x."takenAt" < $3)
               order by x."takenAt" desc, x."createdAt" desc limit $4"#,
            shot_json()
        )))
        .bind(&project_id)
        .bind(user)
        .bind(cursor)
        .bind(take),
        &s.db,
    )
    .await?;
    ok(Value::Array(rows))
}

async fn create(State(s): State<AppState>, me: Me, Path((org, project_id)): Path<(String, String)>, body: Body) -> Reply {
    let b = &body.0;
    dto(&[
        ("uploadPath", is_str(b.get("uploadPath"))),
        ("width", is_int(b.get("width")) && min(b.get("width"), 1.0)),
        ("height", is_int(b.get("height")) && min(b.get("height"), 1.0)),
        ("size", optional(b.get("size"), |v| is_int(v) && min(v, 0.0))),
        ("caption", optional(b.get("caption"), |v| str_len(v, 0, MAX_CAPTION))),
        ("releaseId", optional(b.get("releaseId"), is_str)),
        ("takenAt", optional(b.get("takenAt"), is_str)),
    ])?;
    access(&s, &org, &project_id, &me.id).await?;
    let upload_path = body.str("uploadPath").unwrap_or_default().to_string();
    if !valid_upload_path(&upload_path) {
        return Err(ApiError::bad_request("SCREENSHOT_PATH", "Görüntü yolu geçersiz."));
    }
    let (Some(width), Some(height)) = (clamp_dimension(b.get("width")), clamp_dimension(b.get("height"))) else {
        return Err(ApiError::bad_request("SCREENSHOT_SIZE", "Görüntü boyutu geçersiz (en çok 7680 px)."));
    };
    let meta = read_meta(&upload_path).await;
    if !meta.as_ref().is_some_and(|m| upload_owned_image(m, &me.id)) {
        return Err(ApiError::bad_request("SCREENSHOT_UPLOAD", "Yüklenen dosya bulunamadı ya da sana ait bir görsel değil."));
    }
    let meta = meta.unwrap_or_default();
    let mut release_id: Option<String> = None;
    if let Some(wanted) = body.str("releaseId").filter(|r| !r.is_empty()) {
        release_id = sqlx::query_scalar(r#"select id from "Release" where id = $1 and "projectId" = $2 limit 1"#).bind(wanted).bind(&project_id).fetch_optional(&s.db).await?;
    }
    let size = or_num(js_num_value(meta.get("size"))).or_else(|| or_num(js_num_value(b.get("size")))).unwrap_or(0.0).floor().max(0.0);
    let size = i32::try_from(size as i64).map_err(|_| ApiError::internal("size taşması"))?;
    let shot: sqlx::types::Json<Value> = sqlx::query_scalar(sql(format!(
        r#"with x as (insert into "ProjectScreenshot" (id, "projectId", "userId", "uploadPath", width, height, size, caption, "releaseId", "takenAt", "createdAt")
           values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) returning *) select {} from x"#,
        shot_json()
    )))
    .bind(uuid())
    .bind(&project_id)
    .bind(&me.id)
    .bind(&upload_path)
    .bind(width as i32)
    .bind(height as i32)
    .bind(size)
    .bind(normalize_caption(b.get("caption")))
    .bind(&release_id)
    .bind(normalize_taken_at(b.get("takenAt"), Utc::now().timestamp_millis()))
    .bind(now())
    .fetch_one(&s.db)
    .await?;
    let shot = shot.0;
    emit_project(&s, &org, &project_id, "screenshot:added", &json!({ "projectId": project_id, "screenshot": shot })).await;
    created(shot)
}

async fn update(State(s): State<AppState>, me: Me, Path((org, project_id, id)): Path<(String, String, String)>, body: Body) -> Reply {
    dto(&[("caption", optional(body.get("caption"), |v| str_len(v, 0, MAX_CAPTION)))])?;
    let can_manage = access(&s, &org, &project_id, &me.id).await?;
    let current = find(&s, &project_id, &id).await?;
    if !can_manage_screenshot(current["userId"].as_str().unwrap_or_default(), &me.id, can_manage) {
        return Err(not_found("PERMISSION_DENIED", "Bu görüntüyü düzenleyemezsin."));
    }
    let caption = match body.get("caption") {
        Some(v) => normalize_caption(Some(v)),
        None => current["caption"].as_str().map(str::to_string),
    };
    let shot = one_json(
        sqlx::query_scalar(sql(format!(r#"with x as (update "ProjectScreenshot" set caption = $2 where id = $1 returning *) select {} from x"#, shot_json()))).bind(&id).bind(&caption),
        &s.db,
    )
    .await?
    .ok_or_else(|| missing("ProjectScreenshot"))?;
    emit_project(&s, &org, &project_id, "screenshot:updated", &json!({ "projectId": project_id, "screenshot": shot })).await;
    ok(shot)
}

async fn remove(State(s): State<AppState>, me: Me, Path((org, project_id, id)): Path<(String, String, String)>) -> Reply {
    let can_manage = access(&s, &org, &project_id, &me.id).await?;
    let current = find(&s, &project_id, &id).await?;
    if !can_manage_screenshot(current["userId"].as_str().unwrap_or_default(), &me.id, can_manage) {
        return Err(not_found("PERMISSION_DENIED", "Bu görüntüyü silemezsin."));
    }
    let done = sqlx::query(r#"delete from "ProjectScreenshot" where id = $1"#).bind(&id).execute(&s.db).await?;
    if done.rows_affected() == 0 {
        return Err(missing("ProjectScreenshot"));
    }
    emit_project(&s, &org, &project_id, "screenshot:deleted", &json!({ "projectId": project_id, "id": id })).await;
    ok(json!({ "ok": true }))
}

pub fn routes() -> Router<AppState> {
    let base = "/organizations/{organizationId}/projects/{projectId}/screenshots";
    Router::new()
        .route(base, r(get(list).post(create)))
        .route(&format!("{base}/{{id}}"), r(axum::routing::patch(update).delete(remove)))
}

#[cfg(test)]
mod tests {
    use super::*;

    // screenshots.spec.ts ile aynı kurallar.
    #[test]
    fn kurallar() {
        let id = "3f2a1c9e-1b2c-4d5e-8f90-123456789abc";
        assert!(valid_upload_path(&format!("/uploads/{id}.jpg")));
        assert!(valid_upload_path(&format!("/uploads/{id}.PNG")));
        assert!(!valid_upload_path(&format!("/uploads/{id}.gif")));
        assert!(!valid_upload_path(&format!("/uploads/{id}.jpg.json")));
        assert!(!valid_upload_path("/uploads/../x.jpg"));
        assert_eq!(upload_file_id(&format!("/uploads/{id}.jpg")), format!("{id}.jpg"));
        assert!(upload_owned_image(&json!({ "userId": "u1", "type": "image/jpeg" }), "u1"));
        assert!(!upload_owned_image(&json!({ "userId": "u2", "type": "image/jpeg" }), "u1"));
        assert!(!upload_owned_image(&json!({ "userId": "u1", "type": "application/pdf" }), "u1"));
        assert!(!upload_owned_image(&Value::Null, "u1"));
        assert_eq!(normalize_caption(Some(&json!("  Boss  \n kapısı "))).as_deref(), Some("Boss kapısı"));
        assert_eq!(normalize_caption(Some(&json!("   "))), None);
        assert_eq!(normalize_caption(Some(&json!("x".repeat(550)))).map(|c| c.len()), Some(500));
        assert_eq!(clamp_dimension(Some(&json!(1920))), Some(1920));
        assert_eq!(clamp_dimension(Some(&json!("2160"))), Some(2160));
        assert_eq!(clamp_dimension(Some(&json!(1079.9))), Some(1079));
        assert_eq!(clamp_dimension(Some(&json!(0))), None);
        assert_eq!(clamp_dimension(Some(&json!(7681))), None);
        assert_eq!(clamp_dimension(Some(&json!("abc"))), None);
        let now = DateTime::parse_from_rfc3339("2026-10-23T12:00:00Z").unwrap().timestamp_millis();
        assert_eq!(normalize_taken_at(Some(&json!("2026-10-23T11:58:00Z")), now).and_utc().timestamp_millis(), now - 120_000);
        assert_eq!(normalize_taken_at(Some(&json!("bozuk")), now).and_utc().timestamp_millis(), now);
        assert_eq!(normalize_taken_at(Some(&json!("2026-10-23T13:00:00Z")), now).and_utc().timestamp_millis(), now);
        assert_eq!(normalize_taken_at(Some(&json!("2020-01-01T00:00:00Z")), now).and_utc().timestamp_millis(), now);
        assert_eq!(normalize_taken_at(None, now).and_utc().timestamp_millis(), now);
    }
}
