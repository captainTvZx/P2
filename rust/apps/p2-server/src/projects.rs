//! Projeler, kanal kategorileri, kanallar ve kanal mesajları — `server/src/projects.ts` karşılığı.
//! Ayrıca `app.ts` AppController'daki eski `GET /projects` ve `GET /friends` uçları.
//!
//! Notlar:
//! - Prisma `mode: 'insensitive'` eşitlik ve içerme sorguları PostgreSQL'de `ILIKE` ile (joker
//!   karakterler kaçışsız) yapılır; aynısı korunur.
//! - `PATCH :projectId` gövdesi NestJS'te `@Body()` ile alınır: üretim derlemesinde (tsc,
//!   emitDecoratorMetadata) genel ValidationPipe UpdateProjectDto'yu doğrular. Burada üretim
//!   davranışı izlenir.

use axum::Router;
use axum::extract::{Path, State};
use axum::routing::{MethodRouter, get, post};
use chrono::NaiveDateTime;
use serde_json::{Map, Value, json};

use crate::AppState;
use crate::activity::{ActivityInput, membership, mentioned_user_ids, record};
use crate::ctx::{Body, Me, Query, Reply, created, many_json, not_found, now, ok, one_json, page_args, sql, uuid};
use crate::error::{ApiError, ApiResult};
use crate::perm;
use crate::realtime::{emit_channel, emit_org, emit_project, emit_users};
use crate::validate::js_len;

// ------------------------------------------------------------------ ortak yardımcılar (diğer proje modülleri de kullanır)

/// Prisma Date → JSON: `2026-11-12T19:33:00.123Z`.
pub fn iso_dt(d: NaiveDateTime) -> String {
    d.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

/// SQL tarafında aynı biçim (çalışma anında kurulan sorgular için).
pub fn iso(col: &str) -> String {
    format!(r#"to_char({col}, 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"')"#)
}

/// JS `String.prototype.trim` (Unicode boşlukları + BOM).
pub fn js_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

/// JS `\s` karakter sınıfı.
pub fn js_space(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
}

/// DTO doğrulaması (güvenli mesajı olmayan alanlar): ilk hatalı alan döner.
pub fn dto(fields: &[(&str, bool)]) -> ApiResult<()> {
    match fields.iter().find(|(_, ok)| !ok) {
        Some((field, _)) => Err(ApiError::bad_request("VALIDATION_ERROR", "Girdi alanlarını kontrol edin.").field(field)),
        None => Ok(()),
    }
}

/// `@IsString() @Length(min, max)` (zorunlu).
pub fn str_len(v: Option<&Value>, min: usize, max: usize) -> bool {
    v.and_then(Value::as_str).is_some_and(|s| (min..=max).contains(&js_len(s)))
}

/// `@IsOptional()`: null/undefined ise diğer kısıtlar atlanır.
pub fn optional(v: Option<&Value>, rule: impl Fn(Option<&Value>) -> bool) -> bool {
    matches!(v, None | Some(Value::Null)) || rule(v)
}

pub fn is_str(v: Option<&Value>) -> bool {
    v.is_some_and(Value::is_string)
}

pub fn is_bool(v: Option<&Value>) -> bool {
    v.is_some_and(Value::is_boolean)
}

/// class-validator `@IsInt()` (Number.isInteger).
pub fn is_int(v: Option<&Value>) -> bool {
    v.and_then(Value::as_f64).is_some_and(|f| f.is_finite() && f.fract() == 0.0)
}

/// `@Min(n)`.
pub fn min(v: Option<&Value>, n: f64) -> bool {
    v.and_then(Value::as_f64).is_some_and(|f| f >= n)
}

/// `@IsIn([...])`.
pub fn is_in(v: Option<&Value>, list: &[&str]) -> bool {
    v.and_then(Value::as_str).is_some_and(|s| list.contains(&s))
}

/// JS `String(value)` (ilkel değerler; nesneler için yaklaşık).
pub fn js_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.as_f64().map(js_num).unwrap_or_else(|| n.to_string()),
        Value::Array(items) => items.iter().map(|i| if i.is_null() { String::new() } else { js_string(i) }).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".into(),
    }
}

fn js_num(f: f64) -> String {
    if f.fract() == 0.0 && f.abs() < 1e21 { format!("{}", f as i64) } else { f.to_string() }
}

