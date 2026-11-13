//! Proje dokümanları — `server/src/documents.ts` karşılığı (doküman/klasör CRUD, paylaşım, etiketler).

use axum::Router;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::routing::{get, patch, put};
use serde_json::{Map, Value, json};

use crate::AppState;
use crate::activity::{ActivityInput, mentioned_user_ids, record};
use crate::ctx::{Body, Me, Reply, created, forbidden, not_found, now, ok, sql, uuid};
use crate::direct::BODY_LIMIT;
use crate::error::{ApiError, ApiResult};
use crate::realtime::emit_users;
use crate::validate::{Checker, js_len};

const DOCUMENT_KINDS: &[&str] = &["CANVAS", "PAGE"];
const DOCUMENT_ACCESS: &[&str] = &["READ", "WRITE"];

/// `view(document, userId)`: model alanları + owner + access (permissions düşer). $1 = bakan kişi, d = "ProjectDocument".
const DOCUMENT_JSON: &str = concat!(
    r#"json_build_object('id', d.id, 'projectId', d."projectId", 'ownerId', d."ownerId", 'folderId', d."folderId", 'title', d.title, 'kind', d.kind,
    'content', d.content, 'createdAt', "#,
    crate::iso!(r#"d."createdAt""#),
    ", 'updatedAt', ",
    crate::iso!(r#"d."updatedAt""#),
    r#", 'owner', (select json_build_object('id', u.id, 'displayName', u."displayName", 'username', u.username, 'avatarUrl', u."avatarUrl") from "User" u where u.id = d."ownerId"),
    'access', case when d."ownerId" = $1 then 'OWNER' else (select p.access from "DocumentPermission" p where p."documentId" = d.id and p."userId" = $1 limit 1) end)"#
);

const FOLDER_JSON: &str = concat!(
    r#"json_build_object('id', f.id, 'projectId', f."projectId", 'name', f.name, 'position', f.position, 'createdAt', "#,
    crate::iso!(r#"f."createdAt""#),
    ", 'updatedAt', ",
    crate::iso!(r#"f."updatedAt""#),
    ")"
);

const PERMISSION_JSON: &str = concat!(
    r#"json_build_object('documentId', p."documentId", 'userId', p."userId", 'access', p.access, 'createdAt', "#,
    crate::iso!(r#"p."createdAt""#),
    ", 'updatedAt', ",
    crate::iso!(r#"p."updatedAt""#),
    ")"
);

/// JS nesne anahtar sırası: dizi indisi gibi anahtarlar önce (artan), sonra ekleme sırası.
fn js_keys(map: &Map<String, Value>) -> Vec<&String> {
    let index = |k: &str| -> Option<u32> { k.parse::<u32>().ok().filter(|n| *n != u32::MAX && n.to_string() == k) };
    let mut numeric: Vec<&String> = map.keys().filter(|k| index(k).is_some()).collect();
    numeric.sort_by_key(|k| index(k));
    numeric.into_iter().chain(map.keys().filter(|k| index(k).is_none())).collect()
}

/// Doküman içeriğindeki düz metin (etiket araması için).
pub fn document_text(value: &Value) -> String {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    match value {
        Value::String(s) => RE.get_or_init(|| regex::Regex::new(r"<[^>]+>").unwrap()).replace_all(s, " ").into_owned(),
        Value::Array(items) => items.iter().map(document_text).collect::<Vec<_>>().join(" "),
        Value::Object(map) => js_keys(map).into_iter().map(|k| document_text(&map[k])).collect::<Vec<_>>().join(" "),
        _ => String::new(),
    }
}

struct Doc {
    id: String,
    owner_id: String,
    title: String,
    content: Value,
    /// (userId, access) — Prisma include sırası.
    permissions: Vec<(String, String)>,
}

async fn project(db: &sqlx::PgPool, organization_id: &str, project_id: &str, user_id: &str) -> ApiResult<()> {
    crate::perm::require_project_access(db, organization_id, user_id, project_id).await?;
    let hit: Option<String> = sqlx::query_scalar(r#"select id from "Project" where id = $1 and "organizationId" = $2 and "deletedAt" is null limit 1"#)
        .bind(project_id)
        .bind(organization_id)
        .fetch_optional(db)
        .await?;
    if hit.is_none() {
        return Err(not_found("PROJECT_NOT_FOUND", "Proje bulunamadı."));
    }
    Ok(())
}

async fn permissions_of(db: &sqlx::PgPool, document_id: &str) -> ApiResult<Vec<(String, String)>> {
    Ok(sqlx::query_as(r#"select "userId", access from "DocumentPermission" where "documentId" = $1"#).bind(document_id).fetch_all(db).await?)
}

async fn document(db: &sqlx::PgPool, organization_id: &str, project_id: &str, document_id: &str, user_id: &str) -> ApiResult<(Doc, String)> {
    project(db, organization_id, project_id, user_id).await?;
    let row: Option<(String, String, String, sqlx::types::Json<Value>)> =
        sqlx::query_as(r#"select id, "ownerId", title, content from "ProjectDocument" where id = $1 and "projectId" = $2 limit 1"#)
            .bind(document_id)
            .bind(project_id)
            .fetch_optional(db)
            .await?;
    let missing = || not_found("DOCUMENT_NOT_FOUND", "Doküman bulunamadı.");
    let (id, owner_id, title, content) = row.ok_or_else(missing)?;
    let permissions = permissions_of(db, &id).await?;
    let access = if owner_id == user_id { Some("OWNER".to_string()) } else { permissions.iter().find(|(u, _)| u == user_id).map(|(_, a)| a.clone()) };
    let access = access.ok_or_else(missing)?;
    Ok((Doc { id, owner_id, title, content: content.0, permissions }, access))
}

async fn view(db: &sqlx::PgPool, document_id: &str, user_id: &str) -> ApiResult<Value> {
    let row: sqlx::types::Json<Value> = sqlx::query_scalar(sql(format!(r#"select {DOCUMENT_JSON} from "ProjectDocument" d where d.id = $2"#)))
        .bind(user_id)
        .bind(document_id)
        .fetch_one(db)
        .await?;
    Ok(row.0)
}

async fn audience(db: &sqlx::PgPool, document_id: &str) -> ApiResult<Vec<String>> {
    let owner: Option<String> = sqlx::query_scalar(r#"select "ownerId" from "ProjectDocument" where id = $1"#).bind(document_id).fetch_optional(db).await?;
    let Some(owner) = owner else { return Ok(Vec::new()) };
    let mut out = vec![owner];
    out.extend(permissions_of(db, document_id).await?.into_iter().map(|(u, _)| u));
    Ok(out)
}

async fn folder(db: &sqlx::PgPool, project_id: &str, folder_id: &str) -> ApiResult<()> {
    let hit: Option<String> = sqlx::query_scalar(r#"select id from "ProjectDocumentFolder" where id = $1 and "projectId" = $2 limit 1"#)
        .bind(folder_id)
        .bind(project_id)
        .fetch_optional(db)
        .await?;
    if hit.is_none() {
        return Err(ApiError::bad_request("DOCUMENT_FOLDER_NOT_FOUND", "Doküman klasörü bulunamadı."));
    }
    Ok(())
}

/// DocumentFolderDto: name (1..80).
fn folder_name(body: &Map<String, Value>) -> ApiResult<String> {
    let mut c = Checker::new(body);
    let name = c.str("name");
    c.field("name", &[(name.is_some_and(|v| (1..=80).contains(&js_len(v))), None), (name.is_some(), None)]);
    c.finish()?;
    let name = js_trim(name.unwrap_or_default()).to_string();
    if name.is_empty() {
        return Err(ApiError::bad_request("DOCUMENT_FOLDER_NAME", "Klasör adı boş bırakılamaz."));
    }
    Ok(name)
}

fn js_trim(value: &str) -> &str {
    value.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

fn optional_string(c: &Checker, name: &str) -> bool {
    !c.present(name) || c.str(name).is_some()
}

/// Prisma `equals` + `mode: 'insensitive'` → ILIKE (joker karakterler kaçırılmaz).
async fn duplicate_folder(db: &sqlx::PgPool, project_id: &str, name: &str, except: Option<&str>) -> ApiResult<bool> {
    let hit: Option<String> =
        sqlx::query_scalar(r#"select id from "ProjectDocumentFolder" where "projectId" = $1 and ($3::text is null or id <> $3) and name ilike $2 limit 1"#)
            .bind(project_id)
            .bind(name)
            .bind(except)
            .fetch_optional(db)
            .await?;
    Ok(hit.is_some())
}

// ------------------------------------------------------------------ uçlar

type DocPath = Path<(String, String, String)>;

async fn list(State(s): State<AppState>, me: Me, Path((org, project_id)): Path<(String, String)>) -> Reply {
    project(&s.db, &org, &project_id, &me.id).await?;
    let rows: Vec<sqlx::types::Json<Value>> = sqlx::query_scalar(sql(format!(
        r#"select {DOCUMENT_JSON} from "ProjectDocument" d where d."projectId" = $2
           and (d."ownerId" = $1 or exists (select 1 from "DocumentPermission" p where p."documentId" = d.id and p."userId" = $1)) order by d."updatedAt" desc"#
    )))
    .bind(&me.id)
    .bind(&project_id)
    .fetch_all(&s.db)
    .await?;
    ok(Value::Array(rows.into_iter().map(|j| j.0).collect()))
}

async fn folders(State(s): State<AppState>, me: Me, Path((org, project_id)): Path<(String, String)>) -> Reply {
    project(&s.db, &org, &project_id, &me.id).await?;
    let rows: Vec<sqlx::types::Json<Value>> = sqlx::query_scalar(sql(format!(
        r#"select {FOLDER_JSON} from "ProjectDocumentFolder" f where f."projectId" = $1 order by f.position asc, f."createdAt" asc"#
    )))
    .bind(&project_id)
    .fetch_all(&s.db)
    .await?;
    ok(Value::Array(rows.into_iter().map(|j| j.0).collect()))
}

async fn create_folder(State(s): State<AppState>, me: Me, Path((org, project_id)): Path<(String, String)>, Body(body): Body) -> Reply {
    let mut c = Checker::new(&body);
    let raw = c.str("name");
    c.field("name", &[(raw.is_some_and(|v| (1..=80).contains(&js_len(v))), None), (raw.is_some(), None)]);
    c.finish()?;
    project(&s.db, &org, &project_id, &me.id).await?;
    let name = folder_name(&body)?;
    if duplicate_folder(&s.db, &project_id, &name, None).await? {
        return Err(ApiError::bad_request("DOCUMENT_FOLDER_EXISTS", "Bu adda bir klasör zaten var."));
    }
    let position: i64 = sqlx::query_scalar(r#"select count(*) from "ProjectDocumentFolder" where "projectId" = $1"#).bind(&project_id).fetch_one(&s.db).await?;
    let at = now();
    let row: sqlx::types::Json<Value> = sqlx::query_scalar(sql(format!(
        r#"with f as (insert into "ProjectDocumentFolder" (id, "projectId", name, position, "createdAt", "updatedAt") values ($1, $2, $3, $4, $5, $5) returning *) select {FOLDER_JSON} from f"#
    )))
    .bind(uuid())
    .bind(&project_id)
    .bind(&name)
    .bind(position as i32)
    .bind(at)
    .fetch_one(&s.db)
    .await?;
    created(row.0)
}

async fn rename_folder(State(s): State<AppState>, me: Me, Path((org, project_id, folder_id)): DocPath, Body(body): Body) -> Reply {
    let mut c = Checker::new(&body);
    let raw = c.str("name");
    c.field("name", &[(raw.is_some_and(|v| (1..=80).contains(&js_len(v))), None), (raw.is_some(), None)]);
    c.finish()?;
    project(&s.db, &org, &project_id, &me.id).await?;
    folder(&s.db, &project_id, &folder_id).await?;
    let name = folder_name(&body)?;
    if duplicate_folder(&s.db, &project_id, &name, Some(&folder_id)).await? {
        return Err(ApiError::bad_request("DOCUMENT_FOLDER_EXISTS", "Bu adda bir klasör zaten var."));
    }
    let row: sqlx::types::Json<Value> = sqlx::query_scalar(sql(format!(
        r#"with f as (update "ProjectDocumentFolder" set name = $2, "updatedAt" = $3 where id = $1 returning *) select {FOLDER_JSON} from f"#
    )))
    .bind(&folder_id)
    .bind(&name)
    .bind(now())
    .fetch_one(&s.db)
    .await?;
    ok(row.0)
}

async fn delete_folder(State(s): State<AppState>, me: Me, Path((org, project_id, folder_id)): DocPath) -> Reply {
    project(&s.db, &org, &project_id, &me.id).await?;
    folder(&s.db, &project_id, &folder_id).await?;
    let mut tx = s.db.begin().await?;
    sqlx::query(r#"update "ProjectDocument" set "folderId" = null, "updatedAt" = $3 where "projectId" = $1 and "folderId" = $2"#)
        .bind(&project_id)
        .bind(&folder_id)
        .bind(now())
        .execute(&mut *tx)
        .await?;
    sqlx::query(r#"delete from "ProjectDocumentFolder" where id = $1"#).bind(&folder_id).execute(&mut *tx).await?;
    tx.commit().await?;
    ok(json!({ "ok": true }))
}

async fn create(State(s): State<AppState>, me: Me, Path((org, project_id)): Path<(String, String)>, Body(body): Body) -> Reply {
    let mut c = Checker::new(&body);
    let title = c.str("title");
    c.field("title", &[(title.is_some_and(|v| (1..=100).contains(&js_len(v))), None), (title.is_some(), None)]);
    let kind = c.str("kind");
    c.field("kind", &[(kind.is_some_and(|k| DOCUMENT_KINDS.contains(&k)), None), (kind.is_some(), None)]);
    c.field("folderId", &[(optional_string(&c, "folderId"), None)]);
    c.finish()?;
    let title = js_trim(title.unwrap_or_default()).to_string();
    let kind = kind.unwrap_or_default().to_string();
    project(&s.db, &org, &project_id, &me.id).await?;
    if title.is_empty() {
        return Err(ApiError::bad_request("DOCUMENT_TITLE", "Doküman adı boş bırakılamaz."));
    }
    let content = if kind == "PAGE" { json!({ "version": 1, "body": "" }) } else { json!({ "version": 1, "nodes": [], "viewport": { "x": 0, "y": 0, "zoom": 1 } }) };
    let folder_id = body.get("folderId").and_then(Value::as_str).filter(|f| !f.is_empty()).map(str::to_string);
    if let Some(folder_id) = &folder_id {
        folder(&s.db, &project_id, folder_id).await?;
    }
    let id = uuid();
    let at = now();
    sqlx::query(
        r#"insert into "ProjectDocument" (id, "projectId", "ownerId", "folderId", title, kind, content, "createdAt", "updatedAt") values ($1, $2, $3, $4, $5, $6, $7::jsonb, $8, $8)"#,
    )
    .bind(&id)
    .bind(&project_id)
    .bind(&me.id)
    .bind(&folder_id)
    .bind(&title)
    .bind(&kind)
    .bind(sqlx::types::Json(&content))
    .bind(at)
    .execute(&s.db)
    .await?;
    let document = view(&s.db, &id, &me.id).await?;
    emit_users(&s, std::slice::from_ref(&me.id), "document:created", &json!({ "projectId": project_id, "document": document })).await;
    created(document)
}

async fn get_one(State(s): State<AppState>, me: Me, Path((org, project_id, document_id)): DocPath) -> Reply {
    let (doc, _) = document(&s.db, &org, &project_id, &document_id, &me.id).await?;
    ok(view(&s.db, &doc.id, &me.id).await?)
}

async fn update(State(s): State<AppState>, me: Me, Path((org, project_id, document_id)): DocPath, Body(body): Body) -> Reply {
    let mut c = Checker::new(&body);
    let title = c.str("title");
    c.field("title", &[(!c.present("title") || title.is_some_and(|v| (1..=100).contains(&js_len(v))), None), (optional_string(&c, "title"), None)]);
    c.field("content", &[(!c.present("content") || body["content"].is_object(), None)]);
    c.field("folderId", &[(optional_string(&c, "folderId"), None)]);
    c.finish()?;

    let (doc, access) = document(&s.db, &org, &project_id, &document_id, &me.id).await?;
    if access == "READ" {
        return Err(forbidden("DOCUMENT_READ_ONLY", "Bu doküman yalnızca okunabilir."));
    }
    // `title: null` NestJS'te `null.trim()` TypeError'ına (500) düşer.
    if matches!(body.get("title"), Some(Value::Null)) {
        return Err(ApiError::internal("title: null"));
    }
    if title.is_some_and(|t| js_trim(t).is_empty()) {
        return Err(ApiError::bad_request("DOCUMENT_TITLE", "Doküman adı boş bırakılamaz."));
    }
    let content = body.get("content").filter(|v| v.is_object());
    if let Some(content) = content
        && crate::validate::utf16_len(&serde_json::to_string(content).unwrap_or_default()) > 5_000_000
    {
        return Err(ApiError::bad_request("DOCUMENT_TOO_LARGE", "Doküman içeriği çok büyük."));
    }
    let folder_value = body.get("folderId");
    if let Some(folder_id) = folder_value.and_then(Value::as_str).filter(|f| !f.is_empty()) {
        folder(&s.db, &project_id, folder_id).await?;
    }
    let new_title = title.filter(|t| !t.is_empty()).map(|t| js_trim(t).to_string());
    let set_folder = folder_value.is_some();
    let new_folder = folder_value.and_then(Value::as_str).filter(|f| !f.is_empty()).map(str::to_string);
    // Prisma boş `data` ile güncelleme yapmaz (updatedAt da değişmez).
    let changes = new_title.is_some() || content.is_some() || set_folder;
    sqlx::query(
        r#"update "ProjectDocument" set title = coalesce($2, title), content = coalesce($3::jsonb, content),
           "folderId" = case when $4 then $5 else "folderId" end, "updatedAt" = $6 where id = $1 and $7"#,
    )
    .bind(&doc.id)
    .bind(&new_title)
    .bind(content.map(sqlx::types::Json))
    .bind(set_folder)
    .bind(&new_folder)
    .bind(now())
    .bind(changes)
    .execute(&s.db)
    .await?;
    let updated = view(&s.db, &doc.id, &me.id).await?;
    if let Some(content) = content {
        let before = mentioned_user_ids(&s.db, &org, &document_text(&doc.content)).await?;
        let text = document_text(content);
        let after = mentioned_user_ids(&s.db, &org, &text).await?;
        let mut audience = vec![doc.owner_id.clone()];
        audience.extend(doc.permissions.iter().map(|(u, _)| u.clone()));
        let inputs = after
            .into_iter()
            .filter(|u| !before.contains(u) && audience.contains(u))
            .map(|u| ActivityInput {
                organization_id: org.clone(),
                user_id: u,
                actor_id: me.id.clone(),
                kind: "MENTION",
                preview: text.clone(),
                project_id: Some(project_id.clone()),
                document_id: Some(document_id.clone()),
                channel_name: Some(doc.title.clone()),
                ..Default::default()
            })
            .collect();
        record(&s, inputs).await;
    }
    let targets = audience(&s.db, &document_id).await?;
    emit_users(&s, &targets, "document:updated", &json!({ "projectId": project_id, "documentId": document_id, "editorId": me.id })).await;
    ok(updated)
}

async fn remove(State(s): State<AppState>, me: Me, Path((org, project_id, document_id)): DocPath) -> Reply {
    let (doc, access) = document(&s.db, &org, &project_id, &document_id, &me.id).await?;
    if access != "OWNER" {
        return Err(forbidden("DOCUMENT_OWNER_ONLY", "Dokümanı yalnızca sahibi silebilir."));
    }
    let targets = audience(&s.db, &document_id).await?;
    sqlx::query(r#"delete from "ProjectDocument" where id = $1"#).bind(&doc.id).execute(&s.db).await?;
    emit_users(&s, &targets, "document:deleted", &json!({ "projectId": project_id, "documentId": document_id })).await;
    ok(json!({ "ok": true }))
}

fn owner_only() -> ApiError {
    forbidden("DOCUMENT_OWNER_ONLY", "Paylaşımı yalnızca doküman sahibi yönetebilir.")
}

async fn permissions(State(s): State<AppState>, me: Me, Path((org, project_id, document_id)): DocPath) -> Reply {
    let (doc, access) = document(&s.db, &org, &project_id, &document_id, &me.id).await?;
    if access != "OWNER" {
        return Err(owner_only());
    }
    let owner: Option<sqlx::types::Json<Value>> = sqlx::query_scalar(
        r#"select json_build_object('id', u.id, 'displayName', u."displayName", 'username', u.username, 'avatarUrl', u."avatarUrl") from "User" u where u.id = $1"#,
    )
    .bind(&doc.owner_id)
    .fetch_optional(&s.db)
    .await?;
    let members: Vec<sqlx::types::Json<Value>> = sqlx::query_scalar(
        r#"select (select json_build_object('id', u.id, 'displayName', u."displayName", 'username', u.username, 'avatarUrl', u."avatarUrl") from "User" u where u.id = m."userId")
           from "OrganizationMember" m where m."organizationId" = $1
           and (m.role = 'OWNER' or exists (select 1 from "ProjectMember" pm where pm."userId" = m."userId" and pm."projectId" = $2)
                or exists (select 1 from "Project" p where p."ownerId" = m."userId" and p.id = $2 and p."organizationId" = $1))
           order by m."joinedAt" asc"#,
    )
    .bind(&org)
    .bind(&project_id)
    .fetch_all(&s.db)
    .await?;
    let perms: Vec<Value> = doc.permissions.iter().map(|(u, a)| json!({ "userId": u, "access": a })).collect();
    ok(json!({ "owner": owner.map(|j| j.0), "permissions": perms, "members": members.into_iter().map(|j| j.0).collect::<Vec<_>>() }))
}

async fn grant(State(s): State<AppState>, me: Me, Path((org, project_id, document_id, target)): Path<(String, String, String, String)>, Body(body): Body) -> Reply {
    let mut c = Checker::new(&body);
    let access_value = c.str("access");
    c.field("access", &[(access_value.is_some_and(|a| DOCUMENT_ACCESS.contains(&a)), None), (access_value.is_some(), None)]);
    c.finish()?;
    let wanted = access_value.unwrap_or_default().to_string();
    let (doc, access) = document(&s.db, &org, &project_id, &document_id, &me.id).await?;
    if access != "OWNER" {
        return Err(owner_only());
    }
    if target == doc.owner_id {
        return Err(ApiError::bad_request("DOCUMENT_OWNER_ACCESS", "Doküman sahibi zaten tam yetkili."));
    }
    if !crate::perm::can_view_project(&s.db, &org, &target, &project_id).await? {
        return Err(ApiError::bad_request("MEMBER_NOT_IN_PROJECT", "Bu kişi projeyi göremiyor."));
    }
    let at = now();
    let row: sqlx::types::Json<Value> = sqlx::query_scalar(sql(format!(
        r#"with p as (insert into "DocumentPermission" ("documentId", "userId", access, "createdAt", "updatedAt") values ($1, $2, $3, $4, $4)
           on conflict ("documentId", "userId") do update set access = excluded.access, "updatedAt" = excluded."updatedAt" returning *) select {PERMISSION_JSON} from p"#
    )))
    .bind(&document_id)
    .bind(&target)
    .bind(&wanted)
    .bind(at)
    .fetch_one(&s.db)
    .await?;
    emit_users(&s, std::slice::from_ref(&target), "document:access", &json!({ "projectId": project_id, "documentId": document_id, "access": wanted })).await;
    emit_users(&s, std::slice::from_ref(&doc.owner_id), "document:permissions", &json!({ "projectId": project_id, "documentId": document_id })).await;
    ok(row.0)
}

async fn revoke(State(s): State<AppState>, me: Me, Path((org, project_id, document_id, target)): Path<(String, String, String, String)>) -> Reply {
    let (doc, access) = document(&s.db, &org, &project_id, &document_id, &me.id).await?;
    if access != "OWNER" {
        return Err(owner_only());
    }
    sqlx::query(r#"delete from "DocumentPermission" where "documentId" = $1 and "userId" = $2"#).bind(&document_id).bind(&target).execute(&s.db).await?;
    emit_users(&s, std::slice::from_ref(&target), "document:revoked", &json!({ "projectId": project_id, "documentId": document_id })).await;
    emit_users(&s, std::slice::from_ref(&doc.owner_id), "document:permissions", &json!({ "projectId": project_id, "documentId": document_id })).await;
    ok(json!({ "ok": true }))
}

pub fn routes() -> Router<AppState> {
    let base = "/organizations/{organizationId}/projects/{projectId}/documents";
    Router::new()
        .route(base, get(list).post(create))
        .route(&format!("{base}/folders"), get(folders).post(create_folder))
        .route(&format!("{base}/folders/{{folderId}}"), patch(rename_folder).delete(delete_folder))
        .route(&format!("{base}/{{documentId}}"), get(get_one).patch(update).delete(remove))
        .route(&format!("{base}/{{documentId}}/permissions"), get(permissions))
        .route(&format!("{base}/{{documentId}}/permissions/{{userId}}"), put(grant).delete(revoke))
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metin() {
        let v = json!({ "b": "<p>merhaba</p>", "2": ["@ali", 3], "1": { "x": "y" } });
        assert_eq!(document_text(&v), "y @ali   merhaba ");
    }
}
