//! Roller (yetki kümeleri) — `server/src/roles.ts` karşılığı. Roller kişilere ve kullanıcı
//! gruplarına atanır; silinemeyen iki temel rol vardır: kurucu ve herkes (@everyone).

use axum::Router;
use axum::extract::{Path, State};
use axum::routing::{get, patch, post};
use serde_json::{Value, json};

use crate::AppState;
use crate::ctx::{Body, Me, Reply, created, not_found, now, ok, sql, uuid};
use crate::error::{ApiError, ApiResult};
use crate::perm;
use crate::validate::{Checker, js_len};

/// `shape` + `flatten`: üyeler kullanıcıya, gruplar `{...group, memberCount}` biçimine açılır
/// (spread yüzünden `_count` da kalır). Alias `r`.
pub fn role_json() -> String {
    format!(
        r#"json_build_object('id', r.id, 'handle', r.handle, 'name', r.name, 'description', r.description, 'color', r.color,
             'everyone', r.everyone, 'founder', r.founder, 'permissions', r.permissions, 'createdAt', {}, 'updatedAt', {},
             'members', coalesce((select json_agg(json_build_object('id', u.id, 'username', u.username, 'displayName', u."displayName", 'avatarUrl', u."avatarUrl", 'status', u.status) order by rm."addedAt" asc)
                 from "RoleMember" rm join "User" u on u.id = rm."userId" where rm."roleId" = r.id), '[]'::json),
             'groups', coalesce((select json_agg(json_build_object('id', g.id, 'handle', g.handle, 'name', g.name, 'color', g.color,
                   '_count', json_build_object('members', (select count(*) from "UserGroupMember" x where x."groupId" = g.id)),
                   'memberCount', (select count(*) from "UserGroupMember" x where x."groupId" = g.id)))
                 from "RoleGroup" rg join "UserGroup" g on g.id = rg."groupId" where rg."roleId" = r.id), '[]'::json))"#,
        crate::iso!(r#"r."createdAt""#),
        crate::iso!(r#"r."updatedAt""#)
    )
}

const HANDLE_MESSAGE: &str = "Rol kimliği yalnızca küçük harf, rakam, tire ve alt çizgi içerebilir.";

/// `^[a-z0-9][a-z0-9_-]*$`
pub fn handle_ok(v: &str) -> bool {
    let mut b = v.bytes();
    b.next().is_some_and(|f| f.is_ascii_lowercase() || f.is_ascii_digit()) && b.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-')
}

/// İsteğe bağlı dizgi alanı uzunluk kontrolü: (uzunluk geçti, dizgi mi). Yok/null → geçer.
pub fn opt_len(body: &serde_json::Map<String, Value>, name: &str, min: usize, max: usize) -> (bool, bool) {
    match body.get(name) {
        None | Some(Value::Null) => (true, true),
        Some(Value::String(s)) => ((min..=max).contains(&js_len(s)), true),
        Some(_) => (false, false),
    }
}

/// RoleDto / RolePatchDto (ve aynı kalıptaki UserGroupDto). `optional` → handle/name isteğe bağlı.
pub fn check_dto(body: &serde_json::Map<String, Value>, optional: bool, handle_message: &'static str) -> ApiResult<()> {
    let mut c = Checker::new(body);
    let present = |k: &str| !matches!(body.get(k), None | Some(Value::Null));
    let handle = c.str("handle");
    if optional && !present("handle") {
    } else {
        c.field(
            "handle",
            &[(handle.is_some_and(handle_ok), Some(handle_message)), (handle.is_some_and(|h| (2..=24).contains(&js_len(h))), None), (handle.is_some(), None)],
        );
    }
    if !(optional && !present("name")) {
        let name = c.str("name");
        c.field("name", &[(name.is_some_and(|n| (2..=40).contains(&js_len(n))), None), (name.is_some(), None)]);
    }
    let (len_ok, str_ok) = opt_len(body, "description", 0, 160);
    c.field("description", &[(len_ok, None), (str_ok, None)]);
    let (len_ok, str_ok) = opt_len(body, "color", 0, 20);
    c.field("color", &[(len_ok, None), (str_ok, None)]);
    c.finish()
}

async fn membership(s: &AppState, organization_id: &str, user_id: &str, permission: Option<&str>) -> ApiResult<()> {
    crate::activity::membership(&s.db, organization_id, user_id).await?;
    if let Some(p) = permission {
        perm::require(&s.db, organization_id, user_id, p).await?;
    }
    Ok(())
}

struct RoleRow {
    json: Value,
    handle: String,
    everyone: bool,
    founder: bool,
}

async fn role(s: &AppState, organization_id: &str, role_id: &str) -> ApiResult<RoleRow> {
    let row: Option<(sqlx::types::Json<Value>, String, bool, bool)> =
        sqlx::query_as(sql(format!(r#"select {}, r.handle, r.everyone, r.founder from "Role" r where r.id = $1 and r."organizationId" = $2"#, role_json())))
            .bind(role_id)
            .bind(organization_id)
            .fetch_optional(&s.db)
            .await?;
    let Some((json, handle, everyone, founder)) = row else {
        return Err(not_found("ROLE_NOT_FOUND", "Rol bulunamadı."));
    };
    Ok(RoleRow { json: json.0, handle, everyone, founder })
}

fn guard_base(role: &RoleRow, action: &str) -> ApiResult<()> {
    if role.everyone {
        return Err(ApiError::conflict("EVERYONE_PROTECTED", &format!("Herkes rolü {action}.")));
    }
    if role.founder {
        return Err(ApiError::conflict("FOUNDER_PROTECTED", &format!("Kurucu rolü {action}.")));
    }
    Ok(())
}

/// Kişilere `permissions:changed` gönderir (yetki hesabı düşerse o kişi atlanır).
pub async fn notify_permissions(s: &AppState, organization_id: &str, user_ids: &[String]) {
    let mut seen: Vec<&String> = Vec::new();
    for id in user_ids {
        if seen.contains(&id) {
            continue;
        }
        seen.push(id);
        if let Ok(eff) = perm::effective(&s.db, organization_id, id).await {
            crate::realtime::emit_users(s, std::slice::from_ref(id), "permissions:changed", &json!({ "organizationId": organization_id, "permissions": eff.permissions }))
                .await;
        }
    }
}

// Rolü taşıyan herkesin (doğrudan üyeler + gruplarındaki kişiler) arayüzü tazelenmeli.
async fn notify_holders(s: &AppState, organization_id: &str, role_id: &str) {
    let everyone: Option<bool> = match sqlx::query_scalar(r#"select everyone from "Role" where id = $1"#).bind(role_id).fetch_optional(&s.db).await {
        Ok(v) => v,
        Err(_) => return,
    };
    let Some(everyone) = everyone else { return };
    let ids: Result<Vec<String>, _> = if everyone {
        sqlx::query_scalar(r#"select "userId" from "OrganizationMember" where "organizationId" = $1"#).bind(organization_id).fetch_all(&s.db).await
    } else {
        sqlx::query_scalar(
            r#"select * from (select "userId" from "RoleMember" where "roleId" = $1) a
               union all select gm."userId" from "RoleGroup" rg join "UserGroupMember" gm on gm."groupId" = rg."groupId" where rg."roleId" = $1"#,
        )
        .bind(role_id)
        .fetch_all(&s.db)
        .await
    };
    if let Ok(ids) = ids {
        notify_permissions(s, organization_id, &ids).await;
    }
}

async fn announce(s: &AppState, organization_id: &str) {
    crate::realtime::emit_org(s, organization_id, "roles:changed", &json!({ "organizationId": organization_id })).await;
}

async fn list(State(s): State<AppState>, me: Me, Path(organization_id): Path<String>) -> Reply {
    membership(&s, &organization_id, &me.id, None).await?;
    let rows = crate::ctx::many_json(
        sqlx::query_scalar(sql(format!(
            r#"select {} from "Role" r where r."organizationId" = $1 order by r.founder desc, r.everyone desc, r.handle asc"#,
            role_json()
        )))
        .bind(&organization_id),
        &s.db,
    )
    .await?;
    ok(Value::Array(rows))
}

async fn create(State(s): State<AppState>, me: Me, Path(organization_id): Path<String>, Body(body): Body) -> Reply {
    check_dto(&body, false, HANDLE_MESSAGE)?;
    membership(&s, &organization_id, &me.id, Some("member.roles")).await?;
    let handle = body["handle"].as_str().unwrap_or_default().trim().to_lowercase();
    if handle_taken(&s, &organization_id, &handle).await? {
        return Err(ApiError::conflict("HANDLE_TAKEN", "Bu rol kimliği zaten kullanılıyor.").field("handle"));
    }
    let text = |k: &str| body.get(k).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty()).map(str::to_string);
    let id = uuid();
    sqlx::query(
        r#"insert into "Role" (id, "organizationId", handle, name, description, color, everyone, founder, permissions, "createdById", "createdAt", "updatedAt")
           values ($1, $2, $3, $4, $5, $6, false, false, '{}', $7, $8, $8)"#,
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
    let role = role(&s, &organization_id, &id).await?;
    announce(&s, &organization_id).await;
    created(role.json)
}

async fn handle_taken(s: &AppState, organization_id: &str, handle: &str) -> ApiResult<bool> {
    let hit: Option<i32> = sqlx::query_scalar(r#"select 1 from "Role" where "organizationId" = $1 and handle = $2"#)
        .bind(organization_id)
        .bind(handle)
        .fetch_optional(&s.db)
        .await?;
    Ok(hit.is_some())
}

/// TS'te `dto.x.trim()` null'da TypeError atar → 500.
pub fn null_crash(body: &serde_json::Map<String, Value>, key: &str) -> ApiResult<()> {
    if matches!(body.get(key), Some(Value::Null)) {
        return Err(ApiError::internal(format!("{key}: null.trim()")));
    }
    Ok(())
}

async fn update(State(s): State<AppState>, me: Me, Path((organization_id, role_id)): Path<(String, String)>, Body(body): Body) -> Reply {
    check_dto(&body, true, HANDLE_MESSAGE)?;
    membership(&s, &organization_id, &me.id, Some("member.roles")).await?;
    let current = role(&s, &organization_id, &role_id).await?;
    null_crash(&body, "handle")?;
    let mut handle: Option<String> = None;
    if let Some(h) = body.get("handle").and_then(Value::as_str) {
        let h = h.trim().to_lowercase();
        if h != current.handle {
            guard_base(&current, "yeniden adlandırılamaz")?;
            if handle_taken(&s, &organization_id, &h).await? {
                return Err(ApiError::conflict("HANDLE_TAKEN", "Bu rol kimliği zaten kullanılıyor.").field("handle"));
            }
            handle = Some(h);
        }
    }
    for k in ["name", "description", "color"] {
        null_crash(&body, k)?;
    }
    let s_of = |k: &str| body.get(k).and_then(Value::as_str).map(str::trim);
    sqlx::query(
        r#"update "Role" set handle = coalesce($2, handle), name = coalesce($3, name),
             description = case when $4 then $5 else description end, color = case when $6 then $7 else color end, "updatedAt" = $8
           where id = $1"#,
    )
    .bind(&role_id)
    .bind(&handle)
    .bind(s_of("name"))
    .bind(s_of("description").is_some())
    .bind(s_of("description").filter(|v| !v.is_empty()))
    .bind(s_of("color").is_some())
    .bind(s_of("color").filter(|v| !v.is_empty()))
    .bind(now())
    .execute(&s.db)
    .await?;
    let role = role(&s, &organization_id, &role_id).await?;
    announce(&s, &organization_id).await;
    ok(role.json)
}

async fn set_permissions(State(s): State<AppState>, me: Me, Path((organization_id, role_id)): Path<(String, String)>, Body(body): Body) -> Reply {
    membership(&s, &organization_id, &me.id, Some("member.roles")).await?;
    let current = role(&s, &organization_id, &role_id).await?;
    if current.founder {
        return Err(ApiError::conflict("FOUNDER_PROTECTED", "Kurucu rolünün yetkileri değiştirilemez."));
    }
    sqlx::query(r#"update "Role" set permissions = $2, "updatedAt" = $3 where id = $1"#)
        .bind(&role_id)
        .bind(perm::clean_value(body.get("permissions")))
        .bind(now())
        .execute(&s.db)
        .await?;
    let role = role(&s, &organization_id, &role_id).await?;
    announce(&s, &organization_id).await;
    notify_holders(&s, &organization_id, &role_id).await;
    ok(role.json)
}

async fn remove(State(s): State<AppState>, me: Me, Path((organization_id, role_id)): Path<(String, String)>) -> Reply {
    membership(&s, &organization_id, &me.id, Some("member.roles")).await?;
    guard_base(&role(&s, &organization_id, &role_id).await?, "silinemez")?;
    notify_holders(&s, &organization_id, &role_id).await;
    let n = sqlx::query(r#"delete from "Role" where id = $1"#).bind(&role_id).execute(&s.db).await?.rows_affected();
    if n == 0 {
        return Err(ApiError::internal("Role silinemedi"));
    }
    announce(&s, &organization_id).await;
    ok(json!({ "id": role_id, "removed": true }))
}

// Role kişi atama.
async fn add_member(State(s): State<AppState>, me: Me, Path((organization_id, role_id, user_id)): Path<(String, String, String)>) -> Reply {
    membership(&s, &organization_id, &me.id, Some("member.roles")).await?;
    guard_base(&role(&s, &organization_id, &role_id).await?, "elle düzenlenemez")?;
    let member: Option<i32> = sqlx::query_scalar(r#"select 1 from "OrganizationMember" where "organizationId" = $1 and "userId" = $2"#)
        .bind(&organization_id)
        .bind(&user_id)
        .fetch_optional(&s.db)
        .await?;
    if member.is_none() {
        return Err(not_found("MEMBER_NOT_FOUND", "Üye bulunamadı."));
    }
    sqlx::query(r#"insert into "RoleMember" ("roleId", "userId", "addedAt") values ($1, $2, $3) on conflict do nothing"#)
        .bind(&role_id)
        .bind(&user_id)
        .bind(now())
        .execute(&s.db)
        .await?;
    announce(&s, &organization_id).await;
    notify_holders(&s, &organization_id, &role_id).await;
    created(role(&s, &organization_id, &role_id).await?.json)
}

async fn remove_member(State(s): State<AppState>, me: Me, Path((organization_id, role_id, user_id)): Path<(String, String, String)>) -> Reply {
    membership(&s, &organization_id, &me.id, Some("member.roles")).await?;
    guard_base(&role(&s, &organization_id, &role_id).await?, "elle düzenlenemez")?;
    notify_holders(&s, &organization_id, &role_id).await;
    sqlx::query(r#"delete from "RoleMember" where "roleId" = $1 and "userId" = $2"#).bind(&role_id).bind(&user_id).execute(&s.db).await?;
    announce(&s, &organization_id).await;
    ok(role(&s, &organization_id, &role_id).await?.json)
}

// Role grup atama: gruptaki herkes rolün yetkilerini kazanır.
async fn add_group(State(s): State<AppState>, me: Me, Path((organization_id, role_id, group_id)): Path<(String, String, String)>) -> Reply {
    membership(&s, &organization_id, &me.id, Some("member.roles")).await?;
    guard_base(&role(&s, &organization_id, &role_id).await?, "gruba bağlanamaz")?;
    let group: Option<i32> = sqlx::query_scalar(r#"select 1 from "UserGroup" where id = $1 and "organizationId" = $2"#)
        .bind(&group_id)
        .bind(&organization_id)
        .fetch_optional(&s.db)
        .await?;
    if group.is_none() {
        return Err(not_found("USER_GROUP_NOT_FOUND", "Grup bulunamadı."));
    }
    sqlx::query(r#"insert into "RoleGroup" ("roleId", "groupId", "addedAt") values ($1, $2, $3) on conflict do nothing"#)
        .bind(&role_id)
        .bind(&group_id)
        .bind(now())
        .execute(&s.db)
        .await?;
    announce(&s, &organization_id).await;
    notify_holders(&s, &organization_id, &role_id).await;
    created(role(&s, &organization_id, &role_id).await?.json)
}

async fn remove_group(State(s): State<AppState>, me: Me, Path((organization_id, role_id, group_id)): Path<(String, String, String)>) -> Reply {
    membership(&s, &organization_id, &me.id, Some("member.roles")).await?;
    guard_base(&role(&s, &organization_id, &role_id).await?, "gruba bağlanamaz")?;
    notify_holders(&s, &organization_id, &role_id).await;
    sqlx::query(r#"delete from "RoleGroup" where "roleId" = $1 and "groupId" = $2"#).bind(&role_id).bind(&group_id).execute(&s.db).await?;
    announce(&s, &organization_id).await;
    ok(role(&s, &organization_id, &role_id).await?.json)
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/organizations/{organizationId}/roles", get(list).post(create))
        .route("/organizations/{organizationId}/roles/{roleId}", patch(update).delete(remove))
        .route("/organizations/{organizationId}/roles/{roleId}/permissions", patch(set_permissions))
        .route("/organizations/{organizationId}/roles/{roleId}/members/{userId}", post(add_member).delete(remove_member))
        .route("/organizations/{organizationId}/roles/{roleId}/groups/{groupId}", post(add_group).delete(remove_group))
}
