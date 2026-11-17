//! Sohbet grupları — `server/src/groups.ts` + `groupName.ts` karşılığı (gruplar, üyeler,
//! mesajlar, yanıt dizileri, tepkiler, okundu).
//!
//! İki tür grup vardır (docs/arkadaslar.md):
//! - **Proje grubu** (`projectId` dolu): üyeleri o projeden seçilir, projenin kenar çubuğunda durur.
//! - **Ana sayfa grubu** (`home` = true, `projectId` NULL): projeye bağlı değildir; üyeleri kuranın
//!   arkadaşları ya da proje paylaştığı kişilerdir (engel yoksa). Ana sayfadaki mesaj listesinde durur.

use std::cmp::Ordering;

use axum::Router;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::routing::{delete, get, patch, post};
use serde_json::{Map, Value, json};

use crate::AppState;
use crate::activity::{ActivityInput, mentioned_user_ids, record};
use crate::ctx::{Body, Me, Query, Reply, created, forbidden, not_found, now, ok, page_args, sql, uuid};
use crate::direct::{BODY_LIMIT, group_reactions, react_dto};
use crate::error::ApiResult;
use crate::realtime::emit_users;
use crate::validate::{Checker, js_len, js_slice};

// ------------------------------------------------------------------ grup adı (groupName.ts)

/// tr-TR harf sırası (ICU `localeCompare(…, 'tr')` yaklaşığı).
const TR_ALPHABET: &str = "abcçdefgğhıijklmnoöpqrsştuüvwxyz";

fn tr_lower(ch: char) -> char {
    match ch {
        'I' => 'ı',
        'İ' => 'i',
        _ => ch.to_lowercase().next().unwrap_or(ch),
    }
}

/// Türkçe olmayan aksanlı Latin harfleri: temel harf + ikincil fark.
fn fold_accent(ch: char) -> Option<(char, u32)> {
    let table: &[(&str, char)] = &[
        ("àáâãäåā", 'a'),
        ("èéêëēė", 'e'),
        ("ìíîïī", 'i'),
        ("òóôõøō", 'o'),
        ("ùúûūů", 'u'),
        ("ñń", 'n'),
        ("ýÿ", 'y'),
        ("ćč", 'c'),
        ("śš", 's'),
        ("źżž", 'z'),
        ("ł", 'l'),
        ("ď", 'd'),
        ("ř", 'r'),
        ("ť", 't'),
    ];
    for (chars, base) in table {
        if let Some(pos) = chars.chars().position(|c| c == ch) {
            return Some((*base, pos as u32 + 1));
        }
    }
    None
}

/// (birincil, ikincil, üçüncül) ağırlık: boşluk/noktalama < rakam < harf < diğer yazılar.
fn collation_key(ch: char) -> ((u32, u32), u32, u32) {
    let lower = tr_lower(ch);
    let tertiary = u32::from(lower != ch);
    if let Some(index) = TR_ALPHABET.chars().position(|c| c == lower) {
        return ((3, index as u32), 0, tertiary);
    }
    if let Some((base, secondary)) = fold_accent(lower)
        && let Some(index) = TR_ALPHABET.chars().position(|c| c == base)
    {
        return ((3, index as u32), secondary, tertiary);
    }
    if let Some(d) = lower.to_digit(10) {
        return ((2, d), 0, tertiary);
    }
    if lower.is_alphanumeric() {
        return ((4, lower as u32), 0, tertiary);
    }
    ((1, lower as u32), 0, tertiary)
}

fn tr_compare(a: &str, b: &str) -> Ordering {
    let ka: Vec<_> = a.chars().map(collation_key).collect();
    let kb: Vec<_> = b.chars().map(collation_key).collect();
    let primary = ka.iter().map(|k| k.0).cmp(kb.iter().map(|k| k.0));
    let secondary = || ka.iter().map(|k| k.1).cmp(kb.iter().map(|k| k.1));
    let tertiary = || ka.iter().map(|k| k.2).cmp(kb.iter().map(|k| k.2));
    primary.then_with(secondary).then_with(tertiary).then_with(|| a.cmp(b))
}

/// JS `String.prototype.trim` (Unicode boşlukları + BOM).
fn js_trim(value: &str) -> &str {
    value.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

// Grup adı üyelerin adlarından oluşur: "Ali, Cansu, Hasan".
pub fn group_name_of(names: &[String]) -> String {
    let mut list: Vec<String> = Vec::new();
    for name in names.iter().filter(|n| !n.is_empty()).map(|n| js_trim(n).to_string()) {
        if !list.contains(&name) {
            list.push(name);
        }
    }
    list.sort_by(|a, b| tr_compare(a, b));
    if list.is_empty() {
        return "Yeni grup".into();
    }
    let shown = list.iter().take(4).cloned().collect::<Vec<_>>().join(", ");
    let full = if list.len() > 4 { format!("{shown} +{}", list.len() - 4) } else { shown };
    js_slice(&full, 60)
}

// Ad elle mi verilmiş yoksa üyelerden mi türemiş? Türemişse üye değişince tazelenir.
pub fn is_auto_name(name: &str, names: &[String]) -> bool {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| regex::Regex::new(r"\s\+\d+$").unwrap());
    let stripped = re.replace(name, "");
    let parts: Vec<&str> = stripped.split(',').map(js_trim).filter(|p| !p.is_empty()).collect();
    let pool: Vec<&str> = names.iter().map(|n| js_trim(n)).collect();
    !parts.is_empty() && parts.iter().all(|p| pool.contains(p))
}

