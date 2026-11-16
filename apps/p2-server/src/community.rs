//! Proje toplulukları (docs/proje-bazli-yapi.md): proje davet kodları, kodla katılma ve önizleme,
//! projeden ayrılma/çıkarılma, proje kişi listesi, "projelerim" ve herkesin proje açabilmesi.

use axum::Router;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use chrono::NaiveDateTime;
use serde_json::{Value, json};

use crate::AppState;
use crate::ctx::{Body, Me, Reply, created, forbidden, many_json, not_found, now, ok, sql, uuid};
use crate::error::{ApiError, ApiResult};
use crate::scope::Scope;
use crate::validate::{Checker, js_len};

type J = sqlx::types::Json<Value>;

/// ProjectInvite (alias `i`).
pub fn invite_json() -> String {
    format!(
        r#"json_build_object('id', i.id, 'code', i.code, 'projectId', i."projectId", 'maxUses', i."maxUses", 'useCount', i."useCount", 'expiresAt', {}, 'createdAt', {},
             'revoked', i.revoked, 'revokedAt', {},
             'createdBy', (select json_build_object('id', u.id, 'displayName', u."displayName") from "User" u where u.id = i."createdById"))"#,
        crate::iso!(r#"i."expiresAt""#),
        crate::iso!(r#"i."createdAt""#),
        crate::iso!(r#"i."revokedAt""#)
    )
}

pub async fn insert_invite(
    tx: &mut sqlx::PgConnection,
    project_id: &str,
    user_id: &str,
    code: &str,
    max_uses: Option<i32>,
    expires: Option<NaiveDateTime>,
) -> ApiResult<String> {
    let id = uuid();
    sqlx::query(r#"insert into "ProjectInvite" (id, code, "projectId", "createdById", "expiresAt", "maxUses", "useCount", "createdAt") values ($1, $2, $3, $4, $5, $6, 0, $7)"#)
        .bind(&id)
        .bind(code)
        .bind(project_id)
        .bind(user_id)
        .bind(expires)
        .bind(max_uses)
        .bind(now())
        .execute(&mut *tx)
        .await?;
    Ok(id)
}

/// "p2-ab12 cd34" → "P2-AB12CD34".
pub fn normalize(code: &str) -> String {
    code.trim().to_uppercase().chars().filter(|c| !c.is_whitespace()).collect()
}

pub struct Invite {
    pub id: String,
    pub project_id: String,
    pub organization_id: String,
    expires: Option<NaiveDateTime>,
    max_uses: Option<i32>,
    use_count: i32,
    revoked: bool,
}

impl Invite {
    /// Kod kullanılamıyorsa nedeni.
    pub fn problem(&self) -> Option<ApiError> {
        if self.revoked {
            return Some(ApiError::bad_request("INVITE_REVOKED", "Bu davet kodu iptal edilmiş."));
        }
        if self.expires.is_some_and(|e| e < now()) {
            return Some(ApiError::bad_request("INVITE_EXPIRED", "Bu davetin süresi dolmuş."));
        }
        if self.max_uses.is_some_and(|m| self.use_count >= m) {
            return Some(ApiError::bad_request("INVITE_FULL", "Bu davetin kullanım sınırı dolmuş."));
        }
        None
    }
}

fn invite_missing() -> ApiError {
    not_found("INVITE_NOT_FOUND", "Davet kodu bulunamadı.")
}

/// Proje davet kodu (silinmiş projenin kodu yok sayılır).
pub async fn find_invite(db: &sqlx::PgPool, code: &str) -> ApiResult<Option<Invite>> {
    let row: Option<(String, String, Option<String>, Option<NaiveDateTime>, Option<i32>, i32, bool)> = sqlx::query_as(
        r#"select i.id, i."projectId", p."organizationId", i."expiresAt", i."maxUses", i."useCount", i.revoked
           from "ProjectInvite" i join "Project" p on p.id = i."projectId" where i.code = $1 and p."deletedAt" is null"#,
    )
    .bind(normalize(code))
    .fetch_optional(db)
    .await?;
    Ok(row.and_then(|(id, project_id, org, expires, max_uses, use_count, revoked)| {
        org.map(|organization_id| Invite { id, project_id, organization_id, expires, max_uses, use_count, revoked })
    }))
}

/// Katılmadan önce: kod var ve kullanılabilir olmalı. Eski kuruluş kodu `INVITE_REVOKED`.
pub async fn usable_invite(db: &sqlx::PgPool, code: &str) -> ApiResult<Invite> {
    if let Some(invite) = find_invite(db, code).await? {
        return match invite.problem() {
            Some(error) => Err(error),
            None => Ok(invite),
        };
    }
    let legacy: Option<i32> = sqlx::query_scalar(r#"select 1 from "OrganizationInvite" where code = $1"#).bind(normalize(code)).fetch_optional(db).await?;
    if legacy.is_some() {
        return Err(ApiError::bad_request("INVITE_REVOKED", "Eski ekip davet kodları artık geçerli değil; projenin davet kodunu iste."));
    }
    Err(invite_missing())
}

pub async fn banned(db: &sqlx::PgPool, project_id: &str, user_id: &str) -> ApiResult<bool> {
    let hit: Option<i32> = sqlx::query_scalar(r#"select 1 from "ProjectBan" where "projectId" = $1 and "userId" = $2"#)
        .bind(project_id)
        .bind(user_id)
        .fetch_optional(db)
        .await?;
    Ok(hit.is_some())
}

/// Projenin kişi listesindeki bir satır (alias `u` kullanıcı, `m` ProjectMember, `p` Project).
pub fn member_json() -> String {
    format!(
        r#"json_build_object({}, 'role', m.role, 'owner', m.role = 'OWNER', 'primaryOwner', p."ownerId" = m."userId", 'joinedAt', {})"#,
        crate::organizations::MEMBER_USER_FIELDS,
        crate::iso!(r#"m."joinedAt""#)
    )
}

async fn member_row(db: &sqlx::PgPool, project_id: &str, user_id: &str) -> ApiResult<Option<Value>> {
    let row: Option<J> = sqlx::query_scalar(sql(format!(
        r#"select {} from "ProjectMember" m join "User" u on u.id = m."userId" join "Project" p on p.id = m."projectId" where m."projectId" = $1 and m."userId" = $2"#,
        member_json()
    )))
    .bind(project_id)
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    Ok(row.map(|j| j.0))
}

/// Kişiyi projeye ekler (kuruluş kabına da, yoksa). Zaten üyeyse `false`. Yasaklıysa 403.
pub async fn add_member(s: &AppState, organization_id: &str, project_id: &str, user_id: &str, invite_id: Option<&str>) -> ApiResult<bool> {
    if banned(&s.db, project_id, user_id).await? {
        return Err(forbidden("PROJECT_BANNED", "Bu projeden yasaklandın.").safe());
    }
    if crate::perm::is_member(&s.db, project_id, user_id).await? {
        return Ok(false);
    }
    let at = now();
    let mut tx = s.db.begin().await?;
    sqlx::query(
        r#"insert into "OrganizationMember" ("organizationId", "userId", role, permissions, "deniedPermissions", "joinedAt") values ($1, $2, 'MEMBER', '{}', '{}', $3)
           on conflict do nothing"#,
    )
    .bind(organization_id)
    .bind(user_id)
    .bind(at)
    .execute(&mut *tx)
    .await?;
    if let Some(invite_id) = invite_id {
        // Yarış: aynı anda son kullanım hakkı için iki kişi gelirse biri düşer.
        let n = sqlx::query(r#"update "ProjectInvite" set "useCount" = "useCount" + 1 where id = $1 and not revoked and ("maxUses" is null or "useCount" < "maxUses")"#)
            .bind(invite_id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if n == 0 {
            return Err(ApiError::bad_request("INVITE_FULL", "Bu davetin kullanım sınırı dolmuş."));
        }
    }
    sqlx::query(r#"insert into "ProjectMember" ("projectId", "userId", role, "joinedAt", "inviteId") values ($1, $2, 'MEMBER', $3, $4) on conflict do nothing"#)
        .bind(project_id)
        .bind(user_id)
        .bind(at)
        .bind(invite_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    if let Some(member) = member_row(&s.db, project_id, user_id).await? {
        let payload = json!({ "organizationId": organization_id, "projectId": project_id, "member": member });
        crate::realtime::emit_project(s, organization_id, project_id, "project:member:joined", &payload).await;
        crate::realtime::emit_project(s, organization_id, project_id, "organization:member:joined", &payload).await;
    }
    Ok(true)
}

/// Projeden çıkarma (atılma, yasak, ayrılma, erişimin kapatılması): üyelik, proje rolleri, proje
/// grupları ve projedeki sohbet grubu üyelikleri silinir; kişinin o projedeki aramaları düşer.
pub async fn remove_from_project(s: &AppState, organization_id: &str, project_id: &str, user_id: &str, banned: bool) -> ApiResult<()> {
    let removed_groups: Vec<String> = sqlx::query_scalar(
        r#"select gm."groupId" from "ChatGroupMember" gm where gm."userId" = $1 and gm."groupId" in (select g.id from "ChatGroup" g where g."projectId" = $2)"#,
    )
    .bind(user_id)
    .bind(project_id)
    .fetch_all(&s.db)
    .await?;
    let mut tx = s.db.begin().await?;
    sqlx::query(r#"delete from "ChatGroupMember" where "userId" = $1 and "groupId" in (select id from "ChatGroup" where "projectId" = $2)"#)
        .bind(user_id)
        .bind(project_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(r#"delete from "RoleMember" where "userId" = $1 and "roleId" in (select id from "Role" where "projectId" = $2)"#)
        .bind(user_id)
        .bind(project_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(r#"delete from "UserGroupMember" where "userId" = $1 and "groupId" in (select id from "UserGroup" where "projectId" = $2)"#)
        .bind(user_id)
        .bind(project_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(r#"delete from "ProjectMember" where "projectId" = $1 and "userId" = $2"#).bind(project_id).bind(user_id).execute(&mut *tx).await?;
    tx.commit().await?;
    for group_id in removed_groups {
        crate::calls::drop_from_group_call(s, &group_id, user_id).await;
        crate::realtime::emit_users(s, &[user_id.to_string()], "group:left", &json!({ "groupId": group_id })).await;
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
            crate::realtime::emit_users(s, &ids, "group:updated", &json!({ "group": group.0 })).await;
        }
    }
    let me = [user_id.to_string()];
    crate::realtime::emit_users(s, &me, "project:removed", &json!({ "organizationId": organization_id, "projectId": project_id, "banned": banned })).await;
    crate::realtime::emit_users(s, &me, "project:permissions", &json!({ "organizationId": organization_id, "projectId": project_id, "visible": false })).await;
    let payload = json!({ "organizationId": organization_id, "projectId": project_id, "userId": user_id, "banned": banned });
    crate::realtime::emit_project(s, organization_id, project_id, "project:member:removed", &payload).await;
    crate::realtime::emit_project(s, organization_id, project_id, "organization:member:removed", &payload).await;
    crate::realtime::emit_project(s, organization_id, project_id, "project:members", &json!({ "projectId": project_id })).await;
    Ok(())
}

async fn organization_json(db: &sqlx::PgPool, organization_id: &str, user_id: &str) -> ApiResult<Value> {
    let row: Option<J> = sqlx::query_scalar(sql(format!(
        r#"select json_build_object('id', o.id, 'name', o.name, 'slug', o.slug, 'createdAt', {}, 'updatedAt', {}, 'role', m.role)
           from "Organization" o left join "OrganizationMember" m on m."organizationId" = o.id and m."userId" = $2 where o.id = $1"#,
        crate::iso!(r#"o."createdAt""#),
        crate::iso!(r#"o."updatedAt""#)
    )))
    .bind(organization_id)
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    row.map(|j| j.0).ok_or_else(|| ApiError::internal("Organization bulunamadı"))
}

async fn project_json(db: &sqlx::PgPool, project_id: &str) -> ApiResult<Value> {
    let row: Option<J> = sqlx::query_scalar(sql(format!(r#"select {} from "Project" p where p.id = $1"#, crate::projects::PROJECT_JSON)))
        .bind(project_id)
        .fetch_optional(db)
        .await?;
    row.map(|j| j.0).ok_or_else(|| ApiError::internal("Project bulunamadı"))
}

/// Kodla katılma: `{ project, organization, joined, projectId, organizationId }`.
pub async fn join_with_code(s: &AppState, user_id: &str, code: &str) -> ApiResult<Value> {
    let invite = usable_invite(&s.db, code).await?;
    let joined = add_member(s, &invite.organization_id, &invite.project_id, user_id, Some(&invite.id)).await?;
    let project = project_json(&s.db, &invite.project_id).await?;
    if joined {
        crate::realtime::emit_users(s, &[user_id.to_string()], "project:created", &json!({ "project": project })).await;
    }
    Ok(json!({
        "project": project,
        "organization": organization_json(&s.db, &invite.organization_id, user_id).await?,
        "projectId": invite.project_id,
        "organizationId": invite.organization_id,
        "joined": joined,
    }))
}

fn code_dto(body: &serde_json::Map<String, Value>) -> ApiResult<String> {
    let mut c = Checker::new(body);
    let raw = c.str("code");
    c.field("code", &[(raw.is_some_and(|v| (6..=20).contains(&js_len(v))), None), (raw.is_some(), None)]);
    c.finish()?;
    Ok(raw.unwrap_or_default().to_string())
}

// ------------------------------------------------------------------ uçlar

/// Oturumsuz önizleme: proje adı, görseli, üye sayısı ve kodun durumu.
async fn preview(State(s): State<AppState>, Path(code): Path<String>) -> Reply {
    let row: Option<(String, J, Option<NaiveDateTime>, Option<i32>, i32, bool)> = sqlx::query_as(sql(format!(
        r#"select i.code, json_build_object('id', p.id, 'name', p.name, 'description', p.description, 'iconUrl', p."iconUrl", 'coverUrl', p."coverUrl",
             'memberCount', (select count(*) from "ProjectMember" m where m."projectId" = p.id)),
             i."expiresAt", i."maxUses", i."useCount", i.revoked
           from "ProjectInvite" i join "Project" p on p.id = i."projectId" where i.code = $1 and p."deletedAt" is null and p."organizationId" is not null"#
    )))
    .bind(normalize(&code))
    .fetch_optional(&s.db)
    .await?;
    let Some((code, project, expires, max_uses, use_count, revoked)) = row else { return Err(invite_missing()) };
    let invite = Invite { id: String::new(), project_id: String::new(), organization_id: String::new(), expires, max_uses, use_count, revoked };
    let reason = invite.problem().map(|e| e.body["code"].clone());
    ok(json!({
        "code": code,
        "project": project.0,
        "expiresAt": expires.map(crate::projects::iso_dt),
        "maxUses": max_uses,
        "useCount": use_count,
        "valid": reason.is_none(),
        "reason": reason,
    }))
}

async fn join_code(State(s): State<AppState>, me: Me, Path(code): Path<String>) -> Reply {
    created(join_with_code(&s, &me.id, &code).await?)
}

async fn join_body(State(s): State<AppState>, me: Me, Body(body): Body) -> Reply {
    let code = code_dto(&body)?;
    created(join_with_code(&s, &me.id, &code).await?)
}

/// Üye olduğum projeler (her kuruluştan).
async fn my_projects(State(s): State<AppState>, me: Me) -> Reply {
    let rows = many_json(
        sqlx::query_scalar(sql(format!(
            r#"select ({full}::jsonb || jsonb_build_object('role', m.role, 'owner', m.role = 'OWNER', 'primaryOwner', p."ownerId" = m."userId"))::json
               from "Project" p join "ProjectMember" m on m."projectId" = p.id and m."userId" = $1
               where p."deletedAt" is null and p."organizationId" is not null order by p."updatedAt" desc"#,
            full = crate::projects::PROJECT_FULL_JSON
        )))
        .bind(&me.id),
        &s.db,
    )
    .await?;
    ok(Value::Array(rows))
}

/// Herkes proje açar. Kuruluş verilmezse kişinin ilk kuruluşu, o da yoksa kurulumun ilk kuruluşu.
async fn create_any(State(s): State<AppState>, me: Me, body: Body) -> Reply {
    // Proje açmak sunucu yetkisidir (`project.create`).
    crate::server_admin::require_project_create(&s.db, &me.id).await?;
    let wanted = body.str("organizationId").filter(|o| !o.is_empty()).map(str::to_string);
    let org = match wanted {
        Some(org) => org,
        None => {
            let mine: Option<String> = sqlx::query_scalar(r#"select "organizationId" from "OrganizationMember" where "userId" = $1 order by "joinedAt" asc limit 1"#)
                .bind(&me.id)
                .fetch_optional(&s.db)
                .await?;
            match mine {
                Some(org) => org,
                None => {
                    let first: Option<String> = sqlx::query_scalar(r#"select id from "Organization" order by "createdAt" asc limit 1"#).fetch_optional(&s.db).await?;
                    let first = first.ok_or_else(|| ApiError::conflict("NO_ORGANIZATION", "Bu sunucuda henüz kurulum yapılmamış."))?;
                    sqlx::query(
                        r#"insert into "OrganizationMember" ("organizationId", "userId", role, permissions, "deniedPermissions", "joinedAt") values ($1, $2, 'MEMBER', '{}', '{}', $3)
                           on conflict do nothing"#,
                    )
                    .bind(&first)
                    .bind(&me.id)
                    .bind(now())
                    .execute(&s.db)
                    .await?;
                    first
                }
            }
        }
    };
    crate::projects::create_in(&s, &org, &me.id, &body).await
}

/// Projeden ayrıl: birincil sahip önce sahipliği devretmeli.
async fn leave(State(s): State<AppState>, sc: Scope) -> Reply {
    let owner: Option<String> = sqlx::query_scalar(r#"select "ownerId" from "Project" where id = $1"#).bind(&sc.project).fetch_optional(&s.db).await?;
    if owner.as_deref() == Some(sc.user.as_str()) {
        return Err(ApiError::conflict("OWNER_CANNOT_LEAVE", "Projenin sahibi ayrılamaz; önce sahipliği devret."));
    }
    remove_from_project(&s, &sc.org, &sc.project, &sc.user, false).await?;
    created(json!({ "projectId": sc.project, "left": true }))
}

/// Projenin kişi listesi (yalnız üyeler görür).
async fn members(State(s): State<AppState>, sc: Scope) -> Reply {
    let rows = many_json(
        sqlx::query_scalar(sql(format!(
            r#"select {} from "ProjectMember" m join "User" u on u.id = m."userId" join "Project" p on p.id = m."projectId"
               where m."projectId" = $1 order by m."joinedAt" asc, u.id asc"#,
            member_json()
        )))
        .bind(&sc.project),
        &s.db,
    )
    .await?;
    ok(Value::Array(rows))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/invites/{code}", get(preview))
        .route("/invites/{code}/join", post(join_code))
        .route("/projects", get(my_projects).post(create_any))
        .route("/projects/join", post(join_body))
        .route("/projects/{projectId}/leave", post(leave))
        .route("/organizations/{organizationId}/projects/{projectId}/leave", post(leave))
        .route("/projects/{projectId}/members", get(members))
        .route("/organizations/{organizationId}/projects/{projectId}/members", get(members))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kod_bicimi() {
        assert_eq!(normalize(" p2-ab12 cd34 "), "P2-AB12CD34");
    }

    #[test]
    fn davet_durumu() {
        let base = || Invite { id: "i".into(), project_id: "p".into(), organization_id: "o".into(), expires: None, max_uses: None, use_count: 0, revoked: false };
        assert!(base().problem().is_none());
        let revoked = Invite { revoked: true, ..base() };
        assert_eq!(revoked.problem().unwrap().body["code"], "INVITE_REVOKED");
        let full = Invite { max_uses: Some(2), use_count: 2, ..base() };
        assert_eq!(full.problem().unwrap().body["code"], "INVITE_FULL");
        let expired = Invite { expires: Some(now() - chrono::Duration::days(1)), ..base() };
        assert_eq!(expired.problem().unwrap().body["code"], "INVITE_EXPIRED");
        let later = Invite { expires: Some(now() + chrono::Duration::days(1)), max_uses: Some(3), use_count: 2, ..base() };
        assert!(later.problem().is_none());
    }
}
