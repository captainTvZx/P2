//! Birebir sohbetler — `server/src/direct.ts` karşılığı (sohbet listesi, mesajlar, tepkiler, okundu).
//!
//! İki ayrı tür vardır (docs/arkadaslar.md):
//! - **Ana sayfa sohbeti** (`projectId` NULL): hesaba bağlı; arkadaşlar ya da ortak projesi olanlar,
//!   engel yoksa. Projeden ayrılınca da kalır.
//! - **Proje sohbeti** (`projectId` dolu): o projenin kişi listesinden açılır, yalnız iki kişi de
//!   projenin üyesiyken erişilir; gönderen o projede `direct.send` taşımalıdır.
//!
//! Aynı iki kişinin bir Ana sayfa sohbeti ve paylaştıkları her proje için birer sohbeti olabilir;
//! mesajlar, okunmamışlar ve yıldızlar ayrıdır. Olay yüklerinde `projectId` bulunur (NULL = Ana sayfa).
//!
//! Listeden kaldırma kişi başınadır (`userAHidden` / `userBHidden`): mesaj silinmez; yeni mesaj
//! gelince ya da sohbet yeniden açılınca sohbet listeye geri döner.

use axum::Router;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::routing::{get, patch, post, put};
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
    r#", 'author', (select json_build_object('id', u.id, 'username', u.username, 'displayName', u."displayName", 'avatarUrl', u."avatarUrl") from "User" u where u.id = m."authorId"),
    'quotedMessage', (select json_build_object('id', q.id, 'content', q.content, 'author', (select json_build_object('displayName', qu."displayName") from "User" qu where qu.id = q."authorId")) from "DirectMessage" q where q.id = m."quotedMessageId"),
    'reactions', coalesce((select json_agg(json_build_object('emoji', r.emoji, 'userId', r."userId") order by r."createdAt") from "ChatReaction" r where r."directMessageId" = m.id), '[]'::json))"#
);

/// `shape(row, userId, unread)` (c = "DirectConversation", $1 = kullanıcı). `unread` SQL ifadesi dışarıdan.
fn conversation_json(unread: &str) -> String {
    format!(
        r#"json_build_object('id', c.id, 'projectId', c."projectId",
        'hidden', (case when c."userAId" = $1 then c."userAHidden" else c."userBHidden" end),
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
    /// `None` = Ana sayfa sohbeti; dolu = proje sohbeti.
    project_id: Option<String>,
}

impl Conversation {
    fn users(&self) -> Vec<String> {
        vec![self.user_a.clone(), self.user_b.clone()]
    }
    fn other(&self, me: &str) -> String {
        if self.user_a == me { self.user_b.clone() } else { self.user_a.clone() }
    }
}

/// Proje sohbetinin açık olma koşulu (SQL, c = "DirectConversation"): proje silinmemiş ve iki kişi de üye.
const PROJECT_OPEN: &str = r#"(c."projectId" is null or exists (select 1 from "Project" p where p.id = c."projectId" and p."deletedAt" is null
    and exists (select 1 from "ProjectMember" x where x."projectId" = p.id and x."userId" = c."userAId")
    and exists (select 1 from "ProjectMember" y where y."projectId" = p.id and y."userId" = c."userBId")))"#;

/// Proje sohbeti artık kapalı (biri projeden ayrıldı / proje silindi): mesajlar durur ama erişilmez.
fn project_closed() -> ApiError {
    crate::ctx::forbidden("PROJECT_CONVERSATION_CLOSED", "Bu sohbet yalnızca ikiniz de projedeyken açıktır.").safe()
}

async fn conversation(db: &sqlx::PgPool, organization_id: &str, conversation_id: &str, user_id: &str) -> ApiResult<Conversation> {
    let row: Option<(String, String, Option<String>, bool)> = sqlx::query_as(sql(format!(
        r#"select c."userAId", c."userBId", c."projectId", {PROJECT_OPEN} from "DirectConversation" c
           where c.id = $1 and c."organizationId" = $2 and (c."userAId" = $3 or c."userBId" = $3) limit 1"#
    )))
    .bind(conversation_id)
    .bind(organization_id)
    .bind(user_id)
    .fetch_optional(db)
    .await?;
    let (user_a, user_b, project_id, open) = row.ok_or_else(|| crate::ctx::not_found("CONVERSATION_NOT_FOUND", "Sohbet bulunamadı."))?;
    if !open {
        return Err(project_closed());
    }
    Ok(Conversation { user_a, user_b, project_id })
}

/// Mesaj gönderme kuralı. Proje sohbeti: gönderen o projede `direct.send` taşır (iki kişinin üyeliği
/// `conversation` içinde denetlendi) ve engel yok. Ana sayfa: arkadaş ya da ortak proje, engel yok.
async fn require_send(db: &sqlx::PgPool, conv: &Conversation, me: &str) -> ApiResult<()> {
    let other = conv.other(me);
    match &conv.project_id {
        Some(project) => {
            if crate::friends::blocked_between(db, me, &other).await? {
                return Err(crate::friends::blocked_error());
            }
            crate::perm::require_in(db, project, me, "direct.send").await.map(|_| ())
        }
        None => crate::perm::require_shared(db, me, &other, "direct.send").await,
    }
}

