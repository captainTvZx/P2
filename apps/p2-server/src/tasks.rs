//! Görev panoları — `server/src/tasks.ts` karşılığı: panolar, sprint'ler, kanban görevleri,
//! görev yorumları ve yorum tepkileri. Ayrıca releases.rs'nin de kullandığı küçük
//! class-validator / JS yardımcıları (Length, IsISO8601, `new Date`, trim, String()).

use axum::Router;
use axum::extract::{Path, State};
use axum::routing::{get, patch, post};
use chrono::NaiveDateTime;
use serde_json::{Map, Value, json};
use sqlx::types::Json;

use crate::AppState;
use crate::activity::{self, ActivityInput};
use crate::ctx::{Body, Me, Reply, created, forbidden, not_found, ok, sql, uuid};
use crate::error::{ApiError, ApiResult};
use crate::perm;
use crate::validate::{Checker, js_slice};

pub const TASK_STATUSES: [&str; 5] = ["BACKLOG", "TODO", "IN_PROGRESS", "REVIEW", "DONE"];
pub const TASK_PRIORITIES: [&str; 4] = ["LOW", "MEDIUM", "HIGH", "URGENT"];
pub const TASK_KINDS: [&str; 5] = ["TASK", "BUG", "FEATURE", "IMPROVEMENT", "DOCS"];
const SPRINT_STATUSES: [&str; 3] = ["PLANNED", "ACTIVE", "DONE"];
const COMMENT_EDIT_WINDOW_MS: i64 = 2 * 60 * 60 * 1000;

// ------------------------------------------------------------------ JS / class-validator yardımcıları

/// JS `\s` ve `String.prototype.trim` boşluk kümesi (Rust'ınkinden farklı: U+FEFF var, U+0085 yok).
pub fn is_js_space(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'..='\u{200A}' | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}' | '\u{3000}' | '\u{FEFF}')
}

pub fn js_trim(value: &str) -> &str {
    value.trim_matches(is_js_space)
}

/// `value.replace(/\s+/g, ' ')`
pub fn collapse_js_space(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut in_space = false;
    for c in value.chars() {
        if is_js_space(c) {
            if !in_space {
                out.push(' ');
            }
            in_space = true;
        } else {
            out.push(c);
            in_space = false;
        }
    }
    out
}

/// validator.js `isLength`: UTF-16 uzunluğundan vekil çiftleri ve varyasyon seçicileri düşülür.
pub fn vlen(value: &str) -> usize {
    let mut len = 0usize;
    let mut prev_vs = true;
    for c in value.chars() {
        len += c.len_utf16();
        let vs = matches!(c, '\u{FE0F}' | '\u{FE0E}');
        if c.len_utf16() == 2 {
            len -= 1;
        }
        if vs && !prev_vs {
            len -= 1;
        }
        prev_vs = vs;
    }
    len
}

/// `@IsString() @Length(min, max)`
pub fn str_len(v: Option<&Value>, min: usize, max: usize) -> bool {
    v.and_then(Value::as_str).is_some_and(|s| (min..=max).contains(&vlen(s)))
}

/// `@IsOptional()`: undefined ve null doğrulanmaz.
pub fn absent(v: Option<&Value>) -> bool {
    matches!(v, None | Some(Value::Null))
}

/// `@IsInt()`: typeof number && Number.isInteger.
pub fn is_int(v: Option<&Value>) -> bool {
    v.and_then(Value::as_f64).is_some_and(|f| f.is_finite() && f.fract() == 0.0)
}

pub fn is_in(v: Option<&Value>, list: &[&str]) -> bool {
    v.and_then(Value::as_str).is_some_and(|s| list.contains(&s))
}

/// validator.js `isISO8601` (strict değil) — kendi düzenli ifadesi birebir.
pub fn is_iso8601(v: Option<&Value>) -> bool {
    static RE: std::sync::OnceLock<fancy_regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        fancy_regex::Regex::new(r"^([\+-]?\d{4}(?!\d{2}\b))((-?)((0[1-9]|1[0-2])(\3([12]\d|0[1-9]|3[01]))?|W([0-4]\d|5[0-3])(-?[1-7])?|(00[1-9]|0[1-9]\d|[12]\d{2}|3([0-5]\d|6[1-6])))([T\s]((([01]\d|2[0-3])((:?)[0-5]\d)?|24:?00)([\.,]\d+(?!:))?)?(\17[0-5]\d([\.,]\d+)?)?([zZ]|([\+-])([01]\d|2[0-3]):?([0-5]\d)?)?)?)?$").unwrap()
    });
    v.and_then(Value::as_str).is_some_and(|s| re.is_match(s).unwrap_or(false))
}

/// JS `new Date(text)` (V8'in ISO biçimi): yalnız tarih → UTC, saatli ve bölgesiz → yerel saat.
/// Geçersizse None (Prisma "Invalid Date" ile 500 verir).
pub fn js_date(text: &str) -> Option<NaiveDateTime> {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        regex::Regex::new(r"^([+-]\d{6}|\d{4})(?:-(\d{2})(?:-(\d{2}))?)?(?:[Tt ](\d{2}):(\d{2})(?::(\d{2})(?:[.,](\d+))?)?)?\s*([zZ]|[+-]\d{2}(?::?\d{2})?)?$").unwrap()
    });
    let c = re.captures(text.trim())?;
    let year: i32 = c[1].parse().ok()?;
    let num = |i: usize, d: u32| c.get(i).map_or(Some(d), |m| m.as_str().parse::<u32>().ok());
    let (month, day) = (num(2, 1)?, num(3, 1)?);
    let (hour, minute, second) = (num(4, 0)?, num(5, 0)?, num(6, 0)?);
    let ms: i64 = c.get(7).map_or(0, |m| {
        let digits: String = m.as_str().chars().chain("000".chars()).take(3).collect();
        digits.parse().unwrap_or(0)
    });
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 24 || minute > 59 || second > 59 {
        return None;
    }
    if hour == 24 && (minute > 0 || second > 0 || ms > 0) {
        return None;
    }
    // V8 ayın gününü taşırır (31 Şubat → 3 Mart).
    let date = chrono::NaiveDate::from_ymd_opt(year, month, 1)? + chrono::Duration::days(day as i64 - 1);
    let naive = date.and_hms_opt(0, 0, 0)? + chrono::Duration::milliseconds(((hour as i64 * 60 + minute as i64) * 60 + second as i64) * 1000 + ms);
    let has_time = c.get(4).is_some();
    match c.get(8).map(|m| m.as_str()) {
        Some("z") | Some("Z") => Some(naive),
        Some(offset) => {
            let sign = if offset.starts_with('-') { -1 } else { 1 };
            let digits: String = offset[1..].chars().filter(char::is_ascii_digit).collect();
            let h: i64 = digits.get(0..2)?.parse().ok()?;
            let m: i64 = digits.get(2..4).map_or(Some(0), |d| d.parse().ok())?;
            Some(naive - chrono::Duration::minutes(sign * (h * 60 + m)))
        }
        None if !has_time => Some(naive),
        None => {
            use chrono::TimeZone;
            let local = chrono::Local.from_local_datetime(&naive).earliest()?;
            Some(local.naive_utc())
        }
    }
}

