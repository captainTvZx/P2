//! Sponsorlu içerik (uygulama içi reklam). Sunucu yöneticisi (`server.ads`) sade kartlar yayınlar:
//! sponsor adı, başlık, kısa metin, isteğe bağlı görsel, bağlantı ve düğme yazısı. İstemci bunları
//! rahatsız etmeyen yerlerde en çok birer tane gösterir (Keşfet ızgarası, Keşfet haber sütunu,
//! Kütüphane kenar çubuğu); açılır pencere, ses, video ya da kişisel izleme yoktur.
//!
//! Sayımlar yalnız gün başına toplamdır: bir kişinin bir kartı görmesi ya da tıklaması günde en çok bir
//! kez sayılır (bellekte tutulan tekilleştirme; kişinin kimliği veritabanına yazılmaz).
//!
//! - `GET    /sponsored?placement=discover|news|library` → `{ enabled, ads }` (oturumu olan herkes)
//! - `POST   /sponsored/:id/events` `{ kind: view|click }` → gün toplamına eklenir
//! - `GET    /server/sponsored` → `{ enabled, ads }` (her kartın toplamları ve son 14 günü)
//! - `POST   /server/sponsored`, `PATCH|DELETE /server/sponsored/:id`
//! - `PATCH  /server/sponsored/settings` `{ enabled }` → bütün sunucuda aç/kapat

use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

use axum::Router;
use axum::extract::{Path, State};
use axum::routing::{get, patch, post};
use serde_json::{Value, json};

use crate::AppState;
use crate::ctx::{Body, Me, Query, Reply, created, many_json, not_found, now, ok, sql, uuid};
use crate::error::{ApiError, ApiResult};

pub const PLACEMENTS: &[&str] = &["discover", "news", "library"];
const SETTING: &str = "sponsoredEnabled";

fn missing() -> ApiError {
    not_found("SPONSORED_NOT_FOUND", "Sponsorlu içerik bulunamadı.")
}

async fn enabled(db: &sqlx::PgPool) -> ApiResult<bool> {
    Ok(crate::server_admin::setting(db, SETTING, "true").await? != "false")
}

/// Herkese gösterilen alanlar (alias `a`).
fn public_fields() -> &'static str {
    r#"'id', a.id, 'sponsor', a.sponsor, 'title', a.title, 'body', a.body, 'imageUrl', a."imageUrl", 'linkUrl', a."linkUrl",
       'cta', a.cta, 'placements', a.placements, 'weight', a.weight"#
}

/// Şu an yayında: etkin ve (varsa) tarih aralığının içinde.
const LIVE: &str = r#"a.active and (a."startsAt" is null or a."startsAt" <= now() at time zone 'utc') and (a."endsAt" is null or a."endsAt" > now() at time zone 'utc')"#;

// ------------------------------------------------------------------ uçlar: herkes

async fn feed(State(s): State<AppState>, _me: Me, q: Query) -> Reply {
    if !enabled(&s.db).await? {
        return ok(json!({ "enabled": false, "ads": [] }));
    }
    let placement = q.get("placement").map(String::as_str).unwrap_or("");
    if !placement.is_empty() && !PLACEMENTS.contains(&placement) {
        return Err(ApiError::bad_request("VALIDATION_FAILED", "placement: discover | news | library").field("placement"));
    }
    let ads = many_json(
        sqlx::query_scalar(sql(format!(
            r#"select json_build_object({}) from "SponsoredAd" a
               where {LIVE} and ($1 = '' or $1 = any(a.placements))
               order by a.weight desc, a."createdAt" desc limit 20"#,
            public_fields()
        )))
        .bind(placement),
        &s.db,
    )
    .await?;
    ok(json!({ "enabled": true, "ads": ads }))
}

/// (kişi, kart, tür, gün) — günde bir kez sayılır. Gün değişince küme boşaltılır.
fn seen() -> &'static Mutex<(String, HashSet<(String, String, bool)>)> {
    static SEEN: OnceLock<Mutex<(String, HashSet<(String, String, bool)>)>> = OnceLock::new();
    SEEN.get_or_init(Default::default)
}