/// Olay yükü: her birebir olayda sohbetin kapsamı da gider (istemci doğru listeye yönlendirir).
fn event(conv: &Conversation, conversation_id: &str, mut extra: Value) -> Value {
    if let Some(map) = extra.as_object_mut() {
        map.insert("conversationId".into(), json!(conversation_id));
        map.insert("projectId".into(), json!(conv.project_id));
    }
    extra
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

/// `GET …/direct`: bütün sohbetlerim (Ana sayfa + üyesi olduğum projelerdeki açık proje sohbetleri).
/// `?scope=home` yalnız Ana sayfa, `?projectId=<id>` yalnız o proje. Gizlenenler `hidden: true` ile döner.
async fn list(State(s): State<AppState>, me: Me, Path(org): Path<String>, q: Query) -> Reply {
    membership(&s.db, &org, &me.id).await?;
    let project = q.get("projectId").filter(|p| !p.is_empty()).cloned();
    let home_only = q.get("scope").is_some_and(|v| v == "home");
    let rows: Vec<sqlx::types::Json<Value>> = sqlx::query_scalar(sql(format!(
        r#"select {} from "DirectConversation" c where c."organizationId" = $2 and (c."userAId" = $1 or c."userBId" = $1) and {PROJECT_OPEN}
           and ($3::text is null or c."projectId" = $3) and (not $4 or c."projectId" is null) order by c."updatedAt" desc"#,
        conversation_json(UNREAD)
    )))
    .bind(&me.id)
    .bind(&org)
    .bind(&project)
    .bind(home_only)
    .fetch_all(&s.db)
    .await?;
    ok(Value::Array(rows.into_iter().map(|j| j.0).collect()))
}

async fn start(State(s): State<AppState>, me: Me, Path(org): Path<String>, Body(body): Body) -> Reply {
    let mut c = Checker::new(&body);
    c.field("userId", &[(c.str("userId").is_some(), None)]);
    c.finish()?;
    let other = body["userId"].as_str().unwrap_or_default().to_string();
    let project = body.get("projectId").and_then(Value::as_str).filter(|p| !p.is_empty()).map(str::to_string);
    membership(&s.db, &org, &me.id).await?;
    if other == me.id {
        return Err(ApiError::conflict("SELF_CONVERSATION", "Kendinle sohbet başlatamazsın."));
    }
    if let Some(project) = project {
        return start_in_project(&s, &me, &org, &project, &other).await;
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
        r#"select {} from "DirectConversation" c where c."organizationId" = $2 and c."userAId" = $3 and c."userBId" = $4 and c."projectId" is null"#,
        conversation_json(UNREAD)
    )))
    .bind(&me.id)
    .bind(&org)
    .bind(&user_a)
    .bind(&user_b)
    .fetch_optional(&s.db)
    .await?;
    if let Some(existing) = existing {
        return reopen(&s, &me.id, existing.0).await;
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
    notify(&s, &Conversation { user_a, user_b, project_id: None }, "direct:conversation", json!({ "conversationId": id, "projectId": Value::Null })).await;
    created(shaped.0)
}

/// Var olan sohbet yeniden açıldı: açan kişi için listeye geri döner (gizliyse).
async fn reopen(s: &AppState, me: &str, mut shaped: Value) -> Reply {
    if shaped["hidden"] == json!(true) {
        let id = shaped["id"].as_str().unwrap_or_default().to_string();
        set_hidden(&s.db, &id, me, false).await?;
        shaped["hidden"] = json!(false);
        crate::realtime::emit_users(s, &[me.to_string()], "direct:conversation", &json!({ "conversationId": id, "projectId": shaped["projectId"], "hidden": false })).await;
    }
    created(shaped)
}

async fn set_hidden(db: &sqlx::PgPool, conversation_id: &str, user_id: &str, hidden: bool) -> ApiResult<()> {
    // Gizlerken okunmuş da sayılır: listede görünmeyen sohbet rozet taşımasın.
    sqlx::query(
        r#"update "DirectConversation" set
             "userAHidden" = case when "userAId" = $2 then $3 else "userAHidden" end,
             "userBHidden" = case when "userBId" = $2 then $3 else "userBHidden" end,
             "userALastReadAt" = case when "userAId" = $2 and $3 then $4 else "userALastReadAt" end,
             "userBLastReadAt" = case when "userBId" = $2 and $3 then $4 else "userBLastReadAt" end
           where id = $1"#,
    )
    .bind(conversation_id)
    .bind(user_id)
    .bind(hidden)
    .bind(now())
    .execute(db)
    .await?;
    Ok(())
}

