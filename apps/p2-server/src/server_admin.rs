//! Sunucu (kurulum) yönetimi: sunucu çapında yetkiler, sunucu yasağı, kurulum ayarları ve
//! "Sunucu yönetimi" panelinin uçları (`/server/...`).
//!
//! - Kurulum sahibi = ilk yönetici (`User.role = ADMIN`) ya da bir kuruluşun `OWNER`'ı. Bütün sunucu
//!   yetkilerine kendiliğinden sahiptir.
//! - Diğerleri yetkiyi yalnız sunucu yöneticisi verirse alır (`ServerPermission`). `server.admin` bütün
//!   yetkileri kapsar ve başkasına yetki vermeye izin verir.
//! - `project.create`: proje (topluluk) açmak. Kurulum ayarı `projectCreation = everyone` ise herkes açar.
//! - Sunucu yasağı (`ServerBan`): giriş reddedilir, oturumlar kapanır, erişim belirteci de geçersizleşir
//!   (bellekteki yasaklı listesi `guard` içinde denetlenir).

use std::collections::HashSet;
use std::sync::{OnceLock, RwLock};

use axum::Router;
use axum::extract::{Path, State};
use axum::routing::{delete, get, post, put};
use serde_json::{Value, json};

use crate::AppState;
use crate::ctx::{Body, Me, Query, Reply, created, forbidden, many_json, not_found, now, ok, sql};
use crate::error::{ApiError, ApiResult};

/// Sunucu yetki kataloğu: (kimlik, ad, açıklama).
pub const CATALOG: &[(&str, &str, &str)] = &[
    ("project.create", "Proje oluştur", "Yeni proje (topluluk) açabilir."),
    ("server.view_all", "Her şeyi gör", "Sunucudaki bütün projeleri (üyesi olmadıkları dahil) görür."),
    ("server.manage_users", "Kişileri yönet", "Oturumları kapatır, iki adımlı doğrulamayı sıfırlar."),
    ("server.ban", "Sunucudan yasakla", "Kişiyi sunucudan yasaklar ve yasağı kaldırır."),
    ("server.manage_projects", "Projeleri yönet", "Her projeye girer, sahipliği devreder, siler, geri yükler, kalıcı siler."),
    ("server.settings", "Sunucu ayarları", "Sunucu adı, kayıt kuralı ve proje açma kuralı."),
    ("server.admin", "Sunucu yöneticisi", "Bütün sunucu yetkileri; başkalarına yetki verir."),
];

pub fn all() -> Vec<String> {
    CATALOG.iter().map(|c| c.0.to_string()).collect()
}

fn catalog_json() -> Value {
    Value::Array(CATALOG.iter().map(|(id, label, hint)| json!({ "id": id, "label": label, "description": hint })).collect())
}

// ------------------------------------------------------------------ yasaklı listesi (bellek)

fn banned_set() -> &'static RwLock<HashSet<String>> {
    static SET: OnceLock<RwLock<HashSet<String>>> = OnceLock::new();
    SET.get_or_init(Default::default)
}

/// Erişim belirteci denetimi (`auth::guard`): yasaklı kişinin belirteci de geçersiz.
pub fn is_banned(user_id: &str) -> bool {
    banned_set().read().map(|s| s.contains(user_id)).unwrap_or(false)
}

/// Açılışta yasaklı listesini yükler.
pub fn start(state: &AppState) {
    let db = state.db.clone();
    tokio::spawn(async move {
        if let Ok(ids) = sqlx::query_scalar::<_, String>(r#"select "userId" from "ServerBan""#).fetch_all(&db).await {
            if let Ok(mut set) = banned_set().write() {
                set.extend(ids);
            }
        }
    });
}

