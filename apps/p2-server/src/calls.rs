//! Sesli/görüntülü aramalar — `server/src/realtime.ts` arama bölümü.
//!
//! Aramalar bellekte tutulur (NestJS ile aynı): çağrı geçmişi saklanmaz, sunucu yeniden
//! başlarsa aramalar düşer. JS'teki gibi çağrı nesneleri paylaşılan referanslardır
//! (`Arc<Mutex<Call>>`); katılımcı/davetli listeleri ekleme sırasını korur (JS Map/Set).
//! Kilitler hiçbir zaman `await` boyunca tutulmaz.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};
use socketioxide::extract::{AckSender, Data, SocketRef, State};
use tokio::task::AbortHandle;

use crate::AppState;
use crate::realtime::{emit_channel, emit_users, jstr, now_ms, truthy, user_of};

/// Katılımcının medya durumu. JS `body.muted ?? media.muted` boolean olmayan değeri de
/// olduğu gibi sakladığı için alanlar ham JSON değeri.
#[derive(Clone)]
struct Media {
    muted: Value,
    deafened: Value,
    video: Value,
    screen: Value,
}

impl Media {
    fn new() -> Self {
        Media { muted: false.into(), deafened: false.into(), video: false.into(), screen: false.into() }
    }
}

struct Call {
    id: String,
    kind: &'static str,
    organization_id: String,
    channel_id: Option<String>,
    group_id: Option<String>,
    connected_at: Option<i64>,
    caller: Option<String>,
    timeout: Option<AbortHandle>,
    conversation_id: Option<String>,
    message_id: Option<String>,
    callee: Option<String>,
    participants: Vec<(String, Media)>,
    invited: Vec<String>,
}

type CallRef = Arc<Mutex<Call>>;

impl Call {
    fn new(kind: &'static str, organization_id: &str) -> Self {
        Call {
            id: crate::ctx::uuid(),
            kind,
            organization_id: organization_id.to_string(),
            channel_id: None,
            group_id: None,
            connected_at: None,
            caller: None,
            timeout: None,
            conversation_id: None,
            message_id: None,
            callee: None,
            participants: Vec::new(),
            invited: Vec::new(),
        }
    }
    fn has(&self, user_id: &str) -> bool {
        self.participants.iter().any(|(u, _)| u == user_id)
    }
    fn media(&self, user_id: &str) -> Option<Media> {
        self.participants.iter().find(|(u, _)| u == user_id).map(|(_, m)| m.clone())
    }
    /// JS `Map.set`: var olan katılımcı yerini korur.
    fn set(&mut self, user_id: &str, media: Media) {
        match self.participants.iter_mut().find(|(u, _)| u == user_id) {
            Some(slot) => slot.1 = media,
            None => self.participants.push((user_id.to_string(), media)),
        }
    }
    /// `participants.set(id, participants.get(id) ?? yeni)`.
    fn join(&mut self, user_id: &str) {
        if !self.has(user_id) {
            self.participants.push((user_id.to_string(), Media::new()));
        }
    }
    fn remove(&mut self, user_id: &str) {
        self.participants.retain(|(u, _)| u != user_id);
    }
    fn is_invited(&self, user_id: &str) -> bool {
        self.invited.iter().any(|u| u == user_id)
    }
    fn invite(&mut self, user_id: &str) {
        if !self.is_invited(user_id) {
            self.invited.push(user_id.to_string());
        }
    }
    fn uninvite(&mut self, user_id: &str) {
        self.invited.retain(|u| u != user_id);
    }
    fn keys(&self) -> Vec<String> {
        self.participants.iter().map(|(u, _)| u.clone()).collect()
    }
    /// `[...participants.keys(), ...invited]`
    fn audience(&self) -> Vec<String> {
        self.keys().into_iter().chain(self.invited.iter().cloned()).collect()
    }
    fn clear_timeout(&mut self) {
        if let Some(handle) = self.timeout.take() {
            handle.abort();
        }
    }
}

/// Açık aramalar (ekleme sırasıyla), kanal/grup başına açık arama ve kopma bekleme süreleri.
#[derive(Default)]
pub struct Registry {
    calls: Vec<(String, CallRef)>,
    channel_calls: HashMap<String, String>,
    group_calls: HashMap<String, String>,
    grace: HashMap<String, AbortHandle>,
}

impl Registry {
    fn get(&self, id: &str) -> Option<CallRef> {
        self.calls.iter().find(|(k, _)| k == id).map(|(_, c)| c.clone())
    }
    fn set(&mut self, call: &CallRef) {
        let id = call.lock().unwrap().id.clone();
        if !self.calls.iter().any(|(k, _)| k == &id) {
            self.calls.push((id, call.clone()));
        }
    }
    fn delete(&mut self, id: &str) {
        self.calls.retain(|(k, _)| k != id);
    }
    fn all(&self) -> Vec<CallRef> {
        self.calls.iter().map(|(_, c)| c.clone()).collect()
    }
}

fn find(state: &AppState, id: &str) -> Option<CallRef> {
    state.calls.lock().unwrap().get(id)
}

fn ack_ok(ack: AckSender, ok: bool) {
    let _ = ack.send(&json!({ "ok": ok }));
}

fn denied(ack: AckSender) {
    let _ = ack.send(&json!({ "ok": false, "code": "PERMISSION_DENIED" }));
}

pub(crate) fn register(socket: &SocketRef) {
    socket.on("call:start", call_start);
    socket.on("call:invite", call_invite);
    socket.on("call:accept", call_accept);
    socket.on("call:decline", call_decline);
    socket.on("call:leave", call_leave);
    socket.on("call:sync", call_sync);
    socket.on("call:media", call_media);
    socket.on("call:moderate", call_moderate);
    socket.on("call:signal", call_signal);
}

// ------------------------------------------------------------------ kopma bekleme süresi

/// Kullanıcı yeniden bağlandı: bekleyen düşürme iptal.
pub(crate) fn cancel_grace(state: &AppState, user_id: &str) {
    if let Some(handle) = state.calls.lock().unwrap().grace.remove(user_id) {
        handle.abort();
    }
}

/// Tünel/proxy sıfırlamalarında soket kısa süre kopar; kullanıcı 3 sn sonra hâlâ
/// bağlı değilse aramalardan düşer.
pub(crate) fn start_grace(state: &AppState, user_id: &str) {
    let st = state.clone();
    let id = user_id.to_string();
    let task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(3000)).await;
        st.calls.lock().unwrap().grace.remove(&id);
        if st.presence.lock().unwrap().count(&id) > 0 {
            return;
        }
        drop_from_calls(&st, &id).await;
    });
    if let Some(previous) = state.calls.lock().unwrap().grace.insert(user_id.to_string(), task.abort_handle()) {
        previous.abort();
    }
}

