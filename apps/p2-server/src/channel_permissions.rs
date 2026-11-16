//! Kanala özel izinler — `server/src/channelPermissions.ts` karşılığı. Rol, grup ya da tek kişi
//! için yetki aç/kapat; kurallar herkes → diğer roller → gruplar → kişi sırasıyla uygulanır.

use axum::Router;
use axum::extract::{Path, State};
use axum::routing::{get, put};
use serde_json::{Value, json};

use crate::AppState;
use crate::ctx::{Body, Me, Reply, many_json, not_found, now, ok, uuid};
use crate::error::ApiResult;
use crate::perm;

async fn channel(s: &AppState, organization_id: &str, project_id: &str, channel_id: &str) -> ApiResult<()> {
    let hit: Option<i32> = sqlx::query_scalar(
        r#"select 1 from "ProjectChannel" ch join "ChannelCategory" c on c.id = ch."categoryId" join "Project" p on p.id = c."projectId"
           where ch.id = $1 and ch."deletedAt" is null and c."projectId" = $2 and p."organizationId" = $3"#,
    )
    .bind(channel_id)
    .bind(project_id)
    .bind(organization_id)
    .fetch_optional(&s.db)
    .await?;
    if hit.is_none() {
        return Err(not_found("CHANNEL_NOT_FOUND", "Kanal bulunamadı."));
    }
    Ok(())
}

async fn announce(s: &AppState, organization_id: &str, project_id: &str, channel_id: &str) {
    // Yetkiler değişince herkesin kanal listesi ve düğmeleri tazelenmeli.
    crate::realtime::emit_project(s, organization_id, project_id, "channel:permissions", &json!({ "projectId": project_id, "channelId": channel_id })).await;
}

async fn list(State(s): State<AppState>, me: Me, Path((organization_id, project_id, channel_id)): Path<(String, String, String)>) -> Reply {
    channel(&s, &organization_id, &project_id, &channel_id).await?;
    perm::require_channel(&s.db, &organization_id, &me.id, &channel_id, &["channel.view"]).await?;
    let rows: Vec<(String, String, Vec<String>, Vec<String>)> =
        sqlx::query_as(r#"select "targetType", "targetId", allow, deny from "ChannelPermission" where "channelId" = $1 order by "updatedAt" asc"#)
            .bind(&channel_id)
            .fetch_all(&s.db)
            .await?;
    let roles = many_json(
        sqlx::query_scalar(
            r#"select json_build_object('id', id, 'handle', handle, 'name', name, 'color', color, 'everyone', everyone) from "Role"
               where "projectId" = $1 and founder = false order by everyone desc, handle asc"#,
        )
        .bind(&project_id),
        &s.db,
    )
    .await?;
    let groups = many_json(
        sqlx::query_scalar(
            r#"select json_build_object('id', g.id, 'handle', g.handle, 'name', g.name, 'color', g.color,
                 'memberCount', (select count(*) from "UserGroupMember" x where x."groupId" = g.id))
               from "UserGroup" g where g."projectId" = $1 order by g.handle asc"#,
        )
        .bind(&project_id),
        &s.db,
    )
    .await?;
    let members = many_json(
        sqlx::query_scalar(
            r#"select json_build_object('id', u.id, 'username', u.username, 'displayName', u."displayName", 'avatarUrl', u."avatarUrl", 'owner', m.role = 'OWNER')
               from "ProjectMember" m join "User" u on u.id = m."userId" where m."projectId" = $1 order by m."joinedAt" asc, u.id asc"#,
        )
        .bind(&project_id),
        &s.db,
    )
    .await?;
    let mine = perm::channel_permissions(&s.db, &organization_id, &me.id, &channel_id).await?;
    let catalog: Vec<Value> = perm::catalog_json().as_array().cloned().unwrap_or_default().into_iter().filter(|p| p["channel"] == json!(true)).collect();
    let can_edit = mine.iter().any(|p| p == "channel.permissions");
    ok(json!({
        "catalog": catalog,
        "overrides": rows.into_iter().map(|(target_type, target_id, allow, deny)| json!({
            "targetType": target_type, "targetId": target_id, "allow": perm::clean_channel(&allow), "deny": perm::clean_channel(&deny),
        })).collect::<Vec<_>>(),
        "roles": roles,
        "groups": groups,
        "members": members,
        "mine": mine,
        "canEdit": can_edit,
    }))
}

