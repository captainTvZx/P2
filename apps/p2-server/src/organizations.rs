//! Kuruluşlar — `server/src/organizations.ts` (OrganizationsService + OrganizationsController) karşılığı:
//! kuruluş listesi, kurma (@everyone + kurucu rolleriyle), davet koduyla katılma, üye listesi.

use axum::Router;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use serde_json::{Value, json};

use crate::AppState;
use crate::ctx::{Body, Me, Query, Reply, created, not_found, now, ok, sql, uuid};
use crate::error::ApiError;
use crate::perm;
use crate::validate::{Checker, js_len};

/// `P2-` + 4 rastgele baytın büyük harf onaltılığı (`randomBytes(4).toString('hex').toUpperCase()`).
pub fn invite_code() -> String {
    let raw: [u8; 4] = rand::random();
    format!("P2-{}", hex::encode_upper(raw))
}

/// `select: { id, username, displayName, email, title, avatarUrl, status }` (alias `u`).
pub const MEMBER_USER_FIELDS: &str = r#"'id', u.id, 'username', u.username, 'displayName', u."displayName", 'email', u.email, 'title', u.title, 'avatarUrl', u."avatarUrl", 'status', u.status"#;

/// Organization modelinin tüm alanları (alias `o`).
fn organization_json() -> String {
    format!(
        r#"json_build_object('id', o.id, 'name', o.name, 'slug', o.slug, 'createdAt', {}, 'updatedAt', {})"#,
        crate::iso!(r#"o."createdAt""#),
        crate::iso!(r#"o."updatedAt""#)
    )
}

fn with(mut value: Value, extra: &[(&str, Value)]) -> Value {
    if let Some(map) = value.as_object_mut() {
        for (k, v) in extra {
            map.insert((*k).to_string(), v.clone());
        }
    }
    value
}

async fn list(State(s): State<AppState>, me: Me) -> Reply {
    let rows: Vec<(String, String, String, String, Option<String>)> = sqlx::query_as(
        r#"select o.id, o.name, o.slug, m.role::text,
             (select i.code from "OrganizationInvite" i where i."organizationId" = o.id order by i."createdAt" desc limit 1)
           from "OrganizationMember" m join "Organization" o on o.id = m."organizationId"
           where m."userId" = $1 order by m."joinedAt" asc"#,
    )
    .bind(&me.id)
    .fetch_all(&s.db)
    .await?;
    let mut out = Vec::new();
    for (id, name, slug, role, code) in rows {
        let mut item = json!({ "id": id, "name": name, "slug": slug, "role": role });
        // Davet kodunu yalnızca "davet kodu yönet" yetkisi olanlar görür.
        if perm::has_permission(&s.db, &id, &me.id, "member.invite").await?
            && let Some(code) = code
        {
            item["inviteCode"] = json!(code);
        }
        out.push(item);
    }
    ok(Value::Array(out))
}

async fn create(State(s): State<AppState>, me: Me, Body(body): Body) -> Reply {
    let mut c = Checker::new(&body);
    let name = c.str("name");
    c.field("name", &[(name.is_some_and(|n| (2..=60).contains(&js_len(n))), None), (name.is_some(), None)]);
    let slug = c.str("slug");
    c.field(
        "slug",
        &[
            (slug.is_some_and(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')), None),
            (slug.is_some_and(|v| (2..=40).contains(&js_len(v))), None),
            (slug.is_some(), None),
        ],
    );
    c.finish()?;
    let name = name.unwrap_or_default().trim().to_string();
    let slug = slug.unwrap_or_default().trim().to_lowercase();
    let taken: Option<i32> = sqlx::query_scalar(r#"select 1 from "Organization" where slug = $1"#).bind(&slug).fetch_optional(&s.db).await?;
    if taken.is_some() {
        return Err(ApiError::conflict("SLUG_TAKEN", "Bu kuruluş adresi zaten kullanılıyor.").field("slug"));
    }
    let code = invite_code();
    let at = now();
    let id = uuid();
    let mut tx = s.db.begin().await?;
    let organization: sqlx::types::Json<Value> = sqlx::query_scalar(sql(format!(
        r#"with o as (insert into "Organization" (id, name, slug, "createdAt", "updatedAt") values ($1, $2, $3, $4, $4) returning *) select {} from o"#,
        organization_json()
    )))
    .bind(&id)
    .bind(&name)
    .bind(&slug)
    .bind(at)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(r#"insert into "OrganizationMember" ("organizationId", "userId", role, permissions, "deniedPermissions", "joinedAt") values ($1, $2, 'OWNER', '{}', '{}', $3)"#)
        .bind(&id)
        .bind(&me.id)
        .bind(at)
        .execute(&mut *tx)
        .await?;
    sqlx::query(r#"insert into "OrganizationInvite" (id, code, "organizationId", "createdById", "useCount", "createdAt") values ($1, $2, $3, $4, 0, $5)"#)
        .bind(uuid())
        .bind(&code)
        .bind(&id)
        .bind(&me.id)
        .bind(at)
        .execute(&mut *tx)
        .await?;
    // Her kuruluş silinemeyen bir @everyone rolüyle başlar: temel yetkiler oradan gelir.
    let everyone: Vec<String> = perm::EVERYONE_DEFAULTS.iter().map(|p| p.to_string()).collect();
    let insert_role = r#"insert into "Role" (id, "organizationId", handle, name, description, color, everyone, founder, permissions, "createdById", "createdAt", "updatedAt")
                         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11)"#;
    sqlx::query(insert_role)
        .bind(uuid())
        .bind(&id)
        .bind("everyone")
        .bind("Herkes")
        .bind("Kuruluştaki herkes bu rolün yetkilerini alır.")
        .bind("#79818c")
        .bind(true)
        .bind(false)
        .bind(&everyone)
        .bind(&me.id)
        .bind(at)
        .execute(&mut *tx)
        .await?;
    // İkinci temel rol: kurucu. Üyeliği kuruluş sahipliğini izler.
    let founder = uuid();
    sqlx::query(insert_role)
        .bind(&founder)
        .bind(&id)
        .bind("kurucu")
        .bind("Kurucu")
        .bind("Kuruluşu kuran kişi: tüm yetkiler her zaman açıktır.")
        .bind("#faa61a")
        .bind(false)
        .bind(true)
        .bind(perm::all())
        .bind(&me.id)
        .bind(at)
        .execute(&mut *tx)
        .await?;
    sqlx::query(r#"insert into "RoleMember" ("roleId", "userId", "addedAt") values ($1, $2, $3)"#)
        .bind(&founder)
        .bind(&me.id)
        .bind(at)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    created(with(organization.0, &[("role", json!("OWNER")), ("inviteCode", json!(code))]))
}

async fn join(State(s): State<AppState>, me: Me, Body(body): Body) -> Reply {
    let mut c = Checker::new(&body);
    let raw = c.str("code");
    c.field("code", &[(raw.is_some_and(|v| (6..=20).contains(&js_len(v))), None), (raw.is_some(), None)]);
    c.finish()?;
    let code: String = raw.unwrap_or_default().trim().to_uppercase().chars().filter(|c| !c.is_whitespace()).collect();
    let invite: Option<(String, String, Option<chrono::NaiveDateTime>, Option<i32>, i32, sqlx::types::Json<Value>)> = sqlx::query_as(sql(format!(
        r#"select i.id, i."organizationId", i."expiresAt", i."maxUses", i."useCount", {} from "OrganizationInvite" i join "Organization" o on o.id = i."organizationId" where i.code = $1"#,
        organization_json()
    )))
    .bind(&code)
    .fetch_optional(&s.db)
    .await?;
    let Some((invite_id, organization_id, expires, max_uses, use_count, organization)) = invite else {
        return Err(not_found("INVITE_NOT_FOUND", "Davet kodu bulunamadı."));
    };
    if expires.is_some_and(|e| e < now()) {
        return Err(ApiError::bad_request("INVITE_EXPIRED", "Bu davetin süresi dolmuş."));
    }
    if max_uses.is_some_and(|m| use_count >= m) {
        return Err(ApiError::bad_request("INVITE_FULL", "Bu davetin kullanım sınırı dolmuş."));
    }
    let existing: Option<String> = sqlx::query_scalar(r#"select role::text from "OrganizationMember" where "organizationId" = $1 and "userId" = $2"#)
        .bind(&organization_id)
        .bind(&me.id)
        .fetch_optional(&s.db)
        .await?;
    if let Some(role) = existing {
        return created(with(organization.0, &[("role", json!(role))]));
    }
    let mut tx = s.db.begin().await?;
    sqlx::query(r#"insert into "OrganizationMember" ("organizationId", "userId", role, permissions, "deniedPermissions", "joinedAt") values ($1, $2, 'MEMBER', '{}', '{}', $3)"#)
        .bind(&organization_id)
        .bind(&me.id)
        .bind(now())
        .execute(&mut *tx)
        .await?;
    sqlx::query(r#"update "OrganizationInvite" set "useCount" = "useCount" + 1 where id = $1"#)
        .bind(&invite_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    let member: Option<sqlx::types::Json<Value>> =
        sqlx::query_scalar(sql(format!(r#"select json_build_object({MEMBER_USER_FIELDS}, 'role', 'MEMBER') from "User" u where u.id = $1"#)))
            .bind(&me.id)
            .fetch_optional(&s.db)
            .await?;
    if let Some(member) = member {
        crate::realtime::emit_org(&s, &organization_id, "organization:member:joined", &json!({ "member": member.0 })).await;
    }
    created(with(organization.0, &[("role", json!("MEMBER"))]))
}

async fn members(State(s): State<AppState>, me: Me, Path(organization_id): Path<String>, q: Query) -> Reply {
    crate::activity::membership(&s.db, &organization_id, &me.id).await?;
    let project = q.get("projectId").filter(|p| !p.is_empty()).cloned();
    if let Some(project) = &project {
        perm::require_project_access(&s.db, &organization_id, &me.id, project).await?;
    }
    let rows = crate::ctx::many_json(
        sqlx::query_scalar(sql(format!(
            r#"select json_build_object({MEMBER_USER_FIELDS}, 'role', m.role) from "OrganizationMember" m join "User" u on u.id = m."userId"
               where m."organizationId" = $1 and ($2::text is null or m.role = 'OWNER'
                 or exists (select 1 from "ProjectMember" pm where pm."userId" = m."userId" and pm."projectId" = $2)
                 or exists (select 1 from "Project" p where p."ownerId" = m."userId" and p.id = $2 and p."organizationId" = $1))
               order by m."joinedAt" asc"#
        )))
        .bind(&organization_id)
        .bind(&project),
        &s.db,
    )
    .await?;
    ok(Value::Array(rows))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/organizations", get(list).post(create))
        .route("/organizations/join", post(join))
        .route("/organizations/{organizationId}/members", get(members))
}