async fn event(State(s): State<AppState>, me: Me, Path(id): Path<String>, body: Body) -> Reply {
    let click = match body.str("kind") {
        Some("view") => false,
        Some("click") => true,
        _ => return Err(ApiError::bad_request("VALIDATION_FAILED", "kind: view | click").field("kind")),
    };
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let first = {
        let mut g = seen().lock().unwrap();
        if g.0 != today {
            g.0 = today;
            g.1.clear();
        }
        g.1.insert((me.id.clone(), id.clone(), click))
    };
    if first {
        let exists: Option<i32> = sqlx::query_scalar(r#"select 1 from "SponsoredAd" where id = $1"#).bind(&id).fetch_optional(&s.db).await?;
        if exists.is_none() {
            return Err(missing());
        }
        let (views, clicks) = if click { (0i64, 1i64) } else { (1, 0) };
        sqlx::query(
            r#"insert into "SponsoredAdStat" ("adId", day, impressions, clicks) values ($1, (now() at time zone 'utc')::date, $2, $3)
               on conflict ("adId", day) do update set impressions = "SponsoredAdStat".impressions + $2, clicks = "SponsoredAdStat".clicks + $3"#,
        )
        .bind(&id)
        .bind(views)
        .bind(clicks)
        .execute(&s.db)
        .await?;
    }
    ok(json!({ "ok": true, "counted": first }))
}

// ------------------------------------------------------------------ uçlar: yönetim

async fn admin_rows(db: &sqlx::PgPool, id: Option<&str>) -> ApiResult<Vec<Value>> {
    many_json(
        sqlx::query_scalar(sql(format!(
            r#"select json_build_object({fields}, 'active', a.active, 'startsAt', {starts}, 'endsAt', {ends}, 'createdAt', {created},
                 'live', {LIVE},
                 'impressions', coalesce((select sum(x.impressions) from "SponsoredAdStat" x where x."adId" = a.id), 0),
                 'clicks', coalesce((select sum(x.clicks) from "SponsoredAdStat" x where x."adId" = a.id), 0),
                 'days', coalesce((select json_agg(json_build_object('day', to_char(x.day, 'YYYY-MM-DD'), 'impressions', x.impressions, 'clicks', x.clicks) order by x.day desc)
                                   from "SponsoredAdStat" x where x."adId" = a.id and x.day > (now() at time zone 'utc')::date - 14), '[]'::json))
               from "SponsoredAd" a where $1::text is null or a.id = $1
               order by a."createdAt" desc"#,
            fields = public_fields(),
            starts = crate::iso!(r#"a."startsAt""#),
            ends = crate::iso!(r#"a."endsAt""#),
            created = crate::iso!(r#"a."createdAt""#),
        )))
        .bind(id),
        db,
    )
    .await
}

async fn list(State(s): State<AppState>, me: Me) -> Reply {
    crate::server_admin::require(&s.db, &me.id, "server.ads").await?;
    ok(json!({ "enabled": enabled(&s.db).await?, "ads": admin_rows(&s.db, None).await? }))
}

fn text(body: &Body, key: &str, min: usize, max: usize, label: &str) -> ApiResult<Option<String>> {
    let Some(v) = body.get(key) else { return Ok(None) };
    let v = v.as_str().map(str::trim).unwrap_or_default();
    let n = v.chars().count();
    if n < min || n > max {
        let msg = if min > 0 { format!("{label} {min}–{max} karakter olmalı.") } else { format!("{label} en çok {max} karakter olabilir.") };
        return Err(ApiError::bad_request("VALIDATION_FAILED", &msg).field(key));
    }
    Ok(Some(v.to_string()))
}

fn url_ok(v: &str, allow_upload: bool) -> bool {
    let lower = v.to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://")) && v.len() > 10 && !v.contains(char::is_whitespace) || allow_upload && v.starts_with("/uploads/")
}

fn date(body: &Body, key: &str) -> ApiResult<Option<Option<chrono::NaiveDateTime>>> {
    let Some(v) = body.get(key) else { return Ok(None) };
    match v.as_str().map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(Some(None)),
        Some(s) => chrono::DateTime::parse_from_rfc3339(s)
            .map(|d| Some(Some(d.naive_utc())))
            .or_else(|_| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").map(|d| Some(d.and_hms_opt(0, 0, 0))))
            .map_err(|_| ApiError::bad_request("VALIDATION_FAILED", "Tarih YYYY-AA-GG biçiminde olmalı.").field(key)),
    }
}

/// Gövdedeki alanları doğrular; `full` ise zorunlular da istenir (oluşturma).
struct Fields {
    sponsor: Option<String>,
    title: Option<String>,
    body: Option<String>,
    image: Option<Option<String>>,
    link: Option<String>,
    cta: Option<String>,
    placements: Option<Vec<String>>,
    active: Option<bool>,
    weight: Option<i32>,
    starts: Option<Option<chrono::NaiveDateTime>>,
    ends: Option<Option<chrono::NaiveDateTime>>,
}

fn fields(b: &Body, full: bool) -> ApiResult<Fields> {
    let f = Fields {
        sponsor: text(b, "sponsor", 1, 60, "Sponsor adı")?,
        title: text(b, "title", 1, 80, "Başlık")?,
        body: text(b, "body", 0, 200, "Metin")?,
        image: match b.get("imageUrl") {
            None => None,
            Some(v) => match v.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                None => Some(None),
                Some(u) if url_ok(u, true) && u.len() <= 1000 => Some(Some(u.to_string())),
                Some(_) => return Err(ApiError::bad_request("VALIDATION_FAILED", "Görsel adresi https:// ile ya da /uploads/ ile başlamalı.").field("imageUrl")),
            },
        },
        link: match b.get("linkUrl") {
            None => None,
            Some(v) => match v.as_str().map(str::trim) {
                Some(u) if url_ok(u, false) && u.len() <= 1000 => Some(u.to_string()),
                _ => return Err(ApiError::bad_request("VALIDATION_FAILED", "Bağlantı https:// ile başlayan bir adres olmalı.").field("linkUrl")),
            },
        },
        cta: text(b, "cta", 0, 24, "Düğme yazısı")?,
        placements: match b.get("placements") {
            None => None,
            Some(Value::Array(items)) => {
                let list: Vec<String> = items.iter().filter_map(Value::as_str).map(str::to_string).collect();
                if list.is_empty() || list.len() != items.len() || list.iter().any(|p| !PLACEMENTS.contains(&p.as_str())) {
                    return Err(ApiError::bad_request("VALIDATION_FAILED", "En az bir yer seç: discover, news, library.").field("placements"));
                }
                let mut uniq: Vec<String> = Vec::new();
                for p in list {
                    if !uniq.contains(&p) {
                        uniq.push(p);
                    }
                }
                Some(uniq)
            }
            Some(_) => return Err(ApiError::bad_request("VALIDATION_FAILED", "placements bir dizi olmalı.").field("placements")),
        },
        active: match b.get("active") {
            None => None,
            Some(Value::Bool(v)) => Some(*v),
            Some(_) => return Err(ApiError::bad_request("VALIDATION_FAILED", "active: true | false").field("active")),
        },
        weight: match b.get("weight") {
            None => None,
            Some(v) => match v.as_i64() {
                Some(w @ 1..=100) => Some(w as i32),
                _ => return Err(ApiError::bad_request("VALIDATION_FAILED", "Ağırlık 1–100 arasında olmalı.").field("weight")),
            },
        },
        starts: date(b, "startsAt")?,
        ends: date(b, "endsAt")?,
    };
    if full {
        for (missing, key, label) in [(f.sponsor.is_none(), "sponsor", "Sponsor adı"), (f.title.is_none(), "title", "Başlık"), (f.link.is_none(), "linkUrl", "Bağlantı"), (f.placements.is_none(), "placements", "Yer")] {
            if missing {
                return Err(ApiError::bad_request("VALIDATION_FAILED", &format!("{label} gerekli.")).field(key));
            }
        }
    }
    if let (Some(Some(a)), Some(Some(b))) = (f.starts, f.ends) {
        if b <= a {
            return Err(ApiError::bad_request("VALIDATION_FAILED", "Bitiş tarihi başlangıçtan sonra olmalı.").field("endsAt"));
        }
    }
    Ok(f)
}

async fn create(State(s): State<AppState>, me: Me, body: Body) -> Reply {
    crate::server_admin::require(&s.db, &me.id, "server.ads").await?;
    let f = fields(&body, true)?;
    let id = uuid();
    sqlx::query(
        r#"insert into "SponsoredAd" (id, sponsor, title, body, "imageUrl", "linkUrl", cta, placements, active, weight, "startsAt", "endsAt", "createdById", "createdAt", "updatedAt")
           values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $14)"#,
    )
    .bind(&id)
    .bind(f.sponsor.unwrap_or_default())
    .bind(f.title.unwrap_or_default())
    .bind(f.body.unwrap_or_default())
    .bind(f.image.flatten())
    .bind(f.link.unwrap_or_default())
    .bind(f.cta.unwrap_or_default())
    .bind(f.placements.unwrap_or_default())
    .bind(f.active.unwrap_or(true))
    .bind(f.weight.unwrap_or(1))
    .bind(f.starts.flatten())
    .bind(f.ends.flatten())
    .bind(&me.id)
    .bind(now())
    .execute(&s.db)
    .await?;
    created(admin_rows(&s.db, Some(&id)).await?.into_iter().next().unwrap_or(Value::Null))
}

async fn update(State(s): State<AppState>, me: Me, Path(id): Path<String>, body: Body) -> Reply {
    crate::server_admin::require(&s.db, &me.id, "server.ads").await?;
    let f = fields(&body, false)?;
    let done = sqlx::query(
        r#"update "SponsoredAd" set
             sponsor = coalesce($2, sponsor), title = coalesce($3, title), body = coalesce($4, body),
             "imageUrl" = case when $5 then $6 else "imageUrl" end, "linkUrl" = coalesce($7, "linkUrl"), cta = coalesce($8, cta),
             placements = coalesce($9, placements), active = coalesce($10, active), weight = coalesce($11, weight),
             "startsAt" = case when $12 then $13 else "startsAt" end, "endsAt" = case when $14 then $15 else "endsAt" end,
             "updatedAt" = $16
           where id = $1"#,
    )
    .bind(&id)
    .bind(f.sponsor)
    .bind(f.title)
    .bind(f.body)
    .bind(f.image.is_some())
    .bind(f.image.flatten())
    .bind(f.link)
    .bind(f.cta)
    .bind(f.placements)
    .bind(f.active)
    .bind(f.weight)
    .bind(f.starts.is_some())
    .bind(f.starts.flatten())
    .bind(f.ends.is_some())
    .bind(f.ends.flatten())
    .bind(now())
    .execute(&s.db)
    .await?
    .rows_affected();
    if done == 0 {
        return Err(missing());
    }
    ok(admin_rows(&s.db, Some(&id)).await?.into_iter().next().unwrap_or(Value::Null))
}

async fn remove(State(s): State<AppState>, me: Me, Path(id): Path<String>) -> Reply {
    crate::server_admin::require(&s.db, &me.id, "server.ads").await?;
    let done = sqlx::query(r#"delete from "SponsoredAd" where id = $1"#).bind(&id).execute(&s.db).await?.rows_affected();
    if done == 0 {
        return Err(missing());
    }
    ok(json!({ "ok": true, "id": id }))
}

async fn settings(State(s): State<AppState>, me: Me, body: Body) -> Reply {
    crate::server_admin::require(&s.db, &me.id, "server.ads").await?;
    match body.get("enabled") {
        Some(Value::Bool(v)) => crate::server_admin::set_setting(&s.db, SETTING, if *v { "true" } else { "false" }).await?,
        _ => return Err(ApiError::bad_request("VALIDATION_FAILED", "enabled: true | false").field("enabled")),
    }
    ok(json!({ "enabled": enabled(&s.db).await? }))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/sponsored", get(feed))
        .route("/sponsored/{id}/events", post(event))
        .route("/server/sponsored", get(list).post(create))
        .route("/server/sponsored/settings", patch(settings))
        .route("/server/sponsored/{id}", patch(update).delete(remove))
}

#[cfg(test)]
mod tests {
    use super::url_ok;

    #[test]
    fn urls() {
        assert!(url_ok("https://example.com/x", false));
        assert!(!url_ok("javascript:alert(1)", false));
        assert!(!url_ok("/uploads/a.png", false));
        assert!(url_ok("/uploads/a.png", true));
        assert!(!url_ok("https://exa mple.com", false));
    }
}