/// `{ select: { id, username, displayName, avatarUrl } }` — kişi kimliği sütunu verilir.
pub fn user4(id_col: &str) -> String {
    format!(r#"(select json_build_object('id', u.id, 'username', u.username, 'displayName', u."displayName", 'avatarUrl', u."avatarUrl") from "User" u where u.id = {id_col})"#)
}

/// Prisma `update` kaydı bulamazsa (P2025) NestJS 500 döner.
pub fn missing(what: &str) -> ApiError {
    ApiError::internal(format!("kayıt bulunamadı: {what}"))
}

/// Yol bulunamadığında NestJS'in "Cannot METHOD /yol" 404'ü (axum'un 405'i yerine).
fn r(m: MethodRouter<AppState>) -> MethodRouter<AppState> {
    m.fallback(crate::error::not_found)
}

// ------------------------------------------------------------------ SQL parçaları

const PROJECT_JSON: &str = concat!(
    r#"json_build_object('id', p.id, 'name', p.name, 'slug', p.slug, 'description', p.description, 'engine', p.engine, 'ownerId', p."ownerId",
    'defaultChannelId', p."defaultChannelId", 'coverUrl', p."coverUrl", 'iconUrl', p."iconUrl", 'libraryEnabled', p."libraryEnabled", 'createdAt', "#,
    crate::iso!(r#"p."createdAt""#),
    ", 'updatedAt', ",
    crate::iso!(r#"p."updatedAt""#),
    ")"
);

/// Seçimsiz `findMany` (tüm skaler alanlar, şema sırası).
const PROJECT_FULL_JSON: &str = concat!(
    r#"json_build_object('id', p.id, 'name', p.name, 'slug', p.slug, 'description', p.description, 'engine', p.engine, 'ownerId', p."ownerId",
    'organizationId', p."organizationId", 'defaultChannelId', p."defaultChannelId", 'coverUrl', p."coverUrl", 'iconUrl', p."iconUrl",
    'libraryEnabled', p."libraryEnabled", 'createdAt', "#,
    crate::iso!(r#"p."createdAt""#),
    ", 'updatedAt', ",
    crate::iso!(r#"p."updatedAt""#),
    ", 'deletedAt', ",
    crate::iso!(r#"p."deletedAt""#),
    ", 'purgeAt', ",
    crate::iso!(r#"p."purgeAt""#),
    ")"
);

const CHANNEL_JSON: &str = r#"json_build_object('id', ch.id, 'name', ch.name, 'topic', ch.topic, 'type', ch.type::text, 'position', ch.position)"#;

/// ChannelMessage (`m` takma adı) için Prisma `select` alanları, verilen sırayla.
fn msg_json(fields: &[&str]) -> String {
    let parts: Vec<String> = fields
        .iter()
        .map(|f| match *f {
            "id" => "'id', m.id".to_string(),
            "content" => "'content', m.content".to_string(),
            "channelId" => r#"'channelId', m."channelId""#.to_string(),
            "authorId" => r#"'authorId', m."authorId""#.to_string(),
            "parentId" => r#"'parentId', m."parentId""#.to_string(),
            "quotedMessageId" => r#"'quotedMessageId', m."quotedMessageId""#.to_string(),
            "forwardedFromId" => r#"'forwardedFromId', m."forwardedFromId""#.to_string(),
            "createdAt" => format!("'createdAt', {}", iso(r#"m."createdAt""#)),
            "editedAt" => format!("'editedAt', {}", iso(r#"m."editedAt""#)),
            "reactions" => r#"'reactions', coalesce((select json_agg(json_build_object('emoji', r.emoji, 'userId', r."userId") order by r."createdAt") from "MessageReaction" r where r."messageId" = m.id), '[]'::json)"#.to_string(),
            "quotedMessage" => r#"'quotedMessage', (select json_build_object('id', q.id, 'content', q.content, 'author', (select json_build_object('displayName', qa."displayName") from "User" qa where qa.id = q."authorId")) from "ChannelMessage" q where q.id = m."quotedMessageId")"#.to_string(),
            "_count" => r#"'_count', json_build_object('replies', (select count(*) from "ChannelMessage" c where c."parentId" = m.id))"#.to_string(),
            "replies" => format!(
                r#"'replies', coalesce((select json_agg(x.j) from (select json_build_object('createdAt', {}, 'author', (select json_build_object('displayName', ra."displayName") from "User" ra where ra.id = rr."authorId")) j from "ChannelMessage" rr where rr."parentId" = m.id order by rr."createdAt" desc limit 1) x), '[]'::json)"#,
                iso(r#"rr."createdAt""#)
            ),
            "author" => r#"'author', (select json_build_object('id', a.id, 'username', a.username, 'displayName', a."displayName") from "User" a where a.id = m."authorId")"#.to_string(),
            other => panic!("bilinmeyen alan {other}"),
        })
        .collect();
    format!("json_build_object({})", parts.join(", "))
}

const LIST_FIELDS: &[&str] = &["id", "reactions", "content", "createdAt", "editedAt", "forwardedFromId", "quotedMessage", "_count", "replies", "author"];
const REPLY_LIST_FIELDS: &[&str] = &["id", "reactions", "content", "createdAt", "editedAt", "quotedMessage", "author"];
const CREATED_FIELDS: &[&str] = &["id", "content", "createdAt", "editedAt", "forwardedFromId", "quotedMessage", "author"];
const REPLY_FIELDS: &[&str] = &["id", "content", "createdAt", "editedAt", "quotedMessage", "author"];
const FORWARD_FIELDS: &[&str] = &["id", "content", "createdAt", "editedAt", "forwardedFromId", "author"];
const SEARCH_FIELDS: &[&str] = &["id", "content", "createdAt", "parentId", "author"];
const RAW_FIELDS: &[&str] = &["id", "content", "channelId", "authorId", "parentId", "quotedMessageId", "forwardedFromId", "createdAt", "editedAt"];

/// projects.ts `groupReactions`.
pub fn group_reactions(rows: &Value, user_id: &str) -> Value {
    let mut out: Vec<Map<String, Value>> = Vec::new();
    for row in rows.as_array().into_iter().flatten() {
        let emoji = row.get("emoji").cloned().unwrap_or(Value::Null);
        let mine = row.get("userId").and_then(Value::as_str) == Some(user_id);
        match out.iter_mut().find(|item| item["emoji"] == emoji) {
            Some(item) => {
                let count = item["count"].as_i64().unwrap_or(0) + 1;
                item.insert("count".into(), json!(count));
                if mine {
                    item.insert("reacted".into(), json!(true));
                }
            }
            None => {
                let mut item = Map::new();
                item.insert("emoji".into(), emoji);
                item.insert("count".into(), json!(1));
                item.insert("reacted".into(), json!(mine));
                out.push(item);
            }
        }
    }
    Value::Array(out.into_iter().map(Value::Object).collect())
}

fn regroup(mut rows: Vec<Value>, user_id: &str) -> Vec<Value> {
    for row in &mut rows {
        if let Some(obj) = row.as_object_mut() {
            let grouped = group_reactions(obj.get("reactions").unwrap_or(&Value::Null), user_id);
            obj.insert("reactions".into(), grouped);
        }
    }
    rows
}

fn with(mut v: Value, key: &str, value: Value) -> Value {
    if let Some(obj) = v.as_object_mut() {
        obj.insert(key.into(), value);
    }
    v
}

fn thirty_days(at: NaiveDateTime) -> NaiveDateTime {
    at + chrono::Duration::milliseconds(30 * 24 * 60 * 60 * 1000)
}

// ------------------------------------------------------------------ eski uçlar (app.ts)

async fn legacy_projects(State(s): State<AppState>, me: Me) -> Reply {
    let rows = many_json(
        sqlx::query_scalar(sql(format!(
            r#"select {PROJECT_FULL_JSON} from "Project" p where p."ownerId" = $1 or exists (select 1 from "ProjectMember" pm where pm."projectId" = p.id and pm."userId" = $1) order by p."updatedAt" desc"#
        )))
        .bind(&me.id),
        &s.db,
    )
    .await?;
    ok(Value::Array(rows))
}

async fn friends(State(s): State<AppState>, me: Me) -> Reply {
    let rows = many_json(
        sqlx::query_scalar(
            r#"select json_build_object('id', u.id, 'username', u.username, 'displayName', u."displayName", 'status', u.status::text)
               from "Friendship" f join "User" u on u.id = case when f."requesterId" = $1 then f."addresseeId" else f."requesterId" end
               where f.status = 'ACCEPTED' and (f."requesterId" = $1 or f."addresseeId" = $1)"#,
        )
        .bind(&me.id),
        &s.db,
    )
    .await?;
    ok(Value::Array(rows))
}

// ------------------------------------------------------------------ projeler

async fn list(State(s): State<AppState>, me: Me, Path(org): Path<String>) -> Reply {
    let role = membership(&s.db, &org, &me.id).await?;
    let rows = many_json(
        sqlx::query_scalar(sql(format!(
            r#"select {PROJECT_JSON} from "Project" p where p."organizationId" = $1 and p."deletedAt" is null
               and ($3 or p."ownerId" = $2 or exists (select 1 from "ProjectMember" pm where pm."projectId" = p.id and pm."userId" = $2))
               order by p."updatedAt" desc"#
        )))
        .bind(&org)
        .bind(&me.id)
        .bind(role == "OWNER"),
        &s.db,
    )
    .await?;
    ok(Value::Array(rows))
}

async fn unread(State(s): State<AppState>, me: Me, Path(org): Path<String>) -> Reply {
    let role = membership(&s.db, &org, &me.id).await?;
    let pairs: Vec<(String, String)> = sqlx::query_as(
        r#"select ch.id, c."projectId" from "Project" p join "ChannelCategory" c on c."projectId" = p.id and c."deletedAt" is null
           join "ProjectChannel" ch on ch."categoryId" = c.id and ch."deletedAt" is null
           where p."organizationId" = $1 and p."deletedAt" is null
           and ($3 or p."ownerId" = $2 or exists (select 1 from "ProjectMember" pm where pm."projectId" = p.id and pm."userId" = $2))"#,
    )
    .bind(&org)
    .bind(&me.id)
    .bind(role == "OWNER")
    .fetch_all(&s.db)
    .await?;
    let ids: Vec<String> = pairs.iter().map(|(c, _)| c.clone()).collect();
    if ids.is_empty() {
        return ok(json!({}));
    }
    let visible = perm::visible_channel_ids(&s.db, &org, &me.id, &ids).await?;
    let hidden: Vec<String> = sqlx::query_scalar(r#"select "targetId" from "UserPreference" where "userId" = $1 and "organizationId" = $2 and kind = 'CHANNEL' and hidden"#)
        .bind(&me.id)
        .bind(&org)
        .fetch_all(&s.db)
        .await?;
    let reads: Vec<(String, NaiveDateTime)> = sqlx::query_as(r#"select "channelId", "lastReadAt" from "ChannelRead" where "userId" = $1 and "channelId" = any($2)"#)
        .bind(&me.id)
        .bind(&ids)
        .fetch_all(&s.db)
        .await?;
    let counted: Vec<String> = ids.iter().filter(|id| visible.contains(*id) && !hidden.contains(id)).cloned().collect();
    if counted.is_empty() {
        return ok(json!({}));
    }
    let rows: Vec<(String, i64, Option<NaiveDateTime>)> = sqlx::query_as(
        r#"select "channelId", count(*), max("createdAt") from "ChannelMessage" where "channelId" = any($1) and "authorId" <> $2 group by "channelId""#,
    )
    .bind(&counted)
    .bind(&me.id)
    .fetch_all(&s.db)
    .await?;
    let project_of = |channel: &str| pairs.iter().find(|(c, _)| c == channel).map(|(_, p)| p.clone()).unwrap_or_default();
    let since = |channel: &str| reads.iter().find(|(c, _)| c == channel).map(|(_, at)| *at);
    let mut totals = Map::new();
    let mut add = |project: String, n: i64| {
        let current = totals.get(&project).and_then(Value::as_i64).unwrap_or(0);
        totals.insert(project, json!(current + n));
    };
    // Son okuması olmayan kanallar eşzamanlı, olanlar ayrı sayımla (TS'teki sırayla).
    for (channel, count, _) in &rows {
        if since(channel).is_none() {
            add(project_of(channel), *count);
        }
    }
    for (channel, _, latest) in &rows {
        let Some(at) = since(channel) else { continue };
        if latest.is_some_and(|l| l <= at) {
            continue;
        }
        let n: i64 = sqlx::query_scalar(r#"select count(*) from "ChannelMessage" where "channelId" = $1 and "authorId" <> $2 and "createdAt" > $3"#)
            .bind(channel)
            .bind(&me.id)
            .bind(at)
            .fetch_one(&s.db)
            .await?;
        if n != 0 {
            add(project_of(channel), n);
        }
    }
    ok(Value::Object(totals))
}

/// Yeni projenin (ya da kategorisi hiç olmayan projenin) varsayılan kanalları.
async fn seed_channels(tx: &mut sqlx::PgConnection, project_id: &str, at: NaiveDateTime) -> ApiResult<()> {
    let layout: [(&str, &[(&str, &str)]); 3] = [
        ("GENEL", &[("başlangıç", "ANNOUNCEMENT"), ("genel", "TEXT")]),
        ("ÇALIŞMA", &[("geliştirme", "TEXT"), ("geri-bildirim", "FORUM")]),
        ("TOPLANTI", &[("ekip-odası", "VOICE")]),
    ];
    for (position, (name, channels)) in layout.iter().enumerate() {
        let category = uuid();
        sqlx::query(r#"insert into "ChannelCategory" (id, name, position, "projectId") values ($1, $2, $3, $4)"#)
            .bind(&category)
            .bind(*name)
            .bind(position as i32)
            .bind(project_id)
            .execute(&mut *tx)
            .await?;
        for (index, (channel, kind)) in channels.iter().enumerate() {
            sqlx::query(r#"insert into "ProjectChannel" (id, name, type, position, "categoryId", "createdAt") values ($1, $2, $3::"ProjectChannelType", $4, $5, $6)"#)
                .bind(uuid())
                .bind(*channel)
                .bind(*kind)
                .bind(index as i32)
                .bind(&category)
                .bind(at)
                .execute(&mut *tx)
                .await?;
        }
    }
    Ok(())
}

async fn create(State(s): State<AppState>, me: Me, Path(org): Path<String>, body: Body) -> Reply {
    let b = &body.0;
    let slug_ok = b.get("slug").and_then(Value::as_str).is_some_and(|v| !v.is_empty() && v.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-'));
    dto(&[
        ("name", str_len(b.get("name"), 2, 60)),
        ("slug", str_len(b.get("slug"), 2, 40) && slug_ok),
        ("description", optional(b.get("description"), |v| str_len(v, 0, 240))),
        ("engine", optional(b.get("engine"), |v| is_in(v, &["UNREAL", "UNITY", "GODOT", "CUSTOM"]))),
    ])?;
    membership(&s.db, &org, &me.id).await?;
    perm::require(&s.db, &org, &me.id, "project.create").await?;
    let slug = js_trim(body.str("slug").unwrap_or_default()).to_lowercase();
    let taken: Option<i32> = sqlx::query_scalar(r#"select 1 from "Project" where "organizationId" = $1 and slug = $2"#).bind(&org).bind(&slug).fetch_optional(&s.db).await?;
    if taken.is_some() {
        return Err(ApiError::conflict("PROJECT_SLUG_TAKEN", "Bu proje kodu kuruluşta zaten kullanılıyor.").field("slug"));
    }
    let description = body.str("description").map(js_trim).filter(|d| !d.is_empty()).map(str::to_string);
    let engine = body.str("engine").filter(|e| !e.is_empty()).map(str::to_string);
    let id = uuid();
    let at = now();
    let mut tx = s.db.begin().await?;
    let project: sqlx::types::Json<Value> = sqlx::query_scalar(sql(format!(
        r#"with p as (insert into "Project" (id, name, slug, description, engine, "ownerId", "organizationId", "createdAt", "updatedAt")
           values ($1, $2, $3, $4, $5, $6, $7, $8, $8) returning *) select {PROJECT_JSON} from p"#
    )))
    .bind(&id)
    .bind(js_trim(body.str("name").unwrap_or_default()))
    .bind(&slug)
    .bind(&description)
    .bind(&engine)
    .bind(&me.id)
    .bind(&org)
    .bind(at)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query(r#"insert into "ProjectMember" ("projectId", "userId", role) values ($1, $2, 'OWNER')"#).bind(&id).bind(&me.id).execute(&mut *tx).await?;
    seed_channels(&mut tx, &id, at).await?;
    tx.commit().await?;
    let project = project.0;
    emit_users(&s, &[me.id.clone()], "project:created", &json!({ "project": project })).await;
    created(project)
}

/// Kütüphane kapak/ikon adresi: null/'' → null; geçerli /uploads yolu → kendisi; aksi hâlde hata.
fn upload_url(value: &Value) -> Option<Option<String>> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"^/uploads/[a-f0-9-]{36}(\.[a-zA-Z0-9]{1,10})?$").unwrap());
    match value {
        Value::Null => Some(None),
        Value::String(s) if s.is_empty() => Some(None),
        Value::String(s) if re.is_match(s) => Some(Some(s.clone())),
        _ => None,
    }
}

async fn update_project(State(s): State<AppState>, me: Me, Path((org, project_id)): Path<(String, String)>, body: Body) -> Reply {
    let b = &body.0;
    // Üretimdeki genel ValidationPipe (UpdateProjectDto).
    dto(&[
        ("description", optional(b.get("description"), |v| str_len(v, 0, 4000))),
        ("libraryEnabled", optional(b.get("libraryEnabled"), is_bool)),
    ])?;
    membership(&s.db, &org, &me.id).await?;
    perm::require_project_access(&s.db, &org, &me.id, &project_id).await?;
    perm::require(&s.db, &org, &me.id, "channel.manage").await?;
    let library = b.get("libraryEnabled");
    if library.is_some() {
        perm::require(&s.db, &org, &me.id, "release.publish").await?;
    }
    let library_value = library.map(|v| v.as_bool().unwrap_or(false));
    let description = b.get("description").map(|v| {
        let text = js_string(v);
        let t = js_trim(&text).to_string();
        if t.is_empty() { None } else { Some(t) }
    });
    let cover = match b.get("coverUrl") {
        Some(v) => Some(upload_url(v).ok_or_else(|| not_found("UPLOAD_NOT_FOUND", "Görsel bulunamadı."))?),
        None => None,
    };
    let icon = match b.get("iconUrl") {
        Some(v) => Some(upload_url(v).ok_or_else(|| not_found("UPLOAD_NOT_FOUND", "Görsel bulunamadı."))?),
        None => None,
    };
    let default_channel = match b.get("defaultChannelId") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(Value::String(v)) if v.is_empty() => Some(None),
        Some(v) => {
            let wanted = js_string(v);
            let found: Option<String> = sqlx::query_scalar(
                r#"select ch.id from "ProjectChannel" ch join "ChannelCategory" c on c.id = ch."categoryId"
                   where ch.id = $1 and ch."deletedAt" is null and c."projectId" = $2 and c."deletedAt" is null limit 1"#,
            )
            .bind(&wanted)
            .bind(&project_id)
            .fetch_optional(&s.db)
            .await?;
            Some(Some(found.ok_or_else(|| not_found("CHANNEL_NOT_FOUND", "Kanal bulunamadı."))?))
        }
    };
    let project = one_json(
        sqlx::query_scalar(sql(format!(
            r#"with p as (update "Project" set
                 "libraryEnabled" = case when $2 then $3 else "libraryEnabled" end,
                 description = case when $4 then $5 else description end,
                 "coverUrl" = case when $6 then $7 else "coverUrl" end,
                 "iconUrl" = case when $8 then $9 else "iconUrl" end,
                 "defaultChannelId" = case when $10 then $11 else "defaultChannelId" end,
                 "updatedAt" = case when $13 then $12 else "updatedAt" end
               where id = $1 returning *) select {PROJECT_JSON} from p"#
        )))
        .bind(&project_id)
        .bind(library_value.is_some())
        .bind(library_value.unwrap_or(false))
        .bind(description.is_some())
        .bind(description.clone().flatten())
        .bind(cover.is_some())
        .bind(cover.clone().flatten())
        .bind(icon.is_some())
        .bind(icon.clone().flatten())
        .bind(default_channel.is_some())
        .bind(default_channel.clone().flatten())
        .bind(now())
        // Prisma boş `data` ile güncellemede @updatedAt yazmaz.
        .bind(library_value.is_some() || description.is_some() || cover.is_some() || icon.is_some() || default_channel.is_some()),
        &s.db,
    )
    .await?
    .ok_or_else(|| missing("Project"))?;
    emit_project(&s, &org, &project_id, "project:updated", &json!({ "project": project })).await;
    ok(project)
}

async fn delete_project(State(s): State<AppState>, me: Me, Path((org, project_id)): Path<(String, String)>) -> Reply {
    let role = membership(&s.db, &org, &me.id).await?;
    let owner: Option<String> = sqlx::query_scalar(r#"select "ownerId" from "Project" where id = $1 and "organizationId" = $2 and "deletedAt" is null limit 1"#)
        .bind(&project_id)
        .bind(&org)
        .fetch_optional(&s.db)
        .await?;
    let Some(owner) = owner else { return Err(not_found("PROJECT_NOT_FOUND", "Proje bulunamadı.")) };
    if owner != me.id && !["OWNER", "ADMIN"].contains(&role.as_str()) {
        return Err(not_found("PROJECT_NOT_FOUND", "Proje bulunamadı."));
    }
    let deleted_at = now();
    let purge_at = thirty_days(deleted_at);
    emit_project(&s, &org, &project_id, "project:deleted", &json!({ "projectId": project_id })).await;
    sqlx::query(r#"update "Project" set "deletedAt" = $2, "purgeAt" = $3, "updatedAt" = $4 where id = $1"#)
        .bind(&project_id)
        .bind(deleted_at)
        .bind(purge_at)
        .bind(now())
        .execute(&s.db)
        .await?;
    ok(json!({ "deletedAt": iso_dt(deleted_at), "purgeAt": iso_dt(purge_at) }))
}

// ------------------------------------------------------------------ kanallar ve kategoriler

async fn channels(State(s): State<AppState>, me: Me, Path((org, project_id)): Path<(String, String)>) -> Reply {
    membership(&s.db, &org, &me.id).await?;
    perm::require_project_access(&s.db, &org, &me.id, &project_id).await?;
    let exists: Option<i32> = sqlx::query_scalar(r#"select 1 from "Project" where id = $1 and "organizationId" = $2"#).bind(&project_id).bind(&org).fetch_optional(&s.db).await?;
    if exists.is_none() {
        return Err(not_found("PROJECT_NOT_FOUND", "Proje bulunamadı."));
    }
    let count: i64 = sqlx::query_scalar(r#"select count(*) from "ChannelCategory" where "projectId" = $1"#).bind(&project_id).fetch_one(&s.db).await?;
    if count == 0 {
        let mut tx = s.db.begin().await?;
        seed_channels(&mut tx, &project_id, now()).await?;
        tx.commit().await?;
    }
    let categories: Vec<(String, String, i32)> = sqlx::query_as(r#"select id, name, position from "ChannelCategory" where "projectId" = $1 and "deletedAt" is null order by position asc"#)
        .bind(&project_id)
        .fetch_all(&s.db)
        .await?;
    let category_ids: Vec<String> = categories.iter().map(|c| c.0.clone()).collect();
    let rows: Vec<(String, String, Option<String>, String, i32, String, Option<NaiveDateTime>)> = sqlx::query_as(
        r#"select ch.id, ch.name, ch.topic, ch.type::text, ch.position, ch."categoryId",
             (select cr."lastReadAt" from "ChannelRead" cr where cr."channelId" = ch.id and cr."userId" = $2)
           from "ProjectChannel" ch where ch."categoryId" = any($1) and ch."deletedAt" is null order by ch.position asc"#,
    )
    .bind(&category_ids)
    .bind(&me.id)
    .fetch_all(&s.db)
    .await?;
    let all_ids: Vec<String> = rows.iter().map(|r| r.0.clone()).collect();
    let visible = perm::visible_channel_ids(&s.db, &org, &me.id, &all_ids).await?;
    let mut out = Vec::new();
    for (id, name, position) in categories {
        let mut list = Vec::new();
        for (cid, cname, topic, kind, cpos, _, last) in rows.iter().filter(|r| r.5 == id && visible.contains(&r.0)) {
            let unread: i64 = sqlx::query_scalar(r#"select count(*) from "ChannelMessage" where "channelId" = $1 and "authorId" <> $2 and ($3::timestamp is null or "createdAt" > $3)"#)
                .bind(cid)
                .bind(&me.id)
                .bind(last)
                .fetch_one(&s.db)
                .await?;
            list.push(json!({ "id": cid, "name": cname, "topic": topic, "type": kind, "position": cpos, "unread": unread }));
        }
        out.push(json!({ "id": id, "name": name, "position": position, "channels": list }));
    }
    ok(Value::Array(out))
}

pub struct ChannelRow {
    pub name: String,
    pub category_id: String,
    pub kind: String,
}

/// projects.ts `activeChannel`.
async fn active_channel(s: &AppState, org: &str, project_id: &str, channel_id: &str, user_id: &str) -> ApiResult<ChannelRow> {
    perm::require_project_access(&s.db, org, user_id, project_id).await?;
    let row: Option<(String, String, String, String)> = sqlx::query_as(
        r#"select ch.id, ch.name, ch."categoryId", ch.type::text from "ProjectChannel" ch join "ChannelCategory" c on c.id = ch."categoryId" join "Project" p on p.id = c."projectId"
           where ch.id = $1 and ch."deletedAt" is null and c."projectId" = $2 and p."organizationId" = $3 limit 1"#,
    )
    .bind(channel_id)
    .bind(project_id)
    .bind(org)
    .fetch_optional(&s.db)
    .await?;
    let (_, name, category_id, kind) = row.ok_or_else(|| not_found("CHANNEL_NOT_FOUND", "Kanal bulunamadı."))?;
    Ok(ChannelRow { name, category_id, kind })
}

async fn read_channel(State(s): State<AppState>, me: Me, Path((org, project_id, channel_id)): Path<(String, String, String)>) -> Reply {
    membership(&s.db, &org, &me.id).await?;
    perm::require_channel(&s.db, &org, &me.id, &channel_id, &["channel.view"]).await?;
    active_channel(&s, &org, &project_id, &channel_id, &me.id).await?;
    sqlx::query(
        r#"insert into "ChannelRead" ("channelId", "userId", "lastReadAt") values ($1, $2, $3)
           on conflict ("channelId", "userId") do update set "lastReadAt" = excluded."lastReadAt""#,
    )
    .bind(&channel_id)
    .bind(&me.id)
    .bind(now())
    .execute(&s.db)
    .await?;
    created(json!({ "ok": true }))
}

async fn create_channel(State(s): State<AppState>, me: Me, Path((org, project_id)): Path<(String, String)>, body: Body) -> Reply {
    let b = &body.0;
    dto(&[
        ("categoryId", is_str(b.get("categoryId"))),
        ("name", str_len(b.get("name"), 1, 40)),
        ("type", is_in(b.get("type"), &["TEXT", "ANNOUNCEMENT", "VOICE", "FORUM"])),
    ])?;
    membership(&s.db, &org, &me.id).await?;
    perm::require_project_access(&s.db, &org, &me.id, &project_id).await?;
    perm::require(&s.db, &org, &me.id, "channel.manage").await?;
    let category: Option<String> = sqlx::query_scalar(
        r#"select c.id from "ChannelCategory" c join "Project" p on p.id = c."projectId" where c.id = $1 and c."projectId" = $2 and p."organizationId" = $3 limit 1"#,
    )
    .bind(body.str("categoryId"))
    .bind(&project_id)
    .bind(&org)
    .fetch_optional(&s.db)
    .await?;
    let category = category.ok_or_else(|| not_found("CHANNEL_CATEGORY_NOT_FOUND", "Kanal kategorisi bulunamadı."))?;
    let name = js_trim(body.str("name").unwrap_or_default()).to_string();
    if name.is_empty() {
        return Err(ApiError::conflict("CHANNEL_NAME_INVALID", "Geçerli bir kanal adı gir.").field("name"));
    }
    let taken: Option<i32> = sqlx::query_scalar(r#"select 1 from "ProjectChannel" where "categoryId" = $1 and name ilike $2 and "deletedAt" is null limit 1"#)
        .bind(&category)
        .bind(&name)
        .fetch_optional(&s.db)
        .await?;
    if taken.is_some() {
        return Err(ApiError::conflict("CHANNEL_NAME_TAKEN", "Bu kategoride aynı isimde bir kanal var.").field("name"));
    }
    let last: Option<i32> = sqlx::query_scalar(r#"select max(position) from "ProjectChannel" where "categoryId" = $1 and "deletedAt" is null"#).bind(&category).fetch_one(&s.db).await?;
    let channel: sqlx::types::Json<Value> = sqlx::query_scalar(sql(format!(
        r#"with ch as (insert into "ProjectChannel" (id, name, type, position, "categoryId", "createdAt") values ($1, $2, $3::"ProjectChannelType", $4, $5, $6) returning *) select {CHANNEL_JSON} from ch"#
    )))
    .bind(uuid())
    .bind(&name)
    .bind(body.str("type"))
    .bind(last.unwrap_or(-1) + 1)
    .bind(&category)
    .bind(now())
    .fetch_one(&s.db)
    .await?;
    let channel = channel.0;
    let channel_id = channel["id"].as_str().unwrap_or_default().to_string();
    emit_channel(&s, &org, &channel_id, "channel:created", &json!({ "projectId": project_id, "categoryId": category, "channel": channel })).await;
    created(channel)
}

/// `toLocaleLowerCase('tr-TR')`.
fn tr_lower(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'I' => vec!['ı'],
            'İ' => vec!['i'],
            other => other.to_lowercase().collect(),
        })
        .collect()
}

async fn update_channel(State(s): State<AppState>, me: Me, Path((org, project_id, channel_id)): Path<(String, String, String)>, body: Body) -> Reply {
    let b = &body.0;
    dto(&[
        ("name", optional(b.get("name"), |v| str_len(v, 1, 40))),
        ("topic", optional(b.get("topic"), |v| str_len(v, 0, 240))),
    ])?;
    membership(&s.db, &org, &me.id).await?;
    perm::require_channel(&s.db, &org, &me.id, &channel_id, &["channel.manage"]).await?;
    let current = active_channel(&s, &org, &project_id, &channel_id, &me.id).await?;
    let mut name: Option<String> = None;
    if let Some(v) = b.get("name") {
        // null: TS `dto.name.trim()` TypeError → 500.
        let raw = v.as_str().ok_or_else(|| ApiError::internal("name null"))?;
        let trimmed = js_trim(raw).to_string();
        if trimmed.is_empty() {
            return Err(ApiError::conflict("CHANNEL_NAME_INVALID", "Geçerli bir kanal adı gir.").field("name"));
        }
        if tr_lower(&trimmed) != tr_lower(&current.name) {
            let taken: Option<i32> = sqlx::query_scalar(r#"select 1 from "ProjectChannel" where "categoryId" = $1 and name ilike $2 and "deletedAt" is null limit 1"#)
                .bind(&current.category_id)
                .bind(&trimmed)
                .fetch_optional(&s.db)
                .await?;
            if taken.is_some() {
                return Err(ApiError::conflict("CHANNEL_NAME_TAKEN", "Bu kategoride aynı isimde bir kanal var.").field("name"));
            }
        }
        name = Some(trimmed);
    }
    let topic: Option<Option<String>> = match b.get("topic") {
        None => None,
        Some(v) => {
            let raw = v.as_str().ok_or_else(|| ApiError::internal("topic null"))?;
            let t = js_trim(raw);
            Some(if t.is_empty() { None } else { Some(t.to_string()) })
        }
    };
    let channel = one_json(
        sqlx::query_scalar(sql(format!(
            r#"with ch as (update "ProjectChannel" set name = coalesce($2, name), topic = case when $3 then $4 else topic end where id = $1 returning *) select {CHANNEL_JSON} from ch"#
        )))
        .bind(&channel_id)
        .bind(&name)
        .bind(topic.is_some())
        .bind(topic.flatten()),
        &s.db,
    )
    .await?
    .ok_or_else(|| missing("ProjectChannel"))?;
    emit_channel(&s, &org, &channel_id, "channel:updated", &json!({ "projectId": project_id, "categoryId": current.category_id, "channel": channel })).await;
    ok(channel)
}

async fn delete_channel(State(s): State<AppState>, me: Me, Path((org, project_id, channel_id)): Path<(String, String, String)>) -> Reply {
    membership(&s.db, &org, &me.id).await?;
    perm::require_channel(&s.db, &org, &me.id, &channel_id, &["channel.manage"]).await?;
    active_channel(&s, &org, &project_id, &channel_id, &me.id).await?;
    let deleted_at = now();
    let purge_at = thirty_days(deleted_at);
    sqlx::query(r#"update "ProjectChannel" set "deletedAt" = $2, "purgeAt" = $3 where id = $1"#).bind(&channel_id).bind(deleted_at).bind(purge_at).execute(&s.db).await?;
    emit_org(&s, &org, "channel:deleted", &json!({ "projectId": project_id, "channelId": channel_id })).await;
    ok(json!({ "deletedAt": iso_dt(deleted_at), "purgeAt": iso_dt(purge_at) }))
}

async fn delete_category(State(s): State<AppState>, me: Me, Path((org, project_id, category_id)): Path<(String, String, String)>) -> Reply {
    membership(&s.db, &org, &me.id).await?;
    perm::require_project_access(&s.db, &org, &me.id, &project_id).await?;
    perm::require(&s.db, &org, &me.id, "channel.manage").await?;
    let found: Option<String> = sqlx::query_scalar(
        r#"select c.id from "ChannelCategory" c join "Project" p on p.id = c."projectId" where c.id = $1 and c."projectId" = $2 and c."deletedAt" is null and p."organizationId" = $3 limit 1"#,
    )
    .bind(&category_id)
    .bind(&project_id)
    .bind(&org)
    .fetch_optional(&s.db)
    .await?;
    if found.is_none() {
        return Err(not_found("CHANNEL_CATEGORY_NOT_FOUND", "Kanal kategorisi bulunamadı."));
    }
    let children: Vec<(Option<NaiveDateTime>, Option<NaiveDateTime>)> = sqlx::query_as(r#"select "deletedAt", "purgeAt" from "ProjectChannel" where "categoryId" = $1"#)
        .bind(&category_id)
        .fetch_all(&s.db)
        .await?;
    if children.iter().any(|(deleted, _)| deleted.is_none()) {
        return Err(ApiError::conflict("CHANNEL_CATEGORY_NOT_EMPTY", "Kategoriyi silmek için önce içindeki kanalları silmelisin."));
    }
    let deleted_at = now();
    let purge_at = children.iter().filter_map(|(_, p)| *p).fold(thirty_days(deleted_at), |a, b| a.max(b));
    sqlx::query(r#"update "ChannelCategory" set "deletedAt" = $2, "purgeAt" = $3 where id = $1"#).bind(&category_id).bind(deleted_at).bind(purge_at).execute(&s.db).await?;
    emit_project(&s, &org, &project_id, "category:deleted", &json!({ "projectId": project_id, "categoryId": category_id })).await;
    ok(json!({ "deletedAt": iso_dt(deleted_at), "purgeAt": iso_dt(purge_at) }))
}

async fn create_category(State(s): State<AppState>, me: Me, Path((org, project_id)): Path<(String, String)>, body: Body) -> Reply {
    dto(&[("name", str_len(body.get("name"), 1, 32))])?;
    membership(&s.db, &org, &me.id).await?;
    perm::require_project_access(&s.db, &org, &me.id, &project_id).await?;
    perm::require(&s.db, &org, &me.id, "channel.manage").await?;
    let exists: Option<i32> = sqlx::query_scalar(r#"select 1 from "Project" where id = $1 and "organizationId" = $2"#).bind(&project_id).bind(&org).fetch_optional(&s.db).await?;
    if exists.is_none() {
        return Err(not_found("PROJECT_NOT_FOUND", "Proje bulunamadı."));
    }
    let name = js_trim(body.str("name").unwrap_or_default()).to_string();
    if name.is_empty() {
        return Err(ApiError::conflict("CATEGORY_NAME_INVALID", "Geçerli bir kategori adı gir.").field("name"));
    }
    let taken: Option<i32> = sqlx::query_scalar(r#"select 1 from "ChannelCategory" where "projectId" = $1 and name ilike $2 and "deletedAt" is null limit 1"#)
        .bind(&project_id)
        .bind(&name)
        .fetch_optional(&s.db)
        .await?;
    if taken.is_some() {
        return Err(ApiError::conflict("CATEGORY_NAME_TAKEN", "Aynı isimde bir kategori zaten var.").field("name"));
    }
    let last: Option<i32> = sqlx::query_scalar(r#"select max(position) from "ChannelCategory" where "projectId" = $1 and "deletedAt" is null"#).bind(&project_id).fetch_one(&s.db).await?;
    let id = uuid();
    let position = last.unwrap_or(-1) + 1;
    sqlx::query(r#"insert into "ChannelCategory" (id, name, position, "projectId") values ($1, $2, $3, $4)"#).bind(&id).bind(&name).bind(position).bind(&project_id).execute(&s.db).await?;
    let category = json!({ "id": id, "name": name, "position": position, "channels": [] });
    emit_project(&s, &org, &project_id, "category:created", &json!({ "projectId": project_id, "category": category })).await;
    created(category)
}

// ------------------------------------------------------------------ mesajlar

struct MessageRow {
    id: String,
    content: String,
    author_id: String,
    parent_id: Option<String>,
    created_at: NaiveDateTime,
}

async fn find_message(s: &AppState, message_id: &str, channel_id: &str, top_level: bool) -> ApiResult<Option<MessageRow>> {
    let row: Option<(String, String, String, Option<String>, NaiveDateTime)> = sqlx::query_as(
        r#"select id, content, "authorId", "parentId", "createdAt" from "ChannelMessage" where id = $1 and "channelId" = $2 and (not $3 or "parentId" is null) limit 1"#,
    )
    .bind(message_id)
    .bind(channel_id)
    .bind(top_level)
    .fetch_optional(&s.db)
    .await?;
    Ok(row.map(|(id, content, author_id, parent_id, created_at)| MessageRow { id, content, author_id, parent_id, created_at }))
}

fn message_not_found() -> ApiError {
    not_found("MESSAGE_NOT_FOUND", "Mesaj bulunamadı.")
}

async fn messages(State(s): State<AppState>, me: Me, Path((org, project_id, channel_id)): Path<(String, String, String)>, q: Query) -> Reply {
    membership(&s.db, &org, &me.id).await?;
    perm::require_channel(&s.db, &org, &me.id, &channel_id, &["channel.view"]).await?;
    active_channel(&s, &org, &project_id, &channel_id, &me.id).await?;
    let (take, cursor) = page_args(q.get("before").map(String::as_str), q.get("limit").map(String::as_str), 100, 100);
    let mut rows = many_json(
        sqlx::query_scalar(sql(format!(
            r#"select {} from "ChannelMessage" m where m."channelId" = $1 and m."parentId" is null and ($2::timestamp is null or m."createdAt" < $2) order by m."createdAt" desc limit $3"#,
            msg_json(LIST_FIELDS)
        )))
        .bind(&channel_id)
        .bind(cursor)
        .bind(take),
        &s.db,
    )
    .await?;
    rows.reverse();
    ok(Value::Array(regroup(rows, &me.id)))
}

async fn search(State(s): State<AppState>, me: Me, Path((org, project_id, channel_id)): Path<(String, String, String)>, q: Query) -> Reply {
    membership(&s.db, &org, &me.id).await?;
    perm::require_channel(&s.db, &org, &me.id, &channel_id, &["channel.view"]).await?;
    active_channel(&s, &org, &project_id, &channel_id, &me.id).await?;
    let term = js_trim(q.get("q").map(String::as_str).unwrap_or_default()).to_string();
    if crate::validate::utf16_len(&term) < 2 {
        return ok(json!([]));
    }
    let rows = many_json(
        sqlx::query_scalar(sql(format!(
            r#"select {} from "ChannelMessage" m where m."channelId" = $1 and m.content ilike $2 order by m."createdAt" desc limit 50"#,
            msg_json(SEARCH_FIELDS)
        )))
        .bind(&channel_id)
        .bind(format!("%{term}%")),
        &s.db,
    )
    .await?;
    ok(Value::Array(rows))
}

fn check_message_dto(body: &Body) -> ApiResult<()> {
    dto(&[
        ("content", str_len(body.get("content"), 1, 4000)),
        ("quotedMessageId", optional(body.get("quotedMessageId"), is_str)),
    ])
}

/// `@(everyone|here)\b` (JS ASCII kelime sınırı).
fn mentions_all(content: &str) -> bool {
    for word in ["@everyone", "@here"] {
        let mut from = 0;
        while let Some(i) = content[from..].find(word) {
            let end = from + i + word.len();
            let next = content[end..].chars().next();
            if !next.is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') {
                return true;
            }
            from = from + i + 1;
        }
    }
    false
}

/// Kanala yazma: görüntüleme + mesaj gönderme; dosya ve @everyone/@here için ek yetki.
async fn assert_can_post(s: &AppState, org: &str, user_id: &str, channel_id: &str, content: &str) -> ApiResult<()> {
    let mut needed = vec!["channel.view", "message.send"];
    if content.contains("[[p2-file|") {
        needed.push("message.attach");
    }
    if mentions_all(content) {
        needed.push("message.mention_all");
    }
    perm::require_channel(&s.db, org, user_id, channel_id, &needed).await?;
    Ok(())
}

struct MentionContext<'a> {
    project_id: &'a str,
    channel_id: &'a str,
    channel_name: &'a str,
    message_id: String,
    parent_id: Option<String>,
}

/// @kullanıcı adı geçen (ve kanalı görebilen) kişilere etkinlik kaydı.
async fn notify_mentions(s: &AppState, org: &str, actor_id: &str, content: &str, c: MentionContext<'_>) -> ApiResult<()> {
    let mentioned = mentioned_user_ids(&s.db, org, content).await?;
    let targets = perm::channel_audience(&s.db, org, c.channel_id, &mentioned).await?;
    if targets.is_empty() {
        return Ok(());
    }
    let rows = targets
        .into_iter()
        .map(|user_id| ActivityInput {
            organization_id: org.to_string(),
            user_id,
            actor_id: actor_id.to_string(),
            kind: "MENTION",
            preview: content.to_string(),
            project_id: Some(c.project_id.to_string()),
            channel_id: Some(c.channel_id.to_string()),
            channel_name: Some(c.channel_name.to_string()),
            message_id: Some(c.message_id.clone()),
            parent_id: c.parent_id.clone(),
            ..Default::default()
        })
        .collect();
    record(s, rows).await;
    Ok(())
}

async fn check_quoted(s: &AppState, body: &Body, channel_id: &str) -> ApiResult<Option<String>> {
    let quoted = body.str("quotedMessageId").filter(|q| !q.is_empty()).map(str::to_string);
    if let Some(q) = &quoted
        && find_message(s, q, channel_id, false).await?.is_none()
    {
        return Err(not_found("QUOTED_MESSAGE_NOT_FOUND", "Alıntılanan mesaj bulunamadı."));
    }
    Ok(quoted)
}

async fn insert_message(s: &AppState, channel_id: &str, parent_id: Option<&str>, author_id: &str, content: &str, quoted: Option<&str>, forwarded: Option<&str>, fields: &[&str]) -> ApiResult<Value> {
    let row: sqlx::types::Json<Value> = sqlx::query_scalar(sql(format!(
        r#"with m as (insert into "ChannelMessage" (id, content, "channelId", "authorId", "parentId", "quotedMessageId", "forwardedFromId", "createdAt")
           values ($1, $2, $3, $4, $5, $6, $7, $8) returning *) select {} from m"#,
        msg_json(fields)
    )))
    .bind(uuid())
    .bind(content)
    .bind(channel_id)
    .bind(author_id)
    .bind(parent_id)
    .bind(quoted)
    .bind(forwarded)
    .bind(now())
    .fetch_one(&s.db)
    .await?;
    Ok(row.0)
}

async fn create_message(State(s): State<AppState>, me: Me, Path((org, project_id, channel_id)): Path<(String, String, String)>, body: Body) -> Reply {
    check_message_dto(&body)?;
    let content = body.str("content").unwrap_or_default().to_string();
    membership(&s.db, &org, &me.id).await?;
    assert_can_post(&s, &org, &me.id, &channel_id, &content).await?;
    let channel = active_channel(&s, &org, &project_id, &channel_id, &me.id).await?;
    let quoted = check_quoted(&s, &body, &channel_id).await?;
    let message = insert_message(&s, &channel_id, None, &me.id, js_trim(&content), quoted.as_deref(), None, CREATED_FIELDS).await?;
    let message_id = message["id"].as_str().unwrap_or_default().to_string();
    let payload = json!({ "projectId": project_id, "channelId": channel_id, "channelName": channel.name, "parentId": null, "message": with(message.clone(), "_count", json!({ "replies": 0 })) });
    emit_channel(&s, &org, &channel_id, "message:created", &payload).await;
    notify_mentions(&s, &org, &me.id, &content, MentionContext { project_id: &project_id, channel_id: &channel_id, channel_name: &channel.name, message_id, parent_id: None }).await?;
    created(message)
}

async fn edit_message(State(s): State<AppState>, me: Me, Path((org, project_id, channel_id, message_id)): Path<(String, String, String, String)>, body: Body) -> Reply {
    dto(&[("content", str_len(body.get("content"), 1, 4000))])?;
    let content = body.str("content").unwrap_or_default().to_string();
    membership(&s.db, &org, &me.id).await?;
    active_channel(&s, &org, &project_id, &channel_id, &me.id).await?;
    assert_can_post(&s, &org, &me.id, &channel_id, &content).await?;
    let current = one_json(
        sqlx::query_scalar(sql(format!(
            r#"select {} from "ChannelMessage" m where m.id = $1 and m."channelId" = $2 and m."authorId" = $3 limit 1"#,
            msg_json(RAW_FIELDS)
        )))
        .bind(&message_id)
        .bind(&channel_id)
        .bind(&me.id),
        &s.db,
    )
    .await?
    .ok_or_else(message_not_found)?;
    let trimmed = js_trim(&content).to_string();
    if current["content"].as_str() == Some(trimmed.as_str()) {
        return ok(current);
    }
    let at = now();
    let mut tx = s.db.begin().await?;
    sqlx::query(r#"insert into "MessageEditHistory" (id, "messageId", content, "createdAt") values ($1, $2, $3, $4)"#)
        .bind(uuid())
        .bind(&message_id)
        .bind(current["content"].as_str())
        .bind(at)
        .execute(&mut *tx)
        .await?;
    let updated: Option<sqlx::types::Json<Value>> = sqlx::query_scalar(sql(format!(
        r#"with m as (update "ChannelMessage" set content = $2, "editedAt" = $3 where id = $1 returning *) select {} from m"#,
        msg_json(CREATED_FIELDS)
    )))
    .bind(&message_id)
    .bind(&trimmed)
    .bind(at)
    .fetch_optional(&mut *tx)
    .await?;
    let updated = updated.ok_or_else(|| missing("ChannelMessage"))?.0;
    tx.commit().await?;
    emit_channel(&s, &org, &channel_id, "message:updated", &json!({ "projectId": project_id, "channelId": channel_id, "parentId": current["parentId"], "message": updated })).await;
    ok(updated)
}

async fn message_history(State(s): State<AppState>, me: Me, Path((org, project_id, channel_id, message_id)): Path<(String, String, String, String)>) -> Reply {
    membership(&s.db, &org, &me.id).await?;
    active_channel(&s, &org, &project_id, &channel_id, &me.id).await?;
    perm::require_channel(&s.db, &org, &me.id, &channel_id, &["channel.view"]).await?;
    if find_message(&s, &message_id, &channel_id, false).await?.is_none() {
        return Err(message_not_found());
    }
    let rows = many_json(
        sqlx::query_scalar(sql(format!(
            r#"select json_build_object('id', h.id, 'content', h.content, 'createdAt', {}) from "MessageEditHistory" h where h."messageId" = $1 order by h."createdAt" desc"#,
            iso(r#"h."createdAt""#)
        )))
        .bind(&message_id),
        &s.db,
    )
    .await?;
    ok(Value::Array(rows))
}

async fn delete_message(State(s): State<AppState>, me: Me, Path((org, project_id, channel_id, message_id)): Path<(String, String, String, String)>) -> Reply {
    membership(&s.db, &org, &me.id).await?;
    active_channel(&s, &org, &project_id, &channel_id, &me.id).await?;
    let message = find_message(&s, &message_id, &channel_id, false).await?.ok_or_else(message_not_found)?;
    let (_, granted) = perm::channel_permissions(&s.db, &org, &me.id, &channel_id).await?;
    let has = |p: &str| granted.iter().any(|g| g == p);
    if !has("channel.view") {
        return Err(perm::refuse("channel.view"));
    }
    let moderator = message.author_id != me.id;
    let unlimited = has("message.delete_unlimited");
    if moderator && !unlimited && !has("message.delete_any") {
        perm::require_channel(&s.db, &org, &me.id, &channel_id, &["message.delete_any"]).await?;
    }
    // Normal kullanıcı kendi mesajını 2 saat içinde silebilir; süresiz silme yetkisi bu sınırı kaldırır.
    if !moderator && !unlimited && (now() - message.created_at).num_milliseconds() > 2 * 60 * 60 * 1000 {
        return Err(ApiError::conflict("MESSAGE_DELETE_WINDOW_EXPIRED", "Mesajlar yalnızca ilk 2 saat içinde silinebilir."));
    }
    let deleted = sqlx::query(r#"delete from "ChannelMessage" where id = $1"#).bind(&message_id).execute(&s.db).await?;
    if deleted.rows_affected() == 0 {
        return Err(missing("ChannelMessage"));
    }
    let parent_replies: i64 = match &message.parent_id {
        Some(parent) => sqlx::query_scalar(r#"select count(*) from "ChannelMessage" where "parentId" = $1"#).bind(parent).fetch_one(&s.db).await?,
        None => 0,
    };
    emit_channel(
        &s,
        &org,
        &channel_id,
        "message:deleted",
        &json!({ "projectId": project_id, "channelId": channel_id, "messageId": message_id, "parentId": message.parent_id, "parentReplies": parent_replies }),
    )
    .await;
    ok(json!({ "deleted": true }))
}

async fn forward_message(State(s): State<AppState>, me: Me, Path((org, project_id, channel_id, message_id)): Path<(String, String, String, String)>, body: Body) -> Reply {
    dto(&[("targetChannelId", is_str(body.get("targetChannelId")))])?;
    let target = body.str("targetChannelId").unwrap_or_default().to_string();
    membership(&s.db, &org, &me.id).await?;
    active_channel(&s, &org, &project_id, &channel_id, &me.id).await?;
    let source = find_message(&s, &message_id, &channel_id, false).await?.ok_or_else(message_not_found)?;
    active_channel(&s, &org, &project_id, &target, &me.id).await?;
    perm::require_channel(&s.db, &org, &me.id, &channel_id, &["channel.view", "message.forward"]).await?;
    perm::require_channel(&s.db, &org, &me.id, &target, &["channel.view", "message.send"]).await?;
    let message = insert_message(&s, &target, None, &me.id, &source.content, None, Some(&source.id), FORWARD_FIELDS).await?;
    emit_channel(
        &s,
        &org,
        &channel_id,
        "message:created",
        &json!({ "projectId": project_id, "channelId": target, "parentId": null, "message": with(message.clone(), "_count", json!({ "replies": 0 })) }),
    )
    .await;
    created(message)
}

async fn toggle_reaction(State(s): State<AppState>, me: Me, Path((org, project_id, channel_id, message_id)): Path<(String, String, String, String)>, body: Body) -> Reply {
    let emoji_ok = body.str("emoji").is_some_and(|e| !e.is_empty() && !e.chars().any(js_space));
    dto(&[("emoji", str_len(body.get("emoji"), 1, 16) && emoji_ok)])?;
    membership(&s.db, &org, &me.id).await?;
    active_channel(&s, &org, &project_id, &channel_id, &me.id).await?;
    perm::require_channel(&s.db, &org, &me.id, &channel_id, &["channel.view", "message.react"]).await?;
    let target = find_message(&s, &message_id, &channel_id, false).await?.ok_or_else(message_not_found)?;
    let emoji = js_trim(body.str("emoji").unwrap_or_default()).to_string();
    let existing: Option<String> = sqlx::query_scalar(r#"select id from "MessageReaction" where "messageId" = $1 and "userId" = $2 and emoji = $3"#)
        .bind(&message_id)
        .bind(&me.id)
        .bind(&emoji)
        .fetch_optional(&s.db)
        .await?;
    match existing {
        Some(id) => {
            sqlx::query(r#"delete from "MessageReaction" where id = $1"#).bind(&id).execute(&s.db).await?;
        }
        None => {
            sqlx::query(r#"insert into "MessageReaction" (id, "messageId", "userId", emoji, "createdAt") values ($1, $2, $3, $4, $5)"#)
                .bind(uuid())
                .bind(&message_id)
                .bind(&me.id)
                .bind(&emoji)
                .bind(now())
                .execute(&s.db)
                .await?;
            let channel_name: Option<String> = sqlx::query_scalar(r#"select name from "ProjectChannel" where id = $1"#).bind(&channel_id).fetch_optional(&s.db).await?;
            record(
                &s,
                vec![ActivityInput {
                    organization_id: org.clone(),
                    user_id: target.author_id.clone(),
                    actor_id: me.id.clone(),
                    kind: "REACTION",
                    preview: target.content.clone(),
                    emoji: Some(emoji.clone()),
                    project_id: Some(project_id.clone()),
                    channel_id: Some(channel_id.clone()),
                    channel_name,
                    message_id: Some(target.id.clone()),
                    parent_id: target.parent_id.clone(),
                    ..Default::default()
                }],
            )
            .await;
        }
    }
    let rows: Vec<(String, String)> = sqlx::query_as(r#"select emoji, "userId" from "MessageReaction" where "messageId" = $1 order by "createdAt" asc"#)
        .bind(&message_id)
        .fetch_all(&s.db)
        .await?;
    let rows = Value::Array(rows.into_iter().map(|(emoji, user_id)| json!({ "emoji": emoji, "userId": user_id })).collect());
    emit_channel(&s, &org, &channel_id, "message:reactions", &json!({ "projectId": project_id, "channelId": channel_id, "messageId": message_id, "rows": rows })).await;
    created(group_reactions(&rows, &me.id))
}

async fn thread_rows(s: &AppState, channel_id: &str, parent_id: &str, user_id: &str) -> ApiResult<Value> {
    let rows = many_json(
        sqlx::query_scalar(sql(format!(
            r#"select {} from "ChannelMessage" m where m."channelId" = $1 and m."parentId" = $2 order by m."createdAt" asc"#,
            msg_json(REPLY_LIST_FIELDS)
        )))
        .bind(channel_id)
        .bind(parent_id),
        &s.db,
    )
    .await?;
    Ok(Value::Array(regroup(rows, user_id)))
}

async fn message_replies(State(s): State<AppState>, me: Me, Path((org, project_id, channel_id, message_id)): Path<(String, String, String, String)>) -> Reply {
    membership(&s.db, &org, &me.id).await?;
    active_channel(&s, &org, &project_id, &channel_id, &me.id).await?;
    perm::require_channel(&s.db, &org, &me.id, &channel_id, &["channel.view"]).await?;
    if find_message(&s, &message_id, &channel_id, true).await?.is_none() {
        return Err(message_not_found());
    }
    ok(thread_rows(&s, &channel_id, &message_id, &me.id).await?)
}

async fn count_replies(s: &AppState, parent_id: &str) -> ApiResult<i64> {
    Ok(sqlx::query_scalar(r#"select count(*) from "ChannelMessage" where "parentId" = $1"#).bind(parent_id).fetch_one(&s.db).await?)
}

async fn create_message_reply(State(s): State<AppState>, me: Me, Path((org, project_id, channel_id, message_id)): Path<(String, String, String, String)>, body: Body) -> Reply {
    check_message_dto(&body)?;
    let content = body.str("content").unwrap_or_default().to_string();
    membership(&s.db, &org, &me.id).await?;
    perm::require_channel(&s.db, &org, &me.id, &channel_id, &["channel.view", "thread.reply"]).await?;
    let channel = active_channel(&s, &org, &project_id, &channel_id, &me.id).await?;
    let parent = find_message(&s, &message_id, &channel_id, true).await?.ok_or_else(message_not_found)?;
    let quoted = check_quoted(&s, &body, &channel_id).await?;
    let reply = insert_message(&s, &channel_id, Some(&message_id), &me.id, js_trim(&content), quoted.as_deref(), None, REPLY_FIELDS).await?;
    let reply_id = reply["id"].as_str().unwrap_or_default().to_string();
    let parent_replies = count_replies(&s, &message_id).await?;
    emit_channel(
        &s,
        &org,
        &channel_id,
        "message:created",
        &json!({ "projectId": project_id, "channelId": channel_id, "channelName": channel.name, "parentId": message_id, "parentReplies": parent_replies, "message": reply }),
    )
    .await;
    record(
        &s,
        vec![ActivityInput {
            organization_id: org.clone(),
            user_id: parent.author_id.clone(),
            actor_id: me.id.clone(),
            kind: "REPLY",
            preview: content.clone(),
            project_id: Some(project_id.clone()),
            channel_id: Some(channel_id.clone()),
            channel_name: Some(channel.name.clone()),
            message_id: Some(reply_id.clone()),
            parent_id: Some(message_id.clone()),
            ..Default::default()
        }],
    )
    .await;
    notify_mentions(&s, &org, &me.id, &content, MentionContext { project_id: &project_id, channel_id: &channel_id, channel_name: &channel.name, message_id: reply_id, parent_id: Some(message_id.clone()) }).await?;
    created(with(reply, "parentReplies", json!(parent_replies)))
}

fn not_forum() -> ApiError {
    ApiError::conflict("CHANNEL_NOT_FORUM", "Bu kanal forum değil.")
}

async fn topic_replies(State(s): State<AppState>, me: Me, Path((org, project_id, channel_id, topic_id)): Path<(String, String, String, String)>) -> Reply {
    membership(&s.db, &org, &me.id).await?;
    let channel = active_channel(&s, &org, &project_id, &channel_id, &me.id).await?;
    perm::require_channel(&s.db, &org, &me.id, &channel_id, &["channel.view"]).await?;
    if channel.kind != "FORUM" {
        return Err(not_forum());
    }
    if find_message(&s, &topic_id, &channel_id, true).await?.is_none() {
        return Err(not_found("TOPIC_NOT_FOUND", "Konu bulunamadı."));
    }
    ok(thread_rows(&s, &channel_id, &topic_id, &me.id).await?)
}

async fn create_topic_reply(State(s): State<AppState>, me: Me, Path((org, project_id, channel_id, topic_id)): Path<(String, String, String, String)>, body: Body) -> Reply {
    check_message_dto(&body)?;
    let content = body.str("content").unwrap_or_default().to_string();
    membership(&s.db, &org, &me.id).await?;
    perm::require_channel(&s.db, &org, &me.id, &channel_id, &["channel.view", "thread.reply"]).await?;
    let channel = active_channel(&s, &org, &project_id, &channel_id, &me.id).await?;
    if channel.kind != "FORUM" {
        return Err(not_forum());
    }
    if find_message(&s, &topic_id, &channel_id, true).await?.is_none() {
        return Err(not_found("TOPIC_NOT_FOUND", "Konu bulunamadı."));
    }
    let quoted = check_quoted(&s, &body, &channel_id).await?;
    let reply = insert_message(&s, &channel_id, Some(&topic_id), &me.id, js_trim(&content), quoted.as_deref(), None, REPLY_FIELDS).await?;
    let parent_replies = count_replies(&s, &topic_id).await?;
    emit_channel(
        &s,
        &org,
        &channel_id,
        "message:created",
        &json!({ "projectId": project_id, "channelId": channel_id, "channelName": channel.name, "parentId": topic_id, "parentReplies": parent_replies, "message": reply }),
    )
    .await;
    created(with(reply, "parentReplies", json!(parent_replies)))
}

pub fn routes() -> Router<AppState> {
    let base = "/organizations/{organizationId}/projects";
    let ch = "/organizations/{organizationId}/projects/{projectId}/channels/{channelId}";
    let msg = "/organizations/{organizationId}/projects/{projectId}/channels/{channelId}/messages/{messageId}";
    Router::new()
        .route("/projects", r(get(legacy_projects)))
        .route("/friends", r(get(friends)))
        .route(base, r(get(list).post(create)))
        .route(&format!("{base}/unread"), r(get(unread)))
        .route(&format!("{base}/{{projectId}}"), r(axum::routing::patch(update_project).delete(delete_project)))
        .route(&format!("{base}/{{projectId}}/channels"), r(get(channels).post(create_channel)))
        .route(ch, r(axum::routing::patch(update_channel).delete(delete_channel)))
        .route(&format!("{ch}/read"), r(post(read_channel)))
        .route(&format!("{base}/{{projectId}}/channel-categories"), r(post(create_category)))
        .route(&format!("{base}/{{projectId}}/channel-categories/{{categoryId}}"), r(axum::routing::delete(delete_category)))
        .route(&format!("{ch}/messages"), r(get(messages).post(create_message)))
        .route(&format!("{ch}/search"), r(get(search)))
        .route(msg, r(axum::routing::patch(edit_message).delete(delete_message)))
        .route(&format!("{msg}/history"), r(get(message_history)))
        .route(&format!("{msg}/forward"), r(post(forward_message)))
        .route(&format!("{msg}/reactions"), r(post(toggle_reaction)))
        .route(&format!("{msg}/replies"), r(get(message_replies).post(create_message_reply)))
        .route(&format!("{ch}/topics/{{topicId}}/replies"), r(get(topic_replies).post(create_topic_reply)))
}