fn channel_list(value: Option<&Value>) -> Vec<String> {
    perm::clean_channel(&perm::clean_value(value))
}

// Tek bir hedefin (rol/grup/kişi) kuralını yazar; ikisi de boşsa kural silinir.
async fn save(
    State(s): State<AppState>,
    me: Me,
    Path((organization_id, project_id, channel_id, target_type, target_id)): Path<(String, String, String, String, String)>,
    Body(body): Body,
) -> Reply {
    channel(&s, &organization_id, &project_id, &channel_id).await?;
    perm::require_channel(&s.db, &organization_id, &me.id, &channel_id, &["channel.permissions"]).await?;
    let kind = target_type.to_uppercase();
    if kind != "ROLE" && kind != "GROUP" && kind != "USER" {
        return Err(not_found("TARGET_INVALID", "Geçersiz hedef."));
    }
    let exists = |q: &'static str| {
        let db = s.db.clone();
        let (t, o) = (target_id.clone(), project_id.clone());
        async move { sqlx::query_scalar::<_, i32>(q).bind(t).bind(o).fetch_optional(&db).await.map(|v| v.is_some()) }
    };
    if kind == "ROLE" && !exists(r#"select 1 from "Role" where id = $1 and "projectId" = $2 and founder = false"#).await? {
        return Err(not_found("ROLE_NOT_FOUND", "Rol bulunamadı ya da kural konulamaz."));
    }
    if kind == "GROUP" && !exists(r#"select 1 from "UserGroup" where id = $1 and "projectId" = $2"#).await? {
        return Err(not_found("USER_GROUP_NOT_FOUND", "Grup bulunamadı."));
    }
    if kind == "USER" && !exists(r#"select 1 from "ProjectMember" where "userId" = $1 and "projectId" = $2"#).await? {
        return Err(not_found("MEMBER_NOT_FOUND", "Üye bulunamadı."));
    }
    let allow = channel_list(body.get("allow"));
    let deny: Vec<String> = channel_list(body.get("deny")).into_iter().filter(|d| !allow.contains(d)).collect();
    if allow.is_empty() && deny.is_empty() {
        sqlx::query(r#"delete from "ChannelPermission" where "channelId" = $1 and "targetType" = $2 and "targetId" = $3"#)
            .bind(&channel_id)
            .bind(&kind)
            .bind(&target_id)
            .execute(&s.db)
            .await?;
        announce(&s, &organization_id, &project_id, &channel_id).await;
        return ok(json!({ "targetType": kind, "targetId": target_id, "allow": [], "deny": [], "removed": true }));
    }
    let (row_type, row_target): (String, String) = sqlx::query_as(
        r#"insert into "ChannelPermission" (id, "channelId", "targetType", "targetId", allow, deny, "updatedAt") values ($1, $2, $3, $4, $5, $6, $7)
           on conflict ("channelId", "targetType", "targetId") do update set allow = $5, deny = $6, "updatedAt" = $7
           returning "targetType", "targetId""#,
    )
    .bind(uuid())
    .bind(&channel_id)
    .bind(&kind)
    .bind(&target_id)
    .bind(&allow)
    .bind(&deny)
    .bind(now())
    .fetch_one(&s.db)
    .await?;
    announce(&s, &organization_id, &project_id, &channel_id).await;
    ok(json!({ "targetType": row_type, "targetId": row_target, "allow": allow, "deny": deny }))
}

async fn remove(
    State(s): State<AppState>,
    me: Me,
    Path((organization_id, project_id, channel_id, target_type, target_id)): Path<(String, String, String, String, String)>,
) -> Reply {
    channel(&s, &organization_id, &project_id, &channel_id).await?;
    perm::require_channel(&s.db, &organization_id, &me.id, &channel_id, &["channel.permissions"]).await?;
    sqlx::query(r#"delete from "ChannelPermission" where "channelId" = $1 and "targetType" = $2 and "targetId" = $3"#)
        .bind(&channel_id)
        .bind(target_type.to_uppercase())
        .bind(&target_id)
        .execute(&s.db)
        .await?;
    announce(&s, &organization_id, &project_id, &channel_id).await;
    ok(json!({ "removed": true }))
}

pub fn routes() -> Router<AppState> {
    const BASE: &str = "/organizations/{organizationId}/projects/{projectId}/channels/{channelId}/permissions";
    Router::new().route(BASE, get(list)).route(&format!("{BASE}/{{targetType}}/{{targetId}}"), put(save).delete(remove))
}