/// Proje sohbeti: projenin kişi listesinden açılır. İki kişi de projenin üyesi olmalı, açan kişi o
/// projede `direct.send` taşımalı, engel olmamalı. Ana sayfa sohbetinden ayrıdır.
async fn start_in_project(s: &AppState, me: &Me, org: &str, project: &str, other: &str) -> Reply {
    crate::perm::require_project_access(&s.db, org, &me.id, project).await?;
    if !crate::perm::is_member(&s.db, project, &me.id).await? || !crate::perm::is_member(&s.db, project, other).await? {
        return Err(crate::ctx::forbidden("NOT_PROJECT_MEMBER", "Bu kişi projenin üyesi değil.").safe());
    }
    if crate::friends::blocked_between(&s.db, &me.id, other).await? {
        return Err(crate::friends::blocked_error());
    }
    let (user_a, user_b) = if me.id.as_str() <= other { (me.id.clone(), other.to_string()) } else { (other.to_string(), me.id.clone()) };
    let existing: Option<sqlx::types::Json<Value>> = sqlx::query_scalar(sql(format!(
        r#"select {} from "DirectConversation" c where c."projectId" = $2 and c."userAId" = $3 and c."userBId" = $4"#,
        conversation_json(UNREAD)
    )))
    .bind(&me.id)
    .bind(project)
    .bind(&user_a)
    .bind(&user_b)
    .fetch_optional(&s.db)
    .await?;
    if let Some(existing) = existing {
        return reopen(s, &me.id, existing.0).await;
    }
    crate::perm::require_in(&s.db, project, &me.id, "direct.send").await?;
    let id = uuid();
    let at = now();
    sqlx::query(r#"insert into "DirectConversation" (id, "organizationId", "projectId", "userAId", "userBId", "createdAt", "updatedAt") values ($1, $2, $3, $4, $5, $6, $6)"#)
        .bind(&id)
        .bind(org)
        .bind(project)
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
    notify(s, &Conversation { user_a, user_b, project_id: Some(project.to_string()) }, "direct:conversation", json!({ "conversationId": id, "projectId": project })).await;
    created(shaped.0)
}

/// `PUT …/direct/:id/hidden { hidden }`: sohbeti kendi listemden kaldırır / geri getirir (mesajlar silinmez).
async fn hide(State(s): State<AppState>, me: Me, Path((org, conversation_id)): Path<(String, String)>, Body(body): Body) -> Reply {
    let mut c = Checker::new(&body);
    c.field("hidden", &[(body.get("hidden").is_some_and(Value::is_boolean), None)]);
    c.finish()?;
    let hidden = body["hidden"].as_bool().unwrap_or(true);
    membership(&s.db, &org, &me.id).await?;
    // Kapalı proje sohbeti de listeden kaldırılabilsin diye erişim denetimi yalnız taraf olmaya bakar.
    let row: Option<Option<String>> = sqlx::query_scalar(r#"select "projectId" from "DirectConversation" where id = $1 and "organizationId" = $2 and ("userAId" = $3 or "userBId" = $3)"#)
        .bind(&conversation_id)
        .bind(&org)
        .bind(&me.id)
        .fetch_optional(&s.db)
        .await?;
    let project = row.ok_or_else(|| crate::ctx::not_found("CONVERSATION_NOT_FOUND", "Sohbet bulunamadı."))?;
    set_hidden(&s.db, &conversation_id, &me.id, hidden).await?;
    // Yalnız bana (öteki oturumlarım da güncellensin); karşı taraf hiçbir şey görmez.
    crate::realtime::emit_users(&s, &[me.id.clone()], "direct:conversation", &json!({ "conversationId": conversation_id, "projectId": project, "hidden": hidden })).await;
    ok(json!({ "id": conversation_id, "hidden": hidden }))
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
    require_send(&s.db, &conv, &me.id).await?;
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
    // Yeni mesaj sohbeti iki tarafın listesine de geri getirir (gizlenmişse).
    sqlx::query(sql(format!(r#"update "DirectConversation" set "updatedAt" = $2, {column} = $2, "userAHidden" = false, "userBHidden" = false where id = $1"#)))
        .bind(&conversation_id)
        .bind(at)
        .execute(&s.db)
        .await?;
    notify(&s, &conv, "direct:created", event(&conv, &conversation_id, json!({ "message": message }))).await;
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
    notify(&s, &conv, "direct:updated", event(&conv, &conversation_id, json!({ "message": message }))).await;
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
    let unlimited = match &conv.project_id {
        Some(project) => crate::perm::effective_in(&s.db, project, &me.id).await?.has("message.delete_unlimited"),
        None => crate::perm::has_any(&s.db, &org, &me.id, "message.delete_unlimited").await?,
    };
    if author != me.id && !unlimited {
        return Err(missing());
    }
    sqlx::query(r#"delete from "DirectMessage" where id = $1"#).bind(&message_id).execute(&s.db).await?;
    notify(&s, &conv, "direct:deleted", event(&conv, &conversation_id, json!({ "messageId": message_id }))).await;
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
    notify(&s, &conv, "direct:reactions", event(&conv, &conversation_id, json!({ "messageId": message_id, "rows": rows }))).await;
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
        .route(&format!("{base}/{{conversationId}}/hidden"), put(hide))
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
}