// ------------------------------------------------------------------ yardımcılar

async fn display_name_of(state: &AppState, user_id: &str) -> String {
    let cached = state.presence.lock().unwrap().names.get(user_id).cloned();
    if let Some(name) = cached.filter(|n| !n.is_empty()) {
        return name;
    }
    let row: Option<String> = sqlx::query_scalar(r#"select "displayName" from "User" where id = $1"#)
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or_default();
    let Some(name) = row else { return String::new() };
    state.presence.lock().unwrap().names.insert(user_id.to_string(), name.clone());
    name
}

async fn avatar_url_of(state: &AppState, user_id: &str) -> Value {
    let row: Option<Option<String>> = sqlx::query_scalar(r#"select "avatarUrl" from "User" where id = $1"#)
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or_default();
    row.flatten().map(Value::from).unwrap_or(Value::Null)
}

async fn is_member(state: &AppState, organization_id: &str, user_id: &str) -> bool {
    let hit: Option<i32> = sqlx::query_scalar(r#"select 1 from "OrganizationMember" where "organizationId" = $1 and "userId" = $2"#)
        .bind(organization_id)
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or_default();
    hit.is_some()
}

/// Sesli/görüntülü işlemler yetkiye bağlı; socket tarafında sessizce reddedilir.
async fn may(state: &AppState, organization_id: &str, user_id: &str, permission: &str, channel_id: Option<&str>) -> bool {
    match channel_id {
        Some(channel) => crate::perm::channel_permissions(&state.db, organization_id, user_id, channel)
            .await
            .map(|granted| granted.iter().any(|p| p == permission))
            .unwrap_or(false),
        None => crate::perm::has_any(&state.db, organization_id, user_id, permission).await.unwrap_or(false),
    }
}

async fn call_policy(state: &AppState, user_id: &str) -> &'static str {
    if state.presence.lock().unwrap().count(user_id) == 0 {
        return "reject";
    }
    let preference: Option<String> = sqlx::query_scalar(r#"select "statusPreference" from "User" where id = $1"#)
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or_default();
    match preference.as_deref() {
        None | Some("INVISIBLE") => "reject",
        Some("BUSY") => "silent",
        _ => "ring",
    }
}

async fn policies(state: &AppState, ids: &[String]) -> Vec<(String, &'static str)> {
    let mut out = Vec::new();
    for id in ids {
        out.push((id.clone(), call_policy(state, id).await));
    }
    out
}

async fn public_call(state: &AppState, call: &CallRef) -> Value {
    let (head, participants, invited) = {
        let c = call.lock().unwrap();
        let head = (c.id.clone(), c.kind, c.organization_id.clone(), c.channel_id.clone(), c.group_id.clone(), c.caller.clone(), c.callee.clone());
        (head, c.participants.clone(), c.invited.clone())
    };
    let mut list = Vec::new();
    for (user_id, media) in participants {
        let display_name = display_name_of(state, &user_id).await;
        let avatar_url = avatar_url_of(state, &user_id).await;
        list.push(json!({
            "userId": user_id,
            "displayName": display_name,
            "avatarUrl": avatar_url,
            "muted": media.muted,
            "deafened": media.deafened,
            "video": media.video,
            "screen": media.screen,
        }));
    }
    let (id, kind, organization_id, channel_id, group_id, caller, callee) = head;
    json!({
        "id": id,
        "kind": kind,
        "organizationId": organization_id,
        "channelId": channel_id,
        "groupId": group_id,
        "caller": caller,
        "callee": callee,
        "participants": list,
        "invited": invited,
    })
}

/// `void this.emitChannel(...)`: beklenmeden arka planda.
fn voice_presence(state: &AppState, organization_id: String, channel_id: String, call_id: String, participants: Value) {
    let st = state.clone();
    tokio::spawn(async move {
        let payload = json!({ "channelId": channel_id, "callId": call_id, "participants": participants });
        emit_channel(&st, &organization_id, &channel_id, "voice:presence", &payload).await;
    });
}

async fn broadcast_call(state: &AppState, call: &CallRef) {
    let payload = public_call(state, call).await;
    let (audience, organization_id, channel_id, id) = {
        let c = call.lock().unwrap();
        (c.audience(), c.organization_id.clone(), c.channel_id.clone(), c.id.clone())
    };
    let participants = payload["participants"].clone();
    emit_users(state, &audience, "call:state", &json!({ "call": payload })).await;
    if let Some(channel_id) = channel_id {
        voice_presence(state, organization_id, channel_id, id, participants);
    }
}

async fn end_call(state: &AppState, call: &CallRef, reason: &str, audience: Option<Vec<String>>) {
    let id = {
        let mut c = call.lock().unwrap();
        c.clear_timeout();
        c.id.clone()
    };
    state.calls.lock().unwrap().delete(&id);
    finish_call_log(state, call).await;
    let (organization_id, channel_id, group_id) = {
        let c = call.lock().unwrap();
        (c.organization_id.clone(), c.channel_id.clone(), c.group_id.clone())
    };
    let channel_freed = {
        let mut registry = state.calls.lock().unwrap();
        if let Some(group) = &group_id {
            if registry.group_calls.get(group) == Some(&id) {
                registry.group_calls.remove(group);
            }
        }
        match &channel_id {
            Some(channel) if registry.channel_calls.get(channel) == Some(&id) => {
                registry.channel_calls.remove(channel);
                true
            }
            _ => false,
        }
    };
    if channel_freed {
        voice_presence(state, organization_id, channel_id.unwrap(), id.clone(), json!([]));
    }
    let audience = audience.unwrap_or_else(|| call.lock().unwrap().audience());
    emit_users(state, &audience, "call:ended", &json!({ "callId": id, "reason": reason })).await;
}

/// Grup sohbetine sistem satırı yazar (arama başlatma gibi olaylar için).
async fn write_group_notice(state: &AppState, group_id: &str, author_id: &str, content: &str) {
    let id = crate::ctx::uuid();
    let inserted = sqlx::query(r#"insert into "ChatGroupMessage" (id, "groupId", "authorId", content, kind, "createdAt") values ($1, $2, $3, $4, 'SYSTEM', $5)"#)
        .bind(&id)
        .bind(group_id)
        .bind(author_id)
        .bind(content)
        .bind(crate::ctx::now())
        .execute(&state.db)
        .await;
    if inserted.is_err() {
        return;
    }
    let message: Option<sqlx::types::Json<Value>> = sqlx::query_scalar(crate::ctx::sql(format!(
        r#"select json_build_object('id', m.id, 'content', m.content, 'kind', m.kind, 'createdAt', {}, 'editedAt', {},
             'author', json_build_object('id', u.id, 'username', u.username, 'displayName', u."displayName"))
           from "ChatGroupMessage" m join "User" u on u.id = m."authorId" where m.id = $1"#,
        crate::iso!(r#"m."createdAt""#),
        crate::iso!(r#"m."editedAt""#)
    )))
    .bind(&id)
    .fetch_optional(&state.db)
    .await
    .unwrap_or_default();
    let Some(message) = message else { return };
    let Ok(members) = sqlx::query_scalar::<_, String>(r#"select "userId" from "ChatGroupMember" where "groupId" = $1"#).bind(group_id).fetch_all(&state.db).await else {
        return;
    };
    emit_users(state, &members, "group:message", &json!({ "groupId": group_id, "message": message.0 })).await;
}

/// Grubun süren araması: kendi grup araması ya da grubu doğuran kanal araması.
async fn call_of_group(state: &AppState, group_id: &str) -> Option<CallRef> {
    let mut call_id = state.calls.lock().unwrap().group_calls.get(group_id).cloned();
    if call_id.is_none() {
        let channel: Option<Option<String>> = sqlx::query_scalar(r#"select "channelId" from "ChatGroup" where id = $1"#)
            .bind(group_id)
            .fetch_optional(&state.db)
            .await
            .unwrap_or_default();
        if let Some(channel) = channel.flatten() {
            call_id = state.calls.lock().unwrap().channel_calls.get(&channel).cloned();
        }
    }
    call_id.and_then(|id| find(state, &id))
}

/// Gruba sonradan eklenen kişi, grupta süren bir arama varsa davet edilir (`inviteToGroupCall`).
pub async fn invite_to_group_call(state: &AppState, group_id: &str, user_ids: &[String]) {
    let Some(call) = call_of_group(state, group_id).await else { return };
    let fresh: Vec<String> = {
        let c = call.lock().unwrap();
        if c.participants.is_empty() {
            return;
        }
        // Zaten davetli olan ama katılmamış kişiye çağrı yeniden gönderilir.
        user_ids.iter().filter(|id| !c.has(id)).cloned().collect()
    };
    if fresh.is_empty() {
        return;
    }
    let reachable: Vec<(String, &str)> = policies(state, &fresh).await.into_iter().filter(|(_, p)| *p != "reject").collect();
    {
        let mut c = call.lock().unwrap();
        for (id, _) in &reachable {
            c.invite(id);
        }
    }
    if reachable.is_empty() {
        return;
    }
    let payload = public_call(state, &call).await;
    let from_id = {
        let c = call.lock().unwrap();
        c.caller.clone().or_else(|| c.participants.first().map(|(u, _)| u.clone())).unwrap_or_default()
    };
    let from = json!({ "userId": from_id, "displayName": display_name_of(state, &from_id).await });
    for (id, policy) in &reachable {
        emit_users(state, std::slice::from_ref(id), "call:incoming", &json!({ "call": payload, "from": from, "silent": *policy == "silent" })).await;
    }
    broadcast_call(state, &call).await;
}

/// Gruptan ayrılan kişi o grubun aramasından da çıkarılır (`dropFromGroupCall`).
pub async fn drop_from_group_call(state: &AppState, group_id: &str, user_id: &str) {
    let Some(call) = call_of_group(state, group_id).await else { return };
    let (involved, id) = {
        let c = call.lock().unwrap();
        (c.has(user_id) || c.is_invited(user_id), c.id.clone())
    };
    if !involved {
        return;
    }
    remove_from_call(state, &call, user_id).await;
    // Ayrılan kişinin arayüzü de kapansın: kendisine aramanın bittiği bildirilir.
    emit_users(state, &[user_id.to_string()], "call:ended", &json!({ "callId": id, "reason": "left" })).await;
}

/// Bir kişi aynı anda tek bir sesli görüşmede olabilir: başka bir aramaya geçince eski
/// aramadan düşürülür ve istemciye "switched" nedeniyle bildirilir.
async fn leave_other_calls(state: &AppState, user_id: &str, keep_call_id: &str) {
    let all = state.calls.lock().unwrap().all();
    for call in all {
        let (skip, id) = {
            let c = call.lock().unwrap();
            (c.id == keep_call_id || !c.has(user_id), c.id.clone())
        };
        if skip {
            continue;
        }
        remove_from_call(state, &call, user_id).await;
        emit_users(state, &[user_id.to_string()], "call:ended", &json!({ "callId": id, "reason": "switched" })).await;
    }
}

async fn drop_from_calls(state: &AppState, user_id: &str) {
    let all = state.calls.lock().unwrap().all();
    for call in all {
        let involved = {
            let c = call.lock().unwrap();
            c.has(user_id) || c.is_invited(user_id)
        };
        if involved {
            remove_from_call(state, &call, user_id).await;
        }
    }
}

async fn remove_from_call(state: &AppState, call: &CallRef, user_id: &str) {
    let (audience, end, id) = {
        let mut c = call.lock().unwrap();
        let audience = c.audience();
        c.remove(user_id);
        c.uninvite(user_id);
        // Birebir aramada tek kişi kalırsa arama biter; sesli oda tek kişiyle de sürer.
        let end = c.participants.is_empty() || (c.kind == "direct" && c.participants.len() < 2 && c.invited.is_empty());
        (audience, end, c.id.clone())
    };
    if end {
        end_call(state, call, "ended", Some(audience)).await;
        return;
    }
    let audience = call.lock().unwrap().audience();
    emit_users(state, &audience, "call:left", &json!({ "callId": id, "userId": user_id })).await;
    broadcast_call(state, call).await;
}

/// `groupNameOf` (groupName.ts): adlar Türkçe sıralanır, ilk 4 + "+n", en çok 60 karakter.
pub fn group_name_of(names: &[String]) -> String {
    let mut list: Vec<String> = Vec::new();
    for name in names.iter().filter(|n| !n.is_empty()).map(|n| n.trim().to_string()) {
        if !list.contains(&name) {
            list.push(name);
        }
    }
    if list.is_empty() {
        return "Yeni grup".into();
    }
    list.sort_by(|a, b| tr_compare(a, b));
    let shown = list.iter().take(4).cloned().collect::<Vec<_>>().join(", ");
    let full = if list.len() > 4 { format!("{shown} +{}", list.len() - 4) } else { shown };
    crate::validate::js_slice(&full, 60)
}

/// `localeCompare(b, 'tr')` yaklaşığı: Türk alfabesi sırası, önce büyük/küçük harf duyarsız,
/// eşitlikte küçük harf önce.
fn tr_compare(a: &str, b: &str) -> std::cmp::Ordering {
    const ALPHABET: &str = "abcçdefgğhıijklmnoöpqrsştuüvwxyz";
    let key = |s: &str| -> Vec<(u8, u32)> {
        s.chars()
            .map(|c| {
                let lower = match c {
                    'I' => 'ı',
                    'İ' => 'i',
                    _ => c.to_lowercase().next().unwrap_or(c),
                };
                if let Some(pos) = ALPHABET.chars().position(|x| x == lower) {
                    (2, pos as u32)
                } else if lower.is_ascii_digit() {
                    (1, lower as u32)
                } else if lower.is_alphabetic() {
                    (3, lower as u32)
                } else {
                    (0, lower as u32)
                }
            })
            .collect()
    };
    key(a).cmp(&key(b)).then_with(|| {
        let case = |s: &str| s.chars().map(|c| c.is_uppercase()).collect::<Vec<_>>();
        case(a).cmp(&case(b))
    })
}

/// Kanaldan arama başlatılınca aynı kanal için bir grup açılır; sonraki aramalar aynı grubu
/// kullanır, yeni katılanlar gruba eklenir.
async fn ensure_channel_group(state: &AppState, call: &CallRef, created_by_id: &str) {
    let (channel_id, organization_id, candidates_tail) = {
        let c = call.lock().unwrap();
        (c.channel_id.clone(), c.organization_id.clone(), c.audience())
    };
    let Some(channel_id) = channel_id else { return };
    let db = &state.db;
    let project: Option<String> = sqlx::query_scalar(
        r#"select c."projectId" from "ProjectChannel" ch join "ChannelCategory" c on c.id = ch."categoryId" join "Project" p on p.id = c."projectId"
           where ch.id = $1 and p."organizationId" = $2 and p."deletedAt" is null limit 1"#,
    )
    .bind(&channel_id)
    .bind(&organization_id)
    .fetch_optional(db)
    .await
    .unwrap_or_default();
    let Some(project_id) = project else { return };
    let Ok(members) = sqlx::query_scalar::<_, String>(r#"select "userId" from "OrganizationMember" where "organizationId" = $1"#).bind(&organization_id).fetch_all(db).await else {
        return;
    };
    let mut candidates: Vec<String> = Vec::new();
    for id in std::iter::once(created_by_id.to_string()).chain(members).chain(candidates_tail) {
        if !candidates.contains(&id) {
            candidates.push(id);
        }
    }
    let Ok(audience) = crate::perm::channel_audience(db, &organization_id, &channel_id, &candidates).await else { return };
    if audience.len() < 2 {
        return;
    }
    let people: Vec<String> = sqlx::query_scalar(r#"select "displayName" from "User" where id = any($1)"#).bind(&audience).fetch_all(db).await.unwrap_or_default();
    let existing: Option<String> = sqlx::query_scalar(r#"select id from "ChatGroup" where "organizationId" = $1 and "channelId" = $2 limit 1"#)
        .bind(&organization_id)
        .bind(&channel_id)
        .fetch_optional(db)
        .await
        .unwrap_or_default();
    let created = existing.is_none();
    let group_id = match existing {
        None => {
            let id = crate::ctx::uuid();
            let now = crate::ctx::now();
            let Ok(mut tx) = db.begin().await else { return };
            let ok = sqlx::query(
                r#"insert into "ChatGroup" (id, "organizationId", name, "createdById", "channelId", "projectId", "createdAt", "updatedAt") values ($1, $2, $3, $4, $5, $6, $7, $7)"#,
            )
            .bind(&id)
            .bind(&organization_id)
            .bind(group_name_of(&people))
            .bind(created_by_id)
            .bind(&channel_id)
            .bind(&project_id)
            .bind(now)
            .execute(&mut *tx)
            .await
            .is_ok();
            if !ok {
                return;
            }
            for user in &audience {
                let inserted = sqlx::query(r#"insert into "ChatGroupMember" (id, "groupId", "userId", "createdAt") values ($1, $2, $3, $4)"#)
                    .bind(crate::ctx::uuid())
                    .bind(&id)
                    .bind(user)
                    .bind(now)
                    .execute(&mut *tx)
                    .await;
                if inserted.is_err() {
                    return;
                }
            }
            if tx.commit().await.is_err() {
                return;
            }
            id
        }
        Some(id) => {
            let have: Vec<String> = sqlx::query_scalar(r#"select "userId" from "ChatGroupMember" where "groupId" = $1"#).bind(&id).fetch_all(db).await.unwrap_or_default();
            let now = crate::ctx::now();
            for user in audience.iter().filter(|u| !have.contains(u)) {
                let _ = sqlx::query(r#"insert into "ChatGroupMember" (id, "groupId", "userId", "createdAt") values ($1, $2, $3, $4) on conflict do nothing"#)
                    .bind(crate::ctx::uuid())
                    .bind(&id)
                    .bind(user)
                    .bind(now)
                    .execute(db)
                    .await;
            }
            id
        }
    };
    let group: Option<sqlx::types::Json<Value>> = sqlx::query_scalar(crate::ctx::sql(format!(
        r#"select json_build_object('id', g.id, 'name', g.name, 'channelId', g."channelId", 'projectId', g."projectId", 'createdById', g."createdById",
             'updatedAt', {}, 'members', coalesce((select json_agg(json_build_object('id', u.id, 'username', u.username, 'displayName', u."displayName", 'status', u.status::text) order by m.ctid)
                from "ChatGroupMember" m join "User" u on u.id = m."userId" where m."groupId" = g.id), '[]'::json),
             'lastReadAt', null, 'unread', 0, 'lastMessage', null)
           from "ChatGroup" g where g.id = $1"#,
        crate::iso!(r#"g."updatedAt""#)
    )))
    .bind(&group_id)
    .fetch_optional(db)
    .await
    .unwrap_or_default();
    let Some(group) = group else { return };
    let ids: Vec<String> = group.0["members"].as_array().map(|m| m.iter().filter_map(|u| u["id"].as_str().map(str::to_string)).collect()).unwrap_or_default();
    emit_users(state, &ids, if created { "group:created" } else { "group:updated" }, &json!({ "group": group.0 })).await;
}

// ------------------------------------------------------------------ bireysel arama kaydı

/// Birebir aramalar sohbete bir satır bırakır: "arıyor" → "cevapsız" / "süre".
async fn open_conversation(state: &AppState, organization_id: &str, a: &str, b: &str) -> Result<String, sqlx::Error> {
    let (user_a, user_b) = if a <= b { (a, b) } else { (b, a) };
    let existing: Option<String> = sqlx::query_scalar(r#"select id from "DirectConversation" where "organizationId" = $1 and "userAId" = $2 and "userBId" = $3"#)
        .bind(organization_id)
        .bind(user_a)
        .bind(user_b)
        .fetch_optional(&state.db)
        .await?;
    if let Some(id) = existing {
        return Ok(id);
    }
    let id = crate::ctx::uuid();
    sqlx::query(r#"insert into "DirectConversation" (id, "organizationId", "userAId", "userBId", "createdAt", "updatedAt") values ($1, $2, $3, $4, $5, $5)"#)
        .bind(&id)
        .bind(organization_id)
        .bind(user_a)
        .bind(user_b)
        .bind(crate::ctx::now())
        .execute(&state.db)
        .await?;
    Ok(id)
}

async fn direct_message_json(state: &AppState, id: &str) -> Result<Value, sqlx::Error> {
    let row: sqlx::types::Json<Value> = sqlx::query_scalar(crate::ctx::sql(format!(
        r#"select json_build_object('id', m.id, 'content', m.content, 'kind', m.kind, 'createdAt', {}, 'editedAt', {},
             'author', json_build_object('id', u.id, 'username', u.username, 'displayName', u."displayName"),
             'quotedMessage', case when q.id is null then null else json_build_object('id', q.id, 'content', q.content, 'author', json_build_object('displayName', qu."displayName")) end)
           from "DirectMessage" m join "User" u on u.id = m."authorId"
           left join "DirectMessage" q on q.id = m."quotedMessageId" left join "User" qu on qu.id = q."authorId"
           where m.id = $1"#,
        crate::iso!(r#"m."createdAt""#),
        crate::iso!(r#"m."editedAt""#)
    )))
    .bind(id)
    .fetch_one(&state.db)
    .await?;
    Ok(row.0)
}

async fn write_call_message(state: &AppState, call: &CallRef, kind: &str, content: &str) {
    let (direct, organization_id, caller, other, conversation_id, message_id) = {
        let c = call.lock().unwrap();
        (c.kind == "direct", c.organization_id.clone(), c.caller.clone(), c.callee.clone(), c.conversation_id.clone(), c.message_id.clone())
    };
    let (true, Some(caller), Some(other)) = (direct, caller, other) else { return };
    // Sohbet kaydı arama akışını bozmamalı: hatalar yutulur.
    let _ = async {
        let conversation_id = match conversation_id {
            Some(id) => id,
            None => {
                let id = open_conversation(state, &organization_id, &caller, &other).await?;
                call.lock().unwrap().conversation_id = Some(id.clone());
                id
            }
        };
        let people = [caller.clone(), other.clone()];
        match message_id {
            None => {
                let id = crate::ctx::uuid();
                sqlx::query(r#"insert into "DirectMessage" (id, "conversationId", "authorId", content, kind, "createdAt") values ($1, $2, $3, $4, $5, $6)"#)
                    .bind(&id)
                    .bind(&conversation_id)
                    .bind(&caller)
                    .bind(content)
                    .bind(kind)
                    .bind(crate::ctx::now())
                    .execute(&state.db)
                    .await?;
                let message = direct_message_json(state, &id).await?;
                call.lock().unwrap().message_id = Some(id);
                sqlx::query(r#"update "DirectConversation" set "updatedAt" = $2 where id = $1"#).bind(&conversation_id).bind(crate::ctx::now()).execute(&state.db).await?;
                emit_users(state, &people, "direct:created", &json!({ "conversationId": conversation_id, "message": message })).await;
            }
            Some(id) => {
                let done = sqlx::query(r#"update "DirectMessage" set content = $2, kind = $3 where id = $1"#).bind(&id).bind(content).bind(kind).execute(&state.db).await?;
                if done.rows_affected() == 0 {
                    return Err(sqlx::Error::RowNotFound);
                }
                let message = direct_message_json(state, &id).await?;
                emit_users(state, &people, "direct:updated", &json!({ "conversationId": conversation_id, "message": message })).await;
            }
        }
        Ok::<(), sqlx::Error>(())
    }
    .await;
}

async fn finish_call_log(state: &AppState, call: &CallRef) {
    let (direct, message, connected_at) = {
        let c = call.lock().unwrap();
        (c.kind == "direct", c.message_id.is_some(), c.connected_at)
    };
    if !direct || !message {
        return;
    }
    match connected_at {
        Some(at) => {
            let seconds = (((now_ms() - at) as f64 / 1000.0) + 0.5).floor().max(1.0) as i64;
            write_call_message(state, call, "CALL_ENDED", &seconds.to_string()).await;
        }
        None => write_call_message(state, call, "CALL_MISSED", "0").await,
    }
}

// ------------------------------------------------------------------ olaylar

async fn incoming(state: &AppState, call: &CallRef, user_id: &str, reachable: &[(String, &'static str)]) {
    if reachable.is_empty() {
        return;
    }
    let payload = public_call(state, call).await;
    let from = json!({ "userId": user_id, "displayName": display_name_of(state, user_id).await });
    for (id, policy) in reachable {
        emit_users(state, std::slice::from_ref(id), "call:incoming", &json!({ "call": payload, "from": from, "silent": *policy == "silent" })).await;
    }
}

async fn call_start(socket: SocketRef, Data(body): Data<Value>, ack: AckSender, State(state): State<AppState>) {
    let state = &state;
    let organization_id = jstr(body.get("organizationId"));
    let Some(user_id) = user_of(&socket).filter(|_| !organization_id.is_empty()) else { return ack_ok(ack, false) };
    if !is_member(state, &organization_id, &user_id).await {
        return ack_ok(ack, false);
    }
    let kind = body.get("kind").and_then(Value::as_str).unwrap_or_default();

    if kind == "channel" {
        let channel_id = jstr(body.get("channelId"));
        let channel: Option<i32> = sqlx::query_scalar(
            r#"select 1 from "ProjectChannel" ch join "ChannelCategory" c on c.id = ch."categoryId" join "Project" p on p.id = c."projectId"
               where ch.id = $1 and ch."deletedAt" is null and p."organizationId" = $2 limit 1"#,
        )
        .bind(&channel_id)
        .bind(&organization_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or_default();
        if channel.is_none() {
            return ack_ok(ack, false);
        }
        // Kanalı göremeyen bağlanamaz; zil çaldırmak ayrıca "arama başlat" ister.
        if !may(state, &organization_id, &user_id, "voice.connect", Some(&channel_id)).await {
            return denied(ack);
        }
        let ring = truthy(body.get("ring"));
        if ring && !may(state, &organization_id, &user_id, "call.start", Some(&channel_id)).await {
            return denied(ack);
        }
        // Kanalda zaten bir arama varsa yenisini açmak yerine ona katılınır.
        let existing = {
            let registry = state.calls.lock().unwrap();
            registry.channel_calls.get(&channel_id).and_then(|id| registry.get(id))
        };
        let call = existing.unwrap_or_else(|| {
            let mut c = Call::new("channel", &organization_id);
            c.channel_id = Some(channel_id.clone());
            Arc::new(Mutex::new(c))
        });
        let call_id = call.lock().unwrap().id.clone();
        leave_other_calls(state, &user_id, &call_id).await;
        {
            let mut c = call.lock().unwrap();
            c.join(&user_id);
            c.uninvite(&user_id);
        }
        {
            let mut registry = state.calls.lock().unwrap();
            registry.set(&call);
            registry.channel_calls.insert(channel_id.clone(), call_id.clone());
        }
        // "Kanalda arama başlat": kanalı görebilen herkesin telefonu çalar. Sesli odaya
        // sessizce katılırken (ring=false) kimse rahatsız edilmez.
        if ring {
            let members: Vec<String> = sqlx::query_scalar(r#"select "userId" from "OrganizationMember" where "organizationId" = $1"#)
                .bind(&organization_id)
                .fetch_all(&state.db)
                .await
                .unwrap_or_default();
            let visible = crate::perm::channel_audience(&state.db, &organization_id, &channel_id, &members).await.unwrap_or_default();
            let fresh: Vec<String> = {
                let c = call.lock().unwrap();
                visible.into_iter().filter(|id| id != &user_id && !c.has(id) && !c.is_invited(id)).collect()
            };
            let reachable: Vec<_> = policies(state, &fresh).await.into_iter().filter(|(_, p)| *p != "reject").collect();
            {
                let mut c = call.lock().unwrap();
                for (id, _) in &reachable {
                    c.invite(id);
                }
            }
            incoming(state, &call, &user_id, &reachable).await;
        }
        broadcast_call(state, &call).await;
        // Kanal aramasıyla birlikte katılanların yazışabileceği bir grup oluşur.
        if ring {
            ensure_channel_group(state, &call, &user_id).await;
        }
        let payload = public_call(state, &call).await;
        let _ = ack.send(&json!({ "ok": true, "call": payload }));
        return;
    }

    if kind == "group" {
        // Grup araması: yalnızca grup üyeleri davet edilir.
        let group_id = jstr(body.get("groupId"));
        let group: Option<(Option<String>, Vec<String>)> = sqlx::query_as(
            r#"select g."projectId", coalesce((select array_agg(m."userId" order by m.ctid) from "ChatGroupMember" m where m."groupId" = g.id), '{}')
               from "ChatGroup" g where g.id = $1 and g."organizationId" = $2
               and exists (select 1 from "ChatGroupMember" x where x."groupId" = g.id and x."userId" = $3) limit 1"#,
        )
        .bind(&group_id)
        .bind(&organization_id)
        .bind(&user_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or_default();
        let Some((project_id, member_ids)) = group else { return ack_ok(ack, false) };
        let Some(project_id) = project_id else { return ack_ok(ack, false) };
        if crate::perm::require_project_access(&state.db, &organization_id, &user_id, &project_id).await.is_err() {
            return ack_ok(ack, false);
        }
        let existing = {
            let registry = state.calls.lock().unwrap();
            registry.group_calls.get(&group_id).and_then(|id| registry.get(id))
        };
        let fresh_call = existing.is_none();
        let call = existing.unwrap_or_else(|| {
            let mut c = Call::new("group", &organization_id);
            c.group_id = Some(group_id.clone());
            c.caller = Some(user_id.clone());
            Arc::new(Mutex::new(c))
        });
        let call_id = {
            // Gruptan ayrılmış kişiler eski aramada kalmışsa temizlenir.
            let mut c = call.lock().unwrap();
            c.participants.retain(|(u, _)| member_ids.contains(u));
            c.invited.retain(|u| member_ids.contains(u));
            c.id.clone()
        };
        leave_other_calls(state, &user_id, &call_id).await;
        {
            let mut c = call.lock().unwrap();
            c.join(&user_id);
            c.uninvite(&user_id);
            if c.connected_at.is_none() && c.participants.len() > 1 {
                c.connected_at = Some(now_ms());
            }
        }
        {
            let mut registry = state.calls.lock().unwrap();
            registry.set(&call);
            registry.group_calls.insert(group_id.clone(), call_id.clone());
        }
        if body.get("ring") != Some(&Value::Bool(false)) {
            let fresh: Vec<String> = {
                let c = call.lock().unwrap();
                member_ids.iter().filter(|id| *id != &user_id && !c.has(id) && !c.is_invited(id)).cloned().collect()
            };
            let reachable: Vec<_> = policies(state, &fresh).await.into_iter().filter(|(_, p)| *p != "reject").collect();
            {
                let mut c = call.lock().unwrap();
                for (id, _) in &reachable {
                    c.invite(id);
                }
            }
            incoming(state, &call, &user_id, &reachable).await;
        }
        broadcast_call(state, &call).await;
        // Sohbete "arama başlattı" satırı düşer.
        if fresh_call {
            let name = display_name_of(state, &user_id).await;
            write_group_notice(state, &group_id, &user_id, &format!("{name} bir arama başlattı.")).await;
        }
        let payload = public_call(state, &call).await;
        let _ = ack.send(&json!({ "ok": true, "call": payload }));
        return;
    }

    let target = jstr(body.get("targetUserId"));
    if target.is_empty() || target == user_id || !is_member(state, &organization_id, &target).await {
        return ack_ok(ack, false);
    }
    if crate::perm::require_shared(&state.db, &user_id, &target, "call.start").await.is_err() {
        return denied(ack);
    }
    // Karşı taraf bizi hâlâ arıyorsa yeni ve çakışan bir çağrı oluşturma: geri arama mevcut
    // daveti kabul etmiş sayılır ve iki istemci aynı call id üzerinde buluşur.
    let existing = state.calls.lock().unwrap().all().into_iter().find(|item| {
        let c = item.lock().unwrap();
        c.kind == "direct"
            && c.organization_id == organization_id
            && ((c.caller.as_deref() == Some(&user_id) && c.callee.as_deref() == Some(&target)) || (c.caller.as_deref() == Some(&target) && c.callee.as_deref() == Some(&user_id)))
    });
    if let Some(existing) = existing {
        let (answer, id) = {
            let c = existing.lock().unwrap();
            (c.is_invited(&user_id) && c.has(&target), c.id.clone())
        };
        if answer {
            leave_other_calls(state, &user_id, &id).await;
            {
                let mut c = existing.lock().unwrap();
                c.uninvite(&user_id);
                c.set(&user_id, Media::new());
                if c.connected_at.is_none() {
                    c.connected_at = Some(now_ms());
                }
                c.clear_timeout();
            }
            broadcast_call(state, &existing).await;
        }
        let payload = public_call(state, &existing).await;
        let _ = ack.send(&json!({ "ok": true, "call": payload }));
        return;
    }
    let policy = call_policy(state, &target).await;
    let mut c = Call::new("direct", &organization_id);
    c.participants.push((user_id.clone(), Media::new()));
    c.invited.push(target.clone());
    c.caller = Some(user_id.clone());
    c.callee = Some(target.clone());
    let call_id = c.id.clone();
    let call = Arc::new(Mutex::new(c));
    leave_other_calls(state, &user_id, &call_id).await;
    // 45 saniye içinde kimse açmazsa (kişi çevrimdışıysa da) arama cevapsız olarak kapanır.
    {
        let st = state.clone();
        let id = call_id.clone();
        let people = vec![user_id.clone(), target.clone()];
        let task = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(45000)).await;
            let Some(live) = find(&st, &id) else { return };
            let unanswered = {
                let mut c = live.lock().unwrap();
                // Kendi tutamacımızı bırak: end_call onu iptal etmeye çalışmasın.
                c.timeout = None;
                c.connected_at.is_none()
            };
            if unanswered {
                end_call(&st, &live, "missed", Some(people)).await;
            }
        });
        call.lock().unwrap().timeout = Some(task.abort_handle());
    }
    state.calls.lock().unwrap().set(&call);
    let payload = public_call(state, &call).await;
    if policy != "reject" {
        let from = json!({ "userId": user_id, "displayName": display_name_of(state, &user_id).await });
        emit_users(state, std::slice::from_ref(&target), "call:incoming", &json!({ "call": payload, "from": from, "silent": policy == "silent" })).await;
    }
    write_call_message(state, &call, "CALL_STARTED", "0").await;
    broadcast_call(state, &call).await;
    let _ = ack.send(&json!({ "ok": true, "call": payload }));
}

async fn call_invite(socket: SocketRef, Data(body): Data<Value>, ack: AckSender, State(state): State<AppState>) {
    let state = &state;
    let call = find(state, &jstr(body.get("callId")));
    let target = jstr(body.get("userId"));
    let Some(user_id) = user_of(&socket) else { return ack_ok(ack, false) };
    let Some(call) = call.filter(|c| c.lock().unwrap().has(&user_id)) else { return ack_ok(ack, false) };
    let organization_id = call.lock().unwrap().organization_id.clone();
    let already = call.lock().unwrap().has(&target);
    if target.is_empty() || already || !is_member(state, &organization_id, &target).await || crate::perm::peers_among(&state.db, &user_id, std::slice::from_ref(&target)).await.map_or(true, |p| p.is_empty()) {
        return ack_ok(ack, false);
    }
    let policy = call_policy(state, &target).await;
    if policy == "reject" {
        let _ = ack.send(&json!({ "ok": false, "reason": "unavailable" }));
        return;
    }
    call.lock().unwrap().invite(&target);
    let payload = public_call(state, &call).await;
    let from = json!({ "userId": user_id, "displayName": display_name_of(state, &user_id).await });
    emit_users(state, std::slice::from_ref(&target), "call:incoming", &json!({ "call": payload, "from": from, "silent": policy == "silent" })).await;
    broadcast_call(state, &call).await;
    ack_ok(ack, true);
}

async fn call_accept(socket: SocketRef, Data(body): Data<Value>, ack: AckSender, State(state): State<AppState>) {
    let state = &state;
    let call = find(state, &jstr(body.get("callId")));
    let (Some(user_id), Some(call)) = (user_of(&socket), call) else { return ack_ok(ack, false) };
    // Kanal aramalarına davet beklemeden katılınabilir; birebir aramada davetli olmak gerekir.
    let (blocked, organization_id, id) = {
        let c = call.lock().unwrap();
        (c.kind == "direct" && !c.is_invited(&user_id) && !c.has(&user_id), c.organization_id.clone(), c.id.clone())
    };
    if blocked || !is_member(state, &organization_id, &user_id).await {
        return ack_ok(ack, false);
    }
    leave_other_calls(state, &user_id, &id).await;
    {
        let mut c = call.lock().unwrap();
        c.uninvite(&user_id);
        c.join(&user_id);
        if c.connected_at.is_none() && c.participants.len() > 1 {
            c.connected_at = Some(now_ms());
            c.clear_timeout();
        }
    }
    broadcast_call(state, &call).await;
    let payload = public_call(state, &call).await;
    let _ = ack.send(&json!({ "ok": true, "call": payload }));
}

async fn call_decline(socket: SocketRef, Data(body): Data<Value>, ack: AckSender, State(state): State<AppState>) {
    let state = &state;
    let call = find(state, &jstr(body.get("callId")));
    let (Some(user_id), Some(call)) = (user_of(&socket), call) else { return ack_ok(ack, false) };
    let (participants, id) = {
        let mut c = call.lock().unwrap();
        c.uninvite(&user_id);
        (c.keys(), c.id.clone())
    };
    emit_users(state, &participants, "call:declined", &json!({ "callId": id, "userId": user_id })).await;
    // Birebir aramada karşı taraf reddederse çağrı biter.
    let end = {
        let c = call.lock().unwrap();
        c.kind == "direct" && c.invited.is_empty() && c.participants.len() < 2
    };
    if end {
        end_call(state, &call, "declined", None).await;
    } else {
        broadcast_call(state, &call).await;
    }
    ack_ok(ack, true);
}

async fn call_leave(socket: SocketRef, Data(body): Data<Value>, ack: AckSender, State(state): State<AppState>) {
    let call = find(&state, &jstr(body.get("callId")));
    let (Some(user_id), Some(call)) = (user_of(&socket), call) else { return ack_ok(ack, false) };
    remove_from_call(&state, &call, &user_id).await;
    ack_ok(ack, true);
}

/// Bağlantı koptuktan sonra istemci hâlâ aramada mı diye sorar; değilse arayüz kilitli kalmasın.
async fn call_sync(socket: SocketRef, Data(body): Data<Value>, ack: AckSender, State(state): State<AppState>) {
    let call_id = jstr(body.get("callId"));
    let call = find(&state, &call_id);
    let Some(user_id) = user_of(&socket) else { return ack_ok(ack, false) };
    let Some(call) = call.filter(|c| c.lock().unwrap().has(&user_id)) else {
        emit_users(&state, std::slice::from_ref(&user_id), "call:ended", &json!({ "callId": call_id, "reason": "ended" })).await;
        return ack_ok(ack, false);
    };
    let payload = public_call(&state, &call).await;
    emit_users(&state, &[user_id], "call:state", &json!({ "call": payload })).await;
    ack_ok(ack, true);
}

async fn call_media(socket: SocketRef, Data(body): Data<Value>, ack: AckSender, State(state): State<AppState>) {
    let state = &state;
    let call = find(state, &jstr(body.get("callId")));
    let Some(user_id) = user_of(&socket) else { return ack_ok(ack, false) };
    let Some(call) = call else { return ack_ok(ack, false) };
    let (media, organization_id, channel_id) = {
        let c = call.lock().unwrap();
        (c.media(&user_id), c.organization_id.clone(), if c.kind == "channel" { c.channel_id.clone() } else { None })
    };
    let Some(media) = media else { return ack_ok(ack, false) };
    // Kamera ve ekran paylaşımı yetkiye bağlı; yetkisi yoksa istek yok sayılır.
    let wants_video = body.get("video") == Some(&Value::Bool(true)) && !truthy(Some(&media.video));
    let wants_screen = body.get("screen") == Some(&Value::Bool(true)) && !truthy(Some(&media.screen));
    if wants_video && !may(state, &organization_id, &user_id, "voice.video", channel_id.as_deref()).await {
        return denied(ack);
    }
    if wants_screen && !may(state, &organization_id, &user_id, "screen.share", channel_id.as_deref()).await {
        return denied(ack);
    }
    let wants_voice = body.get("muted") == Some(&Value::Bool(false)) && truthy(Some(&media.muted));
    if wants_voice && !may(state, &organization_id, &user_id, "voice.speak", channel_id.as_deref()).await {
        return denied(ack);
    }
    // `body.x ?? media.x`
    let pick = |key: &str, fallback: &Value| match body.get(key) {
        None | Some(Value::Null) => fallback.clone(),
        Some(v) => v.clone(),
    };
    let next = Media { muted: pick("muted", &media.muted), deafened: pick("deafened", &media.deafened), video: pick("video", &media.video), screen: pick("screen", &media.screen) };
    call.lock().unwrap().set(&user_id, next);
    broadcast_call(state, &call).await;
    ack_ok(ack, true);
}

async fn call_moderate(socket: SocketRef, Data(body): Data<Value>, ack: AckSender, State(state): State<AppState>) {
    let state = &state;
    let call = find(state, &jstr(body.get("callId")));
    let target_id = jstr(body.get("userId"));
    let Some(moderator) = user_of(&socket) else { return ack_ok(ack, false) };
    let Some(call) = call else { return ack_ok(ack, false) };
    let (target, organization_id, id, channel_id, group_id) = {
        let c = call.lock().unwrap();
        (c.media(&target_id), c.organization_id.clone(), c.id.clone(), c.channel_id.clone(), c.group_id.clone())
    };
    let Some(target) = target else { return ack_ok(ack, false) };
    if target_id == moderator || body.get("muted") != Some(&Value::Bool(true)) {
        return ack_ok(ack, false);
    }
    // Susturma yetkisi aramanın projesinde: sahip ya da "üye çıkar" yetkisi olan. Bireysel aramada yok.
    let project: Option<String> = match (channel_id, group_id) {
        (Some(channel), _) => crate::perm::channel_project(&state.db, &organization_id, &channel).await.unwrap_or_default(),
        (None, Some(group)) => sqlx::query_scalar::<_, Option<String>>(r#"select "projectId" from "ChatGroup" where id = $1"#)
            .bind(&group)
            .fetch_optional(&state.db)
            .await
            .unwrap_or_default()
            .flatten(),
        _ => None,
    };
    let Some(project) = project else { return ack_ok(ack, false) };
    if !crate::perm::effective_in(&state.db, &project, &moderator).await.is_ok_and(|e| e.owner || e.has("member.kick")) {
        return ack_ok(ack, false);
    }
    call.lock().unwrap().set(&target_id, Media { muted: true.into(), ..target });
    emit_users(state, std::slice::from_ref(&target_id), "call:force-mute", &json!({ "callId": id, "by": moderator })).await;
    broadcast_call(state, &call).await;
    ack_ok(ack, true);
}

/// SDP/ICE yönlendirmesi: yalnızca aynı çağrının katılımcıları arasında.
async fn call_signal(socket: SocketRef, Data(body): Data<Value>, ack: AckSender, State(state): State<AppState>) {
    let call = find(&state, &jstr(body.get("callId")));
    let to = jstr(body.get("to"));
    let Some(user_id) = user_of(&socket) else { return ack_ok(ack, false) };
    let Some(call) = call else { return ack_ok(ack, false) };
    let (ok, id) = {
        let c = call.lock().unwrap();
        (c.has(&user_id) && c.has(&to), c.id.clone())
    };
    if !ok {
        return ack_ok(ack, false);
    }
    let mut payload = serde_json::Map::new();
    payload.insert("callId".into(), id.into());
    payload.insert("from".into(), user_id.into());
    // `data: undefined` JSON'da düşer.
    if let Some(data) = body.get("data") {
        payload.insert("data".into(), data.clone());
    }
    emit_users(&state, &[to], "call:signal", &Value::Object(payload)).await;
    ack_ok(ack, true);
}

/// Abonelik/anlık görüntü: odaya katılmadan önce de görünmesi gereken aktif ses odaları.
pub(crate) async fn replay_voice(socket: &SocketRef, state: &AppState, user_id: &str, organization_id: &str) {
    let all = state.calls.lock().unwrap().all();
    for call in all {
        let channel = {
            let c = call.lock().unwrap();
            if c.organization_id != organization_id || c.participants.is_empty() { None } else { c.channel_id.clone() }
        };
        let Some(channel_id) = channel else { continue };
        if !may(state, organization_id, user_id, "channel.view", Some(&channel_id)).await {
            continue;
        }
        let payload = public_call(state, &call).await;
        let _ = socket.emit("voice:presence", &json!({ "channelId": channel_id, "callId": payload["id"], "participants": payload["participants"] }));
    }
}