/// JS `String(value)` (JSON değerleri için).
pub fn js_string(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => js_number_string(n.as_f64().unwrap_or(0.0)),
        Value::String(s) => s.clone(),
        Value::Array(items) => items.iter().map(|v| if v.is_null() { String::new() } else { js_string(v) }).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".into(),
    }
}

pub fn js_number_string(f: f64) -> String {
    if f.fract() == 0.0 && f.abs() < 1e21 { format!("{}", f as i128) } else { format!("{f}") }
}

/// JS doğruluk değeri.
pub fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

/// TS'te null bir alanın `.trim()` çağrısı gibi çöken yerler: NestJS'te 500.
pub fn crash(what: &str) -> ApiError {
    ApiError::internal(format!("TypeError: {what}"))
}

/// `toLocaleLowerCase('tr-TR')`
fn tr_lower(value: &str) -> String {
    value
        .chars()
        .flat_map(|c| match c {
            'I' => vec!['ı'],
            'İ' => vec!['i'],
            other => other.to_lowercase().collect(),
        })
        .collect()
}

/// Etiketler virgülle ayrılmış tek dizge olarak saklanır (≤10, her biri ≤24).
pub fn normalize_labels(raw: Option<&Value>) -> String {
    let Some(Value::Array(items)) = raw else { return String::new() };
    let mut out: Vec<String> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for item in items {
        let text = if item.is_null() { String::new() } else { js_string(item) };
        let mut replaced = String::new();
        let mut in_run = false;
        for c in js_trim(&text).chars() {
            if matches!(c, ',' | '\r' | '\n') {
                if !in_run {
                    replaced.push(' ');
                }
                in_run = true;
            } else {
                replaced.push(c);
                in_run = false;
            }
        }
        let label = js_slice(&collapse_js_space(&replaced), 24);
        let key = tr_lower(&label);
        if label.is_empty() || seen.contains(&key) {
            continue;
        }
        seen.push(key);
        out.push(label);
        if out.len() >= 10 {
            break;
        }
    }
    out.join(",")
}

/// projects.ts `groupReactions`: emoji başına sayı ve "ben de" bayrağı, ilk görülme sırasıyla.
pub fn group_reactions(rows: &[(String, String)], user_id: &str) -> Value {
    let mut order: Vec<String> = Vec::new();
    let mut map: std::collections::HashMap<String, (i64, bool)> = Default::default();
    for (emoji, user) in rows {
        let entry = map.entry(emoji.clone()).or_insert_with(|| {
            order.push(emoji.clone());
            (0, false)
        });
        entry.0 += 1;
        if user == user_id {
            entry.1 = true;
        }
    }
    Value::Array(order.into_iter().map(|e| { let (count, reacted) = map[&e]; json!({ "emoji": e, "count": count, "reacted": reacted }) }).collect())
}

fn js_now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

// ------------------------------------------------------------------ JSON biçimleri

