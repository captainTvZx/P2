//! Kullanıcı grupları (@devs, @art) — `server/src/userGroups.ts` karşılığı. Grupların kendi
//! yetkisi yoktur; gruba atanan roller gruptaki herkese geçer. Listeyi herkes görür,
//! yalnız "kullanıcı gruplarını yönet" yetkisi olan değiştirir.

use axum::Router;
use axum::extract::State;
use axum::routing::{delete, get, patch, post};
use serde_json::{Value, json};

use crate::AppState;
use crate::ctx::{Body, Reply, created, not_found, now, ok, sql, uuid};
use crate::error::{ApiError, ApiResult};
use crate::scope::Scope;
use crate::roles::{both, check_dto, null_crash, notify_permissions};
use crate::validate::Checker;

const HANDLE_MESSAGE: &str = "Etiket yalnızca küçük harf, rakam, tire ve alt çizgi içerebilir.";

/// `shape` + `flatten` (alias `g`).
pub fn group_json() -> String {
    format!(
        r#"json_build_object('id', g.id, 'handle', g.handle, 'name', g.name, 'description', g.description, 'color', g.color, 'createdAt', {}, 'updatedAt', {},
             'roles', coalesce((select json_agg(json_build_object('id', r.id, 'handle', r.handle, 'name', r.name, 'color', r.color))
                 from "RoleGroup" rg join "Role" r on r.id = rg."roleId" where rg."groupId" = g.id), '[]'::json),
             'createdBy', (select json_build_object('id', c.id, 'displayName', c."displayName") from "User" c where c.id = g."createdById"),
             'members', coalesce((select json_agg(json_build_object('id', u.id, 'username', u.username, 'displayName', u."displayName", 'avatarUrl', u."avatarUrl", 'status', u.status) order by gm."addedAt" asc)
                 from "UserGroupMember" gm join "User" u on u.id = gm."userId" where gm."groupId" = g.id), '[]'::json))"#,
        crate::iso!(r#"g."createdAt""#),
        crate::iso!(r#"g."updatedAt""#)
    )
}
async fn group(s: &AppState, project_id: &str, group_id: &str) -> ApiResult<(Value, String)> {
    let row: Option<(sqlx::types::Json<Value>, String)> =
        sqlx::query_as(sql(format!(r#"select {}, g.handle from "UserGroup" g where g.id = $1 and g."projectId" = $2"#, group_json())))
            .bind(group_id)
            .bind(project_id)
            .fetch_optional(&s.db)
            .await?;
    let Some((json, handle)) = row else {
        return Err(not_found("USER_GROUP_NOT_FOUND", "Rol bulunamadı."));
    };
    Ok((json.0, handle))
}

async fn handle_taken(s: &AppState, project_id: &str, handle: &str) -> ApiResult<bool> {
    let hit: Option<i32> = sqlx::query_scalar(r#"select 1 from "UserGroup" where "projectId" = $1 and handle = $2"#)
        .bind(project_id)
        .bind(handle)
        .fetch_optional(&s.db)
        .await?;
    Ok(hit.is_some())
}

async fn changed(s: &AppState, sc: &Scope, group: &Value) {
    crate::realtime::emit_project(s, &sc.org, &sc.project, "usergroup:changed", &json!({ "projectId": sc.project, "group": group })).await;
}

async fn list(State(s): State<AppState>, sc: Scope) -> Reply {
    let rows = crate::ctx::many_json(
        sqlx::query_scalar(sql(format!(r#"select {} from "UserGroup" g where g."projectId" = $1 order by g.handle asc"#, group_json()))).bind(&sc.project),
        &s.db,
    )
    .await?;
    ok(Value::Array(rows))
}

async fn create(State(s): State<AppState>, mut sc: Scope, Body(body): Body) -> Reply {
    check_dto(&body, false, HANDLE_MESSAGE)?;
    sc.require(&s, "usergroup.manage").await?;
    let handle = body["handle"].as_str().unwrap_or_default().trim().to_lowercase();
    if handle_taken(&s, &sc.project, &handle).await? {
        return Err(ApiError::conflict("HANDLE_TAKEN", "Bu etiket zaten kullanılıyor.").field("handle"));
    }
    // Prisma `equals` + `mode: 'insensitive'` PostgreSQL'de ILIKE'a dönüşür (`_` joker karakter).
    let user: Option<i32> = sqlx::query_scalar(
        r#"select 1 from "User" u where u.username ilike $2
             and exists (select 1 from "ProjectMember" m where m."userId" = u.id and m."projectId" = $1) limit 1"#,
    )
    .bind(&sc.project)
    .bind(&handle)
    .fetch_optional(&s.db)
    .await?;
    if user.is_some() {
        return Err(ApiError::conflict("HANDLE_IS_USER", "Bu etiket bir kullanıcı adıyla çakışıyor.").field("handle"));
    }
    let text = |k: &str| body.get(k).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).map(str::to_string);
    let id = uuid();
    sqlx::query(
        r#"insert into "UserGroup" (id, "organizationId", "projectId", handle, name, description, color, "createdById", "createdAt", "updatedAt")
           values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)"#,
    )
    .bind(&id)
    .bind(&sc.org)
    .bind(&sc.project)
    .bind(&handle)
    .bind(body["name"].as_str().unwrap_or_default().trim())
    .bind(text("description"))
    .bind(text("color"))
    .bind(&sc.user)
    .bind(now())
    .execute(&s.db)
    .await?;
    let (group, _) = group(&s, &sc.project, &id).await?;
    changed(&s, &sc, &group).await;
    created(group)
}

async fn update(State(s): State<AppState>, mut sc: Scope, Body(body): Body) -> Reply {
    check_dto(&body, true, HANDLE_MESSAGE)?;
    sc.require(&s, "usergroup.manage").await?;
    let group_id = sc.p("groupId");
    let (_, current) = group(&s, &sc.project, &group_id).await?;
    null_crash(&body, "handle")?;
    let mut handle: Option<String> = None;
    if let Some(h) = body.get("handle").and_then(Value::as_str) {
        let h = h.trim().to_lowercase();
        if h != current && handle_taken(&s, &sc.project, &h).await? {
            return Err(ApiError::conflict("HANDLE_TAKEN", "Bu etiket zaten kullanılıyor.").field("handle"));
        }
        handle = Some(h);
    }
    for k in ["name", "description", "color"] {
        null_crash(&body, k)?;
    }
    let s_of = |k: &str| body.get(k).and_then(Value::as_str).map(str::trim);
    sqlx::query(
        r#"update "UserGroup" set handle = coalesce($2, handle), name = coalesce($3, name),
             description = case when $4 then $5 else description end, color = case when $6 then $7 else color end, "updatedAt" = $8
           where id = $1"#,
    )
    .bind(&group_id)
    .bind(&handle)
    .bind(s_of("name"))
    .bind(s_of("description").is_some())
    .bind(s_of("description").filter(|v| !v.is_empty()))
    .bind(s_of("color").is_some())
    .bind(s_of("color").filter(|v| !v.is_empty()))
    .bind(now())
    .execute(&s.db)
    .await?;
    let (group, _) = group(&s, &sc.project, &group_id).await?;
    changed(&s, &sc, &group).await;
    ok(group)
}

async fn remove(State(s): State<AppState>, mut sc: Scope) -> Reply {
    sc.require(&s, "usergroup.manage").await?;
    let group_id = sc.p("groupId");
    group(&s, &sc.project, &group_id).await?;
    let n = sqlx::query(r#"delete from "UserGroup" where id = $1"#).bind(&group_id).execute(&s.db).await?.rows_affected();
    if n == 0 {
        return Err(ApiError::internal("UserGroup silinemedi"));
    }
    crate::realtime::emit_project(&s, &sc.org, &sc.project, "usergroup:removed", &json!({ "projectId": sc.project, "groupId": group_id })).await;
    ok(json!({ "id": group_id, "removed": true }))
}

// Yalnızca projenin üyeleri gruba eklenebilir.
async fn add_members(State(s): State<AppState>, mut sc: Scope, Body(body): Body) -> Reply {
    let mut c = Checker::new(&body);
    let list = body.get("userIds").and_then(Value::as_array);
    c.field(
        "userIds",
        &[
            (list.is_some_and(|l| l.iter().all(Value::is_string)), None),
            (list.is_none_or(|l| l.len() <= 200), None),
            (list.is_some(), None),
        ],
    );
    c.finish()?;
    let ids: Vec<String> = list.map(|l| l.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
    sc.require(&s, "usergroup.manage").await?;
    let group_id = sc.p("groupId");
    group(&s, &sc.project, &group_id).await?;
    let allowed: Vec<String> = sqlx::query_scalar(r#"select "userId" from "ProjectMember" where "projectId" = $1 and "userId" = any($2)"#)
        .bind(&sc.project)
        .bind(&ids)
        .fetch_all(&s.db)
        .await?;
    if allowed.is_empty() {
        return Err(ApiError::conflict("NO_MEMBERS", "Eklenecek proje üyesi bulunamadı."));
    }
    sqlx::query(r#"insert into "UserGroupMember" ("groupId", "userId", "addedAt") select $1, u, $3 from unnest($2::text[]) u on conflict do nothing"#)
        .bind(&group_id)
        .bind(&allowed)
        .bind(now())
        .execute(&s.db)
        .await?;
    let (group, _) = group(&s, &sc.project, &group_id).await?;
    changed(&s, &sc, &group).await;
    notify_permissions(&s, &sc.org, &sc.project, &allowed).await;
    created(group)
}

async fn remove_member(State(s): State<AppState>, mut sc: Scope) -> Reply {
    sc.require(&s, "usergroup.manage").await?;
    let (group_id, user_id) = (sc.p("groupId"), sc.p("userId"));
    group(&s, &sc.project, &group_id).await?;
    sqlx::query(r#"delete from "UserGroupMember" where "groupId" = $1 and "userId" = $2"#).bind(&group_id).bind(&user_id).execute(&s.db).await?;
    let (group, _) = group(&s, &sc.project, &group_id).await?;
    changed(&s, &sc, &group).await;
    notify_permissions(&s, &sc.org, &sc.project, std::slice::from_ref(&user_id)).await;
    ok(group)
}

pub fn routes() -> Router<AppState> {
    let r = Router::new();
    let r = both(r, "/user-groups", get(list).post(create));
    let r = both(r, "/user-groups/{groupId}", patch(update).delete(remove));
    let r = both(r, "/user-groups/{groupId}/members", post(add_members));
    both(r, "/user-groups/{groupId}/members/{userId}", delete(remove_member))
}