/// Yasak gerekçesi (yasaklı değilse `None`).
pub async fn ban_of(db: &sqlx::PgPool, user_id: &str) -> ApiResult<Option<Option<String>>> {
    Ok(sqlx::query_scalar::<_, Option<String>>(r#"select reason from "ServerBan" where "userId" = $1"#).bind(user_id).fetch_optional(db).await?)
}

pub fn banned_error(reason: Option<&str>) -> ApiError {
    let message = match reason.map(str::trim).filter(|r| !r.is_empty()) {
        Some(r) => format!("Bu sunucudan yasaklandın. Gerekçe: {r}"),
        None => "Bu sunucudan yasaklandın.".to_string(),
    };
    let mut e = forbidden("SERVER_BANNED", &message).safe();
    if let Some(obj) = e.body.as_object_mut() {
        obj.insert("reason".into(), json!(reason));
    }
    e
}

// ------------------------------------------------------------------ ayarlar

pub async fn setting(db: &sqlx::PgPool, key: &str, fallback: &str) -> ApiResult<String> {
    let v: Option<String> = sqlx::query_scalar(r#"select value from "InstanceSetting" where key = $1"#).bind(key).fetch_optional(db).await?;
    Ok(v.unwrap_or_else(|| fallback.to_string()))
}

async fn set_setting(db: &sqlx::PgPool, key: &str, value: &str) -> ApiResult<()> {
    sqlx::query(r#"insert into "InstanceSetting" (key, value, "updatedAt") values ($1, $2, $3) on conflict (key) do update set value = excluded.value, "updatedAt" = excluded."updatedAt""#)
        .bind(key)
        .bind(value)
        .bind(now())
        .execute(db)
        .await?;
    Ok(())
}

/// `registrationPolicy`: `open` | `invite` (kayıtta proje davet kodu zorunlu).
pub async fn invite_only(db: &sqlx::PgPool) -> ApiResult<bool> {
    Ok(setting(db, "registrationPolicy", "open").await? == "invite")
}

// ------------------------------------------------------------------ yetki hesabı

/// Kurulum sahibi: ilk yönetici ya da bir kuruluşun sahibi.
pub async fn is_owner(db: &sqlx::PgPool, user_id: &str) -> ApiResult<bool> {
    let owner: bool = sqlx::query_scalar(
        r#"select exists (select 1 from "User" where id = $1 and role = 'ADMIN')
               or exists (select 1 from "OrganizationMember" where "userId" = $1 and role = 'OWNER')"#,
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;
    Ok(owner)
}

async fn granted(db: &sqlx::PgPool, user_id: &str) -> ApiResult<Vec<String>> {
    Ok(sqlx::query_scalar(r#"select permission from "ServerPermission" where "userId" = $1"#).bind(user_id).fetch_all(db).await?)
}

/// Kişinin etkin sunucu yetkileri (katalog sırasıyla). `GET /auth/me` → `serverPermissions`.
pub async fn permissions_of(db: &sqlx::PgPool, user_id: &str) -> ApiResult<Vec<String>> {
    let mut set: HashSet<String> = if is_owner(db, user_id).await? {
        all().into_iter().collect()
    } else {
        let g = granted(db, user_id).await?;
        if g.iter().any(|p| p == "server.admin") { all().into_iter().collect() } else { g.into_iter().collect() }
    };
    if !set.contains("project.create") && setting(db, "projectCreation", "permission").await? == "everyone" {
        set.insert("project.create".into());
    }
    Ok(CATALOG.iter().map(|c| c.0.to_string()).filter(|p| set.contains(p)).collect())
}

pub async fn has(db: &sqlx::PgPool, user_id: &str, permission: &str) -> ApiResult<bool> {
    Ok(permissions_of(db, user_id).await?.iter().any(|p| p == permission))
}

/// Proje açmak: `POST /projects` ve `POST /organizations/:org/projects`.
pub async fn require_project_create(db: &sqlx::PgPool, user_id: &str) -> ApiResult<()> {
    if has(db, user_id, "project.create").await? {
        return Ok(());
    }
    Err(forbidden("PROJECT_CREATE_FORBIDDEN", "Proje oluşturma yetkin yok — sunucu yöneticisinden iste.").safe())
}

async fn require(db: &sqlx::PgPool, user_id: &str, permission: &str) -> ApiResult<Vec<String>> {
    let perms = permissions_of(db, user_id).await?;
    if perms.iter().any(|p| p == permission) {
        return Ok(perms);
    }
    let label = CATALOG.iter().find(|c| c.0 == permission).map(|c| c.1).unwrap_or(permission);
    let mut e = forbidden("SERVER_FORBIDDEN", &format!("Bu işlem için \"{label}\" sunucu yetkisi gerekli.")).safe();
    if let Some(obj) = e.body.as_object_mut() {
        obj.insert("permission".into(), json!(permission));
    }
    Err(e)
}

/// Panele erişim: kurulum sahibi ya da herhangi bir `server.*` yetkisi.
async fn require_panel(db: &sqlx::PgPool, user_id: &str) -> ApiResult<Vec<String>> {
    let perms = permissions_of(db, user_id).await?;
    if perms.iter().any(|p| p.starts_with("server.")) {
        return Ok(perms);
    }
    Err(forbidden("SERVER_FORBIDDEN", "Sunucu yönetimine erişimin yok.").safe())
}

/// Kurulumun (kuruluş kabı) kimliği ve adı: ilk açılan kuruluş.
async fn instance(db: &sqlx::PgPool) -> ApiResult<Option<(String, String)>> {
    Ok(sqlx::query_as(r#"select id, name from "Organization" order by "createdAt" asc limit 1"#).fetch_optional(db).await?)
}

async fn user_exists(db: &sqlx::PgPool, user_id: &str) -> ApiResult<()> {
    let found: Option<i32> = sqlx::query_scalar(r#"select 1 from "User" where id = $1"#).bind(user_id).fetch_optional(db).await?;
    found.map(|_| ()).ok_or_else(|| not_found("USER_NOT_FOUND", "Kişi bulunamadı."))
}

/// Kurulum sahibine ve kendine dokunulamaz (yasak, yetki).
async fn guard_target(db: &sqlx::PgPool, me: &str, target: &str, action: &str) -> ApiResult<()> {
    user_exists(db, target).await?;
    if me == target {
        return Err(ApiError::conflict("SELF_TARGET", &format!("Kendini {action}.")));
    }
    if is_owner(db, target).await? {
        return Err(forbidden("OWNER_PROTECTED", &format!("Sunucu sahibini {action}.")));
    }
    Ok(())
}

async fn notify_permissions(s: &AppState, user_id: &str) {
    if let Ok(perms) = permissions_of(&s.db, user_id).await {
        crate::realtime::emit_users(s, &[user_id.to_string()], "server:permissions", &json!({ "serverPermissions": perms })).await;
    }
}

// ------------------------------------------------------------------ uçlar: genel

async fn me_info(State(s): State<AppState>, me: Me) -> Reply {
    let perms = permissions_of(&s.db, &me.id).await?;
    ok(json!({ "owner": is_owner(&s.db, &me.id).await?, "serverPermissions": perms, "panel": perms.iter().any(|p| p.starts_with("server.")), "catalog": catalog_json() }))
}

async fn overview(State(s): State<AppState>, me: Me) -> Reply {
    require_panel(&s.db, &me.id).await?;
    let counts: (i64, i64, i64, i64, i64, i64) = sqlx::query_as(
        r#"select (select count(*) from "User"), (select count(*) from "ServerBan"),
                  (select count(*) from "Project" where "deletedAt" is null), (select count(*) from "Project" where "deletedAt" is not null),
                  (select count(*) from "Session" where "expiresAt" > now() at time zone 'utc'), (select count(*) from "ProjectChannel")"#,
    )
    .fetch_one(&s.db)
    .await?;
    let online = s.presence.lock().unwrap().connected().len();
    let inst = instance(&s.db).await?;
    ok(json!({
        "name": inst.as_ref().map(|i| i.1.clone()), "organizationId": inst.map(|i| i.0),
        "version": s.cfg.version, "users": counts.0, "banned": counts.1, "projects": counts.2, "deletedProjects": counts.3,
        "sessions": counts.4, "channels": counts.5, "online": online,
        "registrationPolicy": setting(&s.db, "registrationPolicy", "open").await?,
        "projectCreation": setting(&s.db, "projectCreation", "permission").await?,
    }))
}

// ------------------------------------------------------------------ uçlar: kişiler

async fn users(State(s): State<AppState>, me: Me, q: Query) -> Reply {
    require_panel(&s.db, &me.id).await?;
    let search = q.get("q").map(|v| v.trim().to_lowercase()).unwrap_or_default();
    let pattern = format!("%{}%", search.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_"));
    let mut rows = many_json(
        sqlx::query_scalar(sql(format!(
            r#"select json_build_object('id', u.id, 'username', u.username, 'displayName', u."displayName", 'email', u.email, 'avatarUrl', u."avatarUrl",
                 'status', u.status::text, 'twoFactorEnabled', u."twoFactorEnabled", 'createdAt', {created}, 'lastSeenAt', {seen},
                 'admin', u.role = 'ADMIN' or exists (select 1 from "OrganizationMember" om where om."userId" = u.id and om.role = 'OWNER'),
                 'permissions', coalesce((select json_agg(sp.permission order by sp.permission) from "ServerPermission" sp where sp."userId" = u.id), '[]'::json),
                 'sessions', (select count(*) from "Session" se where se."userId" = u.id and se."expiresAt" > now() at time zone 'utc'),
                 'ban', (select json_build_object('reason', b.reason, 'createdAt', {banned}, 'bannedBy', (select bu."displayName" from "User" bu where bu.id = b."bannedById"))
                         from "ServerBan" b where b."userId" = u.id),
                 'projects', coalesce((select json_agg(json_build_object('id', p.id, 'name', p.name, 'role', m.role::text, 'primaryOwner', p."ownerId" = u.id) order by p.name)
                         from "ProjectMember" m join "Project" p on p.id = m."projectId" where m."userId" = u.id and p."deletedAt" is null), '[]'::json))
               from "User" u
               where $1 = '' or lower(u.username) like $2 or lower(u."displayName") like $2 or lower(u.email) like $2
               order by u."createdAt" asc limit 500"#,
            created = crate::iso!(r#"u."createdAt""#),
            seen = crate::iso!(r#"u."lastClientSeenAt""#),
            banned = crate::iso!(r#"b."createdAt""#),
        )))
        .bind(&search)
        .bind(&pattern),
        &s.db,
    )
    .await?;
    let presence = s.presence.lock().unwrap();
    for row in &mut rows {
        let online = row["id"].as_str().is_some_and(|id| presence.is_online(id));
        row["online"] = json!(online);
    }
    drop(presence);
    ok(Value::Array(rows))
}

async fn revoke_sessions(State(s): State<AppState>, me: Me, Path(user_id): Path<String>) -> Reply {
    require(&s.db, &me.id, "server.manage_users").await?;
    user_exists(&s.db, &user_id).await?;
    let closed = sqlx::query(r#"delete from "Session" where "userId" = $1"#).bind(&user_id).execute(&s.db).await?.rows_affected();
    created(json!({ "userId": user_id, "closed": closed }))
}

async fn reset_two_factor(State(s): State<AppState>, me: Me, Path(user_id): Path<String>) -> Reply {
    require(&s.db, &me.id, "server.manage_users").await?;
    user_exists(&s.db, &user_id).await?;
    sqlx::query(r#"update "User" set "twoFactorEnabled" = false, "twoFactorSecret" = null, "updatedAt" = $2 where id = $1"#).bind(&user_id).bind(now()).execute(&s.db).await?;
    created(json!({ "userId": user_id, "twoFactorEnabled": false }))
}

async fn ban(State(s): State<AppState>, me: Me, Path(user_id): Path<String>, body: Body) -> Reply {
    require(&s.db, &me.id, "server.ban").await?;
    guard_target(&s.db, &me.id, &user_id, "yasaklayamazsın").await?;
    let reason = body.str("reason").map(str::trim).filter(|r| !r.is_empty()).map(|r| r.chars().take(300).collect::<String>());
    let mut tx = s.db.begin().await?;
    sqlx::query(r#"insert into "ServerBan" ("userId", "bannedById", reason, "createdAt") values ($1, $2, $3, $4) on conflict ("userId") do update set reason = excluded.reason, "bannedById" = excluded."bannedById""#)
        .bind(&user_id)
        .bind(&me.id)
        .bind(&reason)
        .bind(now())
        .execute(&mut *tx)
        .await?;
    let closed = sqlx::query(r#"delete from "Session" where "userId" = $1"#).bind(&user_id).execute(&mut *tx).await?.rows_affected();
    tx.commit().await?;
    if let Ok(mut set) = banned_set().write() {
        set.insert(user_id.clone());
    }
    crate::realtime::emit_users(&s, &[user_id.clone()], "server:banned", &json!({ "reason": reason })).await;
    if let Some(ns) = s.io.get().and_then(|io| io.of("/stream")) {
        let _ = ns.within(format!("user:{user_id}")).disconnect().await;
    }
    ok(json!({ "userId": user_id, "banned": true, "reason": reason, "closed": closed }))
}

async fn unban(State(s): State<AppState>, me: Me, Path(user_id): Path<String>) -> Reply {
    require(&s.db, &me.id, "server.ban").await?;
    user_exists(&s.db, &user_id).await?;
    sqlx::query(r#"delete from "ServerBan" where "userId" = $1"#).bind(&user_id).execute(&s.db).await?;
    if let Ok(mut set) = banned_set().write() {
        set.remove(&user_id);
    }
    ok(json!({ "userId": user_id, "banned": false }))
}

/// Kişinin sunucu yetkilerini değiştirir (`{ permissions: [...] }` tam liste). Yalnız sunucu yöneticisi.
async fn set_permissions(State(s): State<AppState>, me: Me, Path(user_id): Path<String>, body: Body) -> Reply {
    require(&s.db, &me.id, "server.admin").await?;
    guard_target(&s.db, &me.id, &user_id, "değiştiremezsin").await?;
    let Some(list) = body.get("permissions").and_then(Value::as_array) else {
        return Err(ApiError::bad_request("VALIDATION_FAILED", "permissions dizi olmalı.").field("permissions"));
    };
    let known = all();
    let wanted: Vec<String> = list.iter().filter_map(Value::as_str).filter(|p| known.iter().any(|k| k == p)).map(str::to_string).collect::<HashSet<_>>().into_iter().collect();
    let mut tx = s.db.begin().await?;
    sqlx::query(r#"delete from "ServerPermission" where "userId" = $1 and not (permission = any($2))"#).bind(&user_id).bind(&wanted).execute(&mut *tx).await?;
    for p in &wanted {
        sqlx::query(r#"insert into "ServerPermission" ("userId", permission, "grantedById", "createdAt") values ($1, $2, $3, $4) on conflict do nothing"#)
            .bind(&user_id)
            .bind(p)
            .bind(&me.id)
            .bind(now())
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    notify_permissions(&s, &user_id).await;
    let mut granted_now = granted(&s.db, &user_id).await?;
    granted_now.sort();
    ok(json!({ "userId": user_id, "permissions": granted_now, "serverPermissions": permissions_of(&s.db, &user_id).await? }))
}

// ------------------------------------------------------------------ uçlar: projeler

async fn projects(State(s): State<AppState>, me: Me) -> Reply {
    require(&s.db, &me.id, "server.view_all").await?;
    let rows = many_json(
        sqlx::query_scalar(sql(format!(
            r#"select json_build_object('id', p.id, 'name', p.name, 'slug', p.slug, 'description', p.description, 'iconUrl', p."iconUrl", 'organizationId', p."organizationId",
                 'visibility', coalesce(to_jsonb(p) ->> 'visibility', 'PRIVATE'),
                 'createdAt', {created}, 'deletedAt', {deleted}, 'purgeAt', {purge},
                 'owner', (select json_build_object('id', u.id, 'username', u.username, 'displayName', u."displayName") from "User" u where u.id = p."ownerId"),
                 'members', (select count(*) from "ProjectMember" m where m."projectId" = p.id),
                 'member', exists (select 1 from "ProjectMember" m where m."projectId" = p.id and m."userId" = $1))
               from "Project" p where p."organizationId" is not null
               order by p."deletedAt" is not null, p."createdAt" desc"#,
            created = crate::iso!(r#"p."createdAt""#),
            deleted = crate::iso!(r#"p."deletedAt""#),
            purge = crate::iso!(r#"p."purgeAt""#),
        )))
        .bind(&me.id),
        &s.db,
    )
    .await?;
    ok(Value::Array(rows))
}

/// (kuruluş, birincil sahip, silinme zamanı, ad, kod)
type ProjectRow = (Option<String>, String, Option<chrono::NaiveDateTime>, String, String);

async fn project_row(db: &sqlx::PgPool, project_id: &str) -> ApiResult<ProjectRow> {
    let row: Option<ProjectRow> = sqlx::query_as(r#"select "organizationId", "ownerId", "deletedAt", name, slug from "Project" where id = $1"#).bind(project_id).fetch_optional(db).await?;
    row.ok_or_else(|| not_found("PROJECT_NOT_FOUND", "Proje bulunamadı."))
}

async fn member_project_json(db: &sqlx::PgPool, project_id: &str, user_id: &str) -> ApiResult<Value> {
    let row: Option<sqlx::types::Json<Value>> = sqlx::query_scalar(sql(format!(
        r#"select ({full}::jsonb || jsonb_build_object('role', m.role, 'owner', m.role = 'OWNER', 'primaryOwner', p."ownerId" = m."userId"))::json
           from "Project" p join "ProjectMember" m on m."projectId" = p.id and m."userId" = $2 where p.id = $1"#,
        full = crate::projects::PROJECT_FULL_JSON
    )))
    .bind(project_id)
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    row.map(|j| j.0).ok_or_else(|| not_found("PROJECT_NOT_FOUND", "Proje bulunamadı."))
}

/// Sunucu yöneticisi projeye girer: `OWNER` üye olarak eklenir (yasak denetimi atlanmaz değil — yönetici yasağı kaldırabilir).
async fn enter(State(s): State<AppState>, me: Me, Path(project_id): Path<String>) -> Reply {
    require(&s.db, &me.id, "server.manage_projects").await?;
    let (org, _, deleted, _, _) = project_row(&s.db, &project_id).await?;
    let org = org.ok_or_else(|| not_found("PROJECT_NOT_FOUND", "Proje bulunamadı."))?;
    if deleted.is_some() {
        return Err(ApiError::conflict("PROJECT_DELETED", "Proje geri dönüşümde; önce geri yükle."));
    }
    sqlx::query(r#"delete from "ProjectBan" where "projectId" = $1 and "userId" = $2"#).bind(&project_id).bind(&me.id).execute(&s.db).await?;
    let joined = crate::community::add_member(&s, &org, &project_id, &me.id, None).await?;
    sqlx::query(r#"update "ProjectMember" set role = 'OWNER' where "projectId" = $1 and "userId" = $2"#).bind(&project_id).bind(&me.id).execute(&s.db).await?;
    let project = member_project_json(&s.db, &project_id, &me.id).await?;
    let organization: Option<sqlx::types::Json<Value>> = sqlx::query_scalar(r#"select json_build_object('id', id, 'name', name, 'slug', slug, 'role', coalesce((select role::text from "OrganizationMember" where "organizationId" = o.id and "userId" = $2), 'MEMBER')) from "Organization" o where id = $1"#)
        .bind(&org)
        .bind(&me.id)
        .fetch_optional(&s.db)
        .await?;
    created(json!({ "project": project, "organization": organization.map(|o| o.0), "organizationId": org, "joined": joined }))
}

async fn transfer(State(s): State<AppState>, me: Me, Path(project_id): Path<String>, body: Body) -> Reply {
    require(&s.db, &me.id, "server.manage_projects").await?;
    let target = body.str("userId").unwrap_or_default().to_string();
    user_exists(&s.db, &target).await?;
    let (org, old_owner, _, _, _) = project_row(&s.db, &project_id).await?;
    let org = org.ok_or_else(|| not_found("PROJECT_NOT_FOUND", "Proje bulunamadı."))?;
    if old_owner == target {
        return Err(ApiError::conflict("SELF_OWNER", "Bu kişi zaten projenin sahibi."));
    }
    sqlx::query(r#"delete from "ProjectBan" where "projectId" = $1 and "userId" = $2"#).bind(&project_id).bind(&target).execute(&s.db).await?;
    crate::community::add_member(&s, &org, &project_id, &target, None).await?;
    let founder: Option<String> = sqlx::query_scalar(r#"select id from "Role" where "projectId" = $1 and founder = true limit 1"#).bind(&project_id).fetch_optional(&s.db).await?;
    let mut tx = s.db.begin().await?;
    let set_role = r#"update "ProjectMember" set role = $3::"ProjectRole" where "projectId" = $1 and "userId" = $2"#;
    sqlx::query(set_role).bind(&project_id).bind(&target).bind("OWNER").execute(&mut *tx).await?;
    sqlx::query(set_role).bind(&project_id).bind(&old_owner).bind("MEMBER").execute(&mut *tx).await?;
    sqlx::query(r#"update "Project" set "ownerId" = $2, "updatedAt" = $3 where id = $1"#).bind(&project_id).bind(&target).bind(now()).execute(&mut *tx).await?;
    if let Some(founder) = founder {
        sqlx::query(r#"delete from "RoleMember" where "roleId" = $1 and "userId" = $2"#).bind(&founder).bind(&old_owner).execute(&mut *tx).await?;
        sqlx::query(r#"insert into "RoleMember" ("roleId", "userId", "addedAt") values ($1, $2, $3) on conflict do nothing"#).bind(&founder).bind(&target).bind(now()).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    crate::realtime::emit_project(&s, &org, &project_id, "project:members", &json!({ "projectId": project_id })).await;
    for id in [&target, &old_owner] {
        if let Ok(after) = crate::perm::effective_in(&s.db, &project_id, id).await {
            crate::realtime::emit_users(&s, &[id.clone()], "permissions:changed", &json!({ "organizationId": org, "projectId": project_id, "permissions": after.permissions })).await;
        }
    }
    created(json!({ "projectId": project_id, "owner": target }))
}

async fn delete_project(State(s): State<AppState>, me: Me, Path(project_id): Path<String>) -> Reply {
    require(&s.db, &me.id, "server.manage_projects").await?;
    let (org, _, deleted, _, _) = project_row(&s.db, &project_id).await?;
    if deleted.is_some() {
        return Err(ApiError::conflict("PROJECT_DELETED", "Proje zaten geri dönüşümde."));
    }
    let deleted_at = now();
    let purge_at = deleted_at + chrono::Duration::days(30);
    if let Some(org) = &org {
        crate::realtime::emit_project(&s, org, &project_id, "project:deleted", &json!({ "projectId": project_id })).await;
    }
    sqlx::query(r#"update "Project" set "deletedAt" = $2, "purgeAt" = $3, "updatedAt" = $2 where id = $1"#).bind(&project_id).bind(deleted_at).bind(purge_at).execute(&s.db).await?;
    ok(json!({ "projectId": project_id, "deletedAt": crate::projects::iso_dt(deleted_at), "purgeAt": crate::projects::iso_dt(purge_at) }))
}

async fn restore(State(s): State<AppState>, me: Me, Path(project_id): Path<String>) -> Reply {
    require(&s.db, &me.id, "server.manage_projects").await?;
    let (_, _, deleted, _, _) = project_row(&s.db, &project_id).await?;
    if deleted.is_none() {
        return Err(ApiError::conflict("PROJECT_NOT_DELETED", "Proje silinmemiş."));
    }
    sqlx::query(r#"update "Project" set "deletedAt" = null, "purgeAt" = null, "updatedAt" = $2 where id = $1"#).bind(&project_id).bind(now()).execute(&s.db).await?;
    // Üyelerin rayına geri gelir.
    let members: Vec<String> = sqlx::query_scalar(r#"select "userId" from "ProjectMember" where "projectId" = $1"#).bind(&project_id).fetch_all(&s.db).await?;
    for user in &members {
        if let Ok(project) = member_project_json(&s.db, &project_id, user).await {
            crate::realtime::emit_users(&s, &[user.clone()], "project:created", &json!({ "project": project })).await;
        }
    }
    ok(json!({ "projectId": project_id, "restored": true }))
}

async fn purge(State(s): State<AppState>, me: Me, Path(project_id): Path<String>) -> Reply {
    require(&s.db, &me.id, "server.manage_projects").await?;
    let (_, _, deleted, name, slug) = project_row(&s.db, &project_id).await?;
    if deleted.is_none() {
        return Err(ApiError::conflict("PROJECT_NOT_DELETED", "Önce projeyi silmelisin; yalnızca geri dönüşümdeki projeler kalıcı silinebilir."));
    }
    let audience: Vec<String> = sqlx::query_scalar(r#"select "userId" from "ProjectMember" where "projectId" = $1"#).bind(&project_id).fetch_all(&s.db).await?;
    sqlx::query(r#"delete from "Project" where id = $1"#).bind(&project_id).execute(&s.db).await?;
    crate::realtime::emit_users(&s, &audience, "project:purged", &json!({ "projectId": project_id, "name": name, "slug": slug })).await;
    ok(json!({ "ok": true, "projectId": project_id }))
}

// ------------------------------------------------------------------ uçlar: ayarlar

async fn settings(State(s): State<AppState>, me: Me) -> Reply {
    require_panel(&s.db, &me.id).await?;
    let inst = instance(&s.db).await?;
    ok(json!({
        "name": inst.as_ref().map(|i| i.1.clone()), "organizationId": inst.map(|i| i.0),
        "registrationPolicy": setting(&s.db, "registrationPolicy", "open").await?,
        "projectCreation": setting(&s.db, "projectCreation", "permission").await?,
    }))
}

async fn update_settings(State(s): State<AppState>, me: Me, body: Body) -> Reply {
    require(&s.db, &me.id, "server.settings").await?;
    if let Some(policy) = body.get("registrationPolicy") {
        match policy.as_str() {
            Some(v @ ("open" | "invite")) => set_setting(&s.db, "registrationPolicy", v).await?,
            _ => return Err(ApiError::bad_request("VALIDATION_FAILED", "registrationPolicy: open | invite").field("registrationPolicy")),
        }
    }
    let mut creation_changed = false;
    if let Some(policy) = body.get("projectCreation") {
        match policy.as_str() {
            Some(v @ ("everyone" | "permission")) => {
                set_setting(&s.db, "projectCreation", v).await?;
                creation_changed = true;
            }
            _ => return Err(ApiError::bad_request("VALIDATION_FAILED", "projectCreation: everyone | permission").field("projectCreation")),
        }
    }
    if let Some(name) = body.get("name") {
        let name = name.as_str().map(str::trim).unwrap_or_default();
        if !(2..=60).contains(&name.chars().count()) {
            return Err(ApiError::bad_request("NAME_REQUIRED", "Sunucu adı 2–60 karakter olmalı.").field("name"));
        }
        if let Some((org, _)) = instance(&s.db).await? {
            let organization: sqlx::types::Json<Value> = sqlx::query_scalar(r#"update "Organization" set name = $2, "updatedAt" = $3 where id = $1 returning json_build_object('id', id, 'name', name, 'slug', slug)"#)
                .bind(&org)
                .bind(name)
                .bind(now())
                .fetch_one(&s.db)
                .await?;
            crate::realtime::emit_org(&s, &org, "organization:updated", &json!({ "organization": organization.0 })).await;
        }
    }
    if creation_changed {
        // Proje açma kuralı herkesin `project.create` yetkisini değiştirir: bağlı herkese bildirilir.
        let online = s.presence.lock().unwrap().connected();
        for id in online {
            notify_permissions(&s, &id).await;
        }
    }
    settings(State(s), me).await
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/server/me", get(me_info))
        .route("/server/overview", get(overview))
        .route("/server/settings", get(settings).patch(update_settings))
        .route("/server/users", get(users))
        .route("/server/users/{userId}/sessions/revoke", post(revoke_sessions))
        .route("/server/users/{userId}/2fa/reset", post(reset_two_factor))
        .route("/server/users/{userId}/ban", put(ban).delete(unban))
        .route("/server/users/{userId}/permissions", put(set_permissions))
        .route("/server/projects", get(projects))
        .route("/server/projects/{projectId}", delete(delete_project))
        .route("/server/projects/{projectId}/enter", post(enter))
        .route("/server/projects/{projectId}/transfer", post(transfer))
        .route("/server/projects/{projectId}/restore", post(restore))
        .route("/server/projects/{projectId}/purge", delete(purge))
}
