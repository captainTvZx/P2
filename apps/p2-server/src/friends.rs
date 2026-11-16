//! Arkadaşlar ve engeller — hesaba bağlı, projelerden bağımsız (docs/arkadaslar.md).
//!
//! `Friendship` satırı bir çift için tektir (PENDING: istek, ACCEPTED: arkadaş); engeller yönlüdür
//! ve `UserBlock` tablosunda durur. Bireysel mesaj/arama kuralı (`perm::require_shared`):
//! iki kişi arkadaşsa YA DA bir projeyi paylaşıyorsa ve hiçbiri diğerini engellemediyse.
//!
//! Olaylar (iki tarafın `user:` odalarına): `friend:request`, `friend:accepted`, `friend:removed`,
//! `friend:blocked`, `friend:unblocked` (yalnız engeli kaldırana).

use axum::Router;
use axum::extract::{Path, State};
use axum::routing::{delete, get, post};
use serde_json::{Value, json};
use sqlx::PgPool;

use crate::AppState;
use crate::ctx::{Body, Me, Reply, created, forbidden, many_json, not_found, now, ok, sql, uuid};
use crate::error::{ApiError, ApiResult};

/// Arkadaş listesinde görünen kullanıcı alanları (alias `u`).
const USER_FIELDS: &str = r#"'id', u.id, 'username', u.username, 'displayName', u."displayName", 'title', u.title, 'avatarUrl', u."avatarUrl""#;

/// Sunucudan yasaklanan hesaplar (A ajanının `ServerBan` tablosu) arkadaş listesinde ve istek
/// aramasında görünmez; yasak kalkınca arkadaşlık olduğu gibi geri gelir.
const NOT_BANNED: &str = r#"not exists (select 1 from "ServerBan" sb where sb."userId" = u.id)"#;

// ------------------------------------------------------------------ sorgular (diğer modüller de kullanır)

/// İki kişi arkadaş mı (kabul edilmiş istek).
pub async fn are_friends(db: &PgPool, a: &str, b: &str) -> ApiResult<bool> {
    let hit: Option<i32> = sqlx::query_scalar(
        r#"select 1 from "Friendship" where status::text = 'ACCEPTED' and (("requesterId" = $1 and "addresseeId" = $2) or ("requesterId" = $2 and "addresseeId" = $1)) limit 1"#,
    )
    .bind(a)
    .bind(b)
    .fetch_optional(db)
    .await?;
    Ok(hit.is_some())
}

