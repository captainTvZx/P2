//! Birebir sohbetler — `server/src/direct.ts` karşılığı (sohbet listesi, mesajlar, tepkiler, okundu).

use axum::Router;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::routing::{get, patch, post};
use chrono::NaiveDateTime;
use serde_json::{Map, Value, json};

use crate::AppState;
use crate::ctx::{Body, Me, Query, Reply, created, now, ok, page_args, sql, uuid};
use crate::error::{ApiError, ApiResult};
use crate::validate::{Checker, js_len};

/// Express json({ limit: '340mb' }) karşılığı gövde sınırı.
pub const BODY_LIMIT: usize = 340 * 1024 * 1024;

/// Prisma Date → JSON biçimi (Rust tarafında üretilen zamanlar için).
pub fn iso_of(at: NaiveDateTime) -> String {
    at.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

/// Tepkiler emoji başına toplanır: {emoji, count, reacted}
pub fn group_reactions(rows: &[Value], user_id: &str) -> Value {
    let mut out: Vec<Map<String, Value>> = Vec::new();
    for row in rows {
        let emoji = row["emoji"].as_str().unwrap_or_default();
        let index = match out.iter().position(|item| item["emoji"] == emoji) {
            Some(i) => i,
            None => {
                let mut item = Map::new();
                item.insert("emoji".into(), json!(emoji));
                item.insert("count".into(), json!(0));
                item.insert("reacted".into(), json!(false));
                out.push(item);
                out.len() - 1
            }
        };
        let item = &mut out[index];
        let count = item["count"].as_i64().unwrap_or(0) + 1;
        item.insert("count".into(), json!(count));
        if row["userId"].as_str() == Some(user_id) {
            item.insert("reacted".into(), json!(true));
        }
    }
    Value::Array(out.into_iter().map(Value::Object).collect())
}

/// `messageSelect` (m = "DirectMessage").
const MESSAGE_JSON: &str = concat!(
    r#"json_build_object('id', m.id, 'content', m.content, 'kind', m.kind, 'createdAt', "#,
    crate::iso!(r#"m."createdAt""#),
    ", 'editedAt', ",
    crate::iso!(r#"m."editedAt""#),
    r#", 'author', (select json_build_object('id', u.id, 'username', u.username, 'displayName', u."displayName") from "User" u where u.id = m."authorId"),
    'quotedMessage', (select json_build_object('id', q.id, 'content', q.content, 'author', (select json_build_object('displayName', qu."displayName") from "User" qu where qu.id = q."authorId")) from "DirectMessage" q where q.id = m."quotedMessageId"),
    'reactions', coalesce((select json_agg(json_build_object('emoji', r.emoji, 'userId', r."userId") order by r."createdAt") from "ChatReaction" r where r."directMessageId" = m.id), '[]'::json))"#
);

/// `shape(row, userId, unread)` (c = "DirectConversation", $1 = kullanıcı). `unread` SQL ifadesi dışarıdan.
fn conversation_json(unread: &str) -> String {
    format!(
        r#"json_build_object('id', c.id,
        'user', (select json_build_object('id', u.id, 'username', u.username, 'displayName', u."displayName", 'status', u.status::text, 'avatarUrl', u."avatarUrl", 'title', u.title) from "User" u where u.id = (case when c."userAId" = $1 then c."userBId" else c."userAId" end)),
        'friend', exists (select 1 from "Friendship" f where f.status::text = 'ACCEPTED' and ((f."requesterId" = c."userAId" and f."addresseeId" = c."userBId") or (f."requesterId" = c."userBId" and f."addresseeId" = c."userAId"))),
        'blockedByMe', exists (select 1 from "UserBlock" b where b."blockerId" = $1 and b."blockedId" = (case when c."userAId" = $1 then c."userBId" else c."userAId" end)),
        'updatedAt', {updated}, 'lastReadAt', {read}, 'unread', {unread},
        'lastMessage', (select json_build_object('id', l.id, 'content', l.content, 'createdAt', {last}, 'authorId', l."authorId") from "DirectMessage" l where l."conversationId" = c.id order by l."createdAt" desc limit 1))"#,
        updated = crate::iso!(r#"c."updatedAt""#),
        read = crate::iso!(r#"(case when c."userAId" = $1 then c."userALastReadAt" else c."userBLastReadAt" end)"#),
        last = crate::iso!(r#"l."createdAt""#),
    )
}

/// Okunmamış sayısı: karşı tarafın son okumadan sonraki mesajları (engellediğim kişininkiler sayılmaz).
const UNREAD: &str = r#"(select count(*) from "DirectMessage" x where x."conversationId" = c.id and x."authorId" <> $1
    and not exists (select 1 from "UserBlock" b where b."blockerId" = $1 and b."blockedId" = x."authorId")
    and ((case when c."userAId" = $1 then c."userALastReadAt" else c."userBLastReadAt" end) is null or x."createdAt" > (case when c."userAId" = $1 then c."userALastReadAt" else c."userBLastReadAt" end)))"#;

async fn membership(db: &sqlx::PgPool, organization_id: &str, user_id: &str) -> ApiResult<()> {
    crate::activity::membership(db, organization_id, user_id).await.map(|_| ())
}

struct Conversation {
    user_a: String,
    user_b: String,
}

impl Conversation {
    fn users(&self) -> Vec<String> {
        vec![self.user_a.clone(), self.user_b.clone()]
    }
}

async fn conversation(db: &sqlx::PgPool, organization_id: &str, conversation_id: &str, user_id: &str) -> ApiResult<Conversation> {
    let row: Option<(String, String)> = sqlx::query_as(
        r#"select "userAId", "userBId" from "DirectConversation" where id = $1 and "organizationId" = $2 and ("userAId" = $3 or "userBId" = $3) limit 1"#,
    )
    .bind(conversation_id)
    .bind(organization_id)
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    let (user_a, user_b) = row.ok_or_else(|| crate::ctx::not_found("CONVERSATION_NOT_FOUND", "Sohbet bulunamadı."))?;
    Ok(Conversation { user_a, user_b })
}

async fn message_json(db: &sqlx::PgPool, message_id: &str) -> ApiResult<Value> {
    let row: sqlx::types::Json<Value> = sqlx::query_scalar(sql(format!(r#"select {MESSAGE_JSON} from "DirectMessage" m where m.id = $1"#)))
        .bind(message_id)
        .fetch_one(db)
        .await?;
    Ok(row.0)
}

async fn notify(state: &AppState, conversation: &Conversation, event: &str, payload: Value) {
    crate::realtime::emit_users(state, &conversation.users(), event, &payload).await;
}

/// DirectMessageDto: content (1..4000), quotedMessageId isteğe bağlı dizgi.
fn message_dto(body: &Map<String, Value>) -> ApiResult<String> {
    let mut c = Checker::new(body);
    let content = c.str("content");
    c.field("content", &[(content.is_some_and(|v| (1..=4000).contains(&js_len(v))), None), (content.is_some(), None)]);
    c.field("quotedMessageId", &[(!c.present("quotedMessageId") || c.str("quotedMessageId").is_some(), None)]);
    c.finish()?;
    Ok(content.unwrap_or_default().to_string())
}

/// ReactDto: emoji (1..24).
pub fn react_dto(body: &Map<String, Value>) -> ApiResult<String> {
    let mut c = Checker::new(body);
    let emoji = c.str("emoji");
    c.field("emoji", &[(emoji.is_some_and(|v| (1..=24).contains(&js_len(v))), None), (emoji.is_some(), None)]);
    c.finish()?;
    Ok(emoji.unwrap_or_default().to_string())
}

async fn list(State(s): State<AppState>, me: Me, Path(org): Path<String>) -> Reply {
    membership(&s.db, &org, &me.id).await?;
    let rows: Vec<sqlx::types::Json<Value>> = sqlx::query_scalar(sql(format!(
        r#"select {} from "DirectConversation" c where c."organizationId" = $2 and (c."userAId" = $1 or c."userBId" = $1) order by c."updatedAt" desc"#,
        conversation_json(UNREAD)
    )))
    .bind(&me.id)
    .bind(&org)
    .fetch_all(&s.db)
    .await?;
    ok(Value::Array(rows.into_iter().map(|j| j.0).collect()))
}

async fn start(State(s): State<AppState>, me: Me, Path(org): Path<String>, Body(body): Body) -> Reply {
    let mut c = Checker::new(&body);
    c.field("userId", &[(c.str("userId").is_some(), None)]);
    c.finish()?;
    let other = body["userId"].as_str().unwrap_or_default().to_string();
    membership(&s.db, &org, &me.id).await?;
    if other == me.id {
        return Err(ApiError::conflict("SELF_CONVERSATION", "Kendinle sohbet başlatamazsın."));
    }
    // Arkadaşlar projeden bağımsızdır: karşı taraf bu kurulumun hesap kabında değilse (hiç projeye
    // girmemiş arkadaş) üye olarak eklenir; arkadaş değilse eskisi gibi "bulunamadı".
    if membership(&s.db, &org, &other).await.is_err() {
        if !crate::friends::are_friends(&s.db, &me.id, &other).await? {
            return Err(crate::ctx::not_found("ORGANIZATION_NOT_FOUND", "Kuruluş bulunamadı."));
        }
        sqlx::query(
            r#"insert into "OrganizationMember" ("organizationId", "userId", role, permissions, "deniedPermissions", "joinedAt") values ($1, $2, 'MEMBER', '{}', '{}', $3)
               on conflict do nothing"#,
        )
        .bind(&org)
        .bind(&other)
        .bind(now())
        .execute(&s.db)
        .await?;
    }
    let (user_a, user_b) = if me.id <= other { (me.id.clone(), other.clone()) } else { (other.clone(), me.id.clone()) };
    let existing: Option<sqlx::types::Json<Value>> = sqlx::query_scalar(sql(format!(
        r#"select {} from "DirectConversation" c where c."organizationId" = $2 and c."userAId" = $3 and c."userBId" = $4"#,
        conversation_json(UNREAD)
    )))
    .bind(&me.id)
    .bind(&org)
    .bind(&user_a)
    .bind(&user_b)
    .fetch_optional(&s.db)
    .await?;
    if let Some(existing) = existing {
        return created(existing.0);
    }
    crate::perm::require_shared(&s.db, &me.id, &other, "direct.send").await?;
    let id = uuid();
    let at = now();
    sqlx::query(r#"insert into "DirectConversation" (id, "organizationId", "userAId", "userBId", "createdAt", "updatedAt") values ($1, $2, $3, $4, $5, $5)"#)
        .bind(&id)
        .bind(&org)
        .bind(&user_a)
        .bind(&user_b)
        .bind(at)
        .execute(&s.db)
        .await?;
    let shaped: sqlx::types::Json<Value> = sqlx::query_scalar(sql(format!(r#"select {} from "DirectConversation" c where c.id = $2"#, conversation_json("0"))))
        .bind(&me.id)
        .bind(&id)
        .fetch_one(&s.db)
        .await?;
    notify(&s, &Conversation { user_a, user_b }, "direct:conversation", json!({ "conversationId": id })).await;
    created(shaped.0)
}

// Sayfalama: ?before=<ISO createdAt>&limit=<1..200>; parametresiz çağrı en yeni 200 mesajı artan sırada döner.
async fn messages(State(s): State<AppState>, me: Me, Path((org, conversation_id)): Path<(String, String)>, q: Query) -> Reply {
    membership(&s.db, &org, &me.id).await?;
    conversation(&s.db, &org, &conversation_id, &me.id).await?;
    let (take, cursor) = page_args(q.get("before").map(String::as_str), q.get("limit").map(String::as_str), 200, 200);
    // Engellediğim kişinin mesajları bana gösterilmez (silinmez; engel kalkınca geri gelir).
    let mut rows: Vec<sqlx::types::Json<Value>> = sqlx::query_scalar(sql(format!(
        r#"select {MESSAGE_JSON} from "DirectMessage" m where m."conversationId" = $1 and ($2::timestamp is null or m."createdAt" < $2)
           and not exists (select 1 from "UserBlock" b where b."blockerId" = $4 and b."blockedId" = m."authorId")
           order by m."createdAt" desc limit $3"#
    )))
    .bind(&conversation_id)
    .bind(cursor)
    .bind(take)
    .bind(&me.id)
    .fetch_all(&s.db)
    .await?;
    rows.reverse();
    ok(Value::Array(rows.into_iter().map(|j| j.0).collect()))
}

async fn send(State(s): State<AppState>, me: Me, Path((org, conversation_id)): Path<(String, String)>, Body(body): Body) -> Reply {
    let content = message_dto(&body)?;
    membership(&s.db, &org, &me.id).await?;
    let conv = conversation(&s.db, &org, &conversation_id, &me.id).await?;
    let other = if conv.user_a == me.id { conv.user_b.clone() } else { conv.user_a.clone() };
    // Bireysel mesaj: iki kişi en az bir projeyi paylaşmalı, gönderen orada `direct.send` taşımalı.
    crate::perm::require_shared(&s.db, &me.id, &other, "direct.send").await?;
    let at = now();
    let quoted = body.get("quotedMessageId").and_then(Value::as_str).filter(|v| !v.is_empty()).map(str::to_string);
    if let Some(quoted) = &quoted {
        let hit: Option<String> = sqlx::query_scalar(r#"select id from "DirectMessage" where id = $1 and "conversationId" = $2 limit 1"#)
            .bind(quoted)
            .bind(&conversation_id)
            .fetch_optional(&s.db)
            .await?;
        if hit.is_none() {
            return Err(crate::ctx::not_found("QUOTED_MESSAGE_NOT_FOUND", "Alıntılanan mesaj bulunamadı."));
        }
    }
    let id = uuid();
    sqlx::query(r#"insert into "DirectMessage" (id, "conversationId", "authorId", content, "quotedMessageId", "createdAt") values ($1, $2, $3, $4, $5, $6)"#)
        .bind(&id)
        .bind(&conversation_id)
        .bind(&me.id)
        .bind(content.trim())
        .bind(&quoted)
        .bind(now())
        .execute(&s.db)
        .await?;
    let message = message_json(&s.db, &id).await?;
    let column = if conv.user_a == me.id { r#""userALastReadAt""# } else { r#""userBLastReadAt""# };
    sqlx::query(sql(format!(r#"update "DirectConversation" set "updatedAt" = $2, {column} = $2 where id = $1"#)))
        .bind(&conversation_id)
        .bind(at)
        .execute(&s.db)
        .await?;
    notify(&s, &conv, "direct:created", json!({ "conversationId": conversation_id, "message": message })).await;
    created(message)
}

async fn edit(State(s): State<AppState>, me: Me, Path((org, conversation_id, message_id)): Path<(String, String, String)>, Body(body): Body) -> Reply {
    let content = message_dto(&body)?;
    membership(&s.db, &org, &me.id).await?;
    let conv = conversation(&s.db, &org, &conversation_id, &me.id).await?;
    let hit: Option<String> = sqlx::query_scalar(r#"select id from "DirectMessage" where id = $1 and "conversationId" = $2 and "authorId" = $3 limit 1"#)
        .bind(&message_id)
        .bind(&conversation_id)
        .bind(&me.id)
        .fetch_optional(&s.db)
        .await?;
    if hit.is_none() {
        return Err(crate::ctx::not_found("MESSAGE_NOT_FOUND", "Mesaj bulunamadı."));
    }
    sqlx::query(r#"update "DirectMessage" set content = $2, "editedAt" = $3 where id = $1"#)
        .bind(&message_id)
        .bind(content.trim())
        .bind(now())
        .execute(&s.db)
        .await?;
    let message = message_json(&s.db, &message_id).await?;
    notify(&s, &conv, "direct:updated", json!({ "conversationId": conversation_id, "message": message })).await;
    ok(message)
}

async fn remove(State(s): State<AppState>, me: Me, Path((org, conversation_id, message_id)): Path<(String, String, String)>) -> Reply {
    membership(&s.db, &org, &me.id).await?;
    let conv = conversation(&s.db, &org, &conversation_id, &me.id).await?;
    let author: Option<String> = sqlx::query_scalar(r#"select "authorId" from "DirectMessage" where id = $1 and "conversationId" = $2 limit 1"#)
        .bind(&message_id)
        .bind(&conversation_id)
        .fetch_optional(&s.db)
        .await?;
    let missing = || crate::ctx::not_found("MESSAGE_NOT_FOUND", "Mesaj bulunamadı.");
    let author = author.ok_or_else(missing)?;
    if author != me.id && !crate::perm::has_any(&s.db, &org, &me.id, "message.delete_unlimited").await? {
        return Err(missing());
    }
    sqlx::query(r#"delete from "DirectMessage" where id = $1"#).bind(&message_id).execute(&s.db).await?;
    notify(&s, &conv, "direct:deleted", json!({ "conversationId": conversation_id, "messageId": message_id })).await;
    ok(json!({ "deleted": true }))
}

async fn react(State(s): State<AppState>, me: Me, Path((org, conversation_id, message_id)): Path<(String, String, String)>, Body(body): Body) -> Reply {
    let emoji = react_dto(&body)?;
    let conv = conversation(&s.db, &org, &conversation_id, &me.id).await?;
    let hit: Option<String> = sqlx::query_scalar(r#"select id from "DirectMessage" where id = $1 and "conversationId" = $2 limit 1"#)
        .bind(&message_id)
        .bind(&conversation_id)
        .fetch_optional(&s.db)
        .await?;
    if hit.is_none() {
        return Err(crate::ctx::not_found("MESSAGE_NOT_FOUND", "Mesaj bulunamadı."));
    }
    let emoji = emoji.trim().to_string();
    let existing: Option<String> = sqlx::query_scalar(r#"select id from "ChatReaction" where "directMessageId" = $1 and "userId" = $2 and emoji = $3 limit 1"#)
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
            sqlx::query(r#"insert into "ChatReaction" (id, "directMessageId", "userId", emoji, "createdAt") values ($1, $2, $3, $4, $5)"#)
                .bind(uuid())
                .bind(&message_id)
                .bind(&me.id)
                .bind(&emoji)
                .bind(now())
                .execute(&s.db)
                .await?;
        }
    }
    let rows: Vec<sqlx::types::Json<Value>> = sqlx::query_scalar(
        r#"select json_build_object('emoji', emoji, 'userId', "userId") from "ChatReaction" where "directMessageId" = $1 order by "createdAt" asc"#,
    )
    .bind(&message_id)
    .fetch_all(&s.db)
    .await?;
    let rows: Vec<Value> = rows.into_iter().map(|j| j.0).collect();
    let grouped = group_reactions(&rows, &me.id);
    notify(&s, &conv, "direct:reactions", json!({ "conversationId": conversation_id, "messageId": message_id, "rows": rows })).await;
    created(grouped)
}

async fn read(State(s): State<AppState>, me: Me, Path((org, conversation_id)): Path<(String, String)>) -> Reply {
    membership(&s.db, &org, &me.id).await?;
    let conv = conversation(&s.db, &org, &conversation_id, &me.id).await?;
    let at = now();
    let column = if conv.user_a == me.id { r#""userALastReadAt""# } else { r#""userBLastReadAt""# };
    sqlx::query(sql(format!(r#"update "DirectConversation" set {column} = $2, "updatedAt" = $3 where id = $1"#)))
        .bind(&conversation_id)
        .bind(at)
        .bind(now())
        .execute(&s.db)
        .await?;
    created(json!({ "lastReadAt": iso_of(at) }))
}

pub fn routes() -> Router<AppState> {
    let base = "/organizations/{organizationId}/direct";
    Router::new()
        .route(base, get(list).post(start))
        .route(&format!("{base}/{{conversationId}}/messages"), get(messages).post(send))
        .route(&format!("{base}/{{conversationId}}/messages/{{messageId}}"), patch(edit).delete(remove))
        .route(&format!("{base}/{{conversationId}}/messages/{{messageId}}/reactions"), post(react))
        .route(&format!("{base}/{{conversationId}}/read"), post(read))
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
}
