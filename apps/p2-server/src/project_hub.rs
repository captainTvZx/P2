//! Kütüphane vitrini ("hub") — `server/src/projectHub.ts` karşılığı: elle yazılan aktivite
//! gönderileri, başarımlar, incelemeler ve oynama süresi. Sürüm/parça işleri releases.rs'te.

use axum::Router;
use axum::extract::{Path, State};
use axum::routing::{MethodRouter, get, post};
use serde_json::{Value, json};

use crate::AppState;
use crate::ctx::{Body, Me, Reply, created, many_json, not_found, now, ok, one_json, sql, uuid};
use crate::error::{ApiError, ApiResult};
use crate::perm;
use crate::projects::{dto, is_bool, iso, js_trim, missing, optional, str_len, user4};
use crate::realtime::emit_project;

struct Perms {
    post: bool,
    manage: bool,
}

/// Gönderi: sürüm yayınlayan ya da projeyi yöneten; başarım tanımı: yönetici; inceleme/kazanım: herkes.
async fn access(s: &AppState, org: &str, project_id: &str, user_id: &str) -> ApiResult<Perms> {
    perm::require_project_access(&s.db, org, user_id, project_id).await?;
    let found: Option<i32> = sqlx::query_scalar(r#"select 1 from "Project" where id = $1 and "organizationId" = $2 and "deletedAt" is null"#)
        .bind(project_id)
        .bind(org)
        .fetch_optional(&s.db)
        .await?;
    if found.is_none() {
        return Err(not_found("PROJECT_NOT_FOUND", "Proje bulunamadı."));
    }
    let eff = perm::effective_in(&s.db, project_id, user_id).await?;
    let manage = eff.has("channel.manage");
    let publish = eff.has("release.publish");
    Ok(Perms { post: manage || publish, manage })
}

async fn changed(s: &AppState, org: &str, project_id: &str, what: &str) {
    emit_project(s, org, project_id, "hub:changed", &json!({ "projectId": project_id, "what": what })).await;
}

fn denied(message: &str) -> ApiError {
    not_found("PERMISSION_DENIED", message)
}

fn post_json() -> String {
    format!(
        r#"json_build_object('id', x.id, 'projectId', x."projectId", 'authorId', x."authorId", 'content', x.content, 'kind', x.kind, 'releaseId', x."releaseId",
           'createdAt', {}, 'updatedAt', {}, 'author', {})"#,
        iso(r#"x."createdAt""#),
        iso(r#"x."updatedAt""#),
        user4(r#"x."authorId""#)
    )
}

/// Başarım + kazanımlar (`ordered`: kazanım tarihine göre azalan).
fn achievement_json(ordered: bool) -> String {
    format!(
        r#"json_build_object('id', x.id, 'projectId', x."projectId", 'title', x.title, 'description', x.description, 'icon', x.icon, 'createdById', x."createdById",
           'createdAt', {}, 'unlocks', coalesce((select json_agg(json_build_object('achievementId', au."achievementId", 'userId', au."userId", 'unlockedAt', {}, 'user', {}){})
             from "AchievementUnlock" au where au."achievementId" = x.id), '[]'::json))"#,
        iso(r#"x."createdAt""#),
        iso(r#"au."unlockedAt""#),
        user4(r#"au."userId""#),
        if ordered { r#" order by au."unlockedAt" desc"# } else { "" }
    )
}

fn review_json() -> String {
    format!(
        r#"json_build_object('id', x.id, 'projectId', x."projectId", 'userId', x."userId", 'recommended', x.recommended, 'body', x.body,
           'createdAt', {}, 'updatedAt', {}, 'user', {})"#,
        iso(r#"x."createdAt""#),
        iso(r#"x."updatedAt""#),
        user4(r#"x."userId""#)
    )
}

fn unlocked_by(mut row: Value, user_id: &str) -> Value {
    let mine = row["unlocks"].as_array().is_some_and(|list| list.iter().any(|u| u["userId"].as_str() == Some(user_id)));
    if let Some(obj) = row.as_object_mut() {
        obj.insert("unlockedByMe".into(), json!(mine));
    }
    row
}

fn r(m: MethodRouter<AppState>) -> MethodRouter<AppState> {
    m.fallback(crate::error::not_found)
}

async fn overview(State(s): State<AppState>, me: Me, Path((org, project_id)): Path<(String, String)>) -> Reply {
    let perms = access(&s, &org, &project_id, &me.id).await?;
    let posts = many_json(
        sqlx::query_scalar(sql(format!(r#"select {} from "ProjectPost" x where x."projectId" = $1 order by x."createdAt" desc limit 60"#, post_json()))).bind(&project_id),
        &s.db,
    )
    .await?;
    let achievements = many_json(
        sqlx::query_scalar(sql(format!(r#"select {} from "ProjectAchievement" x where x."projectId" = $1 order by x."createdAt" asc"#, achievement_json(true)))).bind(&project_id),
        &s.db,
    )
    .await?;
    let reviews = many_json(
        sqlx::query_scalar(sql(format!(r#"select {} from "ProjectReview" x where x."projectId" = $1 order by x."updatedAt" desc"#, review_json()))).bind(&project_id),
        &s.db,
    )
    .await?;
    let (mine, last): (Option<i64>, Option<String>) = sqlx::query_as(sql(format!(
        r#"select sum("playSeconds")::bigint, {} from "ReleaseInstall" where "projectId" = $1 and "userId" = $2"#,
        iso(r#"max("lastLaunchedAt")"#)
    )))
    .bind(&project_id)
    .bind(&me.id)
    .fetch_one(&s.db)
    .await?;
    let team: Vec<(String, Option<i64>)> = sqlx::query_as(r#"select "userId", sum("playSeconds")::bigint from "ReleaseInstall" where "projectId" = $1 and "playSeconds" > 0 group by "userId""#)
        .bind(&project_id)
        .fetch_all(&s.db)
        .await?;
    ok(json!({
        "posts": posts,
        "achievements": achievements.into_iter().map(|row| unlocked_by(row, &me.id)).collect::<Vec<_>>(),
        "reviews": reviews,
        "playtime": {
            "mine": mine.unwrap_or(0),
            "lastPlayedAt": last,
            "team": team.iter().map(|(_, n)| n.unwrap_or(0)).sum::<i64>(),
            "players": team.len(),
        },
        "permissions": { "post": perms.post, "manage": perms.manage, "review": true },
    }))
}

// ---- aktivite gönderileri ----

async fn create_post(State(s): State<AppState>, me: Me, Path((org, project_id)): Path<(String, String)>, body: Body) -> Reply {
    dto(&[("content", str_len(body.get("content"), 1, 4000))])?;
    let perms = access(&s, &org, &project_id, &me.id).await?;
    if !perms.post {
        return Err(denied("Aktivite yazma yetkin yok."));
    }
    let at = now();
    let post: sqlx::types::Json<Value> = sqlx::query_scalar(sql(format!(
        r#"with x as (insert into "ProjectPost" (id, "projectId", "authorId", content, "createdAt", "updatedAt") values ($1, $2, $3, $4, $5, $5) returning *) select {} from x"#,
        post_json()
    )))
    .bind(uuid())
    .bind(&project_id)
    .bind(&me.id)
    .bind(js_trim(body.str("content").unwrap_or_default()))
    .bind(at)
    .fetch_one(&s.db)
    .await?;
    changed(&s, &org, &project_id, "posts").await;
    created(post.0)
}

async fn post_author(s: &AppState, post_id: &str, project_id: &str) -> ApiResult<String> {
    let author: Option<String> = sqlx::query_scalar(r#"select "authorId" from "ProjectPost" where id = $1 and "projectId" = $2 limit 1"#)
        .bind(post_id)
        .bind(project_id)
        .fetch_optional(&s.db)
        .await?;
    author.ok_or_else(|| not_found("POST_NOT_FOUND", "Gönderi bulunamadı."))
}

async fn update_post(State(s): State<AppState>, me: Me, Path((org, project_id, post_id)): Path<(String, String, String)>, body: Body) -> Reply {
    dto(&[("content", str_len(body.get("content"), 1, 4000))])?;
    let perms = access(&s, &org, &project_id, &me.id).await?;
    let author = post_author(&s, &post_id, &project_id).await?;
    if author != me.id && !perms.manage {
        return Err(denied("Bu gönderiyi düzenleyemezsin."));
    }
    let post = one_json(
        sqlx::query_scalar(sql(format!(r#"with x as (update "ProjectPost" set content = $2, "updatedAt" = $3 where id = $1 returning *) select {} from x"#, post_json())))
            .bind(&post_id)
            .bind(js_trim(body.str("content").unwrap_or_default()))
            .bind(now()),
        &s.db,
    )
    .await?
    .ok_or_else(|| missing("ProjectPost"))?;
    changed(&s, &org, &project_id, "posts").await;
    ok(post)
}

async fn delete_post(State(s): State<AppState>, me: Me, Path((org, project_id, post_id)): Path<(String, String, String)>) -> Reply {
    let perms = access(&s, &org, &project_id, &me.id).await?;
    let author = post_author(&s, &post_id, &project_id).await?;
    if author != me.id && !perms.manage {
        return Err(denied("Bu gönderiyi silemezsin."));
    }
    let done = sqlx::query(r#"delete from "ProjectPost" where id = $1"#).bind(&post_id).execute(&s.db).await?;
    if done.rows_affected() == 0 {
        return Err(missing("ProjectPost"));
    }
    changed(&s, &org, &project_id, "posts").await;
    ok(json!({ "ok": true }))
}

// ---- başarımlar ----

fn check_achievement(body: &Body) -> ApiResult<()> {
    dto(&[
        ("title", optional(body.get("title"), |v| str_len(v, 1, 60))),
        ("description", optional(body.get("description"), |v| str_len(v, 0, 240))),
        ("icon", optional(body.get("icon"), |v| str_len(v, 1, 8))),
    ])
}

async fn create_achievement(State(s): State<AppState>, me: Me, Path((org, project_id)): Path<(String, String)>, body: Body) -> Reply {
    check_achievement(&body)?;
    let perms = access(&s, &org, &project_id, &me.id).await?;
    if !perms.manage {
        return Err(denied("Başarım tanımlama yetkin yok."));
    }
    let title = js_trim(body.str("title").unwrap_or_default()).to_string();
    if title.is_empty() {
        return Err(not_found("ACHIEVEMENT_TITLE", "Başarım adı gerekli."));
    }
    let description = body.str("description").map(js_trim).filter(|d| !d.is_empty()).map(str::to_string);
    let icon = js_trim(body.str("icon").filter(|i| !i.is_empty()).unwrap_or("🏆")).to_string();
    let icon = if icon.is_empty() { "🏆".to_string() } else { icon };
    let row: sqlx::types::Json<Value> = sqlx::query_scalar(sql(format!(
        r#"with x as (insert into "ProjectAchievement" (id, "projectId", "createdById", title, description, icon, "createdAt") values ($1, $2, $3, $4, $5, $6, $7) returning *) select {} from x"#,
        achievement_json(false)
    )))
    .bind(uuid())
    .bind(&project_id)
    .bind(&me.id)
    .bind(&title)
    .bind(&description)
    .bind(&icon)
    .bind(now())
    .fetch_one(&s.db)
    .await?;
    changed(&s, &org, &project_id, "achievements").await;
    let mut row = row.0;
    if let Some(obj) = row.as_object_mut() {
        obj.insert("unlockedByMe".into(), json!(false));
    }
    created(row)
}

async fn achievement_exists(s: &AppState, achievement_id: &str, project_id: &str) -> ApiResult<()> {
    let found: Option<String> = sqlx::query_scalar(r#"select id from "ProjectAchievement" where id = $1 and "projectId" = $2 limit 1"#)
        .bind(achievement_id)
        .bind(project_id)
        .fetch_optional(&s.db)
        .await?;
    found.map(|_| ()).ok_or_else(|| not_found("ACHIEVEMENT_NOT_FOUND", "Başarım bulunamadı."))
}

/// `dto.x.trim()` — null gelirse TS'te TypeError (500).
fn trimmed(body: &Body, key: &str) -> ApiResult<Option<String>> {
    match body.get(key) {
        None => Ok(None),
        Some(Value::String(v)) => Ok(Some(js_trim(v).to_string())),
        Some(_) => Err(ApiError::internal(format!("{key} null"))),
    }
}

async fn update_achievement(State(s): State<AppState>, me: Me, Path((org, project_id, achievement_id)): Path<(String, String, String)>, body: Body) -> Reply {
    check_achievement(&body)?;
    let perms = access(&s, &org, &project_id, &me.id).await?;
    if !perms.manage {
        return Err(denied("Başarım düzenleme yetkin yok."));
    }
    achievement_exists(&s, &achievement_id, &project_id).await?;
    let title = trimmed(&body, "title")?.filter(|t| !t.is_empty());
    let description = trimmed(&body, "description")?.map(|d| if d.is_empty() { None } else { Some(d) });
    let icon = trimmed(&body, "icon")?.filter(|i| !i.is_empty());
    let row = one_json(
        sqlx::query_scalar(sql(format!(
            r#"with x as (update "ProjectAchievement" set title = coalesce($2, title), description = case when $3 then $4 else description end, icon = coalesce($5, icon)
               where id = $1 returning *) select {} from x"#,
            achievement_json(false)
        )))
        .bind(&achievement_id)
        .bind(&title)
        .bind(description.is_some())
        .bind(description.flatten())
        .bind(&icon),
        &s.db,
    )
    .await?
    .ok_or_else(|| missing("ProjectAchievement"))?;
    changed(&s, &org, &project_id, "achievements").await;
    ok(unlocked_by(row, &me.id))
}

async fn delete_achievement(State(s): State<AppState>, me: Me, Path((org, project_id, achievement_id)): Path<(String, String, String)>) -> Reply {
    let perms = access(&s, &org, &project_id, &me.id).await?;
    if !perms.manage {
        return Err(denied("Başarım silme yetkin yok."));
    }
    let done = sqlx::query(r#"delete from "ProjectAchievement" where id = $1 and "projectId" = $2"#).bind(&achievement_id).bind(&project_id).execute(&s.db).await?;
    if done.rows_affected() == 0 {
        return Err(not_found("ACHIEVEMENT_NOT_FOUND", "Başarım bulunamadı."));
    }
    changed(&s, &org, &project_id, "achievements").await;
    ok(json!({ "ok": true }))
}

/// Kişi kendi kazanımını işaretler/kaldırır.
async fn unlock(State(s): State<AppState>, me: Me, Path((org, project_id, achievement_id)): Path<(String, String, String)>, body: Body) -> Reply {
    dto(&[("unlocked", is_bool(body.get("unlocked")))])?;
    let unlocked = body.get("unlocked").and_then(Value::as_bool).unwrap_or(false);
    access(&s, &org, &project_id, &me.id).await?;
    achievement_exists(&s, &achievement_id, &project_id).await?;
    if unlocked {
        sqlx::query(r#"insert into "AchievementUnlock" ("achievementId", "userId", "unlockedAt") values ($1, $2, $3) on conflict ("achievementId", "userId") do nothing"#)
            .bind(&achievement_id)
            .bind(&me.id)
            .bind(now())
            .execute(&s.db)
            .await?;
    } else {
        sqlx::query(r#"delete from "AchievementUnlock" where "achievementId" = $1 and "userId" = $2"#).bind(&achievement_id).bind(&me.id).execute(&s.db).await?;
    }
    changed(&s, &org, &project_id, "achievements").await;
    created(json!({ "ok": true, "unlocked": unlocked }))
}

// ---- incelemeler (kişi başına bir tane) ----

async fn review(State(s): State<AppState>, me: Me, Path((org, project_id)): Path<(String, String)>, body: Body) -> Reply {
    dto(&[
        ("recommended", is_bool(body.get("recommended"))),
        ("body", optional(body.get("body"), |v| str_len(v, 0, 2000))),
    ])?;
    access(&s, &org, &project_id, &me.id).await?;
    let recommended = body.get("recommended").and_then(Value::as_bool).unwrap_or(false);
    let text = body.str("body").map(js_trim).filter(|b| !b.is_empty()).map(str::to_string);
    let at = now();
    let row: sqlx::types::Json<Value> = sqlx::query_scalar(sql(format!(
        r#"with x as (insert into "ProjectReview" (id, "projectId", "userId", recommended, body, "createdAt", "updatedAt") values ($1, $2, $3, $4, $5, $6, $6)
           on conflict ("projectId", "userId") do update set recommended = excluded.recommended, body = excluded.body, "updatedAt" = excluded."updatedAt" returning *) select {} from x"#,
        review_json()
    )))
    .bind(uuid())
    .bind(&project_id)
    .bind(&me.id)
    .bind(recommended)
    .bind(&text)
    .bind(at)
    .fetch_one(&s.db)
    .await?;
    changed(&s, &org, &project_id, "reviews").await;
    ok(row.0)
}

async fn delete_review(State(s): State<AppState>, me: Me, Path((org, project_id)): Path<(String, String)>) -> Reply {
    access(&s, &org, &project_id, &me.id).await?;
    sqlx::query(r#"delete from "ProjectReview" where "projectId" = $1 and "userId" = $2"#).bind(&project_id).bind(&me.id).execute(&s.db).await?;
    changed(&s, &org, &project_id, "reviews").await;
    ok(json!({ "ok": true }))
}

pub fn routes() -> Router<AppState> {
    let base = "/organizations/{organizationId}/projects/{projectId}/hub";
    Router::new()
        .route(base, r(get(overview)))
        .route(&format!("{base}/posts"), r(post(create_post)))
        .route(&format!("{base}/posts/{{postId}}"), r(axum::routing::patch(update_post).delete(delete_post)))
        .route(&format!("{base}/achievements"), r(post(create_achievement)))
        .route(&format!("{base}/achievements/{{achievementId}}"), r(axum::routing::patch(update_achievement).delete(delete_achievement)))
        .route(&format!("{base}/achievements/{{achievementId}}/unlock"), r(post(unlock)))
        .route(&format!("{base}/reviews"), r(axum::routing::put(review).delete(delete_review)))
}