// ------------------------------------------------------------------ JSON biçimleri

/// `messageSelect` (m = "ChatGroupMessage").
const MESSAGE_JSON: &str = concat!(
    r#"json_build_object('id', m.id, 'content', m.content, 'kind', m.kind, 'createdAt', "#,
    crate::iso!(r#"m."createdAt""#),
    ", 'editedAt', ",
    crate::iso!(r#"m."editedAt""#),
    r#", 'author', (select json_build_object('id', u.id, 'username', u.username, 'displayName', u."displayName") from "User" u where u.id = m."authorId"),
    'quotedMessage', (select json_build_object('id', q.id, 'content', q.content, 'author', (select json_build_object('displayName', qu."displayName") from "User" qu where qu.id = q."authorId")) from "ChatGroupMessage" q where q.id = m."quotedMessageId"),
    'reactions', coalesce((select json_agg(json_build_object('emoji', r.emoji, 'userId', r."userId") order by r."createdAt") from "ChatReaction" r where r."groupMessageId" = m.id), '[]'::json),
    'parentId', m."parentId",
    '_count', json_build_object('replies', (select count(*) from "ChatGroupMessage" rc where rc."parentId" = m.id)),
    'replies', coalesce((select json_agg(x.j order by x.at desc) from (select r."createdAt" as at, json_build_object('id', r.id, 'createdAt', "#,
    crate::iso!(r#"r."createdAt""#),
    r#", 'author', (select json_build_object('id', ru.id, 'displayName', ru."displayName") from "User" ru where ru.id = r."authorId")) as j
        from "ChatGroupMessage" r where r."parentId" = m.id order by r."createdAt" desc limit 3) x), '[]'::json))"#
);

/// `shape(row, userId, unread)` (g = "ChatGroup", $1 = bakan kişi). `full` değilse `messages: []` ile.
fn group_json(full: bool) -> String {
    let (unread, last) = if full {
        (
            r#"(select count(*) from "ChatGroupMessage" x where x."groupId" = g.id and x."authorId" <> $1
                and ((select me."lastReadAt" from "ChatGroupMember" me where me."groupId" = g.id and me."userId" = $1) is null
                     or x."createdAt" > (select me."lastReadAt" from "ChatGroupMember" me where me."groupId" = g.id and me."userId" = $1)))"#
                .to_string(),
            format!(
                r#"(select json_build_object('id', l.id, 'content', l.content, 'createdAt', {}, 'authorId', l."authorId") from "ChatGroupMessage" l where l."groupId" = g.id order by l."createdAt" desc limit 1)"#,
                crate::iso!(r#"l."createdAt""#)
            ),
        )
    } else {
        ("0".to_string(), "null::json".to_string())
    };
    format!(
        r#"json_build_object('id', g.id, 'name', g.name, 'channelId', g."channelId", 'projectId', g."projectId", 'home', g.home, 'createdById', g."createdById", 'updatedAt', {updated},
        'members', coalesce((select json_agg((select json_build_object('id', u.id, 'username', u.username, 'displayName', u."displayName", 'status', u.status::text, 'avatarUrl', u."avatarUrl") from "User" u where u.id = gm."userId")) from "ChatGroupMember" gm where gm."groupId" = g.id), '[]'::json),
        'lastReadAt', (select {read} from "ChatGroupMember" me where me."groupId" = g.id and me."userId" = $1),
        'unread', {unread}, 'lastMessage', {last})"#,
        updated = crate::iso!(r#"g."updatedAt""#),
        read = crate::iso!(r#"me."lastReadAt""#),
    )
}

async fn shape(db: &sqlx::PgPool, group_id: &str, viewer: &str) -> ApiResult<Value> {
    let row: sqlx::types::Json<Value> = sqlx::query_scalar(sql(format!(r#"select {} from "ChatGroup" g where g.id = $2"#, group_json(false))))
        .bind(viewer)
        .bind(group_id)
        .fetch_one(db)
        .await?;
    Ok(row.0)
}

async fn message_json(db: &sqlx::PgPool, message_id: &str) -> ApiResult<Value> {
    let row: sqlx::types::Json<Value> = sqlx::query_scalar(sql(format!(r#"select {MESSAGE_JSON} from "ChatGroupMessage" m where m.id = $1"#)))
        .bind(message_id)
        .fetch_one(db)
        .await?;
    Ok(row.0)
}

// ------------------------------------------------------------------ yardımcılar

struct Member {
    user_id: String,
    display_name: String,
}

struct Group {
    id: String,
    name: String,
    project_id: Option<String>,
    /// Ana sayfa grubu (projeye bağlı değil).
    home: bool,
    created_by: String,
    members: Vec<Member>,
}

impl Group {
    fn member_ids(&self) -> Vec<String> {
        self.members.iter().map(|m| m.user_id.clone()).collect()
    }
    fn names(&self) -> Vec<String> {
        self.members.iter().map(|m| m.display_name.clone()).collect()
    }
}

async fn members_of(db: &sqlx::PgPool, group_id: &str) -> ApiResult<Vec<Member>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        r#"select gm."userId", (select u."displayName" from "User" u where u.id = gm."userId")
           from "ChatGroupMember" gm where gm."groupId" = $1"#,
    )
    .bind(group_id)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(|(user_id, display_name)| Member { user_id, display_name }).collect())
}

/// `findFirst({ where: { id } })` + üyeler (yetki denetimi yok).
async fn load(db: &sqlx::PgPool, where_sql: &str, binds: &[&str]) -> ApiResult<Option<Group>> {
    let mut q = sqlx::query_as::<_, (String, String, Option<String>, String, bool)>(sql(format!(
        r#"select g.id, g.name, g."projectId", g."createdById", g.home from "ChatGroup" g where {where_sql} limit 1"#
    )));
    for b in binds {
        q = q.bind(*b);
    }
    let Some((id, name, project_id, created_by, home)) = q.fetch_optional(db).await? else { return Ok(None) };
    let members = members_of(db, &id).await?;
    Ok(Some(Group { id, name, project_id, home, created_by, members }))
}

async fn membership(db: &sqlx::PgPool, organization_id: &str, user_id: &str) -> ApiResult<()> {
    crate::activity::membership(db, organization_id, user_id).await.map(|_| ())
}

// Grup yalnızca üyelerine görünür; olaylar da yalnızca onların odalarına gider.
async fn group(db: &sqlx::PgPool, organization_id: &str, group_id: &str, user_id: &str) -> ApiResult<Group> {
    let found = load(
        db,
        r#"g.id = $1 and g."organizationId" = $2 and exists (select 1 from "ChatGroupMember" x where x."groupId" = g.id and x."userId" = $3)"#,
        &[group_id, organization_id, user_id],
    )
    .await?;
    let missing = || not_found("GROUP_NOT_FOUND", "Grup bulunamadı.");
    let found = found.ok_or_else(missing)?;
    // Ana sayfa grubu: üyelik yeter. Proje grubu: projeye erişim de gerekir.
    if found.home {
        return Ok(found);
    }
    let Some(project_id) = found.project_id.clone() else { return Err(missing()) };
    crate::perm::require_project_access(db, organization_id, user_id, &project_id).await?;
    Ok(found)
}

/// Ana sayfa grubuna eklenebilecek kişiler: arkadaşım ya da proje paylaştığım, aramızda engel olmayanlar.
async fn home_audience(db: &sqlx::PgPool, user_id: &str, candidates: &[String]) -> ApiResult<Vec<String>> {
    let peers = crate::perm::peers_among(db, user_id, candidates).await?;
    let mut out = Vec::new();
    for id in candidates {
        if id != user_id && peers.contains(id) && !out.contains(id) && !crate::friends::blocked_between(db, user_id, id).await? {
            out.push(id.clone());
        }
    }
    Ok(out)
}

async fn rename(db: &sqlx::PgPool, group_id: &str, name: &str) -> ApiResult<()> {
    sqlx::query(r#"update "ChatGroup" set name = $2, "updatedAt" = $3 where id = $1"#)
        .bind(group_id)
        .bind(name)
        .bind(now())
        .execute(db)
        .await?;
    Ok(())
}

async fn create_message(db: &sqlx::PgPool, group_id: &str, author_id: &str, kind: &str, content: &str, quoted: Option<&str>, parent: Option<&str>) -> ApiResult<Value> {
    let id = uuid();
    sqlx::query(r#"insert into "ChatGroupMessage" (id, "groupId", "authorId", content, kind, "quotedMessageId", "parentId", "createdAt") values ($1, $2, $3, $4, $5, $6, $7, $8)"#)
        .bind(&id)
        .bind(group_id)
        .bind(author_id)
        .bind(content)
        .bind(kind)
        .bind(quoted)
        .bind(parent)
        .bind(now())
        .execute(db)
        .await?;
    message_json(db, &id).await
}

async fn touch(db: &sqlx::PgPool, group_id: &str) -> ApiResult<()> {
    sqlx::query(r#"update "ChatGroup" set "updatedAt" = $2 where id = $1"#).bind(group_id).bind(now()).execute(db).await?;
    Ok(())
}

/// `userIds` dizisi: 1..50 dizgi.
fn check_user_ids(c: &mut Checker, body: &Map<String, Value>) {
    let ids = body.get("userIds").and_then(Value::as_array);
    c.field(
        "userIds",
        &[
            (ids.is_some(), None),
            (ids.is_some_and(|a| !a.is_empty()), None),
            (ids.is_some_and(|a| a.len() <= 50), None),
            (ids.is_some_and(|a| a.iter().all(Value::is_string)), None),
        ],
    );
}

fn user_ids(body: &Map<String, Value>) -> Vec<String> {
    body.get("userIds").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
}

fn content_field(c: &mut Checker) {
    let content = c.str("content");
    c.field("content", &[(content.is_some_and(|v| (1..=4000).contains(&js_len(v))), None), (content.is_some(), None)]);
}

// ------------------------------------------------------------------ uçlar

async fn list(State(s): State<AppState>, me: Me, Path(org): Path<String>, q: Query) -> Reply {
    membership(&s.db, &org, &me.id).await?;
    // `?home=1`: Ana sayfa gruplarım (projeye bağlı olmayanlar). `?all=1`: üye olduğum bütün gruplar.
    let home_only = q.get("home").is_some_and(|v| v == "1");
    if home_only || q.get("all").is_some_and(|v| v == "1") {
        let rows: Vec<sqlx::types::Json<Value>> = sqlx::query_scalar(sql(format!(
            r#"select {} from "ChatGroup" g where g."organizationId" = $2 {}
               and exists (select 1 from "ChatGroupMember" x where x."groupId" = g.id and x."userId" = $1) order by g."updatedAt" desc"#,
            group_json(true),
            if home_only { "and g.home" } else { "" }
        )))
        .bind(&me.id)
        .bind(&org)
        .fetch_all(&s.db)
        .await?;
        return ok(Value::Array(rows.into_iter().map(|j| j.0).collect()));
    }
    let Some(project_id) = q.get("projectId").filter(|p| !p.is_empty()) else { return ok(json!([])) };
    crate::perm::require_project_access(&s.db, &org, &me.id, project_id).await?;
    let rows: Vec<sqlx::types::Json<Value>> = sqlx::query_scalar(sql(format!(
        r#"select {} from "ChatGroup" g where g."organizationId" = $2 and g."projectId" = $3
           and exists (select 1 from "ChatGroupMember" x where x."groupId" = g.id and x."userId" = $1) order by g."updatedAt" desc"#,
        group_json(true)
    )))
    .bind(&me.id)
    .bind(&org)
    .bind(project_id)
    .fetch_all(&s.db)
    .await?;
    ok(Value::Array(rows.into_iter().map(|j| j.0).collect()))
}

async fn create(State(s): State<AppState>, me: Me, Path(org): Path<String>, Body(body): Body) -> Reply {
    let mut c = Checker::new(&body);
    // `projectId` yoksa (ya da boşsa) Ana sayfa grubu kurulur.
    c.field("projectId", &[(!c.present("projectId") || body["projectId"].is_null() || c.str("projectId").is_some(), None)]);
    let name = c.str("name");
    c.field("name", &[(!c.present("name") || name.is_some_and(|v| (1..=60).contains(&js_len(v))), None), (!c.present("name") || name.is_some(), None)]);
    check_user_ids(&mut c, &body);
    c.finish()?;
    let project_id = body.get("projectId").and_then(Value::as_str).filter(|p| !p.is_empty()).map(str::to_string);

    membership(&s.db, &org, &me.id).await?;
    let mut others: Vec<String> = Vec::new();
    for id in user_ids(&body).into_iter().filter(|id| *id != me.id) {
        if !others.contains(&id) {
            others.push(id);
        }
    }
    let mut ids = vec![me.id.clone()];
    match &project_id {
        Some(project_id) => {
            crate::perm::require_in(&s.db, project_id, &me.id, "group.create").await?;
            crate::perm::require_project_access(&s.db, &org, &me.id, project_id).await?;
            ids.extend(crate::perm::project_audience(&s.db, &org, project_id, &others).await?);
        }
        None => {
            // Ana sayfa grubu: yalnız arkadaşlarım ya da proje paylaştığım kişiler (engel yoksa).
            let allowed = home_audience(&s.db, &me.id, &others).await?;
            if allowed.is_empty() {
                return Err(forbidden("NO_SHARED_PROJECT", "Seçtiğin kişilerle arkadaş değilsin ve ortak bir projen yok.").safe());
            }
            // Kurulumun hesap kabında olmayan arkadaş (hiç projeye girmemiş) üye olarak eklenir.
            for user in &allowed {
                sqlx::query(
                    r#"insert into "OrganizationMember" ("organizationId", "userId", role, permissions, "deniedPermissions", "joinedAt") values ($1, $2, 'MEMBER', '{}', '{}', $3)
                       on conflict do nothing"#,
                )
                .bind(&org)
                .bind(user)
                .bind(now())
                .execute(&s.db)
                .await?;
            }
            ids.extend(allowed);
        }
    }
    let people: Vec<String> = sqlx::query_scalar(r#"select "displayName" from "User" where id = any($1)"#).bind(&ids).fetch_all(&s.db).await?;
    let name = body.get("name").and_then(Value::as_str).map(js_trim).filter(|n| !n.is_empty()).map(str::to_string).unwrap_or_else(|| group_name_of(&people));

    let id = uuid();
    let at = now();
    let mut tx = s.db.begin().await?;
    sqlx::query(r#"insert into "ChatGroup" (id, "organizationId", name, "createdById", "projectId", home, "createdAt", "updatedAt") values ($1, $2, $3, $4, $5, $6, $7, $7)"#)
        .bind(&id)
        .bind(&org)
        .bind(&name)
        .bind(&me.id)
        .bind(&project_id)
        .bind(project_id.is_none())
        .bind(at)
        .execute(&mut *tx)
        .await?;
    for user in &ids {
        sqlx::query(r#"insert into "ChatGroupMember" (id, "groupId", "userId", "createdAt") values ($1, $2, $3, $4)"#)
            .bind(uuid())
            .bind(&id)
            .bind(user)
            .bind(at)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    let payload = shape(&s.db, &id, &me.id).await?;
    emit_users(&s, &ids, "group:created", &json!({ "group": payload })).await;
    created(payload)
}

async fn add_members(State(s): State<AppState>, me: Me, Path((org, group_id)): Path<(String, String)>, Body(body): Body) -> Reply {
    let mut c = Checker::new(&body);
    check_user_ids(&mut c, &body);
    c.finish()?;
    let g = group(&s.db, &org, &group_id, &me.id).await?;
    let existing = g.member_ids();
    let candidates: Vec<String> = user_ids(&body).into_iter().filter(|id| !existing.contains(id)).collect();
    let invited = match &g.project_id {
        Some(project) => crate::perm::project_audience(&s.db, &org, project, &candidates).await?,
        None if g.home => home_audience(&s.db, &me.id, &candidates).await?,
        None => Vec::new(),
    };
    if !invited.is_empty() {
        for user in &invited {
            sqlx::query(r#"insert into "ChatGroupMember" (id, "groupId", "userId", "createdAt") values ($1, $2, $3, $4) on conflict do nothing"#)
                .bind(uuid())
                .bind(&group_id)
                .bind(user)
                .bind(now())
                .execute(&s.db)
                .await?;
        }
        crate::calls::invite_to_group_call(&s, &group_id, &invited).await;
        // Sohbete "eklendi" satırı düşer.
        let names: Vec<String> = sqlx::query_scalar(r#"select "displayName" from "User" where id = any($1)"#).bind(&invited).fetch_all(&s.db).await?;
        let notice = create_message(&s.db, &group_id, &me.id, "SYSTEM", &format!("{} gruba eklendi.", names.join(", ")), None, None).await?;
        let audience: Vec<String> = sqlx::query_scalar(r#"select "userId" from "ChatGroupMember" where "groupId" = $1"#).bind(&group_id).fetch_all(&s.db).await?;
        emit_users(&s, &audience, "group:message", &json!({ "groupId": group_id, "message": notice })).await;
    }
    let mut fresh = group(&s.db, &org, &group_id, &me.id).await?;
    // Ad elle verilmediyse (eski üye listesinden üretilmişse) yeni listeyle tazelenir.
    if is_auto_name(&g.name, &g.names()) {
        rename(&s.db, &group_id, &group_name_of(&fresh.names())).await?;
        fresh = group(&s.db, &org, &group_id, &me.id).await?;
    }
    let payload = shape(&s.db, &group_id, &me.id).await?;
    // Yeni eklenenler grubu listelerinde göremiyordu: onlara "oluşturuldu" gider.
    if !invited.is_empty() {
        emit_users(&s, &invited, "group:created", &json!({ "group": payload })).await;
    }
    let rest: Vec<String> = fresh.member_ids().into_iter().filter(|id| !invited.contains(id)).collect();
    emit_users(&s, &rest, "group:updated", &json!({ "group": payload })).await;
    created(payload)
}

/// `PATCH …/groups/:id { name }`: grubu yeniden adlandırır (her üye yapabilir). Boş ad, adı yeniden
/// üyelerden türetir. Sohbete sistem satırı düşer, üyelere `group:updated` gider.
async fn rename_group(State(s): State<AppState>, me: Me, Path((org, group_id)): Path<(String, String)>, Body(body): Body) -> Reply {
    let mut c = Checker::new(&body);
    let name = c.str("name");
    c.field("name", &[(name.is_some_and(|v| js_len(v) <= 60), None), (name.is_some(), None)]);
    c.finish()?;
    let g = group(&s.db, &org, &group_id, &me.id).await?;
    let wanted = js_trim(body["name"].as_str().unwrap_or_default());
    let next = if wanted.is_empty() { group_name_of(&g.names()) } else { wanted.to_string() };
    if next != g.name {
        rename(&s.db, &group_id, &next).await?;
        let who = g.members.iter().find(|m| m.user_id == me.id).map(|m| m.display_name.clone()).unwrap_or_default();
        let notice = create_message(&s.db, &group_id, &me.id, "SYSTEM", &format!("{who} grubun adını «{next}» yaptı."), None, None).await?;
        emit_users(&s, &g.member_ids(), "group:message", &json!({ "groupId": group_id, "message": notice })).await;
    }
    let payload = shape(&s.db, &group_id, &me.id).await?;
    emit_users(&s, &g.member_ids(), "group:updated", &json!({ "group": payload })).await;
    ok(payload)
}

// Gruptan çıkarma: yalnızca grubu kuran kişi yapabilir.
async fn remove_member(State(s): State<AppState>, me: Me, Path((org, group_id, member_id)): Path<(String, String, String)>) -> Reply {
    let g = group(&s.db, &org, &group_id, &me.id).await?;
    if g.created_by != me.id {
        return Err(forbidden("NOT_OWNER", "Yalnızca grubu kuran kişi üye çıkarabilir."));
    }
    if member_id == me.id {
        return Err(forbidden("NOT_SELF", "Kendini çıkaramazsın; gruptan ayrıl."));
    }
    let Some(target) = g.members.iter().find(|m| m.user_id == member_id) else { return Err(not_found("MEMBER_NOT_FOUND", "Üye bulunamadı.")) };

    sqlx::query(r#"delete from "ChatGroupMember" where "groupId" = $1 and "userId" = $2"#).bind(&group_id).bind(&member_id).execute(&s.db).await?;
    crate::calls::drop_from_group_call(&s, &group_id, &member_id).await;
    emit_users(&s, std::slice::from_ref(&member_id), "group:left", &json!({ "groupId": group_id })).await;

    let remaining: Vec<&Member> = g.members.iter().filter(|m| m.user_id != member_id).collect();
    if is_auto_name(&g.name, &g.names()) {
        let names: Vec<String> = remaining.iter().map(|m| m.display_name.clone()).collect();
        rename(&s.db, &group_id, &group_name_of(&names)).await?;
    }
    let message = create_message(&s.db, &group_id, &me.id, "SYSTEM", &format!("{} gruptan çıkarıldı.", target.display_name), None, None).await?;
    let exists: Option<String> = sqlx::query_scalar(r#"select id from "ChatGroup" where id = $1"#).bind(&group_id).fetch_optional(&s.db).await?;
    if exists.is_some() {
        let audience: Vec<String> = remaining.iter().map(|m| m.user_id.clone()).collect();
        emit_users(&s, &audience, "group:message", &json!({ "groupId": group_id, "message": message })).await;
        let payload = shape(&s.db, &group_id, &me.id).await?;
        emit_users(&s, &audience, "group:updated", &json!({ "group": payload })).await;
    }
    ok(json!({ "ok": true }))
}

// Gruptan ayrılma: kalanlara sistem mesajı ve güncel liste gider.
async fn leave(State(s): State<AppState>, me: Me, Path((org, group_id)): Path<(String, String)>) -> Reply {
    let g = group(&s.db, &org, &group_id, &me.id).await?;
    let mine = g.members.iter().find(|m| m.user_id == me.id);
    let remaining: Vec<&Member> = g.members.iter().filter(|m| m.user_id != me.id).collect();
    sqlx::query(r#"delete from "ChatGroupMember" where "groupId" = $1 and "userId" = $2"#).bind(&group_id).bind(&me.id).execute(&s.db).await?;
    crate::calls::drop_from_group_call(&s, &group_id, &me.id).await;
    emit_users(&s, std::slice::from_ref(&me.id), "group:left", &json!({ "groupId": group_id })).await;
    if remaining.is_empty() {
        sqlx::query(r#"delete from "ChatGroup" where id = $1"#).bind(&group_id).execute(&s.db).await?;
        return created(json!({ "ok": true }));
    }
    let who = mine.map(|m| m.display_name.as_str()).filter(|n| !n.is_empty()).unwrap_or("Bir üye");
    let message = create_message(&s.db, &group_id, &me.id, "SYSTEM", &format!("{who} gruptan ayrıldı."), None, None).await?;
    // Ad otomatikse ayrılan kişi adından da düşer.
    if is_auto_name(&g.name, &g.names()) {
        let names: Vec<String> = remaining.iter().map(|m| m.display_name.clone()).collect();
        rename(&s.db, &group_id, &group_name_of(&names)).await?;
    }
    let exists: Option<String> = sqlx::query_scalar(r#"select id from "ChatGroup" where id = $1"#).bind(&group_id).fetch_optional(&s.db).await?;
    if exists.is_some() {
        let audience: Vec<String> = remaining.iter().map(|m| m.user_id.clone()).collect();
        let payload = shape(&s.db, &group_id, &remaining[0].user_id).await?;
        emit_users(&s, &audience, "group:message", &json!({ "groupId": group_id, "message": message })).await;
        emit_users(&s, &audience, "group:updated", &json!({ "group": payload })).await;
    }
    created(json!({ "ok": true }))
}

// Sayfalama: ?before=<ISO createdAt>&limit=<1..200>; parametresiz çağrı en yeni 200 mesajı artan sırada döner.
async fn messages(State(s): State<AppState>, me: Me, Path((org, group_id)): Path<(String, String)>, q: Query) -> Reply {
    group(&s.db, &org, &group_id, &me.id).await?;
    let (take, cursor) = page_args(q.get("before").map(String::as_str), q.get("limit").map(String::as_str), 200, 200);
    let mut rows: Vec<sqlx::types::Json<Value>> = sqlx::query_scalar(sql(format!(
        r#"select {MESSAGE_JSON} from "ChatGroupMessage" m where m."groupId" = $1 and m."parentId" is null and ($2::timestamp is null or m."createdAt" < $2)
           order by m."createdAt" desc limit $3"#
    )))
    .bind(&group_id)
    .bind(cursor)
    .bind(take)
    .fetch_all(&s.db)
    .await?;
    rows.reverse();
    ok(Value::Array(rows.into_iter().map(|j| j.0).collect()))
}

// Grupta @kullanıcı adı geçenlere etkinlik kaydı düşer (yalnızca grup üyeleri).
async fn notify_group_mentions(s: &AppState, organization_id: &str, actor_id: &str, content: &str, g: &Group, message_id: &str, parent_id: Option<&str>) -> ApiResult<()> {
    // Kullanıcı grupları da (@devs) çözülür ama bildirim yine yalnızca sohbet
    // grubundaki kişilere gider; gruba dahil olmayan kimse haberdar edilmez.
    let mentioned = match &g.project_id {
        Some(project) => mentioned_user_ids(&s.db, organization_id, project, content).await?,
        // Ana sayfa grubu: proje yok, @kullanıcı adı doğrudan grup üyeleri arasında aranır.
        None => {
            let names: Vec<String> = crate::activity::mentioned_usernames(content).iter().map(|n| n.to_lowercase()).collect();
            if names.is_empty() {
                Vec::new()
            } else {
                sqlx::query_scalar(r#"select id from "User" where id = any($1) and lower(username) = any($2)"#).bind(g.member_ids()).bind(&names).fetch_all(&s.db).await?
            }
        }
    };
    if mentioned.is_empty() {
        return Ok(());
    }
    let inputs = g
        .members
        .iter()
        .filter(|m| mentioned.contains(&m.user_id))
        .map(|m| ActivityInput {
            organization_id: organization_id.into(),
            user_id: m.user_id.clone(),
            actor_id: actor_id.into(),
            kind: "MENTION",
            preview: content.into(),
            group_id: Some(g.id.clone()),
            group_name: Some(g.name.clone()),
            message_id: Some(message_id.into()),
            parent_id: parent_id.map(str::to_string),
            ..Default::default()
        })
        .collect();
    record(s, inputs).await;
    Ok(())
}

async fn send(State(s): State<AppState>, me: Me, Path((org, group_id)): Path<(String, String)>, Body(body): Body) -> Reply {
    let mut c = Checker::new(&body);
    content_field(&mut c);
    c.field("quotedMessageId", &[(!c.present("quotedMessageId") || c.str("quotedMessageId").is_some(), None)]);
    c.finish()?;
    let content = body["content"].as_str().unwrap_or_default().to_string();
    let g = group(&s.db, &org, &group_id, &me.id).await?;
    let quoted: Option<String> = match body.get("quotedMessageId").and_then(Value::as_str).filter(|v| !v.is_empty()) {
        Some(q) => {
            sqlx::query_scalar(r#"select id from "ChatGroupMessage" where id = $1 and "groupId" = $2 limit 1"#)
                .bind(q)
                .bind(&group_id)
                .fetch_optional(&s.db)
                .await?
        }
        None => None,
    };
    let message = create_message(&s.db, &group_id, &me.id, "TEXT", &content, quoted.as_deref(), None).await?;
    touch(&s.db, &group_id).await?;
    emit_users(&s, &g.member_ids(), "group:message", &json!({ "groupId": group_id, "message": message })).await;
    let id = message["id"].as_str().unwrap_or_default().to_string();
    notify_group_mentions(&s, &org, &me.id, &content, &g, &id, None).await?;
    created(message)
}

async fn replies(State(s): State<AppState>, me: Me, Path((org, group_id, message_id)): Path<(String, String, String)>) -> Reply {
    group(&s.db, &org, &group_id, &me.id).await?;
    let rows: Vec<sqlx::types::Json<Value>> = sqlx::query_scalar(sql(format!(
        r#"select {MESSAGE_JSON} from "ChatGroupMessage" m where m."groupId" = $1 and m."parentId" = $2 order by m."createdAt" asc"#
    )))
    .bind(&group_id)
    .bind(&message_id)
    .fetch_all(&s.db)
    .await?;
    ok(Value::Array(rows.into_iter().map(|j| j.0).collect()))
}

async fn reply(State(s): State<AppState>, me: Me, Path((org, group_id, message_id)): Path<(String, String, String)>, Body(body): Body) -> Reply {
    let mut c = Checker::new(&body);
    content_field(&mut c);
    c.finish()?;
    let content = body["content"].as_str().unwrap_or_default().to_string();
    let g = group(&s.db, &org, &group_id, &me.id).await?;
    let parent: Option<String> = sqlx::query_scalar(r#"select "authorId" from "ChatGroupMessage" where id = $1 and "groupId" = $2 and "parentId" is null limit 1"#)
        .bind(&message_id)
        .bind(&group_id)
        .fetch_optional(&s.db)
        .await?;
    let Some(parent_author) = parent else { return Err(not_found("MESSAGE_NOT_FOUND", "Mesaj bulunamadı.")) };
    let message = create_message(&s.db, &group_id, &me.id, "TEXT", &content, None, Some(&message_id)).await?;
    touch(&s.db, &group_id).await?;
    let count: i64 = sqlx::query_scalar(r#"select count(*) from "ChatGroupMessage" where "parentId" = $1"#).bind(&message_id).fetch_one(&s.db).await?;
    emit_users(&s, &g.member_ids(), "group:reply", &json!({ "groupId": group_id, "parentId": message_id, "message": message, "replies": count })).await;
    let id = message["id"].as_str().unwrap_or_default().to_string();
    record(
        &s,
        vec![ActivityInput {
            organization_id: org.clone(),
            user_id: parent_author,
            actor_id: me.id.clone(),
            kind: "REPLY",
            preview: content.clone(),
            group_id: Some(g.id.clone()),
            group_name: Some(g.name.clone()),
            message_id: Some(id.clone()),
            parent_id: Some(message_id.clone()),
            ..Default::default()
        }],
    )
    .await;
    notify_group_mentions(&s, &org, &me.id, &content, &g, &id, Some(&message_id)).await?;
    created(message)
}

async fn edit(State(s): State<AppState>, me: Me, Path((org, group_id, message_id)): Path<(String, String, String)>, Body(body): Body) -> Reply {
    let mut c = Checker::new(&body);
    content_field(&mut c);
    c.field("quotedMessageId", &[(!c.present("quotedMessageId") || c.str("quotedMessageId").is_some(), None)]);
    c.finish()?;
    let content = body["content"].as_str().unwrap_or_default().to_string();
    let g = group(&s.db, &org, &group_id, &me.id).await?;
    let author: Option<String> = sqlx::query_scalar(r#"select "authorId" from "ChatGroupMessage" where id = $1 and "groupId" = $2 limit 1"#)
        .bind(&message_id)
        .bind(&group_id)
        .fetch_optional(&s.db)
        .await?;
    let Some(author) = author else { return Err(not_found("MESSAGE_NOT_FOUND", "Mesaj bulunamadı.")) };
    if author != me.id {
        return Err(forbidden("NOT_AUTHOR", "Yalnızca kendi mesajını düzenleyebilirsin."));
    }
    sqlx::query(r#"update "ChatGroupMessage" set content = $2, "editedAt" = $3 where id = $1"#)
        .bind(&message_id)
        .bind(&content)
        .bind(now())
        .execute(&s.db)
        .await?;
    let message = message_json(&s.db, &message_id).await?;
    emit_users(&s, &g.member_ids(), "group:message:updated", &json!({ "groupId": group_id, "message": message })).await;
    ok(message)
}

async fn remove(State(s): State<AppState>, me: Me, Path((org, group_id, message_id)): Path<(String, String, String)>) -> Reply {
    let g = group(&s.db, &org, &group_id, &me.id).await?;
    let author: Option<String> = sqlx::query_scalar(r#"select "authorId" from "ChatGroupMessage" where id = $1 and "groupId" = $2 limit 1"#)
        .bind(&message_id)
        .bind(&group_id)
        .fetch_optional(&s.db)
        .await?;
    let Some(author) = author else { return Err(not_found("MESSAGE_NOT_FOUND", "Mesaj bulunamadı.")) };
    // Ana sayfa grubunda proje yetkisi yok: yalnız kendi mesajını silebilirsin.
    let unlimited = match &g.project_id {
        Some(project) => crate::perm::effective_in(&s.db, project, &me.id).await?.has("message.delete_unlimited"),
        None => false,
    };
    if author != me.id && !unlimited {
        return Err(forbidden("NOT_AUTHOR", "Bu mesajı silme yetkin yok."));
    }
    sqlx::query(r#"delete from "ChatGroupMessage" where id = $1"#).bind(&message_id).execute(&s.db).await?;
    emit_users(&s, &g.member_ids(), "group:message:deleted", &json!({ "groupId": group_id, "messageId": message_id })).await;
    ok(json!({ "ok": true }))
}

async fn react(State(s): State<AppState>, me: Me, Path((org, group_id, message_id)): Path<(String, String, String)>, Body(body): Body) -> Reply {
    let emoji = react_dto(&body)?;
    let g = group(&s.db, &org, &group_id, &me.id).await?;
    let target: Option<(String, String, String, Option<String>)> =
        sqlx::query_as(r#"select id, "authorId", content, "parentId" from "ChatGroupMessage" where id = $1 and "groupId" = $2 limit 1"#)
            .bind(&message_id)
            .bind(&group_id)
            .fetch_optional(&s.db)
            .await?;
    let Some((target_id, target_author, target_content, target_parent)) = target else { return Err(not_found("MESSAGE_NOT_FOUND", "Mesaj bulunamadı.")) };
    let emoji = emoji.trim().to_string();
    let existing: Option<String> = sqlx::query_scalar(r#"select id from "ChatReaction" where "groupMessageId" = $1 and "userId" = $2 and emoji = $3 limit 1"#)
        .bind(&message_id)
        .bind(&me.id)
        .bind(&emoji)
        .fetch_optional(&s.db)
        .await?;
    match existing {
        Some(id) => {
            sqlx::query(r#"delete from "ChatReaction" where id = $1"#).bind(id).execute(&s.db).await?;
        }
        None => {
            sqlx::query(r#"insert into "ChatReaction" (id, "groupMessageId", "userId", emoji, "createdAt") values ($1, $2, $3, $4, $5)"#)
                .bind(uuid())
                .bind(&message_id)
                .bind(&me.id)
                .bind(&emoji)
                .bind(now())
                .execute(&s.db)
                .await?;
            record(
                &s,
                vec![ActivityInput {
                    organization_id: org.clone(),
                    user_id: target_author,
                    actor_id: me.id.clone(),
                    kind: "REACTION",
                    preview: target_content,
                    emoji: Some(emoji.clone()),
                    group_id: Some(g.id.clone()),
                    group_name: Some(g.name.clone()),
                    message_id: Some(target_id),
                    parent_id: target_parent,
                    ..Default::default()
                }],
            )
            .await;
        }
    }
    let rows: Vec<sqlx::types::Json<Value>> = sqlx::query_scalar(
        r#"select json_build_object('emoji', emoji, 'userId', "userId") from "ChatReaction" where "groupMessageId" = $1 order by "createdAt" asc"#,
    )
    .bind(&message_id)
    .fetch_all(&s.db)
    .await?;
    let rows: Vec<Value> = rows.into_iter().map(|j| j.0).collect();
    let grouped = group_reactions(&rows, &me.id);
    emit_users(&s, &g.member_ids(), "group:reactions", &json!({ "groupId": group_id, "messageId": message_id, "rows": rows })).await;
    created(grouped)
}

async fn read(State(s): State<AppState>, me: Me, Path((org, group_id)): Path<(String, String)>) -> Reply {
    group(&s.db, &org, &group_id, &me.id).await?;
    sqlx::query(r#"update "ChatGroupMember" set "lastReadAt" = $3 where "groupId" = $1 and "userId" = $2"#)
        .bind(&group_id)
        .bind(&me.id)
        .bind(now())
        .execute(&s.db)
        .await?;
    created(json!({ "ok": true }))
}

pub fn routes() -> Router<AppState> {
    let base = "/organizations/{organizationId}/groups";
    Router::new()
        .route(base, get(list).post(create))
        .route(&format!("{base}/{{groupId}}"), patch(rename_group))
        .route(&format!("{base}/{{groupId}}/members"), post(add_members))
        .route(&format!("{base}/{{groupId}}/members/{{memberId}}"), delete(remove_member))
        .route(&format!("{base}/{{groupId}}/leave"), post(leave))
        .route(&format!("{base}/{{groupId}}/messages"), get(messages).post(send))
        .route(&format!("{base}/{{groupId}}/messages/{{messageId}}/replies"), get(replies).post(reply))
        .route(&format!("{base}/{{groupId}}/messages/{{messageId}}"), patch(edit).delete(remove))
        .route(&format!("{base}/{{groupId}}/messages/{{messageId}}/reactions"), post(react))
        .route(&format!("{base}/{{groupId}}/read"), post(read))
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grup_adi() {
        let names = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(group_name_of(&names(&["Hasan", "ali", "Çağrı", "Cansu"])), "ali, Cansu, Çağrı, Hasan");
        assert_eq!(group_name_of(&names(&["İpek", "Ilgın", "Hakan"])), "Hakan, Ilgın, İpek");
        assert_eq!(group_name_of(&names(&["a", "b", "c", "d", "e"])), "a, b, c, d +1");
        assert_eq!(group_name_of(&[]), "Yeni grup");
        assert!(is_auto_name("a, b, c, d +1", &names(&["a", "b", "c", "d", "e"])));
        assert!(!is_auto_name("Ekip", &names(&["a"])));
    }
}
