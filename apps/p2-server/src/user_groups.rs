//! Kullanıcı grupları (@devs, @art) — `server/src/userGroups.ts` karşılığı. Grupların kendi
//! yetkisi yoktur; gruba atanan roller gruptaki herkese geçer. Listeyi herkes görür,
//! yalnız "kullanıcı gruplarını yönet" yetkisi olan değiştirir.

use axum::Router;
use axum::extract::{Path, State};
use axum::routing::{delete, get, patch};
use serde_json::{Value, json};

use crate::AppState;
use crate::ctx::{Body, Me, Reply, created, not_found, now, ok, sql, uuid};
use crate::error::{ApiError, ApiResult};
use crate::perm;
use crate::roles::{check_dto, null_crash, notify_permissions};
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

async fn membership(s: &AppState, organization_id: &str, user_id: &str, staff_only: bool) -> ApiResult<()> {
    crate::activity::membership(&s.db, organization_id, user_id).await?;
    if staff_only {
        perm::require(&s.db, organization_id, user_id, "usergroup.manage").await?;
    }
    Ok(())
}

async fn group(s: &AppState, organization_id: &str, group_id: &str) -> ApiResult<(Value, String)> {
    let row: Option<(sqlx::types::Json<Value>, String)> =
        sqlx::query_as(sql(format!(r#"select {}, g.handle from "UserGroup" g where g.id = $1 and g."organizationId" = $2"#, group_json())))
            .bind(group_id)
            .bind(organization_id)
            .fetch_optional(&s.db)
            .await?;
    let Some((json, handle)) = row else {
        return Err(not_found("USER_GROUP_NOT_FOUND", "Rol bulunamadı."));
    };
    Ok((json.0, handle))
}

async fn handle_taken(s: &AppState, organization_id: &str, handle: &str) -> ApiResult<bool> {
    let hit: Option<i32> = sqlx::query_scalar(r#"select 1 from "UserGroup" where "organizationId" = $1 and handle = $2"#)
        .bind(organization_id)
        .bind(handle)
        .fetch_optional(&s.db)
        .await?;
    Ok(hit.is_some())
}

async fn list(State(s): State<AppState>, me: Me, Path(organization_id): Path<String>) -> Reply {
    membership(&s, &organization_id, &me.id, false).await?;
    let rows = crate::ctx::many_json(
        sqlx::query_scalar(sql(format!(r#"select {} from "UserGroup" g where g."organizationId" = $1 order by g.handle asc"#, group_json()))).bind(&organization_id),
        &s.db,
    )
    .await?;
    ok(Value::Array(rows))
}

async fn create(State(s): State<AppState>, me: Me, Path(organization_id): Path<String>, Body(body): Body) -> Reply {
    check_dto(&body, false, HANDLE_MESSAGE)?;
    membership(&s, &organization_id, &me.id, true).await?;
    let handle = body["handle"].as_str().unwrap_or_default().trim().to_lowercase();
    if handle_taken(&s, &organization_id, &handle).await? {
        return Err(ApiError::conflict("HANDLE_TAKEN", "Bu etiket zaten kullanılıyor.").field("handle"));
    }
    // Prisma `equals` + `mode: 'insensitive'` PostgreSQL'de ILIKE'a dönüşür (`_` joker karakter).
    let user: Option<i32> = sqlx::query_scalar(
        r#"select 1 from "User" u where u.username ilike $2
             and exists (select 1 from "OrganizationMember" m where m."userId" = u.id and m."organizationId" = $1) limit 1"#,
    )
    .bind(&organization_id)
    .bind(&handle)
    .fetch_optional(&s.db)
    .await?;
    if user.is_some() {
        return Err(ApiError::conflict("HANDLE_IS_USER", "Bu etiket bir kullanıcı adıyla çakışıyor.").field("handle"));
    }
    let text = |k: &str| body.get(k).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).map(str::to_string);
    let id = uuid();
    sqlx::query(
        r#"insert into "UserGroup" (id, "organizationId", handle, name, description, color, "createdById", "createdAt", "updatedAt")
           values ($1, $2, $3, $4, $5, $6, $7, $8, $8)"#,
    )
    .bind(&id)
    .bind(&organization_id)
    .bind(&handle)
    .bind(body["name"].as_str().unwrap_or_default().trim())
    .bind(text("description"))
    .bind(text("color"))
    .bind(&me.id)
    .bind(now())
    .execute(&s.db)
    .await?;
    let (group, _) = group(&s, &organization_id, &id).await?;
    crate::realtime::emit_org(&s, &organization_id, "usergroup:changed", &json!({ "group": group })).await;
    created(group)
}

async fn update(State(s): State<AppState>, me: Me, Path((organization_id, group_id)): Path<(String, String)>, Body(body): Body) -> Reply {
    check_dto(&body, true, HANDLE_MESSAGE)?;
    membership(&s, &organization_id, &me.id, true).await?;
    let (_, current) = group(&s, &organization_id, &group_id).await?;
    null_crash(&body, "handle")?;
    let mut handle: Option<String> = None;
    if let Some(h) = body.get("handle").and_then(Value::as_str) {
        let h = h.trim().to_lowercase();
        if h != current && handle_taken(&s, &organization_id, &h).await? {
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
    let (group, _) = group(&s, &organization_id, &group_id).await?;
    crate::realtime::emit_org(&s, &organization_id, "usergroup:changed", &json!({ "group": group })).await;
    ok(group)
}

async fn remove(State(s): State<AppState>, me: Me, Path((organization_id, group_id)): Path<(String, String)>) -> Reply {
    membership(&s, &organization_id, &me.id, true).await?;
    group(&s, &organization_id, &group_id).await?;
    let n = sqlx::query(r#"delete from "UserGroup" where id = $1"#).bind(&group_id).execute(&s.db).await?.rows_affected();
    if n == 0 {
        return Err(ApiError::internal("UserGroup silinemedi"));
    }
    crate::realtime::emit_org(&s, &organization_id, "usergroup:removed", &json!({ "groupId": group_id })).await;
    ok(json!({ "id": group_id, "removed": true }))
}

// Yalnızca kuruluş üyeleri gruba eklenebilir.
async fn add_members(State(s): State<AppState>, me: Me, Path((organization_id, group_id)): Path<(String, String)>, Body(body): Body) -> Reply {
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
    membership(&s, &organization_id, &me.id, true).await?;
    group(&s, &organization_id, &group_id).await?;
    let allowed: Vec<String> = sqlx::query_scalar(r#"select "userId" from "OrganizationMember" where "organizationId" = $1 and "userId" = any($2)"#)
        .bind(&organization_id)
        .bind(&ids)
        .fetch_all(&s.db)
        .await?;
    if allowed.is_empty() {
        return Err(ApiError::conflict("NO_MEMBERS", "Eklenecek kuruluş üyesi bulunamadı."));
    }
    sqlx::query(r#"insert into "UserGroupMember" ("groupId", "userId", "addedAt") select $1, u, $3 from unnest($2::text[]) u on conflict do nothing"#)
        .bind(&group_id)
        .bind(&allowed)
        .bind(now())
        .execute(&s.db)
        .await?;
    let (group, _) = group(&s, &organization_id, &group_id).await?;
    crate::realtime::emit_org(&s, &organization_id, "usergroup:changed", &json!({ "group": group })).await;
    notify_permissions(&s, &organization_id, &allowed).await;
    created(group)
}

async fn remove_member(State(s): State<AppState>, me: Me, Path((organization_id, group_id, user_id)): Path<(String, String, String)>) -> Reply {
    membership(&s, &organization_id, &me.id, true).await?;
    group(&s, &organization_id, &group_id).await?;
    sqlx::query(r#"delete from "UserGroupMember" where "groupId" = $1 and "userId" = $2"#).bind(&group_id).bind(&user_id).execute(&s.db).await?;
    let (group, _) = group(&s, &organization_id, &group_id).await?;
    crate::realtime::emit_org(&s, &organization_id, "usergroup:changed", &json!({ "group": group })).await;
    notify_permissions(&s, &organization_id, std::slice::from_ref(&user_id)).await;
    ok(group)
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/organizations/{organizationId}/user-groups", get(list).post(create))
        .route("/organizations/{organizationId}/user-groups/{groupId}", patch(update).delete(remove))
        .route("/organizations/{organizationId}/user-groups/{groupId}/members", axum::routing::post(add_members))
        .route("/organizations/{organizationId}/user-groups/{groupId}/members/{userId}", delete(remove_member))
}
