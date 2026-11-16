//! Kuruluşlar — `server/src/organizations.ts` (OrganizationsService + OrganizationsController) karşılığı:
//! kuruluş (kurulum kabı) listesi ve kurma, proje davet koduyla katılma (eski uç), kişi listesi.

use axum::Router;
use axum::extract::{Path, State};
use axum::routing::{get, post};
use serde_json::{Value, json};

use crate::AppState;
use crate::ctx::{Body, Me, Query, Reply, created, now, ok, sql, uuid};
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

/// Kişinin hesap kabı olarak üye olduğu kuruluşlar. Kuruluş davet kodu artık yok: davetler projeye özel.
async fn list(State(s): State<AppState>, me: Me) -> Reply {
    let rows: Vec<(String, String, String, String)> = sqlx::query_as(
        r#"select o.id, o.name, o.slug, m.role::text from "OrganizationMember" m join "Organization" o on o.id = m."organizationId"
           where m."userId" = $1 order by m."joinedAt" asc"#,
    )
    .bind(&me.id)
    .fetch_all(&s.db)
    .await?;
    let out: Vec<Value> = rows.into_iter().map(|(id, name, slug, role)| json!({ "id": id, "name": name, "slug": slug, "role": role })).collect();
    ok(Value::Array(out))
}

/// Kurulumun kabı (ilk kurulumda). Kuruluş artık hiçbir erişim vermez; roller ve davetler projelerdedir.
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
    tx.commit().await?;
    created(with(organization.0, &[("role", json!("OWNER"))]))
}

/// Eski "kuruluşa katıl" ucu: artık proje davet koduyla o projeye katılır (yanıt eski biçimde kuruluş
/// nesnesi + `projectId`/`project`). Eski kuruluş kodları `INVITE_REVOKED`.
async fn join(State(s): State<AppState>, me: Me, Body(body): Body) -> Reply {
    let mut c = Checker::new(&body);
    let raw = c.str("code");
    c.field("code", &[(raw.is_some_and(|v| (6..=20).contains(&js_len(v))), None), (raw.is_some(), None)]);
    c.finish()?;
    let joined = crate::community::join_with_code(&s, &me.id, raw.unwrap_or_default()).await?;
    let mut organization = joined["organization"].clone();
    for key in ["projectId", "project", "joined"] {
        organization[key] = joined[key].clone();
    }
    created(organization)
}

/// Kişi listesi: `projectId` verilirse o projenin üyeleri; verilmezse benimle en az bir projeyi
/// paylaşan herkes (DM ve kişi seçici için). `role` kuruluş kabındaki roldür.
async fn members(State(s): State<AppState>, me: Me, Path(organization_id): Path<String>, q: Query) -> Reply {
    crate::activity::membership(&s.db, &organization_id, &me.id).await?;
    let project = q.get("projectId").filter(|p| !p.is_empty()).cloned();
    if let Some(project) = &project {
        perm::require_project_access(&s.db, &organization_id, &me.id, project).await?;
    }
    let rows = crate::ctx::many_json(
        sqlx::query_scalar(sql(format!(
            r#"select json_build_object({MEMBER_USER_FIELDS}, 'role', m.role) from "OrganizationMember" m join "User" u on u.id = m."userId"
               where m."organizationId" = $1 and (
                 ($2::text is not null and exists (select 1 from "ProjectMember" pm where pm."userId" = m."userId" and pm."projectId" = $2))
                 or ($2::text is null and (m."userId" = $3 or exists (
                   select 1 from "ProjectMember" mine join "ProjectMember" other on other."projectId" = mine."projectId" join "Project" p on p.id = mine."projectId"
                   where mine."userId" = $3 and other."userId" = m."userId" and p."deletedAt" is null and p."organizationId" = $1))))
               order by m."joinedAt" asc"#
        )))
        .bind(&organization_id)
        .bind(&project)
        .bind(&me.id),
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
