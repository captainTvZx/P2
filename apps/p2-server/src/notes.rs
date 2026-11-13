//! Proje notları — `server/src/notes.ts` + `notesRules.ts` karşılığı: oyun içi overlay "Notlar"
//! penceresi ve Kütüphane "Notlar" sekmesi. PRIVATE not yalnız yazarına döner/yayınlanır; TEAM not
//! proje erişimi olan herkese. PATCH iyimser kilitle çalışır: istemci bildiği `version`ı gönderir,
//! farklıysa 409 NOTE_CONFLICT (gövdede güncel not).

use axum::Router;
use axum::extract::{Path, State};
use axum::routing::{MethodRouter, get};
use serde_json::{Value, json};

use crate::AppState;
use crate::ctx::{Body, Me, Query, Reply, created, many_json, not_found, now, ok, one_json, page_args, sql, uuid};
use crate::error::{ApiError, ApiResult};
use crate::perm;
use crate::projects::{dto, is_bool, is_in, is_int, iso, min, missing, optional, str_len, user4};
use crate::realtime::{emit_project, emit_users};
use crate::validate::js_slice;

// ------------------------------------------------------------------ notesRules.ts

pub fn normalize_visibility(value: Option<&Value>, fallback: &'static str) -> &'static str {
    match value.and_then(Value::as_str) {
        Some("TEAM") => "TEAM",
        Some("PRIVATE") => "PRIVATE",
        _ => fallback,
    }
}

pub fn note_visible_to(author_id: &str, visibility: &str, user_id: &str) -> bool {
    visibility == "TEAM" || author_id == user_id
}

pub fn can_edit_note(author_id: &str, visibility: &str, user_id: &str, can_manage: bool) -> bool {
    if author_id == user_id {
        return true;
    }
    visibility == "TEAM" && can_manage
}

/// İyimser kilit: istemci `version` gönderdiyse sunucudaki ile aynı olmalı.
pub fn version_conflicts(current: i64, sent: Option<&Value>) -> bool {
    match sent {
        None | Some(Value::Null) => false,
        Some(v) => v.as_f64() != Some(current as f64),
    }
}

pub fn next_version(current: i64) -> i64 {
    current.max(1) + 1
}

// ------------------------------------------------------------------ uçlar

fn note_json() -> String {
    format!(
        r#"json_build_object('id', n.id, 'projectId', n."projectId", 'authorId', n."authorId", 'title', n.title, 'body', n.body, 'visibility', n.visibility,
           'pinned', n.pinned, 'version', n.version, 'createdAt', {}, 'updatedAt', {}, 'author', {})"#,
        iso(r#"n."createdAt""#),
        iso(r#"n."updatedAt""#),
        user4(r#"n."authorId""#)
    )
}

/// Hub ile aynı vekil: projeyi yöneten (channel.manage) ekip notlarını da düzenler/siler.
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
    Ok(perm::effective(&s.db, org, user_id).await?.has("channel.manage"))
}

async fn find(s: &AppState, project_id: &str, note_id: &str, user_id: &str) -> ApiResult<Value> {
    let note = one_json(
        sqlx::query_scalar(sql(format!(r#"select {} from "ProjectNote" n where n.id = $1 and n."projectId" = $2 limit 1"#, note_json()))).bind(note_id).bind(project_id),
        &s.db,
    )
    .await?;
    match note {
        Some(n) if note_visible_to(field(&n, "authorId"), field(&n, "visibility"), user_id) => Ok(n),
        _ => Err(not_found("NOTE_NOT_FOUND", "Not bulunamadı.")),
    }
}

fn field<'a>(note: &'a Value, key: &str) -> &'a str {
    note[key].as_str().unwrap_or_default()
}

/// PRIVATE → yalnız yazar; TEAM → proje izleyicileri. Görünürlük değişince eski kitle de haberdar
/// edilir (TEAM→PRIVATE: ekip için "silindi", yazar için "güncellendi").
async fn changed(s: &AppState, org: &str, note: &Value, action: &str, payload: Option<&Value>, previous: Option<&str>) {
    let project_id = field(note, "projectId");
    let note_id = field(note, "id");
    let author = field(note, "authorId");
    let mut base = json!({ "projectId": project_id, "action": action, "noteId": note_id });
    if let Some(p) = payload {
        base["note"] = p.clone();
    }
    if field(note, "visibility") == "TEAM" {
        emit_project(s, org, project_id, "note:changed", &base).await;
        return;
    }
    emit_users(s, &[author.to_string()], "note:changed", &base).await;
    if previous == Some("TEAM") && action != "deleted" {
        emit_project(s, org, project_id, "note:changed", &json!({ "projectId": project_id, "action": "deleted", "noteId": note_id, "hiddenFor": author })).await;
    }
}

fn r(m: MethodRouter<AppState>) -> MethodRouter<AppState> {
    m.fallback(crate::error::not_found)
}

async fn list(State(s): State<AppState>, me: Me, Path((org, project_id)): Path<(String, String)>, q: Query) -> Reply {
    access(&s, &org, &project_id, &me.id).await?;
    let (take, cursor) = page_args(q.get("before").map(String::as_str), q.get("limit").map(String::as_str), 200, 500);
    let rows = many_json(
        sqlx::query_scalar(sql(format!(
            r#"select {} from "ProjectNote" n where n."projectId" = $1 and (n.visibility = 'TEAM' or n."authorId" = $2) and ($3::timestamp is null or n."updatedAt" < $3)
               order by n.pinned desc, n."updatedAt" desc limit $4"#,
            note_json()
        )))
        .bind(&project_id)
        .bind(&me.id)
        .bind(cursor)
        .bind(take),
        &s.db,
    )
    .await?;
    ok(Value::Array(rows))
}

fn check(body: &Body, with_version: bool) -> ApiResult<()> {
    let mut fields = vec![
        ("title", optional(body.get("title"), |v| str_len(v, 0, 200))),
        ("body", optional(body.get("body"), |v| str_len(v, 0, 20000))),
        ("visibility", optional(body.get("visibility"), |v| is_in(v, &["PRIVATE", "TEAM"]))),
        ("pinned", optional(body.get("pinned"), is_bool)),
    ];
    if with_version {
        fields.push(("version", optional(body.get("version"), |v| is_int(v) && min(v, 1.0))));
    }
    dto(&fields)
}

async fn create(State(s): State<AppState>, me: Me, Path((org, project_id)): Path<(String, String)>, body: Body) -> Reply {
    check(&body, false)?;
    access(&s, &org, &project_id, &me.id).await?;
    let at = now();
    let note: sqlx::types::Json<Value> = sqlx::query_scalar(sql(format!(
        r#"with n as (insert into "ProjectNote" (id, "projectId", "authorId", title, body, visibility, pinned, version, "createdAt", "updatedAt")
           values ($1, $2, $3, $4, $5, $6, $7, 1, $8, $8) returning *) select {} from n"#,
        note_json()
    )))
    .bind(uuid())
    .bind(&project_id)
    .bind(&me.id)
    .bind(js_slice(body.str("title").unwrap_or_default(), 200))
    .bind(body.str("body").unwrap_or_default())
    .bind(normalize_visibility(body.get("visibility"), "PRIVATE"))
    .bind(body.get("pinned").and_then(Value::as_bool).unwrap_or(false))
    .bind(at)
    .fetch_one(&s.db)
    .await?;
    let note = note.0;
    changed(&s, &org, &note, "created", Some(&note), None).await;
    created(note)
}

async fn update(State(s): State<AppState>, me: Me, Path((org, project_id, note_id)): Path<(String, String, String)>, body: Body) -> Reply {
    check(&body, true)?;
    let can_manage = access(&s, &org, &project_id, &me.id).await?;
    let current = find(&s, &project_id, &note_id, &me.id).await?;
    if !can_edit_note(field(&current, "authorId"), field(&current, "visibility"), &me.id, can_manage) {
        return Err(not_found("PERMISSION_DENIED", "Bu notu düzenleyemezsin."));
    }
    let version = current["version"].as_i64().unwrap_or(1);
    if version_conflicts(version, body.get("version")) {
        let mut error = ApiError::conflict("NOTE_CONFLICT", "Not başka bir cihazda değişti.");
        error.body["note"] = current.clone();
        return Err(error);
    }
    // TS: null başlık `.slice` TypeError, null gövde Prisma doğrulama hatası → 500.
    let title = match body.get("title") {
        None => None,
        Some(Value::String(t)) => Some(js_slice(t, 200)),
        Some(_) => return Err(ApiError::internal("title null")),
    };
    let text = match body.get("body") {
        None => None,
        Some(Value::String(t)) => Some(t.clone()),
        Some(_) => return Err(ApiError::internal("body null")),
    };
    let visibility = body.get("visibility").map(|v| normalize_visibility(Some(v), if field(&current, "visibility") == "TEAM" { "TEAM" } else { "PRIVATE" }));
    let pinned = body.get("pinned").map(|v| v.as_bool().unwrap_or(false));
    let note = one_json(
        sqlx::query_scalar(sql(format!(
            r#"with n as (update "ProjectNote" set version = $2, title = coalesce($3, title), body = coalesce($4, body), visibility = coalesce($5, visibility),
                 pinned = coalesce($6, pinned), "updatedAt" = $7 where id = $1 returning *) select {} from n"#,
            note_json()
        )))
        .bind(&note_id)
        .bind(next_version(version) as i32)
        .bind(&title)
        .bind(&text)
        .bind(visibility)
        .bind(pinned)
        .bind(now()),
        &s.db,
    )
    .await?
    .ok_or_else(|| missing("ProjectNote"))?;
    changed(&s, &org, &note, "updated", Some(&note), Some(field(&current, "visibility"))).await;
    ok(note)
}

async fn remove(State(s): State<AppState>, me: Me, Path((org, project_id, note_id)): Path<(String, String, String)>) -> Reply {
    let can_manage = access(&s, &org, &project_id, &me.id).await?;
    let current = find(&s, &project_id, &note_id, &me.id).await?;
    if !can_edit_note(field(&current, "authorId"), field(&current, "visibility"), &me.id, can_manage) {
        return Err(not_found("PERMISSION_DENIED", "Bu notu silemezsin."));
    }
    let done = sqlx::query(r#"delete from "ProjectNote" where id = $1"#).bind(&note_id).execute(&s.db).await?;
    if done.rows_affected() == 0 {
        return Err(missing("ProjectNote"));
    }
    changed(&s, &org, &current, "deleted", None, None).await;
    ok(json!({ "ok": true }))
}

pub fn routes() -> Router<AppState> {
    let base = "/organizations/{organizationId}/projects/{projectId}/notes";
    Router::new()
        .route(base, r(get(list).post(create)))
        .route(&format!("{base}/{{noteId}}"), r(axum::routing::patch(update).delete(remove)))
}

#[cfg(test)]
mod tests {
    use super::*;

    // notes.spec.ts ile aynı kurallar.
    #[test]
    fn kurallar() {
        assert_eq!(normalize_visibility(Some(&json!("TEAM")), "PRIVATE"), "TEAM");
        assert_eq!(normalize_visibility(Some(&json!("x")), "PRIVATE"), "PRIVATE");
        assert_eq!(normalize_visibility(None, "TEAM"), "TEAM");
        assert!(note_visible_to("a", "TEAM", "b"));
        assert!(!note_visible_to("a", "PRIVATE", "b"));
        assert!(can_edit_note("a", "PRIVATE", "a", false));
        assert!(!can_edit_note("a", "PRIVATE", "b", true));
        assert!(can_edit_note("a", "TEAM", "b", true));
        assert!(!version_conflicts(3, None));
        assert!(!version_conflicts(3, Some(&json!(3))));
        assert!(version_conflicts(3, Some(&json!(2))));
        assert_eq!(next_version(1), 2);
        assert_eq!(next_version(0), 2);
    }
}
