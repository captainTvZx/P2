//! Kuruluş yönetim paneli — `server/src/admin.ts` (AdminController + PermissionsController) karşılığı.
//! Her uç kendi yetkisini ister (admin.panel, member.kick, member.invite, member.roles,
//! organization.manage); sahiplik devri sahibe özeldir.

use std::collections::{HashMap, HashSet};

use axum::Router;
use axum::extract::{Path, State};
use axum::routing::{delete, get, patch, post};
use serde_json::{Map, Value, json};

use crate::AppState;
use crate::ctx::{Body, Me, Reply, created, forbidden, many_json, not_found, now, ok, sql, uuid};
use crate::error::{ApiError, ApiResult};
use crate::perm;
use crate::validate::Checker;

type J = sqlx::types::Json<Value>;

/// Panelin her ucu bir yetki ister; üyelik yoksa 404. Üyenin kuruluş rolünü döndürür.
async fn staff(s: &AppState, organization_id: &str, user_id: &str, permission: &str) -> ApiResult<String> {
    let role = crate::activity::membership(&s.db, organization_id, user_id).await?;
    perm::require(&s.db, organization_id, user_id, permission).await?;
    Ok(role)
}

async fn member_role(s: &AppState, organization_id: &str, user_id: &str) -> ApiResult<Option<String>> {
    Ok(sqlx::query_scalar(r#"select role::text from "OrganizationMember" where "organizationId" = $1 and "userId" = $2"#)
        .bind(organization_id)
        .bind(user_id)
        .fetch_optional(&s.db)
        .await?)
}

fn member_not_found() -> ApiError {
    not_found("MEMBER_NOT_FOUND", "Üye bulunamadı.")
}

fn push(mut value: Value, extra: Vec<(&str, Value)>) -> Value {
    if let Some(map) = value.as_object_mut() {
        for (k, v) in extra {
            map.insert(k.to_string(), v);
        }
    }
    value
}

fn visibility(body: &Map<String, Value>) -> ApiResult<bool> {
    let mut c = Checker::new(body);
    c.field("visible", &[(body.get("visible").is_some_and(Value::is_boolean), None)]);
    c.finish()?;
    Ok(body["visible"].as_bool().unwrap_or(false))
}

// ------------------------------------------------------------------ erişim (proje/kanal görünürlüğü)

async fn access(State(s): State<AppState>, me: Me, Path(organization_id): Path<String>) -> Reply {
    staff(&s, &organization_id, &me.id, "member.roles").await?;
    let members: Vec<(String, J)> = sqlx::query_as(
        r#"select u.id, json_build_object('id', u.id, 'username', u.username, 'displayName', u."displayName", 'avatarUrl', u."avatarUrl", 'owner', m.role = 'OWNER')
           from "OrganizationMember" m join "User" u on u.id = m."userId" where m."organizationId" = $1 order by m."joinedAt" asc"#,
    )
    .bind(&organization_id)
    .fetch_all(&s.db)
    .await?;
    let projects: Vec<(String, String, String)> =
        sqlx::query_as(r#"select id, name, "ownerId" from "Project" where "organizationId" = $1 and "deletedAt" is null order by name asc"#)
            .bind(&organization_id)
            .fetch_all(&s.db)
            .await?;
    let project_ids: Vec<String> = projects.iter().map(|p| p.0.clone()).collect();
    let project_members: Vec<(String, String)> = sqlx::query_as(r#"select "projectId", "userId" from "ProjectMember" where "projectId" = any($1)"#)
        .bind(&project_ids)
        .fetch_all(&s.db)
        .await?;
    let categories: Vec<(String, String, String)> =
        sqlx::query_as(r#"select id, name, "projectId" from "ChannelCategory" where "projectId" = any($1) and "deletedAt" is null order by position asc"#)
            .bind(&project_ids)
            .fetch_all(&s.db)
            .await?;
    let category_ids: Vec<String> = categories.iter().map(|c| c.0.clone()).collect();
    let channels: Vec<(String, String, String, String)> = sqlx::query_as(
        r#"select id, name, type::text, "categoryId" from "ProjectChannel" where "categoryId" = any($1) and "deletedAt" is null order by position asc"#,
    )
    .bind(&category_ids)
    .fetch_all(&s.db)
    .await?;
    // Proje → kategori sırası → kanal sırası.
    let mut ordered: Vec<(String, String, String, String, String)> = Vec::new(); // (proje, kanal id, ad, tür, kategori adı)
    for (pid, _, _) in &projects {
        for (cid, cname, cpid) in &categories {
            if cpid != pid {
                continue;
            }
            for (chid, chname, chtype, chcat) in &channels {
                if chcat == cid {
                    ordered.push((pid.clone(), chid.clone(), chname.clone(), chtype.clone(), cname.clone()));
                }
            }
        }
    }
    let channel_ids: Vec<String> = ordered.iter().map(|c| c.1.clone()).collect();
    let mut visible_by_user: HashMap<String, HashSet<String>> = HashMap::new();
    for (uid, _) in &members {
        visible_by_user.insert(uid.clone(), perm::visible_channel_ids(&s.db, &organization_id, uid, &channel_ids).await?);
    }
    let projects_json: Vec<Value> = projects
        .iter()
        .map(|(pid, name, owner)| {
            json!({
                "id": pid, "name": name, "ownerId": owner,
                "memberIds": project_members.iter().filter(|(p, _)| p == pid).map(|(_, u)| u.clone()).collect::<Vec<_>>(),
                "channels": ordered.iter().filter(|c| &c.0 == pid).map(|(_, id, name, kind, category)| json!({
                    "id": id, "name": name, "type": kind, "category": category,
                    "visibleUserIds": members.iter().filter(|(uid, _)| visible_by_user.get(uid).is_some_and(|v| v.contains(id))).map(|(uid, _)| uid.clone()).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    ok(json!({ "members": members.into_iter().map(|(_, j)| j.0).collect::<Vec<_>>(), "projects": projects_json }))
}

async fn set_project_access(State(s): State<AppState>, me: Me, Path((organization_id, project_id, user_id)): Path<(String, String, String)>, Body(body): Body) -> Reply {
    let visible = visibility(&body)?;
    staff(&s, &organization_id, &me.id, "member.roles").await?;
    let target = member_role(&s, &organization_id, &user_id).await?;
    let project: Option<String> = sqlx::query_scalar(r#"select "ownerId" from "Project" where id = $1 and "organizationId" = $2 and "deletedAt" is null"#)
        .bind(&project_id)
        .bind(&organization_id)
        .fetch_optional(&s.db)
        .await?;
    let Some(target) = target else { return Err(member_not_found()) };
    let Some(owner) = project else { return Err(not_found("PROJECT_NOT_FOUND", "Proje bulunamadı.")) };
    if !visible && (target == "OWNER" || owner == user_id) {
        return Err(ApiError::conflict("ACCESS_PROTECTED", "Kuruluş veya proje sahibinin proje erişimi kapatılamaz."));
    }
    if visible {
        sqlx::query(r#"insert into "ProjectMember" ("projectId", "userId", role) values ($1, $2, $3::"ProjectRole") on conflict do nothing"#)
            .bind(&project_id)
            .bind(&user_id)
            .bind(if owner == user_id { "OWNER" } else { "MEMBER" })
            .execute(&s.db)
            .await?;
    } else {
        let removed: Vec<String> = sqlx::query_scalar(
            r#"select gm."groupId" from "ChatGroupMember" gm where gm."userId" = $1 and gm."groupId" in (select g.id from "ChatGroup" g where g."projectId" = $2)"#,
        )
        .bind(&user_id)
        .bind(&project_id)
        .fetch_all(&s.db)
        .await?;
        let mut tx = s.db.begin().await?;
        sqlx::query(r#"delete from "ChatGroupMember" where "userId" = $1 and "groupId" in (select id from "ChatGroup" where "projectId" = $2)"#)
            .bind(&user_id)
            .bind(&project_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(r#"delete from "ProjectMember" where "projectId" = $1 and "userId" = $2"#).bind(&project_id).bind(&user_id).execute(&mut *tx).await?;
        tx.commit().await?;
        for group_id in removed {
            crate::calls::drop_from_group_call(&s, &group_id, &user_id).await;
            crate::realtime::emit_users(&s, std::slice::from_ref(&user_id), "group:left", &json!({ "groupId": group_id })).await;
            let group: Option<(J, Vec<String>)> = sqlx::query_as(sql(format!(
                r#"select json_build_object('id', g.id, 'name', g.name, 'channelId', g."channelId", 'projectId', g."projectId", 'createdById', g."createdById", 'updatedAt', {},
                     'members', coalesce((select json_agg(json_build_object('id', u.id, 'username', u.username, 'displayName', u."displayName", 'status', u.status, 'avatarUrl', u."avatarUrl"))
                        from "ChatGroupMember" m join "User" u on u.id = m."userId" where m."groupId" = g.id), '[]'::json),
                     'lastReadAt', null, 'unread', 0, 'lastMessage', null),
                   array(select m."userId" from "ChatGroupMember" m where m."groupId" = g.id)
                   from "ChatGroup" g where g.id = $1"#,
                crate::iso!(r#"g."updatedAt""#)
            )))
            .bind(&group_id)
            .fetch_optional(&s.db)
            .await?;
            if let Some((group, ids)) = group {
                crate::realtime::emit_users(&s, &ids, "group:updated", &json!({ "group": group.0 })).await;
            }
        }
    }
    crate::realtime::emit_users(&s, std::slice::from_ref(&user_id), "project:permissions", &json!({ "organizationId": organization_id, "projectId": project_id, "visible": visible })).await;
    crate::realtime::emit_project(&s, &organization_id, &project_id, "project:members", &json!({ "projectId": project_id })).await;
    ok(json!({ "projectId": project_id, "userId": user_id, "visible": visible }))
}

async fn set_channel_access(State(s): State<AppState>, me: Me, Path((organization_id, channel_id, user_id)): Path<(String, String, String)>, Body(body): Body) -> Reply {
    let visible = visibility(&body)?;
    staff(&s, &organization_id, &me.id, "member.roles").await?;
    let target = member_role(&s, &organization_id, &user_id).await?;
    let channel: Option<String> = sqlx::query_scalar(
        r#"select c."projectId" from "ProjectChannel" ch join "ChannelCategory" c on c.id = ch."categoryId" join "Project" p on p.id = c."projectId"
           where ch.id = $1 and ch."deletedAt" is null and p."organizationId" = $2 and p."deletedAt" is null"#,
    )
    .bind(&channel_id)
    .bind(&organization_id)
    .fetch_optional(&s.db)
    .await?;
    let Some(target) = target else { return Err(member_not_found()) };
    let Some(project_id) = channel else { return Err(not_found("CHANNEL_NOT_FOUND", "Kanal bulunamadı.")) };
    if target == "OWNER" {
        return Err(ApiError::conflict("ACCESS_PROTECTED", "Kuruluş sahibinin kanal erişimi kapatılamaz."));
    }
    let current: Option<(Vec<String>, Vec<String>)> =
        sqlx::query_as(r#"select allow, deny from "ChannelPermission" where "channelId" = $1 and "targetType" = 'USER' and "targetId" = $2"#)
            .bind(&channel_id)
            .bind(&user_id)
            .fetch_optional(&s.db)
            .await?;
    let (allow, deny) = current.unwrap_or_default();
    let mut allow: Vec<String> = allow.into_iter().filter(|p| p != "channel.view").collect();
    let mut deny: Vec<String> = deny.into_iter().filter(|p| p != "channel.view").collect();
    if visible {
        allow.push("channel.view".into());
    } else {
        deny.push("channel.view".into());
    }
    sqlx::query(
        r#"insert into "ChannelPermission" (id, "channelId", "targetType", "targetId", allow, deny, "updatedAt") values ($1, $2, 'USER', $3, $4, $5, $6)
           on conflict ("channelId", "targetType", "targetId") do update set allow = $4, deny = $5, "updatedAt" = $6"#,
    )
    .bind(uuid())
    .bind(&channel_id)
    .bind(&user_id)
    .bind(&allow)
    .bind(&deny)
    .bind(now())
    .execute(&s.db)
    .await?;
    crate::realtime::emit_users(
        &s,
        std::slice::from_ref(&user_id),
        "channel:permissions",
        &json!({ "organizationId": organization_id, "projectId": project_id, "channelId": channel_id, "visible": visible }),
    )
    .await;
    ok(json!({ "channelId": channel_id, "userId": user_id, "visible": visible }))
}

// ------------------------------------------------------------------ genel bakış

/// Yüklenen dosyalar diskte tutuluyor; toplam boyut klasörden okunur.
fn storage_used() -> Value {
    let root = std::env::var("P2_UPLOAD_DIR").ok().filter(|v| !v.is_empty()).map(std::path::PathBuf::from).unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_default().join(".p2-data").join("uploads")
    });
    let (mut bytes, mut files) = (0u64, 0u64);
    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            // stat sembolik bağı izler.
            if let Ok(info) = std::fs::metadata(entry.path())
                && info.is_file()
            {
                bytes += info.len();
                files += 1;
            }
        }
    }
    json!({ "bytes": bytes, "files": files })
}

async fn overview(State(s): State<AppState>, me: Me, Path(organization_id): Path<String>) -> Reply {
    staff(&s, &organization_id, &me.id, "admin.panel").await?;
    let organization: Option<J> = sqlx::query_scalar(sql(format!(
        r#"select json_build_object('id', id, 'name', name, 'slug', slug, 'createdAt', {}) from "Organization" where id = $1"#,
        crate::iso!(r#""createdAt""#)
    )))
    .bind(&organization_id)
    .fetch_optional(&s.db)
    .await?;
    let organization = organization.ok_or_else(|| ApiError::internal("Organization bulunamadı"))?.0;
    let at = now();
    let week = at - chrono::Duration::milliseconds(7 * 86_400_000);
    let c: (i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        r#"select
             (select count(*) from "OrganizationMember" where "organizationId" = $1),
             (select count(*) from "OrganizationMember" where "organizationId" = $1 and role = 'OWNER'),
             (select count(*) from "OrganizationMember" m join "User" u on u.id = m."userId" where m."organizationId" = $1 and u.status <> 'OFFLINE'),
             (select count(*) from "OrganizationMember" where "organizationId" = $1 and "joinedAt" > $2),
             (select count(*) from "Project" where "organizationId" = $1),
             (select count(*) from "ProjectChannel" ch join "ChannelCategory" c on c.id = ch."categoryId" join "Project" p on p.id = c."projectId" where p."organizationId" = $1 and ch."deletedAt" is null),
             (select count(*) from "ProjectChannel" ch join "ChannelCategory" c on c.id = ch."categoryId" join "Project" p on p.id = c."projectId" where p."organizationId" = $1 and ch."deletedAt" is not null),
             (select count(*) from "ChatGroup" where "organizationId" = $1),
             (select count(*) from "ChannelMessage" m join "ProjectChannel" ch on ch.id = m."channelId" join "ChannelCategory" c on c.id = ch."categoryId" join "Project" p on p.id = c."projectId" where p."organizationId" = $1),
             (select count(*) from "DirectMessage" m join "DirectConversation" d on d.id = m."conversationId" where d."organizationId" = $1),
             (select count(*) from "ChatGroupMessage" m join "ChatGroup" g on g.id = m."groupId" where g."organizationId" = $1),
             (select count(*) from "OrganizationInvite" where "organizationId" = $1),
             (select count(*) from "OrganizationMember" m join "User" u on u.id = m."userId" where m."organizationId" = $1 and u."twoFactorEnabled"),
             (select count(*) from "UserGroup" where "organizationId" = $1),
             (select count(*) from "Session" x where x."expiresAt" > $3 and exists (select 1 from "OrganizationMember" m where m."userId" = x."userId" and m."organizationId" = $1))"#,
    )
    .bind(&organization_id)
    .bind(week)
    .bind(at)
    .fetch_one(&s.db)
    .await?;
    let (total, owners, online, joined, projects, channels, deleted_channels, groups, channel_messages, direct_messages, group_messages, invites, two_factor, role_count, sessions) = c;
    let version = if s.cfg.version.is_empty() { "dev".to_string() } else { s.cfg.version.clone() };
    let storage = tokio::task::spawn_blocking(storage_used).await.unwrap_or_else(|_| json!({ "bytes": 0, "files": 0 }));
    ok(json!({
        "organization": organization,
        "server": { "version": version },
        "members": { "total": total, "owners": owners, "roles": role_count, "online": online, "joinedThisWeek": joined, "twoFactor": two_factor },
        "content": {
            "projects": projects, "channels": channels, "deletedChannels": deleted_channels, "groups": groups,
            "channelMessages": channel_messages, "directMessages": direct_messages, "groupMessages": group_messages,
            "messages": channel_messages + direct_messages + group_messages,
        },
        "invites": invites,
        "sessions": sessions,
        "storage": storage,
    }))
}

async fn members(State(s): State<AppState>, me: Me, Path(organization_id): Path<String>) -> Reply {
    staff(&s, &organization_id, &me.id, "admin.panel").await?;
    let rows: Vec<(String, bool, String, J)> = sqlx::query_as(sql(format!(
        r#"select u.id, m.role = 'OWNER', {}, json_build_object('id', u.id, 'username', u.username, 'displayName', u."displayName", 'email', u.email, 'title', u.title,
             'avatarUrl', u."avatarUrl", 'status', u.status, 'twoFactorEnabled', u."twoFactorEnabled", 'createdAt', {}, 'lastClientVersion', u."lastClientVersion",
             'lastClientSeenAt', {})
           from "OrganizationMember" m join "User" u on u.id = m."userId" where m."organizationId" = $1 order by m."joinedAt" asc"#,
        crate::iso!(r#"m."joinedAt""#),
        crate::iso!(r#"u."createdAt""#),
        crate::iso!(r#"u."lastClientSeenAt""#)
    )))
    .bind(&organization_id)
    .fetch_all(&s.db)
    .await?;
    let counts: Vec<(String, i64)> = sqlx::query_as(
        r#"select m."authorId", count(*) from "ChannelMessage" m join "ProjectChannel" ch on ch.id = m."channelId" join "ChannelCategory" c on c.id = ch."categoryId"
           join "Project" p on p.id = c."projectId" where p."organizationId" = $1 group by m."authorId""#,
    )
    .bind(&organization_id)
    .fetch_all(&s.db)
    .await?;
    let by_author: HashMap<String, i64> = counts.into_iter().collect();
    // Kişinin rolleri: doğrudan atananlar + gruplarından gelenler.
    let roles: Vec<(String, String, String, Option<String>, bool, Vec<String>, Vec<String>)> = sqlx::query_as(
        r#"select r.id, r.handle, r.name, r.color, r.founder,
             array(select "userId" from "RoleMember" where "roleId" = r.id),
             array(select gm."userId" from "RoleGroup" rg join "UserGroupMember" gm on gm."groupId" = rg."groupId" where rg."roleId" = r.id)
           from "Role" r where r."organizationId" = $1 and r.everyone = false order by r.founder desc, r.handle asc"#,
    )
    .bind(&organization_id)
    .fetch_all(&s.db)
    .await?;
    let group_rows: Vec<(String, J)> = sqlx::query_as(
        r#"select gm."userId", (select json_build_object('id', g.id, 'handle', g.handle, 'name', g.name, 'color', g.color) from "UserGroup" g where g.id = gm."groupId")
           from "UserGroupMember" gm left join "UserGroup" as j0 on (j0.id) = (gm."groupId") where (j0."organizationId" = $1 and (j0.id is not null)) offset 0"#,
    )
    .bind(&organization_id)
    .fetch_all(&s.db)
    .await?;
    let mut roles_of: HashMap<String, Vec<Value>> = HashMap::new();
    for (id, handle, name, color, founder, direct, via) in &roles {
        let mut holders: Vec<&String> = Vec::new();
        for u in direct.iter().chain(via.iter()) {
            if !holders.contains(&u) {
                holders.push(u);
            }
        }
        for u in holders {
            roles_of.entry(u.clone()).or_default().push(json!({
                "id": id, "handle": handle, "name": name, "color": color, "founder": founder, "viaGroup": !direct.contains(u),
            }));
        }
    }
    let mut groups_of: HashMap<String, Vec<Value>> = HashMap::new();
    for (user, group) in group_rows {
        groups_of.entry(user).or_default().push(group.0);
    }
    let out: Vec<Value> = rows
        .into_iter()
        .map(|(id, owner, joined, user)| {
            push(
                user.0,
                vec![
                    ("owner", json!(owner)),
                    ("roles", json!(roles_of.get(&id).cloned().unwrap_or_default())),
                    ("groups", json!(groups_of.get(&id).cloned().unwrap_or_default())),
                    ("joinedAt", json!(joined)),
                    ("channelMessages", json!(by_author.get(&id).copied().unwrap_or(0))),
                ],
            )
        })
        .collect();
    ok(Value::Array(out))
}

// ------------------------------------------------------------------ üyeler

async fn notify(s: &AppState, organization_id: &str, user_id: &str) -> ApiResult<()> {
    let after = perm::effective(&s.db, organization_id, user_id).await?;
    crate::realtime::emit_users(s, &[user_id.to_string()], "permissions:changed", &json!({ "organizationId": organization_id, "permissions": after.permissions })).await;
    Ok(())
}

// Sahiplik devri: yalnızca mevcut sahip devredebilir.
async fn transfer_owner(State(s): State<AppState>, me: Me, Path((organization_id, user_id)): Path<(String, String)>) -> Reply {
    let mine = member_role(&s, &organization_id, &me.id).await?;
    if mine.as_deref() != Some("OWNER") {
        return Err(forbidden("OWNER_ONLY", "Sahipliği yalnızca kuruluş sahibi devredebilir."));
    }
    if user_id == me.id {
        return Err(ApiError::conflict("SELF_OWNER", "Sahiplik zaten sende."));
    }
    if member_role(&s, &organization_id, &user_id).await?.is_none() {
        return Err(member_not_found());
    }
    let founder: Option<String> = sqlx::query_scalar(r#"select id from "Role" where "organizationId" = $1 and founder = true limit 1"#)
        .bind(&organization_id)
        .fetch_optional(&s.db)
        .await?;
    let mut tx = s.db.begin().await?;
    let set_role = r#"update "OrganizationMember" set role = $3::"OrganizationRole" where "organizationId" = $1 and "userId" = $2"#;
    sqlx::query(set_role).bind(&organization_id).bind(&user_id).bind("OWNER").execute(&mut *tx).await?;
    sqlx::query(set_role).bind(&organization_id).bind(&me.id).bind("MEMBER").execute(&mut *tx).await?;
    // Kurucu rolü kuruluş sahipliğini izler.
    if let Some(founder) = founder {
        sqlx::query(r#"delete from "RoleMember" where "roleId" = $1"#).bind(&founder).execute(&mut *tx).await?;
        sqlx::query(r#"insert into "RoleMember" ("roleId", "userId", "addedAt") values ($1, $2, $3)"#)
            .bind(&founder)
            .bind(&user_id)
            .bind(now())
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    for id in [&user_id, &me.id] {
        notify(&s, &organization_id, id).await?;
    }
    created(json!({ "owner": user_id }))
}

// Üyeyi çıkarma: sahip çıkarılamaz, kimse kendini çıkaramaz.
async fn remove_member(State(s): State<AppState>, me: Me, Path((organization_id, user_id)): Path<(String, String)>) -> Reply {
    let mine = staff(&s, &organization_id, &me.id, "member.kick").await?;
    if user_id == me.id {
        return Err(ApiError::conflict("SELF_REMOVE", "Kendini kuruluştan çıkaramazsın."));
    }
    let Some(target) = member_role(&s, &organization_id, &user_id).await? else { return Err(member_not_found()) };
    if target == "OWNER" {
        return Err(ApiError::conflict("OWNER_PROTECTED", "Kuruluş sahibi çıkarılamaz."));
    }
    if mine == "ADMIN" && target == "ADMIN" {
        return Err(forbidden("ADMIN_PROTECTED", "Bir yöneticiyi yalnızca kuruluş sahibi çıkarabilir."));
    }
    let mut tx = s.db.begin().await?;
    sqlx::query(r#"delete from "ChatGroupMember" where "userId" = $1 and "groupId" in (select id from "ChatGroup" where "organizationId" = $2)"#)
        .bind(&user_id)
        .bind(&organization_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(r#"delete from "UserPreference" where "userId" = $1 and "organizationId" = $2"#).bind(&user_id).bind(&organization_id).execute(&mut *tx).await?;
    sqlx::query(r#"delete from "OrganizationMember" where "organizationId" = $1 and "userId" = $2"#).bind(&organization_id).bind(&user_id).execute(&mut *tx).await?;
    tx.commit().await?;
    crate::realtime::emit_users(&s, std::slice::from_ref(&user_id), "organization:removed", &json!({ "organizationId": organization_id })).await;
    // Kalan herkesin kişi listesi anında güncellensin.
    crate::realtime::emit_org(&s, &organization_id, "organization:member:removed", &json!({ "organizationId": organization_id, "userId": user_id })).await;
    ok(json!({ "userId": user_id, "removed": true }))
}

// Çalınan cihaz / paylaşılmış parola durumunda kişinin tüm oturumlarını düşürür.
async fn revoke_sessions(State(s): State<AppState>, me: Me, Path((organization_id, user_id)): Path<(String, String)>) -> Reply {
    let mine = staff(&s, &organization_id, &me.id, "member.kick").await?;
    let Some(target) = member_role(&s, &organization_id, &user_id).await? else { return Err(member_not_found()) };
    if target == "OWNER" && mine != "OWNER" {
        return Err(forbidden("OWNER_PROTECTED", "Kuruluş sahibinin oturumlarını yalnızca sahibi kapatabilir."));
    }
    let closed = sqlx::query(r#"delete from "Session" where "userId" = $1"#).bind(&user_id).execute(&s.db).await?.rows_affected();
    created(json!({ "userId": user_id, "closed": closed }))
}

// Yetki kataloğu + her üyenin nereden hangi yetkiyi aldığı.
async fn permissions(State(s): State<AppState>, me: Me, Path(organization_id): Path<String>) -> Reply {
    staff(&s, &organization_id, &me.id, "member.roles").await?;
    let members: Vec<(String, bool, Vec<String>, Vec<String>, J)> = sqlx::query_as(
        r#"select u.id, m.role = 'OWNER', m.permissions, m."deniedPermissions",
             json_build_object('id', u.id, 'username', u.username, 'displayName', u."displayName", 'avatarUrl', u."avatarUrl")
           from "OrganizationMember" m join "User" u on u.id = m."userId" where m."organizationId" = $1 order by m."joinedAt" asc"#,
    )
    .bind(&organization_id)
    .fetch_all(&s.db)
    .await?;
    let mut rows = Vec::new();
    for (id, owner, allow, deny, user) in &members {
        let eff = perm::effective(&s.db, &organization_id, id).await?;
        rows.push(push(
            user.0.clone(),
            vec![
                ("owner", json!(owner)),
                ("roleIds", json!(eff.roles.iter().filter(|r| !r.everyone).map(|r| r.id.clone()).collect::<Vec<_>>())),
                ("allow", json!(perm::clean(allow))),
                ("deny", json!(perm::clean(deny))),
                ("effective", json!(eff.permissions)),
            ],
        ));
    }
    let roles: Vec<(String, String, String, Option<String>, Option<String>, bool, bool, Vec<String>, Vec<String>, Vec<String>, J)> = sqlx::query_as(
        r#"select r.id, r.handle, r.name, r.color, r.description, r.everyone, r.founder, r.permissions,
             array(select "userId" from "RoleMember" where "roleId" = r.id),
             array(select gm."userId" from "RoleGroup" rg join "UserGroupMember" gm on gm."groupId" = rg."groupId" where rg."roleId" = r.id),
             coalesce((select json_agg(json_build_object('id', g.id, 'handle', g.handle, 'name', g.name, 'color', g.color,
                 'memberCount', (select count(*) from "UserGroupMember" x where x."groupId" = g.id)))
               from "RoleGroup" rg join "UserGroup" g on g.id = rg."groupId" where rg."roleId" = r.id), '[]'::json)
           from "Role" r where r."organizationId" = $1 order by r.founder desc, r.everyone desc, r.handle asc"#,
    )
    .bind(&organization_id)
    .fetch_all(&s.db)
    .await?;
    let groups = many_json(
        sqlx::query_scalar(
            r#"select json_build_object('id', g.id, 'handle', g.handle, 'name', g.name, 'color', g.color,
                 'memberCount', (select count(*) from "UserGroupMember" x where x."groupId" = g.id))
               from "UserGroup" g where g."organizationId" = $1 order by g.handle asc"#,
        )
        .bind(&organization_id),
        &s.db,
    )
    .await?;
    let all_ids: Vec<String> = members.iter().map(|m| m.0.clone()).collect();
    // Prisma ilişkiyi ayrı sorguyla (`roleId IN (...)`) okur; üye sırası o sorgunun satır sırası.
    let role_ids: Vec<String> = roles.iter().map(|r| r.0.clone()).collect();
    let role_members: Vec<(String, String)> = sqlx::query_as(r#"select "roleId", "userId" from "RoleMember" where "roleId" = any($1) offset 0"#)
        .bind(&role_ids)
        .fetch_all(&s.db)
        .await?;
    let roles_json: Vec<Value> = roles
        .into_iter()
        .map(|(id, handle, name, color, description, everyone, founder, permissions, direct, via, groups)| {
            let holders: HashSet<&String> = direct.iter().chain(via.iter()).collect();
            json!({
                "id": id, "handle": handle, "name": name, "color": color, "description": description, "everyone": everyone, "founder": founder,
                "permissions": perm::clean(&permissions),
                "memberCount": if everyone { all_ids.len() } else { holders.len() },
                "memberIds": if everyone { all_ids.clone() } else { role_members.iter().filter(|(r, _)| *r == id).map(|(_, u)| u.clone()).collect() },
                "groups": groups.0,
            })
        })
        .collect();
    ok(json!({ "catalog": perm::catalog_json(), "members": rows, "groups": groups, "roles": roles_json }))
}

// Kişiye özel yetkiler: rolün üstüne eklenenler ve rolden/gruptan gelse de kapatılanlar.
async fn set_permissions(State(s): State<AppState>, me: Me, Path((organization_id, user_id)): Path<(String, String)>, Body(body): Body) -> Reply {
    staff(&s, &organization_id, &me.id, "member.roles").await?;
    let Some(target) = member_role(&s, &organization_id, &user_id).await? else { return Err(member_not_found()) };
    if target == "OWNER" {
        return Err(ApiError::conflict("OWNER_PROTECTED", "Kuruluş sahibinin yetkileri kısıtlanamaz."));
    }
    let allow = perm::clean_value(body.get("allow"));
    let deny: Vec<String> = perm::clean_value(body.get("deny")).into_iter().filter(|d| !allow.contains(d)).collect();
    sqlx::query(r#"update "OrganizationMember" set permissions = $3, "deniedPermissions" = $4 where "organizationId" = $1 and "userId" = $2"#)
        .bind(&organization_id)
        .bind(&user_id)
        .bind(&allow)
        .bind(&deny)
        .execute(&s.db)
        .await?;
    let eff = perm::effective(&s.db, &organization_id, &user_id).await?;
    crate::realtime::emit_users(&s, std::slice::from_ref(&user_id), "permissions:changed", &json!({ "organizationId": organization_id, "permissions": eff.permissions })).await;
    ok(json!({ "userId": user_id, "allow": allow, "deny": deny, "effective": eff.permissions }))
}

// ------------------------------------------------------------------ davetler

fn invite_json() -> String {
    format!(
        r#"json_build_object('id', i.id, 'code', i.code, 'maxUses', i."maxUses", 'useCount', i."useCount", 'expiresAt', {}, 'createdAt', {},
             'createdBy', (select json_build_object('id', u.id, 'displayName', u."displayName") from "User" u where u.id = i."createdById"))"#,
        crate::iso!(r#"i."expiresAt""#),
        crate::iso!(r#"i."createdAt""#)
    )
}

async fn insert_invite(s: &AppState, organization_id: &str, user_id: &str, code: &str, max_uses: Option<i32>, expires: Option<chrono::NaiveDateTime>) -> ApiResult<Value> {
    let row: J = sqlx::query_scalar(sql(format!(
        r#"with i as (insert into "OrganizationInvite" (id, code, "organizationId", "createdById", "expiresAt", "maxUses", "useCount", "createdAt")
             values ($1, $2, $3, $4, $5, $6, 0, $7) returning *) select {} from i"#,
        invite_json()
    )))
    .bind(uuid())
    .bind(code)
    .bind(organization_id)
    .bind(user_id)
    .bind(expires)
    .bind(max_uses)
    .bind(now())
    .fetch_one(&s.db)
    .await?;
    Ok(row.0)
}

async fn invites(State(s): State<AppState>, me: Me, Path(organization_id): Path<String>) -> Reply {
    staff(&s, &organization_id, &me.id, "member.invite").await?;
    let rows = many_json(
        sqlx::query_scalar(sql(format!(r#"select {} from "OrganizationInvite" i where i."organizationId" = $1 order by i."createdAt" desc"#, invite_json())))
            .bind(&organization_id),
        &s.db,
    )
    .await?;
    ok(Value::Array(rows))
}

async fn create_invite(State(s): State<AppState>, me: Me, Path(organization_id): Path<String>, Body(body): Body) -> Reply {
    let int = |k: &str, max: f64| match body.get(k) {
        None | Some(Value::Null) => true,
        Some(v) => v.as_f64().is_some_and(|f| f.fract() == 0.0 && (1.0..=max).contains(&f)),
    };
    let mut c = Checker::new(&body);
    c.field("maxUses", &[(int("maxUses", 1000.0), None)]);
    c.field("expiresInDays", &[(int("expiresInDays", 365.0), None)]);
    c.finish()?;
    staff(&s, &organization_id, &me.id, "member.invite").await?;
    let max_uses = body.get("maxUses").and_then(Value::as_f64).map(|f| f as i32);
    let expires = body.get("expiresInDays").and_then(Value::as_f64).map(|d| now() + chrono::Duration::milliseconds((d * 86_400_000.0) as i64));
    created(insert_invite(&s, &organization_id, &me.id, &crate::organizations::invite_code(), max_uses, expires).await?)
}

// "Davet kodunu yenile": yeni kod üretir, eski varsayılan kodu geçersiz kılar.
async fn rotate_invite(State(s): State<AppState>, me: Me, Path(organization_id): Path<String>) -> Reply {
    staff(&s, &organization_id, &me.id, "member.invite").await?;
    let previous: Option<String> = sqlx::query_scalar(
        r#"select id from "OrganizationInvite" where "organizationId" = $1 and "maxUses" is null and "expiresAt" is null order by "createdAt" desc limit 1"#,
    )
    .bind(&organization_id)
    .fetch_optional(&s.db)
    .await?;
    let code = crate::organizations::invite_code();
    let invite = insert_invite(&s, &organization_id, &me.id, &code, None, None).await?;
    if let Some(previous) = previous {
        let _ = sqlx::query(r#"delete from "OrganizationInvite" where id = $1"#).bind(&previous).execute(&s.db).await;
    }
    crate::realtime::emit_org(&s, &organization_id, "organization:invite", &json!({ "organizationId": organization_id, "inviteCode": code })).await;
    created(invite)
}

async fn revoke_invite(State(s): State<AppState>, me: Me, Path((organization_id, invite_id)): Path<(String, String)>) -> Reply {
    staff(&s, &organization_id, &me.id, "member.invite").await?;
    let hit: Option<i32> = sqlx::query_scalar(r#"select 1 from "OrganizationInvite" where id = $1 and "organizationId" = $2"#)
        .bind(&invite_id)
        .bind(&organization_id)
        .fetch_optional(&s.db)
        .await?;
    if hit.is_none() {
        return Err(not_found("INVITE_NOT_FOUND", "Davet bulunamadı."));
    }
    let count: i64 = sqlx::query_scalar(r#"select count(*) from "OrganizationInvite" where "organizationId" = $1"#).bind(&organization_id).fetch_one(&s.db).await?;
    if count < 2 {
        return Err(ApiError::conflict("LAST_INVITE", "Kuruluşun en az bir davet kodu olmalı."));
    }
    let n = sqlx::query(r#"delete from "OrganizationInvite" where id = $1"#).bind(&invite_id).execute(&s.db).await?.rows_affected();
    if n == 0 {
        return Err(ApiError::internal("OrganizationInvite silinemedi"));
    }
    ok(json!({ "id": invite_id, "revoked": true }))
}

// ------------------------------------------------------------------ projeler / kanallar

// Silinen projeler 30 gün geri dönüşümde bekler.
async fn deleted_projects(State(s): State<AppState>, me: Me, Path(organization_id): Path<String>) -> Reply {
    staff(&s, &organization_id, &me.id, "organization.manage").await?;
    let rows = many_json(
        sqlx::query_scalar(sql(format!(
            r#"select json_build_object('id', p.id, 'name', p.name, 'slug', p.slug, 'deletedAt', {}, 'purgeAt', {}, 'createdAt', {},
                 'owner', coalesce(nullif((select u."displayName" from "User" u where u.id = p."ownerId"), ''), '—'),
                 'categories', (select count(*) from "ChannelCategory" c where c."projectId" = p.id),
                 'members', (select count(*) from "ProjectMember" m where m."projectId" = p.id))
               from "Project" p where p."organizationId" = $1 and p."deletedAt" is not null order by p."deletedAt" desc"#,
            crate::iso!(r#"p."deletedAt""#),
            crate::iso!(r#"p."purgeAt""#),
            crate::iso!(r#"p."createdAt""#)
        )))
        .bind(&organization_id),
        &s.db,
    )
    .await?;
    ok(Value::Array(rows))
}

// Kalıcı silme: proje ve ona bağlı her şey veritabanından gider.
async fn purge_project(State(s): State<AppState>, me: Me, Path((organization_id, project_id)): Path<(String, String)>) -> Reply {
    staff(&s, &organization_id, &me.id, "organization.manage").await?;
    let project: Option<(String, String, Option<chrono::NaiveDateTime>)> =
        sqlx::query_as(r#"select name, slug, "deletedAt" from "Project" where id = $1 and "organizationId" = $2"#)
            .bind(&project_id)
            .bind(&organization_id)
            .fetch_optional(&s.db)
            .await?;
    let Some((name, slug, deleted)) = project else { return Err(not_found("PROJECT_NOT_FOUND", "Proje bulunamadı.")) };
    if deleted.is_none() {
        return Err(ApiError::conflict("PROJECT_NOT_DELETED", "Önce projeyi silmelisin; yalnızca geri dönüşümdeki projeler kalıcı silinebilir."));
    }
    sqlx::query(r#"delete from "Project" where id = $1"#).bind(&project_id).execute(&s.db).await?;
    crate::realtime::emit_org(&s, &organization_id, "project:purged", &json!({ "projectId": project_id, "name": name, "slug": slug })).await;
    ok(json!({ "ok": true, "projectId": project_id }))
}

async fn channels(State(s): State<AppState>, me: Me, Path(organization_id): Path<String>) -> Reply {
    staff(&s, &organization_id, &me.id, "admin.panel").await?;
    let rows = many_json(
        sqlx::query_scalar(sql(format!(
            r#"select json_build_object('id', ch.id, 'name', ch.name, 'topic', ch.topic, 'type', ch.type, 'deletedAt', {}, 'purgeAt', {}, 'createdAt', {},
                 'messages', (select count(*) from "ChannelMessage" m where m."channelId" = ch.id), 'category', c.name, 'project', p.name, 'projectId', p.id)
               from "ProjectChannel" ch join "ChannelCategory" c on c.id = ch."categoryId" join "Project" p on p.id = c."projectId"
               where p."organizationId" = $1 order by ch."deletedAt" asc, ch.position asc"#,
            crate::iso!(r#"ch."deletedAt""#),
            crate::iso!(r#"ch."purgeAt""#),
            crate::iso!(r#"ch."createdAt""#)
        )))
        .bind(&organization_id),
        &s.db,
    )
    .await?;
    ok(Value::Array(rows))
}

async fn rename(State(s): State<AppState>, me: Me, Path(organization_id): Path<String>, Body(body): Body) -> Reply {
    let mut c = Checker::new(&body);
    let (len_ok, str_ok) = crate::roles::opt_len(&body, "name", 2, 60);
    c.field("name", &[(len_ok, None), (str_ok, None)]);
    c.finish()?;
    staff(&s, &organization_id, &me.id, "organization.manage").await?;
    let name = body.get("name").and_then(Value::as_str).map(str::trim).unwrap_or_default();
    if name.is_empty() {
        return Err(ApiError::bad_request("NAME_REQUIRED", "Kuruluş adı boş olamaz.").field("name"));
    }
    let organization: Option<J> = sqlx::query_scalar(
        r#"update "Organization" set name = $2, "updatedAt" = $3 where id = $1 returning json_build_object('id', id, 'name', name, 'slug', slug)"#,
    )
    .bind(&organization_id)
    .bind(name)
    .bind(now())
    .fetch_optional(&s.db)
    .await?;
    let organization = organization.ok_or_else(|| ApiError::internal("Organization bulunamadı"))?.0;
    crate::realtime::emit_org(&s, &organization_id, "organization:updated", &json!({ "organization": organization })).await;
    ok(organization)
}

// ------------------------------------------------------------------ PermissionsController

// Arayüzün neyi göstereceğine karar verebilmesi için herkes kendi yetkisini okur.
async fn my_permissions(State(s): State<AppState>, me: Me, Path(organization_id): Path<String>) -> Reply {
    let eff = perm::effective(&s.db, &organization_id, &me.id).await?;
    // Prisma'nın ürettiği sorgunun aynısı (sırasız findMany: satır sırası plana bağlı, anahtar sırası buna göre).
    let channels: Vec<String> = if eff.owner {
        sqlx::query_scalar(
            r#"select ch.id from "ProjectChannel" ch left join "ChannelCategory" as j0 on (j0.id) = (ch."categoryId") left join "Project" as j1 on (j1.id) = (j0."projectId")
               where (ch."deletedAt" is null and (j1."organizationId" = $1 and j1."deletedAt" is null and (j1.id is not null) and (j0.id is not null))) offset $2"#,
        )
        .bind(&organization_id)
        .bind(0i64)
        .fetch_all(&s.db)
        .await?
    } else {
        sqlx::query_scalar(
            r#"select ch.id from "ProjectChannel" ch left join "ChannelCategory" as j0 on (j0.id) = (ch."categoryId") left join "Project" as j1 on (j1.id) = (j0."projectId")
               where (ch."deletedAt" is null and (j1."organizationId" = $1 and j1."deletedAt" is null and (j1."ownerId" = $2 or exists(select t2."projectId" from "ProjectMember" as t2
                 where (t2."userId" = $3 and (j1.id) = (t2."projectId") and t2."projectId" is not null))) and (j1.id is not null) and (j0.id is not null))) offset $4"#,
        )
        .bind(&organization_id)
        .bind(&me.id)
        .bind(&me.id)
        .bind(0i64)
        .fetch_all(&s.db)
        .await?
    };
    let overrides: Vec<(String, String, String, Vec<String>, Vec<String>)> =
        sqlx::query_as(r#"select "channelId", "targetType", "targetId", allow, deny from "ChannelPermission" where "channelId" = any($1)"#)
            .bind(&channels)
            .fetch_all(&s.db)
            .await?;
    let mut by_channel: HashMap<String, Vec<perm::Override>> = HashMap::new();
    for (channel_id, target_type, target_id, allow, deny) in overrides {
        by_channel.entry(channel_id.clone()).or_default().push(perm::Override { channel_id, target_type, target_id, allow, deny });
    }
    let mut per_channel = Map::new();
    for id in channels {
        let granted = perm::apply_overrides(&eff.permissions, &eff, by_channel.get(&id).map(Vec::as_slice).unwrap_or(&[]));
        per_channel.insert(id, json!(granted));
    }
    ok(json!({ "owner": eff.owner, "permissions": eff.permissions, "roles": eff.roles_json(), "allow": eff.allow, "deny": eff.deny, "channels": per_channel }))
}

pub fn routes() -> Router<AppState> {
    const A: &str = "/organizations/{organizationId}/admin";
    Router::new()
        .route(A, patch(rename))
        .route(&format!("{A}/access"), get(access))
        .route(&format!("{A}/access/projects/{{projectId}}/members/{{userId}}"), patch(set_project_access))
        .route(&format!("{A}/access/channels/{{channelId}}/members/{{userId}}"), patch(set_channel_access))
        .route(&format!("{A}/overview"), get(overview))
        .route(&format!("{A}/members"), get(members))
        .route(&format!("{A}/members/{{userId}}"), delete(remove_member))
        .route(&format!("{A}/members/{{userId}}/transfer-owner"), post(transfer_owner))
        .route(&format!("{A}/members/{{userId}}/sessions/revoke"), post(revoke_sessions))
        .route(&format!("{A}/members/{{userId}}/permissions"), patch(set_permissions))
        .route(&format!("{A}/permissions"), get(permissions))
        .route(&format!("{A}/invites"), get(invites).post(create_invite))
        .route(&format!("{A}/invites/rotate"), post(rotate_invite))
        .route(&format!("{A}/invites/{{inviteId}}"), delete(revoke_invite))
        .route(&format!("{A}/projects/trash"), get(deleted_projects))
        .route(&format!("{A}/projects/{{projectId}}/purge"), delete(purge_project))
        .route(&format!("{A}/channels"), get(channels))
        .route("/organizations/{organizationId}/permissions/me", get(my_permissions))
}