/// `blocker`, `blocked` kişisini engellemiş mi.
pub async fn has_blocked(db: &PgPool, blocker: &str, blocked: &str) -> ApiResult<bool> {
    let hit: Option<i32> = sqlx::query_scalar(r#"select 1 from "UserBlock" where "blockerId" = $1 and "blockedId" = $2"#)
        .bind(blocker)
        .bind(blocked)
        .fetch_optional(db)
        .await?;
    Ok(hit.is_some())
}

/// İkisinden biri diğerini engellemiş mi.
pub async fn blocked_between(db: &PgPool, a: &str, b: &str) -> ApiResult<bool> {
    let hit: Option<i32> = sqlx::query_scalar(
        r#"select 1 from "UserBlock" where ("blockerId" = $1 and "blockedId" = $2) or ("blockerId" = $2 and "blockedId" = $1) limit 1"#,
    )
    .bind(a)
    .bind(b)
    .fetch_optional(db)
    .await?;
    Ok(hit.is_some())
}

/// Verilen kişilerden, kişinin arkadaşı olanlar (çevrimiçi/durum yayını için).
pub async fn friends_among(db: &PgPool, user_id: &str, candidates: &[String]) -> ApiResult<Vec<String>> {
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    Ok(sqlx::query_scalar(
        r#"select case when f."requesterId" = $1 then f."addresseeId" else f."requesterId" end from "Friendship" f
           where f.status::text = 'ACCEPTED' and ((f."requesterId" = $1 and f."addresseeId" = any($2)) or (f."addresseeId" = $1 and f."requesterId" = any($2)))"#,
    )
    .bind(user_id)
    .bind(candidates)
    .fetch_all(db)
    .await?)
}

/// Engel hatası: kimin engellediği karşı tarafa söylenmez.
pub fn blocked_error() -> ApiError {
    forbidden("USER_BLOCKED", "Bu kişiyle iletişim kuramazsın.").safe()
}

// ------------------------------------------------------------------ yardımcılar

async fn user_json(db: &PgPool, user_id: &str, with_status: bool) -> ApiResult<Value> {
    let status = if with_status { r#", 'status', u.status::text"# } else { "" };
    let row: Option<sqlx::types::Json<Value>> = sqlx::query_scalar(sql(format!(r#"select json_build_object({USER_FIELDS}{status}) from "User" u where u.id = $1"#)))
        .bind(user_id)
        .fetch_optional(db)
        .await?;
    row.map(|j| j.0).ok_or_else(user_missing)
}

fn user_missing() -> ApiError {
    not_found("USER_NOT_FOUND", "Böyle bir kullanıcı bulunamadı.").safe()
}

fn request_missing() -> ApiError {
    not_found("FRIEND_REQUEST_NOT_FOUND", "Arkadaşlık isteği bulunamadı.").safe()
}

struct Row {
    id: String,
    requester: String,
    addressee: String,
    status: String,
}

async fn pair(db: &PgPool, a: &str, b: &str) -> ApiResult<Option<Row>> {
    let row: Option<(String, String, String, String)> = sqlx::query_as(
        r#"select id, "requesterId", "addresseeId", status::text from "Friendship"
           where ("requesterId" = $1 and "addresseeId" = $2) or ("requesterId" = $2 and "addresseeId" = $1) limit 1"#,
    )
    .bind(a)
    .bind(b)
    .fetch_optional(db)
    .await?;
    Ok(row.map(|(id, requester, addressee, status)| Row { id, requester, addressee, status }))
}

async fn pending(db: &PgPool, request_id: &str) -> ApiResult<Row> {
    let row: Option<(String, String, String, String)> = sqlx::query_as(r#"select id, "requesterId", "addresseeId", status::text from "Friendship" where id = $1 and status::text = 'PENDING'"#)
        .bind(request_id)
        .fetch_optional(db)
        .await?;
    row.map(|(id, requester, addressee, status)| Row { id, requester, addressee, status }).ok_or_else(request_missing)
}

async fn emit(state: &AppState, users: [&str; 2], event: &str, payload: Value) {
    crate::realtime::emit_users(state, &[users[0].to_string(), users[1].to_string()], event, &payload).await;
}

/// İsteği kabul eder: iki tarafa `friend:accepted`, ardından birbirlerinin güncel durumu.
async fn accept_row(state: &AppState, row: &Row) -> ApiResult<Value> {
    sqlx::query(r#"update "Friendship" set status = 'ACCEPTED'::"FriendshipStatus", "acceptedAt" = $2 where id = $1"#)
        .bind(&row.id)
        .bind(now())
        .execute(&state.db)
        .await?;
    ensure_common_org(&state.db, &row.requester, &row.addressee).await?;
    let requester = user_json(&state.db, &row.requester, true).await?;
    let addressee = user_json(&state.db, &row.addressee, true).await?;
    let payload = json!({ "requestId": row.id, "requester": requester, "addressee": addressee });
    emit(state, [&row.requester, &row.addressee], "friend:accepted", payload.clone()).await;
    // Arkadaşlar artık birbirinin çevrimiçi durumunu görür.
    crate::realtime::emit_users(state, &[row.addressee.clone()], "presence:update", &json!({ "userId": row.requester, "status": requester["status"], "activity": Value::Null })).await;
    crate::realtime::emit_users(state, &[row.requester.clone()], "presence:update", &json!({ "userId": row.addressee, "status": addressee["status"], "activity": Value::Null })).await;
    Ok(payload)
}

/// Bireysel sohbetler bir kuruluşun (kurulumun hesap kabı) altında durur. Arkadaş olan iki hesabın
/// ortak kuruluşu yoksa (biri hiç projeye girmemişse) biri diğerinin en eski kuruluşuna `MEMBER`
/// eklenir; kuruluş üyeliği hiçbir projeye erişim vermez (docs/proje-bazli-yapi.md).
async fn ensure_common_org(db: &PgPool, a: &str, b: &str) -> ApiResult<()> {
    let common: Option<i32> = sqlx::query_scalar(
        r#"select 1 from "OrganizationMember" x join "OrganizationMember" y on y."organizationId" = x."organizationId" where x."userId" = $1 and y."userId" = $2 limit 1"#,
    )
    .bind(a)
    .bind(b)
    .fetch_optional(db)
    .await?;
    if common.is_some() {
        return Ok(());
    }
    for (host, guest) in [(a, b), (b, a)] {
        let org: Option<String> = sqlx::query_scalar(r#"select "organizationId" from "OrganizationMember" where "userId" = $1 order by "joinedAt" asc limit 1"#)
            .bind(host)
            .fetch_optional(db)
            .await?;
        if let Some(org) = org {
            sqlx::query(
                r#"insert into "OrganizationMember" ("organizationId", "userId", role, permissions, "deniedPermissions", "joinedAt") values ($1, $2, 'MEMBER', '{}', '{}', $3)
                   on conflict do nothing"#,
            )
            .bind(&org)
            .bind(guest)
            .bind(now())
            .execute(db)
            .await?;
            return Ok(());
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ uçlar

/// `GET /friends`: `{ friends, incoming, outgoing, blocked }`.
async fn list(State(s): State<AppState>, me: Me) -> Reply {
    let other = r#"(case when f."requesterId" = $1 then f."addresseeId" else f."requesterId" end)"#;
    let friends = many_json(
        sqlx::query_scalar(sql(format!(
            r#"select json_build_object({USER_FIELDS}, 'status', u.status::text, 'since', {since}) from "Friendship" f join "User" u on u.id = {other}
               where f.status::text = 'ACCEPTED' and (f."requesterId" = $1 or f."addresseeId" = $1) and {NOT_BANNED}
               order by lower(u."displayName"), u.id"#,
            since = crate::iso!(r#"coalesce(f."acceptedAt", f."createdAt")"#),
        )))
        .bind(&me.id),
        &s.db,
    )
    .await?;
    let request = |column: &str, mine: &str| {
        format!(
            r#"select json_build_object('id', f.id, 'user', json_build_object({USER_FIELDS}), 'createdAt', {at}) from "Friendship" f join "User" u on u.id = f."{column}"
               where f.status::text = 'PENDING' and f."{mine}" = $1 and {NOT_BANNED} order by f."createdAt" desc"#,
            at = crate::iso!(r#"f."createdAt""#),
        )
    };
    let incoming = many_json(sqlx::query_scalar(sql(request("requesterId", "addresseeId"))).bind(&me.id), &s.db).await?;
    let outgoing = many_json(sqlx::query_scalar(sql(request("addresseeId", "requesterId"))).bind(&me.id), &s.db).await?;
    let blocked = many_json(
        sqlx::query_scalar(sql(format!(
            r#"select json_build_object('user', json_build_object({USER_FIELDS}), 'createdAt', {at}) from "UserBlock" b join "User" u on u.id = b."blockedId"
               where b."blockerId" = $1 order by b."createdAt" desc"#,
            at = crate::iso!(r#"b."createdAt""#),
        )))
        .bind(&me.id),
        &s.db,
    )
    .await?;
    ok(json!({ "friends": friends, "incoming": incoming, "outgoing": outgoing, "blocked": blocked }))
}

/// `POST /friends/requests { username | userId }`.
async fn request(State(s): State<AppState>, me: Me, body: Body) -> Reply {
    let by_id = body.str("userId").map(str::trim).filter(|v| !v.is_empty());
    let by_name = body.str("username").map(|v| v.trim().trim_start_matches('@').to_string()).filter(|v| !v.is_empty());
    let target: Option<String> = match (by_id, by_name) {
        (Some(id), _) => sqlx::query_scalar(sql(format!(r#"select u.id from "User" u where u.id = $1 and {NOT_BANNED}"#))).bind(id).fetch_optional(&s.db).await?,
        (None, Some(name)) => sqlx::query_scalar(sql(format!(r#"select u.id from "User" u where lower(u.username) = lower($1) and {NOT_BANNED} limit 1"#))).bind(name).fetch_optional(&s.db).await?,
        (None, None) => return Err(ApiError::bad_request("VALIDATION_ERROR", "Kullanıcı adı gerekli.").field("username")),
    };
    let target = target.ok_or_else(user_missing)?;
    if target == me.id {
        return Err(ApiError::conflict("SELF_FRIEND", "Kendine arkadaşlık isteği gönderemezsin.").safe());
    }
    if has_blocked(&s.db, &me.id, &target).await? {
        return Err(ApiError::conflict("BLOCKED_BY_ME", "Bu kişiyi engelledin. Önce engeli kaldır.").safe());
    }
    if has_blocked(&s.db, &target, &me.id).await? {
        return Err(blocked_error());
    }
    if let Some(row) = pair(&s.db, &me.id, &target).await? {
        if row.status == "ACCEPTED" {
            return Err(ApiError::conflict("ALREADY_FRIENDS", "Zaten arkadaşsınız.").safe());
        }
        if row.requester == me.id {
            return Err(ApiError::conflict("FRIEND_REQUEST_EXISTS", "Bu kişiye zaten istek gönderdin.").safe());
        }
        // Karşı taraf zaten istek göndermiş: istek kabul edilmiş sayılır.
        let payload = accept_row(&s, &row).await?;
        return created(json!({ "status": "ACCEPTED", "requestId": row.id, "user": payload["requester"] }));
    }
    let (id, at) = (uuid(), now());
    sqlx::query(r#"insert into "Friendship" (id, "requesterId", "addresseeId", status, "createdAt") values ($1, $2, $3, 'PENDING'::"FriendshipStatus", $4)"#)
        .bind(&id)
        .bind(&me.id)
        .bind(&target)
        .bind(at)
        .execute(&s.db)
        .await?;
    let from = user_json(&s.db, &me.id, false).await?;
    let to = user_json(&s.db, &target, false).await?;
    emit(&s, [&me.id, &target], "friend:request", json!({ "requestId": id, "from": from, "to": to, "createdAt": crate::direct::iso_of(at) })).await;
    created(json!({ "status": "PENDING", "requestId": id, "user": to }))
}

async fn accept(State(s): State<AppState>, me: Me, Path(request_id): Path<String>) -> Reply {
    let row = pending(&s.db, &request_id).await?;
    if row.addressee != me.id {
        return Err(request_missing());
    }
    if blocked_between(&s.db, &row.requester, &row.addressee).await? {
        return Err(blocked_error());
    }
    let payload = accept_row(&s, &row).await?;
    created(json!({ "status": "ACCEPTED", "requestId": row.id, "user": payload["requester"] }))
}

async fn drop_request(s: &AppState, me: &Me, request_id: &str, as_addressee: bool, reason: &str) -> Reply {
    let row = pending(&s.db, request_id).await?;
    if (as_addressee && row.addressee != me.id) || (!as_addressee && row.requester != me.id) {
        return Err(request_missing());
    }
    sqlx::query(r#"delete from "Friendship" where id = $1"#).bind(&row.id).execute(&s.db).await?;
    emit(s, [&row.requester, &row.addressee], "friend:removed", json!({ "requestId": row.id, "userIds": [row.requester, row.addressee], "byId": me.id, "reason": reason })).await;
    ok(json!({ "removed": true }))
}

async fn decline(State(s): State<AppState>, me: Me, Path(request_id): Path<String>) -> Reply {
    drop_request(&s, &me, &request_id, true, "declined").await
}

async fn cancel(State(s): State<AppState>, me: Me, Path(request_id): Path<String>) -> Reply {
    drop_request(&s, &me, &request_id, false, "cancelled").await
}

/// `DELETE /friends/:userId`: arkadaşlıktan çıkar (bekleyen istek varsa onu da siler).
async fn unfriend(State(s): State<AppState>, me: Me, Path(user_id): Path<String>) -> Reply {
    let row = pair(&s.db, &me.id, &user_id).await?.ok_or_else(|| not_found("NOT_FRIENDS", "Bu kişiyle arkadaş değilsin.").safe())?;
    sqlx::query(r#"delete from "Friendship" where id = $1"#).bind(&row.id).execute(&s.db).await?;
    emit(&s, [&me.id, &user_id], "friend:removed", json!({ "requestId": row.id, "userIds": [row.requester, row.addressee], "byId": me.id, "reason": "removed" })).await;
    // Ortak proje de yoksa artık birbirlerinin durumunu görmezler.
    if crate::perm::shared_projects(&s.db, &me.id, &user_id).await?.is_empty() {
        crate::realtime::emit_users(&s, &[user_id.clone()], "presence:update", &json!({ "userId": me.id, "status": "OFFLINE", "activity": Value::Null })).await;
        crate::realtime::emit_users(&s, &[me.id.clone()], "presence:update", &json!({ "userId": user_id, "status": "OFFLINE", "activity": Value::Null })).await;
    }
    ok(json!({ "removed": true }))
}

/// `POST /blocks/:userId`: engeller; arkadaşlık ve bekleyen istekler silinir.
async fn block(State(s): State<AppState>, me: Me, Path(user_id): Path<String>) -> Reply {
    if user_id == me.id {
        return Err(ApiError::conflict("SELF_BLOCK", "Kendini engelleyemezsin.").safe());
    }
    let user = user_json(&s.db, &user_id, false).await?;
    let mut tx = s.db.begin().await?;
    sqlx::query(r#"insert into "UserBlock" ("blockerId", "blockedId", "createdAt") values ($1, $2, $3) on conflict do nothing"#)
        .bind(&me.id)
        .bind(&user_id)
        .bind(now())
        .execute(&mut *tx)
        .await?;
    sqlx::query(r#"delete from "Friendship" where ("requesterId" = $1 and "addresseeId" = $2) or ("requesterId" = $2 and "addresseeId" = $1)"#)
        .bind(&me.id)
        .bind(&user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    emit(&s, [&me.id, &user_id], "friend:blocked", json!({ "byId": me.id, "userId": user_id })).await;
    created(json!({ "blocked": true, "user": user }))
}

async fn unblock(State(s): State<AppState>, me: Me, Path(user_id): Path<String>) -> Reply {
    let done = sqlx::query(r#"delete from "UserBlock" where "blockerId" = $1 and "blockedId" = $2"#).bind(&me.id).bind(&user_id).execute(&s.db).await?;
    if done.rows_affected() == 0 {
        return Err(not_found("NOT_BLOCKED", "Bu kişi engelli değil.").safe());
    }
    crate::realtime::emit_users(&s, &[me.id.clone()], "friend:unblocked", &json!({ "byId": me.id, "userId": user_id })).await;
    ok(json!({ "blocked": false }))
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/friends", get(list))
        .route("/friends/requests", post(request))
        .route("/friends/requests/{requestId}/accept", post(accept))
        .route("/friends/requests/{requestId}/decline", post(decline))
        .route("/friends/requests/{requestId}", delete(cancel))
        .route("/friends/{userId}", delete(unfriend))
        .route("/blocks/{userId}", post(block).delete(unblock))
}