const TASK_JSON: &str = concat!(
    r#"json_build_object('id', t.id, 'number', t.number, 'title', t.title, 'description', t.description, 'status', t.status, 'priority', t.priority,
    'kind', t.kind, 'labels', t.labels, 'points', t.points, 'position', t.position, 'assigneeId', t."assigneeId", 'createdById', t."createdById",
    'sprintId', t."sprintId", 'projectId', t."projectId", 'boardId', t."boardId", 'dueAt', "#,
    crate::iso!(r#"t."dueAt""#),
    ", 'createdAt', ",
    crate::iso!(r#"t."createdAt""#),
    ", 'updatedAt', ",
    crate::iso!(r#"t."updatedAt""#),
    ")"
);

const BOARD_JSON: &str = concat!(
    r#"json_build_object('id', b.id, 'name', b.name, 'position', b.position, 'projectId', b."projectId", 'createdById', b."createdById", 'createdAt', "#,
    crate::iso!(r#"b."createdAt""#),
    ")"
);

const SPRINT_JSON: &str = concat!(
    r#"json_build_object('id', s.id, 'organizationId', s."organizationId", 'projectId', s."projectId", 'boardId', s."boardId", 'name', s.name, 'goal', s.goal,
    'status', s.status, 'startsAt', "#,
    crate::iso!(r#"s."startsAt""#),
    ", 'endsAt', ",
    crate::iso!(r#"s."endsAt""#),
    r#", 'createdById', s."createdById", 'createdAt', "#,
    crate::iso!(r#"s."createdAt""#),
    ", 'updatedAt', ",
    crate::iso!(r#"s."updatedAt""#),
    ")"
);

const COMMENT_JSON: &str = concat!(
    r#"json_build_object('id', c.id, 'taskId', c."taskId", 'authorId', c."authorId", 'content', c.content, 'createdAt', "#,
    crate::iso!(r#"c."createdAt""#),
    ", 'updatedAt', ",
    crate::iso!(r#"c."updatedAt""#),
    ")"
);

// ------------------------------------------------------------------ erişim yardımcıları

async fn member(db: &sqlx::PgPool, organization_id: &str, user_id: &str) -> ApiResult<()> {
    let hit: Option<String> = sqlx::query_scalar(r#"select "userId" from "OrganizationMember" where "organizationId" = $1 and "userId" = $2"#)
        .bind(organization_id)
        .bind(user_id)
        .fetch_optional(db)
        .await?;
    hit.map(|_| ()).ok_or_else(|| not_found("NOT_MEMBER", "Bu kuruluşun üyesi değilsin."))
}

/// Pano projeye ait: adresteki proje gerçekten bu kuruluşun mu?
async fn project(db: &sqlx::PgPool, organization_id: &str, project_id: &str, user_id: &str) -> ApiResult<()> {
    member(db, organization_id, user_id).await?;
    perm::require_project_access(db, organization_id, user_id, project_id).await?;
    let hit: Option<String> = sqlx::query_scalar(r#"select id from "Project" where id = $1 and "organizationId" = $2 and "deletedAt" is null"#)
        .bind(project_id)
        .bind(organization_id)
        .fetch_optional(db)
        .await?;
    hit.map(|_| ()).ok_or_else(|| not_found("PROJECT_NOT_FOUND", "Proje bulunamadı."))
}

/// Adresteki pano gerçekten bu projenin mi?
async fn board(db: &sqlx::PgPool, organization_id: &str, project_id: &str, board_id: &str, user_id: &str) -> ApiResult<Value> {
    project(db, organization_id, project_id, user_id).await?;
    let row: Option<Json<Value>> = sqlx::query_scalar(sql(format!(r#"select {BOARD_JSON} from "Board" b where b.id = $1 and b."projectId" = $2"#)))
        .bind(board_id)
        .bind(project_id)
        .fetch_optional(db)
        .await?;
    row.map(|j| j.0).ok_or_else(|| not_found("BOARD_NOT_FOUND", "Pano bulunamadı."))
}

/// Görev yalnızca kendi panosunun sprint'ine bağlanabilir.
async fn sprint_of_board(db: &sqlx::PgPool, board_id: &str, sprint_id: &str) -> ApiResult<()> {
    let hit: Option<String> = sqlx::query_scalar(r#"select id from "Sprint" where id = $1 and "boardId" = $2"#)
        .bind(sprint_id)
        .bind(board_id)
        .fetch_optional(db)
        .await?;
    hit.map(|_| ()).ok_or_else(|| ApiError::bad_request("SPRINT_NOT_IN_BOARD", "Sprint bu panoya ait değil."))
}

/// Kendi görevini herkes düzenleyip taşıyabilir; başkasınınkine board.manage gerekir.
async fn can_edit(db: &sqlx::PgPool, project_id: &str, user_id: &str, created_by: &str, assignee: Option<&str>) -> ApiResult<bool> {
    if created_by == user_id || assignee == Some(user_id) {
        return Ok(true);
    }
    Ok(perm::effective_in(db, project_id, user_id).await?.has("board.manage"))
}

/// Etiketlenen kişilerden projeyi görebilenler.
async fn visible_targets(db: &sqlx::PgPool, organization_id: &str, project_id: &str, ids: Vec<String>) -> ApiResult<Vec<String>> {
    let mut targets = Vec::new();
    for id in ids {
        if perm::can_view_project(db, organization_id, &id, project_id).await? {
            targets.push(id);
        }
    }
    Ok(targets)
}

fn str_field(task: &Value, key: &str) -> Option<String> {
    task.get(key).and_then(Value::as_str).map(str::to_string)
}

// ------------------------------------------------------------------ panolar

/// Projedeki panolar. Hiç yoksa ilk giren için "Genel" panosu açılır.
async fn boards(State(s): State<AppState>, me: Me, Path((org, project_id)): Path<(String, String)>) -> Reply {
    project(&s.db, &org, &project_id, &me.id).await?;
    let rows: Vec<Json<Value>> = sqlx::query_scalar(sql(format!(
        r#"select {BOARD_JSON} from "Board" b where b."projectId" = $1 order by b.position asc, b."createdAt" asc"#
    )))
    .bind(&project_id)
    .fetch_all(&s.db)
    .await?;
    if !rows.is_empty() {
        return ok(Value::Array(rows.into_iter().map(|j| j.0).collect()));
    }
    let first = insert_board(&s, &org, &project_id, "Genel", 0, &me.id).await?;
    crate::realtime::emit_project(&s, &org, &project_id, "board:updated", &json!({ "board": first })).await;
    ok(json!([first]))
}

async fn insert_board(s: &AppState, org: &str, project_id: &str, name: &str, position: i32, user: &str) -> ApiResult<Value> {
    let now = crate::ctx::now();
    let row: Json<Value> = sqlx::query_scalar(sql(format!(
        r#"with b as (insert into "Board" (id, "organizationId", "projectId", name, position, "createdById", "createdAt", "updatedAt")
           values ($1, $2, $3, $4, $5, $6, $7, $7) returning *) select {BOARD_JSON} from b b"#
    )))
    .bind(uuid())
    .bind(org)
    .bind(project_id)
    .bind(name)
    .bind(position)
    .bind(user)
    .bind(now)
    .fetch_one(&s.db)
    .await?;
    Ok(row.0)
}

fn check_board(body: &Map<String, Value>) -> ApiResult<()> {
    let mut c = Checker::new(body);
    c.field("name", &[(str_len(body.get("name"), 2, 60), None)]);
    c.finish()
}

async fn create_board(State(s): State<AppState>, me: Me, Path((org, project_id)): Path<(String, String)>, Body(body): Body) -> Reply {
    check_board(&body)?;
    project(&s.db, &org, &project_id, &me.id).await?;
    perm::require_in(&s.db, &project_id, &me.id, "board.manage").await?;
    let last: Option<i32> = sqlx::query_scalar(r#"select position from "Board" where "projectId" = $1 order by position desc limit 1"#)
        .bind(&project_id)
        .fetch_optional(&s.db)
        .await?;
    let name = js_trim(body["name"].as_str().unwrap_or_default()).to_string();
    let board = insert_board(&s, &org, &project_id, &name, last.unwrap_or(-1) + 1, &me.id).await?;
    crate::realtime::emit_project(&s, &org, &project_id, "board:updated", &json!({ "board": board })).await;
    created(board)
}

async fn rename_board(State(s): State<AppState>, me: Me, Path((org, project_id, board_id)): Path<(String, String, String)>, Body(body): Body) -> Reply {
    check_board(&body)?;
    board(&s.db, &org, &project_id, &board_id, &me.id).await?;
    perm::require_in(&s.db, &project_id, &me.id, "board.manage").await?;
    let name = js_trim(body["name"].as_str().unwrap_or_default()).to_string();
    let row: Json<Value> = sqlx::query_scalar(sql(format!(
        r#"with b as (update "Board" set name = $2, "updatedAt" = $3 where id = $1 returning *) select {BOARD_JSON} from b b"#
    )))
    .bind(&board_id)
    .bind(&name)
    .bind(crate::ctx::now())
    .fetch_one(&s.db)
    .await?;
    crate::realtime::emit_project(&s, &org, &project_id, "board:updated", &json!({ "board": row.0 })).await;
    ok(row.0)
}

/// Pano silinince sprint'leri ve görevleri de gider; projenin son panosu silinemez.
async fn delete_board(State(s): State<AppState>, me: Me, Path((org, project_id, board_id)): Path<(String, String, String)>) -> Reply {
    board(&s.db, &org, &project_id, &board_id, &me.id).await?;
    perm::require_in(&s.db, &project_id, &me.id, "board.manage").await?;
    let total: i64 = sqlx::query_scalar(r#"select count(*) from "Board" where "projectId" = $1"#).bind(&project_id).fetch_one(&s.db).await?;
    if total < 2 {
        return Err(ApiError::bad_request("LAST_BOARD", "Projenin son panosu silinemez."));
    }
    sqlx::query(r#"delete from "Board" where id = $1"#).bind(&board_id).execute(&s.db).await?;
    crate::realtime::emit_project(&s, &org, &project_id, "board:deleted", &json!({ "boardId": board_id, "projectId": project_id })).await;
    ok(json!({ "ok": true }))
}

async fn content(State(s): State<AppState>, me: Me, Path((org, project_id, board_id)): Path<(String, String, String)>) -> Reply {
    let board = board(&s.db, &org, &project_id, &board_id, &me.id).await?;
    let sprints: Vec<Json<Value>> = sqlx::query_scalar(sql(format!(
        r#"select {SPRINT_JSON} from "Sprint" s where s."boardId" = $1 order by s.status asc, s."createdAt" desc"#
    )))
    .bind(&board_id)
    .fetch_all(&s.db)
    .await?;
    let tasks: Vec<Json<Value>> = sqlx::query_scalar(sql(format!(
        r#"select {TASK_JSON} from "Task" t where t."boardId" = $1 order by t.position asc, t."createdAt" asc"#
    )))
    .bind(&board_id)
    .fetch_all(&s.db)
    .await?;
    ok(json!({
        "board": board,
        "sprints": sprints.into_iter().map(|j| j.0).collect::<Vec<_>>(),
        "tasks": tasks.into_iter().map(|j| j.0).collect::<Vec<_>>(),
    }))
}

// ------------------------------------------------------------------ sprint'ler

fn date_or_crash(v: &Value) -> ApiResult<NaiveDateTime> {
    match v {
        // `new Date(null)` → 1970-01-01
        Value::Null => Ok(chrono::DateTime::from_timestamp_millis(0).unwrap().naive_utc()),
        Value::String(text) => js_date(text).ok_or_else(|| crash("Invalid Date")),
        _ => Err(crash("Invalid Date")),
    }
}

async fn create_sprint(State(s): State<AppState>, me: Me, Path((org, project_id, board_id)): Path<(String, String, String)>, Body(body): Body) -> Reply {
    let mut c = Checker::new(&body);
    c.field("name", &[(str_len(body.get("name"), 2, 60), None)]);
    c.field("goal", &[(absent(body.get("goal")) || str_len(body.get("goal"), 0, 240), None)]);
    c.field("startsAt", &[(absent(body.get("startsAt")) || is_iso8601(body.get("startsAt")), None)]);
    c.field("endsAt", &[(absent(body.get("endsAt")) || is_iso8601(body.get("endsAt")), None)]);
    c.finish()?;
    board(&s.db, &org, &project_id, &board_id, &me.id).await?;
    perm::require_in(&s.db, &project_id, &me.id, "board.manage").await?;
    let name = js_trim(body["name"].as_str().unwrap_or_default()).to_string();
    let goal = body.get("goal").and_then(Value::as_str).map(js_trim).filter(|g| !g.is_empty()).map(str::to_string);
    let date = |key: &str| -> ApiResult<Option<NaiveDateTime>> {
        if truthy(body.get(key)) { date_or_crash(&body[key]).map(Some) } else { Ok(None) }
    };
    let (starts, ends) = (date("startsAt")?, date("endsAt")?);
    let now = crate::ctx::now();
    let sprint: Json<Value> = sqlx::query_scalar(sql(format!(
        r#"with s as (insert into "Sprint" (id, "organizationId", "projectId", "boardId", name, goal, "startsAt", "endsAt", "createdById", "createdAt", "updatedAt")
           values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10) returning *) select {SPRINT_JSON} from s s"#
    )))
    .bind(uuid())
    .bind(&org)
    .bind(&project_id)
    .bind(&board_id)
    .bind(&name)
    .bind(&goal)
    .bind(starts)
    .bind(ends)
    .bind(&me.id)
    .bind(now)
    .fetch_one(&s.db)
    .await?;
    crate::realtime::emit_project(&s, &org, &project_id, "sprint:updated", &json!({ "sprint": sprint.0 })).await;
    created(sprint.0)
}

async fn update_sprint(State(s): State<AppState>, me: Me, Path((org, project_id, board_id, sprint_id)): Path<(String, String, String, String)>, Body(body): Body) -> Reply {
    let mut c = Checker::new(&body);
    c.field("name", &[(absent(body.get("name")) || str_len(body.get("name"), 2, 60), None)]);
    c.field("goal", &[(absent(body.get("goal")) || str_len(body.get("goal"), 0, 240), None)]);
    c.field("status", &[(absent(body.get("status")) || is_in(body.get("status"), &SPRINT_STATUSES), None)]);
    c.field("startsAt", &[(absent(body.get("startsAt")) || is_iso8601(body.get("startsAt")), None)]);
    c.field("endsAt", &[(absent(body.get("endsAt")) || is_iso8601(body.get("endsAt")), None)]);
    c.finish()?;
    board(&s.db, &org, &project_id, &board_id, &me.id).await?;
    perm::require_in(&s.db, &project_id, &me.id, "board.manage").await?;
    let current: Option<Option<NaiveDateTime>> = sqlx::query_scalar(r#"select "startsAt" from "Sprint" where id = $1 and "boardId" = $2"#)
        .bind(&sprint_id)
        .bind(&board_id)
        .fetch_optional(&s.db)
        .await?;
    let Some(current_starts) = current else { return Err(not_found("SPRINT_NOT_FOUND", "Sprint bulunamadı.")) };
    let status = body.get("status");
    let activating = status.and_then(Value::as_str) == Some("ACTIVE");
    let now = crate::ctx::now();
    // Prisma verisi önce hazırlanır: null alanlar TS'te `.trim()` ya da zorunlu sütun yüzünden düşer.
    let name = match body.get("name") {
        None => None,
        Some(Value::String(n)) => Some(js_trim(n).to_string()),
        Some(_) => return Err(crash("name.trim")),
    };
    let goal = match body.get("goal") {
        None => None,
        Some(Value::String(g)) => Some(Some(js_trim(g).to_string()).filter(|g| !g.is_empty())),
        Some(_) => return Err(crash("goal.trim")),
    };
    let status_value = match status {
        None => None,
        Some(Value::String(v)) => Some(v.clone()),
        Some(_) => return Err(crash("status null")),
    };
    let mut starts = match body.get("startsAt") {
        None => None,
        Some(v) => Some(date_or_crash(v)?),
    };
    let ends = match body.get("endsAt") {
        None => None,
        Some(v) => Some(date_or_crash(v)?),
    };
    if activating && current_starts.is_none() {
        starts = Some(now);
    }
    // Aynı panoda aynı anda tek sprint aktif olur: yenisi başlarken öncekiler planlanana döner.
    if activating {
        sqlx::query(r#"update "Sprint" set status = 'PLANNED', "updatedAt" = $3 where "boardId" = $1 and status = 'ACTIVE' and id <> $2"#)
            .bind(&board_id)
            .bind(&sprint_id)
            .bind(now)
            .execute(&s.db)
            .await?;
    }
    let sprint: Json<Value> = sqlx::query_scalar(sql(format!(
        r#"with s as (update "Sprint" set
             name = case when $2 then $3 else name end,
             goal = case when $4 then $5 else goal end,
             status = case when $6 then $7 else status end,
             "startsAt" = case when $8 then $9 else "startsAt" end,
             "endsAt" = case when $10 then $11 else "endsAt" end,
             "updatedAt" = $12
           where id = $1 returning *) select {SPRINT_JSON} from s s"#
    )))
    .bind(&sprint_id)
    .bind(name.is_some())
    .bind(name)
    .bind(goal.is_some())
    .bind(goal.flatten())
    .bind(status_value.is_some())
    .bind(status_value)
    .bind(starts.is_some())
    .bind(starts)
    .bind(ends.is_some())
    .bind(ends)
    .bind(now)
    .fetch_one(&s.db)
    .await?;
    crate::realtime::emit_project(&s, &org, &project_id, "sprint:updated", &json!({ "sprint": sprint.0, "reload": activating })).await;
    ok(sprint.0)
}

async fn delete_sprint(State(s): State<AppState>, me: Me, Path((org, project_id, board_id, sprint_id)): Path<(String, String, String, String)>) -> Reply {
    board(&s.db, &org, &project_id, &board_id, &me.id).await?;
    perm::require_in(&s.db, &project_id, &me.id, "board.manage").await?;
    let hit: Option<String> = sqlx::query_scalar(r#"select id from "Sprint" where id = $1 and "boardId" = $2"#)
        .bind(&sprint_id)
        .bind(&board_id)
        .fetch_optional(&s.db)
        .await?;
    if hit.is_none() {
        return Err(not_found("SPRINT_NOT_FOUND", "Sprint bulunamadı."));
    }
    // Görevler silinmez, backlog'a düşer.
    sqlx::query(r#"update "Task" set "sprintId" = null, "updatedAt" = $2 where "sprintId" = $1"#)
        .bind(&sprint_id)
        .bind(crate::ctx::now())
        .execute(&s.db)
        .await?;
    sqlx::query(r#"delete from "Sprint" where id = $1"#).bind(&sprint_id).execute(&s.db).await?;
    crate::realtime::emit_project(&s, &org, &project_id, "sprint:deleted", &json!({ "sprintId": sprint_id, "projectId": project_id, "boardId": board_id })).await;
    ok(json!({ "ok": true }))
}

// ------------------------------------------------------------------ görevler

/// CreateTaskDto / UpdateTaskDto. UpdateTaskDto'da class-validator önce alt sınıfın kendi
/// alanlarını (title, position), sonra kalıtılanları sıralar.
fn check_task(body: &Map<String, Value>, update: bool) -> ApiResult<()> {
    let mut c = Checker::new(body);
    let g = |k: &str| body.get(k);
    let opt = |k: &str, ok: bool| absent(g(k)) || ok;
    if update {
        c.field("title", &[(opt("title", str_len(g("title"), 2, 160)), None)]);
        c.field("position", &[(opt("position", is_int(g("position"))), None)]);
    } else {
        c.field("title", &[(str_len(g("title"), 2, 160), None)]);
    }
    c.field("description", &[(opt("description", str_len(g("description"), 0, 12000)), None)]);
    c.field("status", &[(opt("status", is_in(g("status"), &TASK_STATUSES)), None)]);
    c.field("priority", &[(opt("priority", is_in(g("priority"), &TASK_PRIORITIES)), None)]);
    c.field("kind", &[(opt("kind", is_in(g("kind"), &TASK_KINDS)), None)]);
    let labels_ok = matches!(g("labels"), Some(Value::Array(items)) if items.iter().all(Value::is_string));
    c.field("labels", &[(opt("labels", labels_ok), None)]);
    c.field("assigneeId", &[(opt("assigneeId", g("assigneeId").is_some_and(Value::is_string)), None)]);
    c.field("sprintId", &[(opt("sprintId", g("sprintId").is_some_and(Value::is_string)), None)]);
    let points = g("points").and_then(Value::as_f64);
    c.field("points", &[(opt("points", is_int(g("points")) && points.is_some_and(|p| (0.0..=100.0).contains(&p))), None)]);
    c.field("dueAt", &[(opt("dueAt", is_iso8601(g("dueAt"))), None)]);
    c.finish()
}

fn int32(v: &Value) -> ApiResult<i32> {
    v.as_f64().filter(|f| *f >= i32::MIN as f64 && *f <= i32::MAX as f64).map(|f| f as i32).ok_or_else(|| crash("int overflow"))
}

async fn create_task(State(s): State<AppState>, me: Me, Path((org, project_id, board_id)): Path<(String, String, String)>, Body(body): Body) -> Reply {
    check_task(&body, false)?;
    board(&s.db, &org, &project_id, &board_id, &me.id).await?;
    perm::require_in(&s.db, &project_id, &me.id, "task.create").await?;
    let text = |k: &str| body.get(k).and_then(Value::as_str).filter(|v| !v.is_empty()).map(str::to_string);
    let assignee = text("assigneeId");
    let sprint = text("sprintId");
    if let Some(a) = &assignee {
        member(&s.db, &org, a).await?;
    }
    if let Some(sp) = &sprint {
        sprint_of_board(&s.db, &board_id, sp).await?;
    }
    // Görev numarası proje içinde sıralı: tüm panolar tek numara dizisini paylaşır.
    let last: Option<i32> = sqlx::query_scalar(r#"select number from "Task" where "projectId" = $1 order by number desc limit 1"#)
        .bind(&project_id)
        .fetch_optional(&s.db)
        .await?;
    let status = text("status").unwrap_or_else(|| "TODO".into());
    let first: Option<i32> = sqlx::query_scalar(r#"select position from "Task" where "boardId" = $1 and status = $2 order by position asc limit 1"#)
        .bind(&board_id)
        .bind(&status)
        .fetch_optional(&s.db)
        .await?;
    let description = body.get("description").and_then(Value::as_str).map(js_trim).filter(|d| !d.is_empty()).map(str::to_string);
    let points = match body.get("points") {
        None | Some(Value::Null) => None,
        Some(v) => Some(int32(v)?),
    };
    let due = if truthy(body.get("dueAt")) { Some(date_or_crash(&body["dueAt"])?) } else { None };
    let now = crate::ctx::now();
    let task: Json<Value> = sqlx::query_scalar(sql(format!(
        r#"with t as (insert into "Task" (id, "organizationId", "projectId", "boardId", number, title, description, status, priority, kind, labels,
             "assigneeId", "sprintId", points, "dueAt", position, "createdById", "createdAt", "updatedAt")
           values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $18) returning *) select {TASK_JSON} from t t"#
    )))
    .bind(uuid())
    .bind(&org)
    .bind(&project_id)
    .bind(&board_id)
    .bind(last.filter(|n| *n != 0).unwrap_or(0) + 1)
    .bind(js_trim(body["title"].as_str().unwrap_or_default()))
    .bind(&description)
    .bind(&status)
    .bind(text("priority").unwrap_or_else(|| "MEDIUM".into()))
    .bind(text("kind").unwrap_or_else(|| "TASK".into()))
    .bind(normalize_labels(body.get("labels")))
    .bind(&assignee)
    .bind(&sprint)
    .bind(points)
    .bind(due)
    .bind(first.unwrap_or(0) - 1)
    .bind(&me.id)
    .bind(now)
    .fetch_one(&s.db)
    .await?;
    let task = task.0;
    let task_id = str_field(&task, "id").unwrap_or_default();
    let title = str_field(&task, "title").unwrap_or_default();
    if let Some(desc) = str_field(&task, "description") {
        let mentioned = activity::mentioned_user_ids(&s.db, &org, &project_id, &desc).await?;
        let targets = visible_targets(&s.db, &org, &project_id, mentioned).await?;
        let preview = if desc.is_empty() { title.clone() } else { desc.clone() };
        activity::record(&s, mention_rows(&org, &me.id, targets, &preview, &project_id, &board_id, &task_id, None)).await;
    }
    // Görev başkasına atandıysa atanan kişiye "sana bir görev atadı" etkinliği.
    if let Some(a) = str_field(&task, "assigneeId").filter(|a| *a != me.id) {
        activity::record(&s, vec![assigned_row(&org, &me.id, &a, &title, &project_id, &board_id, &task_id)]).await;
    }
    crate::realtime::emit_project(&s, &org, &project_id, "task:updated", &json!({ "task": task })).await;
    created(task)
}

#[allow(clippy::too_many_arguments)]
fn mention_rows(org: &str, actor: &str, targets: Vec<String>, preview: &str, project_id: &str, board_id: &str, task_id: &str, message_id: Option<&str>) -> Vec<ActivityInput> {
    targets
        .into_iter()
        .map(|user_id| ActivityInput {
            organization_id: org.into(),
            user_id,
            actor_id: actor.into(),
            kind: "MENTION",
            preview: preview.into(),
            project_id: Some(project_id.into()),
            board_id: Some(board_id.into()),
            task_id: Some(task_id.into()),
            message_id: message_id.map(str::to_string),
            ..Default::default()
        })
        .collect()
}

fn assigned_row(org: &str, actor: &str, user: &str, title: &str, project_id: &str, board_id: &str, task_id: &str) -> ActivityInput {
    ActivityInput {
        organization_id: org.into(),
        user_id: user.into(),
        actor_id: actor.into(),
        kind: "ASSIGNED",
        preview: title.into(),
        project_id: Some(project_id.into()),
        board_id: Some(board_id.into()),
        task_id: Some(task_id.into()),
        ..Default::default()
    }
}

async fn task_owner(db: &sqlx::PgPool, task_id: &str, board_id: &str) -> ApiResult<(String, Option<String>, Option<String>)> {
    let row: Option<(String, Option<String>, Option<String>)> =
        sqlx::query_as(r#"select "createdById", "assigneeId", description from "Task" where id = $1 and "boardId" = $2"#)
            .bind(task_id)
            .bind(board_id)
            .fetch_optional(db)
            .await?;
    row.ok_or_else(|| not_found("TASK_NOT_FOUND", "Görev bulunamadı."))
}

async fn update_task(State(s): State<AppState>, me: Me, Path((org, project_id, board_id, task_id)): Path<(String, String, String, String)>, Body(body): Body) -> Reply {
    check_task(&body, true)?;
    board(&s.db, &org, &project_id, &board_id, &me.id).await?;
    if let Some(sp) = body.get("sprintId").and_then(Value::as_str).filter(|v| !v.is_empty()) {
        sprint_of_board(&s.db, &board_id, sp).await?;
    }
    let (created_by, current_assignee, current_description) = task_owner(&s.db, &task_id, &board_id).await?;
    if !can_edit(&s.db, &project_id, &me.id, &created_by, current_assignee.as_deref()).await? {
        return Err(forbidden("NO_PERMISSION", "Bu görevi düzenleme yetkin yok."));
    }
    if let Some(a) = body.get("assigneeId").and_then(Value::as_str).filter(|v| !v.is_empty()) {
        member(&s.db, &org, a).await?;
    }
    // Prisma verisi: null alanların TS'teki davranışı (trim çöker, zorunlu sütun reddedilir).
    let text_or_crash = |key: &str| -> ApiResult<Option<String>> {
        match body.get(key) {
            None => Ok(None),
            Some(Value::String(v)) => Ok(Some(v.clone())),
            Some(_) => Err(crash(key)),
        }
    };
    let title = text_or_crash("title")?.map(|t| js_trim(&t).to_string());
    let description = text_or_crash("description")?.map(|d| Some(js_trim(&d).to_string()).filter(|d| !d.is_empty()));
    let status = text_or_crash("status")?;
    let priority = text_or_crash("priority")?;
    let kind = text_or_crash("kind")?;
    let labels = body.get("labels").map(|v| normalize_labels(Some(v)));
    let points = match body.get("points") {
        None => None,
        Some(Value::Null) => Some(None),
        Some(v) => Some(Some(int32(v)?)),
    };
    let position = match body.get("position") {
        None => None,
        Some(Value::Null) => return Err(crash("position null")),
        Some(v) => Some(int32(v)?),
    };
    let due = match body.get("dueAt") {
        None => None,
        Some(v) if truthy(Some(v)) => Some(Some(date_or_crash(v)?)),
        Some(_) => Some(None),
    };
    // Boş dizge "kimseye atanmadı" / "backlog" anlamına gelir.
    let nullable = |key: &str| body.get(key).map(|v| v.as_str().filter(|s| !s.is_empty()).map(str::to_string));
    let assignee = nullable("assigneeId");
    let sprint = nullable("sprintId");
    let task: Json<Value> = sqlx::query_scalar(sql(format!(
        r#"with t as (update "Task" set
             title = case when $2 then $3 else title end,
             description = case when $4 then $5 else description end,
             status = case when $6 then $7 else status end,
             priority = case when $8 then $9 else priority end,
             kind = case when $10 then $11 else kind end,
             labels = case when $12 then $13 else labels end,
             points = case when $14 then $15 else points end,
             position = case when $16 then $17 else position end,
             "dueAt" = case when $18 then $19 else "dueAt" end,
             "assigneeId" = case when $20 then $21 else "assigneeId" end,
             "sprintId" = case when $22 then $23 else "sprintId" end,
             "updatedAt" = $24
           where id = $1 returning *) select {TASK_JSON} from t t"#
    )))
    .bind(&task_id)
    .bind(title.is_some())
    .bind(title)
    .bind(description.is_some())
    .bind(description.clone().flatten())
    .bind(status.is_some())
    .bind(status)
    .bind(priority.is_some())
    .bind(priority)
    .bind(kind.is_some())
    .bind(kind)
    .bind(labels.is_some())
    .bind(labels)
    .bind(points.is_some())
    .bind(points.flatten())
    .bind(position.is_some())
    .bind(position)
    .bind(due.is_some())
    .bind(due.flatten())
    .bind(assignee.is_some())
    .bind(assignee.clone().flatten())
    .bind(sprint.is_some())
    .bind(sprint.flatten())
    .bind(crate::ctx::now())
    .fetch_one(&s.db)
    .await?;
    let task = task.0;
    let title = str_field(&task, "title").unwrap_or_default();
    if body.contains_key("description") {
        let new_description = str_field(&task, "description").unwrap_or_default();
        let before = activity::mentioned_user_ids(&s.db, &org, &project_id, current_description.as_deref().unwrap_or("")).await?;
        let after = activity::mentioned_user_ids(&s.db, &org, &project_id, &new_description).await?;
        let fresh: Vec<String> = after.into_iter().filter(|id| !before.contains(id)).collect();
        let targets = visible_targets(&s.db, &org, &project_id, fresh).await?;
        let preview = if new_description.is_empty() { title.clone() } else { new_description };
        activity::record(&s, mention_rows(&org, &me.id, targets, &preview, &project_id, &board_id, &task_id, None)).await;
    }
    // Atanan kişi değiştiyse ve yeni atanan aktör değilse: "sana bir görev atadı".
    if assignee.is_some()
        && let Some(a) = str_field(&task, "assigneeId").filter(|a| Some(a) != current_assignee.as_ref() && *a != me.id)
    {
        activity::record(&s, vec![assigned_row(&org, &me.id, &a, &title, &project_id, &board_id, &task_id)]).await;
    }
    crate::realtime::emit_project(&s, &org, &project_id, "task:updated", &json!({ "task": task })).await;
    ok(task)
}

async fn delete_task(State(s): State<AppState>, me: Me, Path((org, project_id, board_id, task_id)): Path<(String, String, String, String)>) -> Reply {
    board(&s.db, &org, &project_id, &board_id, &me.id).await?;
    let (created_by, assignee, _) = task_owner(&s.db, &task_id, &board_id).await?;
    if !can_edit(&s.db, &project_id, &me.id, &created_by, assignee.as_deref()).await? {
        return Err(forbidden("NO_PERMISSION", "Bu görevi silme yetkin yok."));
    }
    sqlx::query(r#"delete from "Task" where id = $1"#).bind(&task_id).execute(&s.db).await?;
    crate::realtime::emit_project(&s, &org, &project_id, "task:deleted", &json!({ "taskId": task_id, "projectId": project_id, "boardId": board_id })).await;
    ok(json!({ "ok": true }))
}

/// Sütun içinde sıralama: sürükle bırak sonrası yeni düzen tek istekte yazılır.
async fn move_task(State(s): State<AppState>, me: Me, Path((org, project_id, board_id, task_id)): Path<(String, String, String, String)>, Body(body): Body) -> Reply {
    board(&s.db, &org, &project_id, &board_id, &me.id).await?;
    let (created_by, assignee, _) = task_owner(&s.db, &task_id, &board_id).await?;
    if !can_edit(&s.db, &project_id, &me.id, &created_by, assignee.as_deref()).await? {
        return Err(forbidden("NO_PERMISSION", "Bu görevi taşıma yetkin yok."));
    }
    let status = if truthy(body.get("status")) { js_string(&body["status"]) } else { String::new() };
    if !status.is_empty() && !TASK_STATUSES.contains(&status.as_str()) {
        return Err(ApiError::bad_request("INVALID_STATUS", "Geçersiz sütun."));
    }
    let order: Vec<Value> = match body.get("order") {
        Some(Value::Array(items)) => items.iter().take(500).cloned().collect(),
        _ => Vec::new(),
    };
    // Prisma sorguları kurarken dizgi olmayan kimliği reddeder; hiçbir şey yazılmaz.
    let order: Vec<String> = order.into_iter().map(|v| v.as_str().map(str::to_string).ok_or_else(|| crash("order id"))).collect::<ApiResult<_>>()?;
    let now = crate::ctx::now();
    let mut tx = s.db.begin().await?;
    if !status.is_empty() {
        let n = sqlx::query(r#"update "Task" set status = $2, "updatedAt" = $3 where id = $1"#).bind(&task_id).bind(&status).bind(now).execute(&mut *tx).await?;
        if n.rows_affected() == 0 {
            return Err(crash("task not found"));
        }
    }
    for (index, id) in order.iter().enumerate() {
        sqlx::query(r#"update "Task" set position = $3, "updatedAt" = $4 where id = $1 and "boardId" = $2"#)
            .bind(id)
            .bind(&board_id)
            .bind(index as i32)
            .bind(now)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    let mut ids = vec![task_id.clone()];
    ids.extend(order);
    let tasks: Vec<Json<Value>> = sqlx::query_scalar(sql(format!(r#"select {TASK_JSON} from "Task" t where t."boardId" = $1 and t.id = any($2)"#)))
        .bind(&board_id)
        .bind(&ids)
        .fetch_all(&s.db)
        .await?;
    let tasks: Vec<Value> = tasks.into_iter().map(|j| j.0).collect();
    crate::realtime::emit_project(&s, &org, &project_id, "task:moved", &json!({ "tasks": tasks, "projectId": project_id, "boardId": board_id })).await;
    ok(json!({ "ok": true, "tasks": tasks }))
}

// ------------------------------------------------------------------ görev yorumları

async fn task_exists(db: &sqlx::PgPool, task_id: &str, board_id: &str) -> ApiResult<()> {
    let hit: Option<String> = sqlx::query_scalar(r#"select id from "Task" where id = $1 and "boardId" = $2"#)
        .bind(task_id)
        .bind(board_id)
        .fetch_optional(db)
        .await?;
    hit.map(|_| ()).ok_or_else(|| not_found("TASK_NOT_FOUND", "Görev bulunamadı."))
}

async fn comments(State(s): State<AppState>, me: Me, Path((org, project_id, board_id, task_id)): Path<(String, String, String, String)>) -> Reply {
    board(&s.db, &org, &project_id, &board_id, &me.id).await?;
    task_exists(&s.db, &task_id, &board_id).await?;
    let rows: Vec<Json<Value>> = sqlx::query_scalar(sql(format!(r#"select {COMMENT_JSON} from "TaskComment" c where c."taskId" = $1 order by c."createdAt" asc"#)))
        .bind(&task_id)
        .fetch_all(&s.db)
        .await?;
    let reactions: Vec<(String, String, String)> = sqlx::query_as(
        r#"select r."commentId", r.emoji, r."userId" from "TaskCommentReaction" r join "TaskComment" c on c.id = r."commentId" where c."taskId" = $1 order by r."createdAt" asc"#,
    )
    .bind(&task_id)
    .fetch_all(&s.db)
    .await?;
    let out: Vec<Value> = rows
        .into_iter()
        .map(|j| {
            let mut row = j.0;
            let id = row["id"].as_str().unwrap_or_default().to_string();
            let mine: Vec<(String, String)> = reactions.iter().filter(|r| r.0 == id).map(|r| (r.1.clone(), r.2.clone())).collect();
            row.as_object_mut().unwrap().insert("reactions".into(), group_reactions(&mine, &me.id));
            row
        })
        .collect();
    ok(Value::Array(out))
}

fn check_comment(body: &Map<String, Value>) -> ApiResult<()> {
    let mut c = Checker::new(body);
    c.field("content", &[(str_len(body.get("content"), 1, 12000), None)]);
    c.finish()
}

/// Panoyu görebilen herkes yorum yazabilir.
async fn add_comment(State(s): State<AppState>, me: Me, Path((org, project_id, board_id, task_id)): Path<(String, String, String, String)>, Body(body): Body) -> Reply {
    check_comment(&body)?;
    board(&s.db, &org, &project_id, &board_id, &me.id).await?;
    task_exists(&s.db, &task_id, &board_id).await?;
    let content = body["content"].as_str().unwrap_or_default().to_string();
    let now = crate::ctx::now();
    let comment: Json<Value> = sqlx::query_scalar(sql(format!(
        r#"with c as (insert into "TaskComment" (id, "taskId", "authorId", content, "createdAt", "updatedAt") values ($1, $2, $3, $4, $5, $5) returning *)
           select {COMMENT_JSON} from c c"#
    )))
    .bind(uuid())
    .bind(&task_id)
    .bind(&me.id)
    .bind(js_trim(&content))
    .bind(now)
    .fetch_one(&s.db)
    .await?;
    let comment = comment.0;
    let comment_id = str_field(&comment, "id").unwrap_or_default();
    let mentioned = activity::mentioned_user_ids(&s.db, &org, &project_id, &content).await?;
    let targets = visible_targets(&s.db, &org, &project_id, mentioned).await?;
    activity::record(&s, mention_rows(&org, &me.id, targets, &content, &project_id, &board_id, &task_id, Some(&comment_id))).await;
    crate::realtime::emit_project(&s, &org, &project_id, "task:comment", &json!({ "comment": comment, "projectId": project_id, "boardId": board_id })).await;
    created(comment)
}

/// Yorum tepkileri: aynı emoji ikinci kez gönderilince geri alınır.
async fn react_comment(
    State(s): State<AppState>,
    me: Me,
    Path((org, project_id, board_id, task_id, comment_id)): Path<(String, String, String, String, String)>,
    Body(body): Body,
) -> Reply {
    let mut c = Checker::new(&body);
    c.field("emoji", &[(str_len(body.get("emoji"), 1, 24), None)]);
    c.finish()?;
    board(&s.db, &org, &project_id, &board_id, &me.id).await?;
    let hit: Option<String> = sqlx::query_scalar(r#"select id from "TaskComment" where id = $1 and "taskId" = $2"#)
        .bind(&comment_id)
        .bind(&task_id)
        .fetch_optional(&s.db)
        .await?;
    if hit.is_none() {
        return Err(not_found("COMMENT_NOT_FOUND", "Yorum bulunamadı."));
    }
    let emoji = js_trim(body["emoji"].as_str().unwrap_or_default()).to_string();
    let existing: Option<String> = sqlx::query_scalar(r#"select id from "TaskCommentReaction" where "commentId" = $1 and "userId" = $2 and emoji = $3"#)
        .bind(&comment_id)
        .bind(&me.id)
        .bind(&emoji)
        .fetch_optional(&s.db)
        .await?;
    match existing {
        Some(id) => {
            sqlx::query(r#"delete from "TaskCommentReaction" where id = $1"#).bind(&id).execute(&s.db).await?;
        }
        None => {
            sqlx::query(r#"insert into "TaskCommentReaction" (id, "commentId", "userId", emoji, "createdAt") values ($1, $2, $3, $4, $5)"#)
                .bind(uuid())
                .bind(&comment_id)
                .bind(&me.id)
                .bind(&emoji)
                .bind(crate::ctx::now())
                .execute(&s.db)
                .await?;
        }
    }
    let rows: Vec<(String, String)> = sqlx::query_as(r#"select emoji, "userId" from "TaskCommentReaction" where "commentId" = $1 order by "createdAt" asc"#)
        .bind(&comment_id)
        .fetch_all(&s.db)
        .await?;
    let payload_rows: Vec<Value> = rows.iter().map(|(e, u)| json!({ "emoji": e, "userId": u })).collect();
    crate::realtime::emit_project(
        &s,
        &org,
        &project_id,
        "task:comment:reactions",
        &json!({ "commentId": comment_id, "taskId": task_id, "projectId": project_id, "boardId": board_id, "rows": payload_rows }),
    )
    .await;
    created(group_reactions(&rows, &me.id))
}

/// Kendi yorumunu herkes düzenler (ilk 2 saat).
async fn edit_comment(
    State(s): State<AppState>,
    me: Me,
    Path((org, project_id, board_id, task_id, comment_id)): Path<(String, String, String, String, String)>,
    Body(body): Body,
) -> Reply {
    check_comment(&body)?;
    board(&s.db, &org, &project_id, &board_id, &me.id).await?;
    let current: Option<(String, NaiveDateTime, String)> = sqlx::query_as(r#"select "authorId", "createdAt", content from "TaskComment" where id = $1 and "taskId" = $2"#)
        .bind(&comment_id)
        .bind(&task_id)
        .fetch_optional(&s.db)
        .await?;
    let Some((author, created_at, old_content)) = current else { return Err(not_found("COMMENT_NOT_FOUND", "Yorum bulunamadı.")) };
    if author != me.id {
        return Err(forbidden("NO_PERMISSION", "Yalnızca kendi yorumunu düzenleyebilirsin."));
    }
    if js_now_ms() - created_at.and_utc().timestamp_millis() > COMMENT_EDIT_WINDOW_MS {
        return Err(ApiError::conflict("COMMENT_EDIT_WINDOW_EXPIRED", "Yorumlar yalnızca ilk 2 saat içinde düzenlenebilir."));
    }
    let content = body["content"].as_str().unwrap_or_default().to_string();
    let comment: Json<Value> = sqlx::query_scalar(sql(format!(
        r#"with c as (update "TaskComment" set content = $2, "updatedAt" = $3 where id = $1 returning *) select {COMMENT_JSON} from c c"#
    )))
    .bind(&comment_id)
    .bind(js_trim(&content))
    .bind(crate::ctx::now())
    .fetch_one(&s.db)
    .await?;
    let comment = comment.0;
    let before = activity::mentioned_user_ids(&s.db, &org, &project_id, &old_content).await?;
    let after = activity::mentioned_user_ids(&s.db, &org, &project_id, &content).await?;
    let fresh: Vec<String> = after.into_iter().filter(|id| !before.contains(id)).collect();
    let targets = visible_targets(&s.db, &org, &project_id, fresh).await?;
    activity::record(&s, mention_rows(&org, &me.id, targets, &content, &project_id, &board_id, &task_id, Some(&comment_id))).await;
    crate::realtime::emit_project(&s, &org, &project_id, "task:comment", &json!({ "comment": comment, "projectId": project_id, "boardId": board_id })).await;
    ok(comment)
}

/// Kendi yorumunu (ilk 2 saat) herkes siler; başkasınınkini board.manage siler.
async fn remove_comment(
    State(s): State<AppState>,
    me: Me,
    Path((org, project_id, board_id, task_id, comment_id)): Path<(String, String, String, String, String)>,
) -> Reply {
    board(&s.db, &org, &project_id, &board_id, &me.id).await?;
    let current: Option<(String, NaiveDateTime)> = sqlx::query_as(r#"select "authorId", "createdAt" from "TaskComment" where id = $1 and "taskId" = $2"#)
        .bind(&comment_id)
        .bind(&task_id)
        .fetch_optional(&s.db)
        .await?;
    let Some((author, created_at)) = current else { return Err(not_found("COMMENT_NOT_FOUND", "Yorum bulunamadı.")) };
    let eff = perm::effective_in(&s.db, &project_id, &me.id).await?;
    let unlimited = eff.has("message.delete_unlimited");
    let own_and_fresh = author == me.id && js_now_ms() - created_at.and_utc().timestamp_millis() <= COMMENT_EDIT_WINDOW_MS;
    if !own_and_fresh && !unlimited && !eff.has("board.manage") {
        if author == me.id {
            return Err(ApiError::conflict("COMMENT_DELETE_WINDOW_EXPIRED", "Yorumlar yalnızca ilk 2 saat içinde silinebilir."));
        }
        return Err(forbidden("NO_PERMISSION", "Bu yorumu silme yetkin yok."));
    }
    sqlx::query(r#"delete from "TaskComment" where id = $1"#).bind(&comment_id).execute(&s.db).await?;
    crate::realtime::emit_project(
        &s,
        &org,
        &project_id,
        "task:comment:deleted",
        &json!({ "commentId": comment_id, "taskId": task_id, "projectId": project_id, "boardId": board_id }),
    )
    .await;
    ok(json!({ "ok": true }))
}

pub fn routes() -> Router<AppState> {
    let base = "/organizations/{organizationId}/projects/{projectId}/boards";
    Router::new()
        .route(base, get(boards).post(create_board))
        .route(&format!("{base}/{{boardId}}"), get(content).patch(rename_board).delete(delete_board))
        .route(&format!("{base}/{{boardId}}/sprints"), post(create_sprint))
        .route(&format!("{base}/{{boardId}}/sprints/{{sprintId}}"), patch(update_sprint).delete(delete_sprint))
        .route(&format!("{base}/{{boardId}}/tasks"), post(create_task))
        .route(&format!("{base}/{{boardId}}/tasks/{{taskId}}"), patch(update_task).delete(delete_task))
        .route(&format!("{base}/{{boardId}}/tasks/{{taskId}}/move"), patch(move_task))
        .route(&format!("{base}/{{boardId}}/tasks/{{taskId}}/comments"), get(comments).post(add_comment))
        .route(&format!("{base}/{{boardId}}/tasks/{{taskId}}/comments/{{commentId}}"), patch(edit_comment).delete(remove_comment))
        .route(&format!("{base}/{{boardId}}/tasks/{{taskId}}/comments/{{commentId}}/reactions"), post(react_comment))
}
