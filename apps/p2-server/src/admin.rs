//! Proje yönetim paneli (docs/proje-bazli-yapi.md). Her proje kendi panelini taşır: üyeler (çıkar,
//! yasakla, yetki), davet kodları, erişim tablosu, kanallar, genel bakış ve ayarlar. Uçlar
//! `/projects/{projectId}/admin/...` altındadır; eski `/organizations/{organizationId}/admin/...`
//! adresleri proje kapsamına eşlenir (`crate::scope`).
//!
//! Kurulum (kuruluş sahibi) uçları ayrıdır: kuruluş adı, bir kişinin bütün oturumlarını kapatmak,
//! geri dönüşümdeki projeler.

use std::collections::{HashMap, HashSet};

use axum::Router;
use axum::extract::{Path, State};
use axum::routing::{delete, get, patch, post};
use serde_json::{Map, Value, json};

use crate::AppState;
use crate::community::{invite_json, remove_from_project};
use crate::ctx::{Body, Me, Query, Reply, created, forbidden, many_json, not_found, now, ok, sql};
use crate::error::{ApiError, ApiResult};
use crate::perm;
use crate::roles::both;
use crate::scope::{Scope, instance_owner};
use crate::validate::Checker;

type J = sqlx::types::Json<Value>;

/// (üyelik rolü, birincil sahip mi) — üye değilse `None`.
async fn member_role(s: &AppState, project_id: &str, user_id: &str) -> ApiResult<Option<(String, bool)>> {
    Ok(sqlx::query_as(
        r#"select m.role::text, p."ownerId" = m."userId" from "ProjectMember" m join "Project" p on p.id = m."projectId" where m."projectId" = $1 and m."userId" = $2"#,
    )
    .bind(project_id)
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

/// Çıkarma/yasaklama kuralları: kendini çıkaramazsın; birincil sahip korunur; sahip üyeleri yalnız
/// birincil sahip çıkarır. Hedefin üyeliğini döndürür (üye değilse `None`).
async fn guard_target(s: &AppState, sc: &Scope, user_id: &str, action: &str) -> ApiResult<Option<(String, bool)>> {
    if user_id == sc.user {
        return Err(ApiError::conflict("SELF_REMOVE", &format!("Kendini {action}.")));
    }
    let target = member_role(s, &sc.project, user_id).await?;
    if let Some((role, primary)) = &target {
        if *primary {
            return Err(ApiError::conflict("OWNER_PROTECTED", "Projenin sahibi çıkarılamaz ya da yasaklanamaz."));
        }
        if role == "OWNER" {
            let me = member_role(s, &sc.project, &sc.user).await?;
            if !me.is_some_and(|(_, primary)| primary) {
                return Err(forbidden("ADMIN_PROTECTED", "Bir sahibi yalnızca projenin birincil sahibi çıkarabilir."));
            }
        }
    }
    Ok(target)
}

// ------------------------------------------------------------------ erişim (üyelik/kanal görünürlüğü)

async fn access(State(s): State<AppState>, mut sc: Scope) -> Reply {
    sc.require(&s, "member.roles").await?;
    let members: Vec<(String, J)> = sqlx::query_as(
        r#"select u.id, json_build_object('id', u.id, 'username', u.username, 'displayName', u."displayName", 'avatarUrl', u."avatarUrl", 'owner', m.role = 'OWNER')
           from "ProjectMember" m join "User" u on u.id = m."userId" where m."projectId" = $1 order by m."joinedAt" asc, u.id asc"#,
    )
    .bind(&sc.project)
    .fetch_all(&s.db)
    .await?;
    let project: (String, String) = sqlx::query_as(r#"select name, "ownerId" from "Project" where id = $1"#).bind(&sc.project).fetch_one(&s.db).await?;
    let channels: Vec<(String, String, String, String)> = sqlx::query_as(
        r#"select ch.id, ch.name, ch.type::text, c.name from "ProjectChannel" ch join "ChannelCategory" c on c.id = ch."categoryId"
           where c."projectId" = $1 and c."deletedAt" is null and ch."deletedAt" is null order by c.position asc, ch.position asc"#,
    )
    .bind(&sc.project)
    .fetch_all(&s.db)
    .await?;
    let channel_ids: Vec<String> = channels.iter().map(|c| c.0.clone()).collect();
    let mut visible_by_user: HashMap<String, HashSet<String>> = HashMap::new();
    for (uid, _) in &members {
        visible_by_user.insert(uid.clone(), perm::visible_channel_ids(&s.db, &sc.org, uid, &channel_ids).await?);
    }
    let project_json = json!({
        "id": sc.project, "name": project.0, "ownerId": project.1,
        "memberIds": members.iter().map(|(u, _)| u.clone()).collect::<Vec<_>>(),
        "channels": channels.iter().map(|(id, name, kind, category)| json!({
            "id": id, "name": name, "type": kind, "category": category,
            "visibleUserIds": members.iter().filter(|(uid, _)| visible_by_user.get(uid).is_some_and(|v| v.contains(id))).map(|(uid, _)| uid.clone()).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    });
    ok(json!({ "projectId": sc.project, "members": members.into_iter().map(|(_, j)| j.0).collect::<Vec<_>>(), "projects": [project_json] }))
}

/// Kişiyi projeye ekle/çıkar. Eklemek için kişi benimle başka bir projeyi paylaşmalı ve yasaklı olmamalı.
async fn set_project_access(State(s): State<AppState>, mut sc: Scope, Body(body): Body) -> Reply {
    let visible = visibility(&body)?;
    sc.require(&s, "member.roles").await?;
    let user_id = sc.p("userId");
    if visible {
        if perm::peers_among(&s.db, &sc.user, std::slice::from_ref(&user_id)).await?.is_empty() {
            return Err(member_not_found());
        }
        crate::community::add_member(&s, &sc.org, &sc.project, &user_id, None).await?;
    } else {
        match guard_target(&s, &sc, &user_id, "projeden çıkaramazsın").await {
            Err(e) if e.body["code"] == "OWNER_PROTECTED" || e.body["code"] == "ADMIN_PROTECTED" => {
                return Err(ApiError::conflict("ACCESS_PROTECTED", "Proje sahibinin proje erişimi kapatılamaz."));
            }
            Err(e) => return Err(e),
            Ok(None) => {}
            Ok(Some(_)) => remove_from_project(&s, &sc.org, &sc.project, &user_id, false).await?,
        }
    }
    crate::realtime::emit_users(&s, std::slice::from_ref(&user_id), "project:permissions", &json!({ "organizationId": sc.org, "projectId": sc.project, "visible": visible })).await;
    crate::realtime::emit_project(&s, &sc.org, &sc.project, "project:members", &json!({ "projectId": sc.project })).await;
    ok(json!({ "projectId": sc.project, "userId": user_id, "visible": visible }))
}

/// Kişiye kanal görünürlüğü. Kanalın projesi esas alınır (eski adreste proje yok).
async fn set_channel_access(State(s): State<AppState>, me: Me, Path(params): Path<HashMap<String, String>>, Body(body): Body) -> Reply {
    let visible = visibility(&body)?;
    let channel_id = params.get("channelId").cloned().unwrap_or_default();
    let user_id = params.get("userId").cloned().unwrap_or_default();
    let project: Option<(String, Option<String>)> = sqlx::query_as(
        r#"select c."projectId", p."organizationId" from "ProjectChannel" ch join "ChannelCategory" c on c.id = ch."categoryId" join "Project" p on p.id = c."projectId"
           where ch.id = $1 and ch."deletedAt" is null and p."deletedAt" is null"#,
    )
    .bind(&channel_id)
    .fetch_optional(&s.db)
    .await?;
    let Some((project_id, Some(organization_id))) = project else { return Err(not_found("CHANNEL_NOT_FOUND", "Kanal bulunamadı.")) };
    let wrong_place = params.get("projectId").is_some_and(|p| *p != project_id) || params.get("organizationId").is_some_and(|o| *o != organization_id);
    if wrong_place || !perm::is_member(&s.db, &project_id, &me.id).await? {
        return Err(not_found("CHANNEL_NOT_FOUND", "Kanal bulunamadı."));
    }
    perm::require_in(&s.db, &project_id, &me.id, "member.roles").await?;
    let Some((role, _)) = member_role(&s, &project_id, &user_id).await? else { return Err(member_not_found()) };
    if role == "OWNER" {
        return Err(ApiError::conflict("ACCESS_PROTECTED", "Proje sahibinin kanal erişimi kapatılamaz."));
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
    .bind(crate::ctx::uuid())
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

async fn overview(State(s): State<AppState>, mut sc: Scope) -> Reply {
    sc.require(&s, "admin.panel").await?;
    let organization: Option<J> = sqlx::query_scalar(sql(format!(
        r#"select json_build_object('id', id, 'name', name, 'slug', slug, 'createdAt', {}) from "Organization" where id = $1"#,
        crate::iso!(r#""createdAt""#)
    )))
    .bind(&sc.org)
    .fetch_optional(&s.db)
    .await?;
    let organization = organization.ok_or_else(|| ApiError::internal("Organization bulunamadı"))?.0;
    let project: J = sqlx::query_scalar(sql(format!(r#"select {} from "Project" p where p.id = $1"#, crate::projects::PROJECT_JSON))).bind(&sc.project).fetch_one(&s.db).await?;
    let at = now();
    let week = at - chrono::Duration::milliseconds(7 * 86_400_000);
    let c: (i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        r#"select
             (select count(*) from "ProjectMember" where "projectId" = $1),
             (select count(*) from "ProjectMember" where "projectId" = $1 and role = 'OWNER'),
             (select count(*) from "ProjectMember" m join "User" u on u.id = m."userId" where m."projectId" = $1 and u.status <> 'OFFLINE'),
             (select count(*) from "ProjectMember" where "projectId" = $1 and "joinedAt" > $2),
             (select count(*) from "ProjectChannel" ch join "ChannelCategory" c on c.id = ch."categoryId" where c."projectId" = $1 and ch."deletedAt" is null),
             (select count(*) from "ProjectChannel" ch join "ChannelCategory" c on c.id = ch."categoryId" where c."projectId" = $1 and ch."deletedAt" is not null),
             (select count(*) from "ChatGroup" where "projectId" = $1),
             (select count(*) from "ChannelMessage" m join "ProjectChannel" ch on ch.id = m."channelId" join "ChannelCategory" c on c.id = ch."categoryId" where c."projectId" = $1),
             (select count(*) from "ChatGroupMessage" m join "ChatGroup" g on g.id = m."groupId" where g."projectId" = $1),
             (select count(*) from "ProjectInvite" where "projectId" = $1 and not revoked),
             (select count(*) from "ProjectMember" m join "User" u on u.id = m."userId" where m."projectId" = $1 and u."twoFactorEnabled"),
             (select count(*) from "Role" where "projectId" = $1 and not everyone),
             (select count(*) from "Session" x where x."expiresAt" > $3 and exists (select 1 from "ProjectMember" m where m."userId" = x."userId" and m."projectId" = $1)),
             (select count(*) from "ProjectBan" where "projectId" = $1)"#,
    )
    .bind(&sc.project)
    .bind(week)
    .bind(at)
    .fetch_one(&s.db)
    .await?;
    let (total, owners, online, joined, channels, deleted_channels, groups, channel_messages, group_messages, invites, two_factor, role_count, sessions, bans) = c;
    let version = if s.cfg.version.is_empty() { "dev".to_string() } else { s.cfg.version.clone() };
    let storage = tokio::task::spawn_blocking(storage_used).await.unwrap_or_else(|_| json!({ "bytes": 0, "files": 0 }));
    ok(json!({
        "organization": organization,
        "project": project.0,
        "server": { "version": version },
        "members": { "total": total, "owners": owners, "roles": role_count, "online": online, "joinedThisWeek": joined, "twoFactor": two_factor, "banned": bans },
        "content": {
            "projects": 1, "channels": channels, "deletedChannels": deleted_channels, "groups": groups,
            "channelMessages": channel_messages, "directMessages": 0, "groupMessages": group_messages,
            "messages": channel_messages + group_messages,
        },
        "invites": invites,
        "sessions": sessions,
        "storage": storage,
    }))
}

async fn members(State(s): State<AppState>, mut sc: Scope) -> Reply {
    sc.require(&s, "admin.panel").await?;
    let rows: Vec<(String, bool, String, J)> = sqlx::query_as(sql(format!(
        r#"select u.id, m.role = 'OWNER', {}, json_build_object('id', u.id, 'username', u.username, 'displayName', u."displayName", 'email', u.email, 'title', u.title,
             'avatarUrl', u."avatarUrl", 'status', u.status, 'twoFactorEnabled', u."twoFactorEnabled", 'createdAt', {}, 'lastClientVersion', u."lastClientVersion",
             'lastClientSeenAt', {}, 'role', m.role, 'primaryOwner', p."ownerId" = m."userId")
           from "ProjectMember" m join "User" u on u.id = m."userId" join "Project" p on p.id = m."projectId" where m."projectId" = $1 order by m."joinedAt" asc, u.id asc"#,
        crate::iso!(r#"m."joinedAt""#),
        crate::iso!(r#"u."createdAt""#),
        crate::iso!(r#"u."lastClientSeenAt""#)
    )))
    .bind(&sc.project)
    .fetch_all(&s.db)
    .await?;
    let counts: Vec<(String, i64)> = sqlx::query_as(
        r#"select m."authorId", count(*) from "ChannelMessage" m join "ProjectChannel" ch on ch.id = m."channelId" join "ChannelCategory" c on c.id = ch."categoryId"
           where c."projectId" = $1 group by m."authorId""#,
    )
    .bind(&sc.project)
    .fetch_all(&s.db)
    .await?;
    let by_author: HashMap<String, i64> = counts.into_iter().collect();
    // Kişinin rolleri: doğrudan atananlar + gruplarından gelenler.
    let roles: Vec<(String, String, String, Option<String>, bool, Vec<String>, Vec<String>)> = sqlx::query_as(
        r#"select r.id, r.handle, r.name, r.color, r.founder,
             array(select "userId" from "RoleMember" where "roleId" = r.id),
             array(select gm."userId" from "RoleGroup" rg join "UserGroupMember" gm on gm."groupId" = rg."groupId" where rg."roleId" = r.id)
           from "Role" r where r."projectId" = $1 and r.everyone = false order by r.founder desc, r.handle asc"#,
    )
    .bind(&sc.project)
    .fetch_all(&s.db)
    .await?;
    let group_rows: Vec<(String, J)> = sqlx::query_as(
        r#"select gm."userId", json_build_object('id', g.id, 'handle', g.handle, 'name', g.name, 'color', g.color)
           from "UserGroupMember" gm join "UserGroup" g on g.id = gm."groupId" where g."projectId" = $1 order by g.handle asc"#,
    )
    .bind(&sc.project)
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

async fn notify(s: &AppState, organization_id: &str, project_id: &str, user_id: &str) {
    if let Ok(after) = perm::effective_in(&s.db, project_id, user_id).await {
        crate::realtime::emit_users(
            s,
            &[user_id.to_string()],
            "permissions:changed",
            &json!({ "organizationId": organization_id, "projectId": project_id, "permissions": after.permissions }),
        )
        .await;
    }
}

// Sahiplik devri: yalnızca projenin birincil sahibi devredebilir; kurucu rolü de geçer.
async fn transfer_owner(State(s): State<AppState>, sc: Scope) -> Reply {
    let user_id = sc.p("userId");
    let mine = member_role(&s, &sc.project, &sc.user).await?;
    if !mine.is_some_and(|(_, primary)| primary) {
        return Err(forbidden("OWNER_ONLY", "Sahipliği yalnızca projenin sahibi devredebilir."));
    }
    if user_id == sc.user {
        return Err(ApiError::conflict("SELF_OWNER", "Sahiplik zaten sende."));
    }
    if member_role(&s, &sc.project, &user_id).await?.is_none() {
        return Err(member_not_found());
    }
    let founder: Option<String> = sqlx::query_scalar(r#"select id from "Role" where "projectId" = $1 and founder = true limit 1"#)
        .bind(&sc.project)
        .fetch_optional(&s.db)
        .await?;
    let mut tx = s.db.begin().await?;
    let set_role = r#"update "ProjectMember" set role = $3::"ProjectRole" where "projectId" = $1 and "userId" = $2"#;
    sqlx::query(set_role).bind(&sc.project).bind(&user_id).bind("OWNER").execute(&mut *tx).await?;
    sqlx::query(set_role).bind(&sc.project).bind(&sc.user).bind("MEMBER").execute(&mut *tx).await?;
    sqlx::query(r#"update "Project" set "ownerId" = $2, "updatedAt" = $3 where id = $1"#).bind(&sc.project).bind(&user_id).bind(now()).execute(&mut *tx).await?;
    // Kurucu rolü proje sahipliğini izler.
    if let Some(founder) = founder {
        sqlx::query(r#"delete from "RoleMember" where "roleId" = $1 and "userId" = $2"#).bind(&founder).bind(&sc.user).execute(&mut *tx).await?;
        sqlx::query(r#"insert into "RoleMember" ("roleId", "userId", "addedAt") values ($1, $2, $3) on conflict do nothing"#)
            .bind(&founder)
            .bind(&user_id)
            .bind(now())
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    for id in [&user_id, &sc.user] {
        notify(&s, &sc.org, &sc.project, id).await;
    }
    crate::realtime::emit_project(&s, &sc.org, &sc.project, "project:members", &json!({ "projectId": sc.project })).await;
    created(json!({ "owner": user_id, "projectId": sc.project }))
}

// Üyeyi projeden çıkarma: tekrar davetle girebilir.
async fn remove_member(State(s): State<AppState>, mut sc: Scope) -> Reply {
    sc.require(&s, "member.kick").await?;
    let user_id = sc.p("userId");
    if guard_target(&s, &sc, &user_id, "projeden çıkaramazsın").await?.is_none() {
        return Err(member_not_found());
    }
    remove_from_project(&s, &sc.org, &sc.project, &user_id, false).await?;
    ok(json!({ "userId": user_id, "projectId": sc.project, "removed": true }))
}

async fn bans(State(s): State<AppState>, mut sc: Scope) -> Reply {
    sc.require(&s, "member.kick").await?;
    let rows = many_json(
        sqlx::query_scalar(sql(format!(
            r#"select json_build_object('userId', b."userId", 'reason', b.reason, 'createdAt', {},
                 'user', (select json_build_object('id', u.id, 'username', u.username, 'displayName', u."displayName", 'avatarUrl', u."avatarUrl") from "User" u where u.id = b."userId"),
                 'bannedBy', (select json_build_object('id', u.id, 'displayName', u."displayName") from "User" u where u.id = b."bannedById"))
               from "ProjectBan" b where b."projectId" = $1 order by b."createdAt" desc"#,
            crate::iso!(r#"b."createdAt""#)
        )))
        .bind(&sc.project),
        &s.db,
    )
    .await?;
    ok(Value::Array(rows))
}

// Yasakla: üyeyse çıkarılır; yasak kalkana kadar hiçbir kodla geri katılamaz. Üye olmayan biri
// (ör. az önce çıkarılan) yalnız bu projede daha önce bulunduysa ya da yöneticiyle bir projeyi paylaşıyorsa yasaklanır.
async fn ban(State(s): State<AppState>, mut sc: Scope, Body(body): Body) -> Reply {
    let mut c = Checker::new(&body);
    let reason = c.str("reason");
    c.field("reason", &[(!c.present("reason") || reason.is_some_and(|r| crate::validate::js_len(r) <= 240), None)]);
    c.finish()?;
    sc.require(&s, "member.kick").await?;
    let user_id = sc.p("userId");
    let target = guard_target(&s, &sc, &user_id, "yasaklayamazsın").await?;
    if target.is_none() && perm::peers_among(&s.db, &sc.user, std::slice::from_ref(&user_id)).await?.is_empty() {
        let was_here: Option<i32> = sqlx::query_scalar(r#"select 1 from "ChannelMessage" m join "ProjectChannel" ch on ch.id = m."channelId" join "ChannelCategory" c on c.id = ch."categoryId" where c."projectId" = $1 and m."authorId" = $2 limit 1"#)
            .bind(&sc.project)
            .bind(&user_id)
            .fetch_optional(&s.db)
            .await?;
        if was_here.is_none() {
            return Err(member_not_found());
        }
    }
    let reason = reason.map(str::trim).filter(|r| !r.is_empty()).map(str::to_string);
    sqlx::query(
        r#"insert into "ProjectBan" ("projectId", "userId", "bannedById", reason, "createdAt") values ($1, $2, $3, $4, $5)
           on conflict ("projectId", "userId") do update set reason = excluded.reason, "bannedById" = excluded."bannedById""#,
    )
    .bind(&sc.project)
    .bind(&user_id)
    .bind(&sc.user)
    .bind(&reason)
    .bind(now())
    .execute(&s.db)
    .await?;
    if target.is_some() {
        remove_from_project(&s, &sc.org, &sc.project, &user_id, true).await?;
    }
    ok(json!({ "userId": user_id, "projectId": sc.project, "banned": true }))
}

async fn unban(State(s): State<AppState>, mut sc: Scope) -> Reply {
    sc.require(&s, "member.kick").await?;
    let user_id = sc.p("userId");
    let n = sqlx::query(r#"delete from "ProjectBan" where "projectId" = $1 and "userId" = $2"#).bind(&sc.project).bind(&user_id).execute(&s.db).await?.rows_affected();
    if n == 0 {
        return Err(not_found("BAN_NOT_FOUND", "Yasak bulunamadı."));
    }
    ok(json!({ "userId": user_id, "projectId": sc.project, "banned": false }))
}

// Çalınan cihaz / paylaşılmış parola: kişinin BÜTÜN oturumları düşer. Projeler arası bir güç olduğu
// için yalnız kurulum (kuruluş) sahibine açıktır.
async fn revoke_sessions(State(s): State<AppState>, me: Me, Path((organization_id, user_id)): Path<(String, String)>) -> Reply {
    crate::activity::membership(&s.db, &organization_id, &me.id).await?;
    if !instance_owner(&s.db, &organization_id, &me.id).await? {
        return Err(forbidden("OWNER_ONLY", "Bir kişinin bütün oturumlarını yalnızca sunucu sahibi kapatabilir."));
    }
    let target: Option<i32> = sqlx::query_scalar(r#"select 1 from "OrganizationMember" where "organizationId" = $1 and "userId" = $2"#)
        .bind(&organization_id)
        .bind(&user_id)
        .fetch_optional(&s.db)
        .await?;
    if target.is_none() {
        return Err(member_not_found());
    }
    let closed = sqlx::query(r#"delete from "Session" where "userId" = $1"#).bind(&user_id).execute(&s.db).await?.rows_affected();
    created(json!({ "userId": user_id, "closed": closed }))
}

// Yetki kataloğu + her üyenin nereden hangi yetkiyi aldığı.
async fn permissions(State(s): State<AppState>, mut sc: Scope) -> Reply {
    sc.require(&s, "member.roles").await?;
    let members: Vec<(String, bool, Vec<String>, Vec<String>, J)> = sqlx::query_as(
        r#"select u.id, m.role = 'OWNER', m.permissions, m."deniedPermissions",
             json_build_object('id', u.id, 'username', u.username, 'displayName', u."displayName", 'avatarUrl', u."avatarUrl")
           from "ProjectMember" m join "User" u on u.id = m."userId" where m."projectId" = $1 order by m."joinedAt" asc, u.id asc"#,
    )
    .bind(&sc.project)
    .fetch_all(&s.db)
    .await?;
    let mut rows = Vec::new();
    for (id, owner, allow, deny, user) in &members {
        let eff = perm::effective_in(&s.db, &sc.project, id).await?;
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
           from "Role" r where r."projectId" = $1 order by r.founder desc, r.everyone desc, r.handle asc"#,
    )
    .bind(&sc.project)
    .fetch_all(&s.db)
    .await?;
    let groups = many_json(
        sqlx::query_scalar(
            r#"select json_build_object('id', g.id, 'handle', g.handle, 'name', g.name, 'color', g.color,
                 'memberCount', (select count(*) from "UserGroupMember" x where x."groupId" = g.id))
               from "UserGroup" g where g."projectId" = $1 order by g.handle asc"#,
        )
        .bind(&sc.project),
        &s.db,
    )
    .await?;
    let all_ids: Vec<String> = members.iter().map(|m| m.0.clone()).collect();
    let role_ids: Vec<String> = roles.iter().map(|r| r.0.clone()).collect();
    let role_members: Vec<(String, String)> = sqlx::query_as(r#"select "roleId", "userId" from "RoleMember" where "roleId" = any($1) order by "addedAt" asc"#)
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
    ok(json!({ "projectId": sc.project, "catalog": perm::catalog_json(), "members": rows, "groups": groups, "roles": roles_json }))
}

// Kişiye özel yetkiler: rolün üstüne eklenenler ve rolden/gruptan gelse de kapatılanlar.
async fn set_permissions(State(s): State<AppState>, mut sc: Scope, Body(body): Body) -> Reply {
    sc.require(&s, "member.roles").await?;
    let user_id = sc.p("userId");
    let Some((role, _)) = member_role(&s, &sc.project, &user_id).await? else { return Err(member_not_found()) };
    if role == "OWNER" {
        return Err(ApiError::conflict("OWNER_PROTECTED", "Proje sahibinin yetkileri kısıtlanamaz."));
    }
    let allow = perm::clean_value(body.get("allow"));
    let deny: Vec<String> = perm::clean_value(body.get("deny")).into_iter().filter(|d| !allow.contains(d)).collect();
    sqlx::query(r#"update "ProjectMember" set permissions = $3, "deniedPermissions" = $4 where "projectId" = $1 and "userId" = $2"#)
        .bind(&sc.project)
        .bind(&user_id)
        .bind(&allow)
        .bind(&deny)
        .execute(&s.db)
        .await?;
    let eff = perm::effective_in(&s.db, &sc.project, &user_id).await?;
    crate::realtime::emit_users(
        &s,
        std::slice::from_ref(&user_id),
        "permissions:changed",
        &json!({ "organizationId": sc.org, "projectId": sc.project, "permissions": eff.permissions }),
    )
    .await;
    ok(json!({ "userId": user_id, "projectId": sc.project, "allow": allow, "deny": deny, "effective": eff.permissions }))
}

// ------------------------------------------------------------------ davetler

async fn invites(State(s): State<AppState>, mut sc: Scope, q: Query) -> Reply {
    sc.require(&s, "member.invite").await?;
    let all = q.get("includeRevoked").is_some_and(|v| v == "1" || v == "true");
    let rows = many_json(
        sqlx::query_scalar(sql(format!(
            r#"select {} from "ProjectInvite" i where i."projectId" = $1 and ($2 or not i.revoked) order by i."createdAt" desc"#,
            invite_json()
        )))
        .bind(&sc.project)
        .bind(all),
        &s.db,
    )
    .await?;
    ok(Value::Array(rows))
}

async fn invite_by_id(s: &AppState, invite_id: &str) -> ApiResult<Value> {
    let row: J = sqlx::query_scalar(sql(format!(r#"select {} from "ProjectInvite" i where i.id = $1"#, invite_json()))).bind(invite_id).fetch_one(&s.db).await?;
    Ok(row.0)
}

async fn create_invite(State(s): State<AppState>, mut sc: Scope, Body(body): Body) -> Reply {
    let int = |k: &str, max: f64| match body.get(k) {
        None | Some(Value::Null) => true,
        Some(v) => v.as_f64().is_some_and(|f| f.fract() == 0.0 && (1.0..=max).contains(&f)),
    };
    let mut c = Checker::new(&body);
    c.field("maxUses", &[(int("maxUses", 1000.0), None)]);
    c.field("expiresInDays", &[(int("expiresInDays", 365.0), None)]);
    c.finish()?;
    sc.require(&s, "member.invite").await?;
    let max_uses = body.get("maxUses").and_then(Value::as_f64).map(|f| f as i32);
    let expires = body.get("expiresInDays").and_then(Value::as_f64).map(|d| now() + chrono::Duration::milliseconds((d * 86_400_000.0) as i64));
    let mut tx = s.db.begin().await?;
    let id = crate::community::insert_invite(&mut tx, &sc.project, &sc.user, &crate::organizations::invite_code(), max_uses, expires).await?;
    tx.commit().await?;
    created(invite_by_id(&s, &id).await?)
}

// "Davet kodunu yenile": yeni kalıcı kod üretir, eski kalıcı kodu iptal eder.
async fn rotate_invite(State(s): State<AppState>, mut sc: Scope) -> Reply {
    sc.require(&s, "member.invite").await?;
    let code = crate::organizations::invite_code();
    let mut tx = s.db.begin().await?;
    sqlx::query(r#"update "ProjectInvite" set revoked = true, "revokedAt" = $2 where "projectId" = $1 and not revoked and "maxUses" is null and "expiresAt" is null"#)
        .bind(&sc.project)
        .bind(now())
        .execute(&mut *tx)
        .await?;
    let id = crate::community::insert_invite(&mut tx, &sc.project, &sc.user, &code, None, None).await?;
    tx.commit().await?;
    crate::realtime::emit_project(&s, &sc.org, &sc.project, "project:invite", &json!({ "organizationId": sc.org, "projectId": sc.project })).await;
    created(invite_by_id(&s, &id).await?)
}

async fn revoke_invite(State(s): State<AppState>, mut sc: Scope) -> Reply {
    sc.require(&s, "member.invite").await?;
    let invite_id = sc.p("inviteId");
    let n = sqlx::query(r#"update "ProjectInvite" set revoked = true, "revokedAt" = coalesce("revokedAt", $3) where id = $1 and "projectId" = $2"#)
        .bind(&invite_id)
        .bind(&sc.project)
        .bind(now())
        .execute(&s.db)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(not_found("INVITE_NOT_FOUND", "Davet bulunamadı."));
    }
    ok(json!({ "id": invite_id, "revoked": true }))
}

// ------------------------------------------------------------------ projeler / kanallar

// Silinen projeler 30 gün geri dönüşümde bekler. Kurulum sahibi hepsini, diğerleri yalnız sahibi olduklarını görür.
async fn deleted_projects(State(s): State<AppState>, me: Me, Path(organization_id): Path<String>) -> Reply {
    crate::activity::membership(&s.db, &organization_id, &me.id).await?;
    let everything = instance_owner(&s.db, &organization_id, &me.id).await?;
    let rows = many_json(
        sqlx::query_scalar(sql(format!(
            r#"select json_build_object('id', p.id, 'name', p.name, 'slug', p.slug, 'deletedAt', {}, 'purgeAt', {}, 'createdAt', {},
                 'owner', coalesce(nullif((select u."displayName" from "User" u where u.id = p."ownerId"), ''), '—'),
                 'categories', (select count(*) from "ChannelCategory" c where c."projectId" = p.id),
                 'members', (select count(*) from "ProjectMember" m where m."projectId" = p.id))
               from "Project" p where p."organizationId" = $1 and p."deletedAt" is not null
                 and ($3 or exists (select 1 from "ProjectMember" m where m."projectId" = p.id and m."userId" = $2 and m.role = 'OWNER'))
               order by p."deletedAt" desc"#,
            crate::iso!(r#"p."deletedAt""#),
            crate::iso!(r#"p."purgeAt""#),
            crate::iso!(r#"p."createdAt""#)
        )))
        .bind(&organization_id)
        .bind(&me.id)
        .bind(everything),
        &s.db,
    )
    .await?;
    ok(Value::Array(rows))
}

// Kalıcı silme: proje ve ona bağlı her şey veritabanından gider. Kurulum sahibi ya da projenin sahibi.
async fn purge_project(State(s): State<AppState>, me: Me, Path((organization_id, project_id)): Path<(String, String)>) -> Reply {
    crate::activity::membership(&s.db, &organization_id, &me.id).await?;
    let project: Option<(String, String, Option<chrono::NaiveDateTime>, bool)> = sqlx::query_as(
        r#"select name, slug, "deletedAt", exists (select 1 from "ProjectMember" m where m."projectId" = p.id and m."userId" = $3 and m.role = 'OWNER')
           from "Project" p where id = $1 and "organizationId" = $2"#,
    )
    .bind(&project_id)
    .bind(&organization_id)
    .bind(&me.id)
    .fetch_optional(&s.db)
    .await?;
    let Some((name, slug, deleted, owner)) = project else { return Err(not_found("PROJECT_NOT_FOUND", "Proje bulunamadı.")) };
    if !owner && !instance_owner(&s.db, &organization_id, &me.id).await? {
        return Err(not_found("PROJECT_NOT_FOUND", "Proje bulunamadı."));
    }
    if deleted.is_none() {
        return Err(ApiError::conflict("PROJECT_NOT_DELETED", "Önce projeyi silmelisin; yalnızca geri dönüşümdeki projeler kalıcı silinebilir."));
    }
    let audience: Vec<String> = sqlx::query_scalar(r#"select "userId" from "ProjectMember" where "projectId" = $1"#).bind(&project_id).fetch_all(&s.db).await?;
    sqlx::query(r#"delete from "Project" where id = $1"#).bind(&project_id).execute(&s.db).await?;
    crate::realtime::emit_users(&s, &audience, "project:purged", &json!({ "projectId": project_id, "name": name, "slug": slug })).await;
    ok(json!({ "ok": true, "projectId": project_id }))
}

async fn channels(State(s): State<AppState>, mut sc: Scope) -> Reply {
    sc.require(&s, "admin.panel").await?;
    let rows = many_json(
        sqlx::query_scalar(sql(format!(
            r#"select json_build_object('id', ch.id, 'name', ch.name, 'topic', ch.topic, 'type', ch.type, 'deletedAt', {}, 'purgeAt', {}, 'createdAt', {},
                 'messages', (select count(*) from "ChannelMessage" m where m."channelId" = ch.id), 'category', c.name, 'project', p.name, 'projectId', p.id)
               from "ProjectChannel" ch join "ChannelCategory" c on c.id = ch."categoryId" join "Project" p on p.id = c."projectId"
               where p.id = $1 order by ch."deletedAt" asc, ch.position asc"#,
            crate::iso!(r#"ch."deletedAt""#),
            crate::iso!(r#"ch."purgeAt""#),
            crate::iso!(r#"ch."createdAt""#)
        )))
        .bind(&sc.project),
        &s.db,
    )
    .await?;
    ok(Value::Array(rows))
}

/// Kuruluş (kurulum) adı: yalnız kurulum sahibi.
async fn rename(State(s): State<AppState>, me: Me, Path(organization_id): Path<String>, Body(body): Body) -> Reply {
    let mut c = Checker::new(&body);
    let (len_ok, str_ok) = crate::roles::opt_len(&body, "name", 2, 60);
    c.field("name", &[(len_ok, None), (str_ok, None)]);
    c.finish()?;
    crate::activity::membership(&s.db, &organization_id, &me.id).await?;
    if !instance_owner(&s.db, &organization_id, &me.id).await? {
        return Err(perm::refuse("organization.manage"));
    }
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

/// Proje ayarları (ad, açıklama, ikon, kapak, varsayılan kanal, kütüphane): "Projeyi düzenle".
async fn settings(State(s): State<AppState>, sc: Scope, body: Body) -> Reply {
    crate::projects::update_project_in(&s, &sc.org, &sc.project, &sc.user, &body, "organization.manage").await
}

// ------------------------------------------------------------------ kendi yetkilerim

fn grants_json(channels: HashMap<String, Vec<String>>) -> Map<String, Value> {
    let mut per_channel = Map::new();
    for (id, granted) in channels {
        per_channel.insert(id, json!(granted));
    }
    per_channel
}

fn mine_json(eff: &perm::Effective, project_id: &str, channels: HashMap<String, Vec<String>>) -> Value {
    json!({ "projectId": project_id, "owner": eff.owner, "permissions": eff.permissions, "roles": eff.roles_json(), "allow": eff.allow, "deny": eff.deny, "channels": grants_json(channels) })
}

async fn project_channel_ids(s: &AppState, project_ids: &[String]) -> ApiResult<Vec<String>> {
    Ok(sqlx::query_scalar(
        r#"select ch.id from "ProjectChannel" ch join "ChannelCategory" c on c.id = ch."categoryId" where c."projectId" = any($1) and ch."deletedAt" is null"#,
    )
    .bind(project_ids)
    .fetch_all(&s.db)
    .await?)
}

async fn my_project_permissions(State(s): State<AppState>, sc: Scope) -> Reply {
    let eff = sc.effective(&s).await?;
    let ids = project_channel_ids(&s, std::slice::from_ref(&sc.project)).await?;
    let channels = perm::channel_grants(&s.db, &sc.org, &sc.user, &ids).await?;
    ok(mine_json(&eff, &sc.project, channels))
}

// Arayüzün neyi göstereceğine karar verebilmesi için herkes kendi yetkisini okur. `?projectId=` yoksa
// üye olunan projelerin birleşimi (kanallar her biri kendi projesine göre) ve proje başına döküm.
async fn my_permissions(State(s): State<AppState>, me: Me, Path(organization_id): Path<String>, q: Query) -> Reply {
    crate::activity::membership(&s.db, &organization_id, &me.id).await?;
    if let Some(project) = q.get("projectId").filter(|p| !p.is_empty()) {
        perm::require_project_access(&s.db, &organization_id, &me.id, project).await?;
        let eff = perm::effective_in(&s.db, project, &me.id).await?;
        let ids = project_channel_ids(&s, std::slice::from_ref(project)).await?;
        let channels = perm::channel_grants(&s.db, &organization_id, &me.id, &ids).await?;
        return ok(mine_json(&eff, project, channels));
    }
    let projects = perm::member_projects(&s.db, &organization_id, &me.id).await?;
    let mut owner = false;
    let mut union: Vec<String> = Vec::new();
    let mut per_project = Map::new();
    for project in &projects {
        let eff = perm::effective_in(&s.db, project, &me.id).await?;
        owner |= eff.owner;
        for p in &eff.permissions {
            if !union.contains(p) {
                union.push(p.clone());
            }
        }
        per_project.insert(project.clone(), json!({ "owner": eff.owner, "permissions": eff.permissions }));
    }
    let ids = project_channel_ids(&s, &projects).await?;
    let channels = perm::channel_grants(&s.db, &organization_id, &me.id, &ids).await?;
    ok(json!({ "projectId": null, "owner": owner, "permissions": union, "roles": [], "allow": [], "deny": [], "channels": grants_json(channels), "projects": per_project }))
}

pub fn routes() -> Router<AppState> {
    const ORG: &str = "/organizations/{organizationId}/admin";
    let r = Router::new()
        // Kurulum (kuruluş) uçları.
        .route(ORG, patch(rename))
        .route(&format!("{ORG}/members/{{userId}}/sessions/revoke"), post(revoke_sessions))
        .route(&format!("{ORG}/projects/trash"), get(deleted_projects))
        .route(&format!("{ORG}/projects/{{projectId}}/purge"), delete(purge_project))
        // Eski erişim adresleri (proje/kanal adreste).
        .route(&format!("{ORG}/access/projects/{{projectId}}/members/{{userId}}"), patch(set_project_access))
        .route(&format!("{ORG}/access/channels/{{channelId}}/members/{{userId}}"), patch(set_channel_access))
        .route("/projects/{projectId}/admin/access/channels/{channelId}/members/{userId}", patch(set_channel_access))
        .route("/projects/{projectId}/admin/access/members/{userId}", patch(set_project_access))
        // Proje ayarları.
        .route("/projects/{projectId}/admin", patch(settings))
        .route("/organizations/{organizationId}/projects/{projectId}/admin", patch(settings))
        // Kendi yetkilerim.
        .route("/organizations/{organizationId}/permissions/me", get(my_permissions))
        .route("/projects/{projectId}/permissions/me", get(my_project_permissions));
    let r = both(r, "/admin/access", get(access));
    let r = both(r, "/admin/overview", get(overview));
    let r = both(r, "/admin/members", get(members));
    let r = both(r, "/admin/members/{userId}", delete(remove_member));
    let r = both(r, "/admin/members/{userId}/transfer-owner", post(transfer_owner));
    let r = both(r, "/admin/members/{userId}/permissions", patch(set_permissions));
    let r = both(r, "/admin/bans", get(bans));
    let r = both(r, "/admin/bans/{userId}", axum::routing::put(ban).delete(unban));
    let r = both(r, "/admin/permissions", get(permissions));
    let r = both(r, "/admin/invites", get(invites).post(create_invite));
    let r = both(r, "/admin/invites/rotate", post(rotate_invite));
    let r = both(r, "/admin/invites/{inviteId}", delete(revoke_invite));
    both(r, "/admin/channels", get(channels))
}
