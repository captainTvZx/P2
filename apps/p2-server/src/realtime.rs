//! socket.io `/stream` namespace — `server/src/realtime.ts` (RealtimeGateway) karşılığı:
//! bağlantı doğrulaması, çevrimiçi durum (boşta, tercih, etkinlik), yazıyor bildirimi,
//! kütüphane duyurusu, doküman imleçleri, abonelik + anlık görüntü. Arama sistemi `calls.rs`.
//!
//! Ölçek notu: bu durum (bağlantı sayıları, boşta bilgisi, etkinlikler, aramalar) tek sürecin
//! belleğinde — NestJS sürümüyle aynı. Birden fazla sunucu kopyası için paylaşılan bir katmana
//! (Redis/NATS adaptörü) taşınır; socketioxide bunu adaptör olarak destekliyor.

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value, json};
use socketioxide::extract::{AckSender, Data, SocketRef, State};

use crate::AppState;
use crate::auth::{self, AccessClaims};
use crate::validate::{js_number, js_slice};

#[derive(Default)]
pub struct Presence {
    online: HashMap<String, usize>,
    idle: HashSet<String>,
    /// Görünen ad önbelleği (NestJS `names`: hiç tazelenmez).
    pub(crate) names: HashMap<String, String>,
    /// Etkinlikler ekleme sırasıyla (JS Map gibi): (kullanıcı, etkinlik nesnesi).
    activities: Vec<(String, Value)>,
}

#[derive(Clone)]
struct UserId(String);

pub(crate) fn user_of(socket: &SocketRef) -> Option<String> {
    socket.extensions.get::<UserId>().map(|u| u.0)
}

// ------------------------------------------------------------------ JS değer yardımcıları

/// JS doğruluk değeri.
pub(crate) fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

/// JS `String(value)`.
pub(crate) fn js_string(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => match n.as_f64() {
            Some(f) if f.fract() == 0.0 && f.abs() < 1e21 => format!("{}", f as i64),
            _ => n.to_string(),
        },
        Value::String(s) => s.clone(),
        Value::Array(items) => items.iter().map(|v| if v.is_null() { String::new() } else { js_string(v) }).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".into(),
    }
}

/// JS `String(value || '')`.
pub(crate) fn jstr(value: Option<&Value>) -> String {
    if truthy(value) { js_string(value.unwrap()) } else { String::new() }
}

/// JS `Number(value)` (NaN → None).
pub(crate) fn js_num(value: Option<&Value>) -> Option<f64> {
    match value {
        None => None,
        Some(Value::Null) => Some(0.0),
        Some(Value::Bool(b)) => Some(if *b { 1.0 } else { 0.0 }),
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => js_number(s),
        Some(Value::Array(items)) if items.is_empty() => Some(0.0),
        Some(Value::Array(items)) if items.len() == 1 => js_num(Some(&Value::String(js_string(&items[0])))),
        Some(_) => None,
    }
}

/// Sayıyı JSON'a JS'in yazdığı gibi koyar (tam sayılar `.0` olmadan).
pub(crate) fn num(f: f64) -> Value {
    if f.fract() == 0.0 && f.abs() < 9.007_199_254_740_992e15 {
        Value::from(f as i64)
    } else {
        serde_json::Number::from_f64(f).map(Value::Number).unwrap_or(Value::Null)
    }
}

pub(crate) fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn ack_ok(ack: AckSender, ok: bool) {
    let _ = ack.send(&json!({ "ok": ok }));
}

// ------------------------------------------------------------------ bağlantı

pub async fn on_connect(socket: SocketRef, Data(auth): Data<Value>, State(state): State<AppState>) {
    let token = jstr(auth.get("token"));
    let Some(claims) = auth::verify::<AccessClaims>(&token, &state.cfg.access_secret) else {
        // NestJS: bağlantıyı kabul edip hemen kapatır (connect_error değil, disconnect).
        socket.disconnect().ok();
        return;
    };
    let user_id = claims.sub;
    socket.extensions.insert(UserId(user_id.clone()));
    socket.join(format!("user:{user_id}"));
    *state.presence.lock().unwrap().online.entry(user_id.clone()).or_default() += 1;
    crate::calls::cancel_grace(&state, &user_id);

    socket.on("subscribe", subscribe);
    socket.on("presence:snapshot", snapshot);
    socket.on("presence:prefer", prefer);
    socket.on("presence:idle", idle);
    socket.on("typing", typing);
    socket.on("activity:set", set_activity);
    socket.on("library:announce", announce_play);
    socket.on("document:cursor", document_cursor);
    crate::calls::register(&socket);
    socket.on_disconnect(on_disconnect);

    refresh(&state, &user_id).await;
}

async fn on_disconnect(socket: SocketRef, State(state): State<AppState>) {
    let Some(id) = user_of(&socket) else { return };
    let gone = {
        let mut presence = state.presence.lock().unwrap();
        let count = presence.online.get(&id).copied().unwrap_or(1).saturating_sub(1);
        if count > 0 {
            presence.online.insert(id.clone(), count);
            false
        } else {
            presence.online.remove(&id);
            presence.idle.remove(&id);
            presence.activities.retain(|(u, _)| u != &id);
            true
        }
    };
    // Son bağlantısı da koptuysa aramalardan düşer; ama önce kısa bir geri dönüş süresi tanınır.
    if gone {
        crate::calls::start_grace(&state, &id);
    }
    refresh(&state, &id).await;
}

impl Presence {
    fn activity(&self, user_id: &str) -> Option<Value> {
        self.activities.iter().find(|(u, _)| u == user_id).map(|(_, a)| a.clone())
    }
    /// JS `Map.set`: var olan anahtar yerini korur.
    fn set_activity(&mut self, user_id: &str, activity: Value) {
        match self.activities.iter_mut().find(|(u, _)| u == user_id) {
            Some(slot) => slot.1 = activity,
            None => self.activities.push((user_id.to_string(), activity)),
        }
    }
    pub(crate) fn count(&self, user_id: &str) -> usize {
        self.online.get(user_id).copied().unwrap_or(0)
    }
}

/// Seçilen durum + bağlantı/boşta bilgisinden herkesin göreceği durumu üretir.
async fn refresh(state: &AppState, user_id: &str) {
    let row: Option<(String, String)> = sqlx::query_as(r#"select status::text, "statusPreference" from "User" where id = $1"#)
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or_default();
    let Some((current, preference)) = row else { return };
    let (connected, idle) = {
        let presence = state.presence.lock().unwrap();
        (presence.count(user_id) > 0, presence.idle.contains(user_id))
    };
    let status = match (connected, preference.as_str()) {
        (false, _) | (true, "INVISIBLE") => "OFFLINE",
        (true, "BUSY") => "BUSY",
        (true, "AWAY") => "AWAY",
        (true, _) if idle => "AWAY",
        (true, _) => "ONLINE",
    };
    if status != current {
        let _ = sqlx::query(r#"update "User" set status = $2::"UserStatus", "updatedAt" = $3 where id = $1"#)
            .bind(user_id)
            .bind(status)
            .bind(auth::now())
            .execute(&state.db)
            .await;
    }
    let activity = if status == "OFFLINE" { Value::Null } else { state.presence.lock().unwrap().activity(user_id).unwrap_or(Value::Null) };
    emit_peers(state, user_id, "presence:update", &json!({ "userId": user_id, "status": status, "activity": activity })).await;
    emit_users(state, &[user_id.to_string()], "presence:self", &json!({ "preference": preference, "status": status })).await;
}

async fn member_exists(state: &AppState, organization_id: &str, user_id: &str) -> bool {
    let hit: Option<i32> = sqlx::query_scalar(r#"select 1 from "OrganizationMember" where "organizationId" = $1 and "userId" = $2"#)
        .bind(organization_id)
        .bind(user_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or_default();
    hit.is_some()
}

// ------------------------------------------------------------------ yazıyor

/// "yazıyor" bildirimi: kanalda kuruluş odasına, bireysel sohbette iki katılımcıya.
async fn typing(socket: SocketRef, Data(body): Data<Value>, ack: AckSender, State(state): State<AppState>) {
    let Some(user_id) = user_of(&socket) else { return ack_ok(ack, false) };
    let cached = state.presence.lock().unwrap().names.get(&user_id).cloned();
    let display_name = match cached {
        Some(name) if !name.is_empty() => name,
        _ => {
            let row: Option<String> = sqlx::query_scalar(r#"select "displayName" from "User" where id = $1"#)
                .bind(&user_id)
                .fetch_optional(&state.db)
                .await
                .unwrap_or_default();
            let Some(name) = row else { return ack_ok(ack, false) };
            state.presence.lock().unwrap().names.insert(user_id.clone(), name.clone());
            name
        }
    };
    let pick = |key: &str| if truthy(body.get(key)) { body[key].clone() } else { Value::Null };
    let payload = json!({
        "userId": user_id,
        "displayName": display_name,
        "channelId": pick("channelId"),
        "conversationId": pick("conversationId"),
        "typing": body.get("typing") != Some(&Value::Bool(false)),
    });
    if truthy(body.get("conversationId")) {
        let conversation: Option<(String, String)> = sqlx::query_as(
            r#"select "userAId", "userBId" from "DirectConversation" where id = $1 and ("userAId" = $2 or "userBId" = $2) limit 1"#,
        )
        .bind(js_string(&body["conversationId"]))
        .bind(&user_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or_default();
        let Some((a, b)) = conversation else { return ack_ok(ack, false) };
        emit_users(&state, &[a, b], "typing", &payload).await;
        return ack_ok(ack, true);
    }
    if truthy(body.get("organizationId")) && truthy(body.get("channelId")) {
        let organization = js_string(&body["organizationId"]);
        if !member_exists(&state, &organization, &user_id).await {
            return ack_ok(ack, false);
        }
        emit_channel(&state, &organization, &js_string(&body["channelId"]), "typing", &payload).await;
        return ack_ok(ack, true);
    }
    ack_ok(ack, false)
}

// ------------------------------------------------------------------ durum tercihi / boşta

async fn prefer(socket: SocketRef, Data(body): Data<Value>, ack: AckSender, State(state): State<AppState>) {
    let preference = jstr(body.get("status"));
    let Some(user_id) = user_of(&socket).filter(|_| ["ONLINE", "AWAY", "BUSY", "INVISIBLE"].contains(&preference.as_str())) else {
        return ack_ok(ack, false);
    };
    let _ = sqlx::query(r#"update "User" set "statusPreference" = $2, "updatedAt" = $3 where id = $1"#)
        .bind(&user_id)
        .bind(&preference)
        .bind(auth::now())
        .execute(&state.db)
        .await;
    refresh(&state, &user_id).await;
    let _ = ack.send(&json!({ "ok": true, "preference": preference }));
}

/// Uygulama arka plana atıldığında/boşta kaldığında otomatik "uzakta".
async fn idle(socket: SocketRef, Data(body): Data<Value>, ack: AckSender, State(state): State<AppState>) {
    let Some(user_id) = user_of(&socket) else { return ack_ok(ack, false) };
    {
        let mut presence = state.presence.lock().unwrap();
        if truthy(body.get("away")) {
            presence.idle.insert(user_id.clone());
        } else {
            presence.idle.remove(&user_id);
        }
    }
    refresh(&state, &user_id).await;
    ack_ok(ack, true);
}

// ------------------------------------------------------------------ etkinlik

/// Zengin etkinlik: istemci ne yaptığını bildirir (kind=null temizler). Görünmez kişinin
/// etkinliği dışarı çıkmaz.
async fn set_activity(socket: SocketRef, Data(body): Data<Value>, ack: AckSender, State(state): State<AppState>) {
    let Some(user_id) = user_of(&socket) else { return ack_ok(ack, false) };
    if !truthy(body.get("kind")) {
        state.presence.lock().unwrap().activities.retain(|(u, _)| u != &user_id);
    } else {
        let kind = body["kind"].as_str().filter(|k| ["playing", "downloading", "uploading", "working"].contains(k));
        let Some(kind) = kind else { return ack_ok(ack, false) };
        let project_id = js_slice(&jstr(body.get("projectId")), 64);
        let project_name = js_slice(&jstr(body.get("projectName")), 80);
        if project_id.is_empty() || project_name.is_empty() {
            return ack_ok(ack, false);
        }
        let percent = js_num(body.get("percent")).filter(|f| f.is_finite()).map(|f| (f + 0.5).floor().clamp(0.0, 100.0));
        let detail = js_slice(&jstr(body.get("detail")), 80);
        let mut presence = state.presence.lock().unwrap();
        let previous = presence.activity(&user_id);
        let since = match previous {
            Some(p) if p["kind"] == kind && p["projectId"] == project_id.as_str() => p["since"].clone(),
            _ => js_num(body.get("since")).filter(|f| *f != 0.0 && !f.is_nan()).map(num).unwrap_or_else(|| Value::from(now_ms())),
        };
        let mut activity = Map::new();
        activity.insert("kind".into(), kind.into());
        activity.insert("projectId".into(), project_id.into());
        activity.insert("projectName".into(), project_name.into());
        if !detail.is_empty() {
            activity.insert("detail".into(), detail.into());
        }
        if let Some(p) = percent {
            activity.insert("percent".into(), num(p));
        }
        activity.insert("since".into(), since);
        presence.set_activity(&user_id, Value::Object(activity));
    }
    let preference: Option<String> = sqlx::query_scalar(r#"select "statusPreference" from "User" where id = $1"#)
        .bind(&user_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or_default();
    if preference.is_none_or(|p| p == "INVISIBLE") {
        return ack_ok(ack, true);
    }
    let activity = state.presence.lock().unwrap().activity(&user_id).unwrap_or(Value::Null);
    emit_peers(&state, &user_id, "presence:activity", &json!({ "userId": user_id, "activity": activity })).await;
    ack_ok(ack, true);
}

/// "X şu projeyi çalıştırdı" duyurusu: projeyi görebilen çevrimiçi herkese (gönderen hariç).
async fn announce_play(socket: SocketRef, Data(body): Data<Value>, ack: AckSender, State(state): State<AppState>) {
    let organization = jstr(body.get("organizationId"));
    let project_id = js_slice(&jstr(body.get("projectId")), 64);
    let Some(user_id) = user_of(&socket).filter(|_| !organization.is_empty() && !project_id.is_empty()) else {
        return ack_ok(ack, false);
    };
    let user: Option<(String, String, String, Option<String>, String)> =
        sqlx::query_as(r#"select id, username, "displayName", "avatarUrl", "statusPreference" from "User" where id = $1"#)
            .bind(&user_id)
            .fetch_optional(&state.db)
            .await
            .unwrap_or_default();
    let Some((id, username, display_name, avatar_url, preference)) = user else { return ack_ok(ack, true) };
    if preference == "INVISIBLE" {
        return ack_ok(ack, true);
    }
    let connected: Vec<String> = state.presence.lock().unwrap().connected().into_iter().filter(|u| u != &user_id).collect();
    if connected.is_empty() {
        return ack_ok(ack, true);
    }
    if let Ok(audience) = crate::perm::project_audience(&state.db, &organization, &project_id, &connected).await {
        let payload = json!({
            "user": { "id": id, "username": username, "displayName": display_name, "avatarUrl": avatar_url },
            "projectId": project_id,
            "projectName": js_slice(&jstr(body.get("projectName")), 80),
            "at": now_ms(),
        });
        emit_users(&state, &audience, "library:playing", &payload).await;
    }
    ack_ok(ack, true);
}

// ------------------------------------------------------------------ doküman imleçleri

/// Doküman/tuval imleçleri: erişimi doğrulandıktan sonra aynı dokümana erişebilen diğerlerine.
async fn document_cursor(socket: SocketRef, Data(body): Data<Value>, ack: AckSender, State(state): State<AppState>) {
    let area = body.get("area").map(js_string).unwrap_or_else(|| "undefined".into());
    let ready = truthy(body.get("organizationId")) && truthy(body.get("projectId")) && truthy(body.get("documentId")) && ["CANVAS", "PAGE"].contains(&area.as_str());
    let Some(user_id) = user_of(&socket).filter(|_| ready) else { return ack_ok(ack, false) };
    let document: Option<(String, Vec<String>)> = sqlx::query_as(
        r#"select d."ownerId", coalesce((select array_agg(dp."userId") from "DocumentPermission" dp where dp."documentId" = d.id), '{}')
           from "ProjectDocument" d join "Project" p on p.id = d."projectId"
           where d.id = $1 and d."projectId" = $2 and p."organizationId" = $3 and p."deletedAt" is null
             and (d."ownerId" = $4 or exists (select 1 from "DocumentPermission" x where x."documentId" = d.id and x."userId" = $4))
           limit 1"#,
    )
    .bind(js_string(&body["documentId"]))
    .bind(js_string(&body["projectId"]))
    .bind(js_string(&body["organizationId"]))
    .bind(&user_id)
    .fetch_optional(&state.db)
    .await
    .unwrap_or_default();
    let Some((owner, permitted)) = document else { return ack_ok(ack, false) };
    let user: Option<(String, Option<String>)> = sqlx::query_as(r#"select "displayName", "avatarUrl" from "User" where id = $1"#)
        .bind(&user_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or_default();
    let Some((display_name, avatar_url)) = user else { return ack_ok(ack, false) };
    let audience: Vec<String> = std::iter::once(owner).chain(permitted).filter(|id| id != &user_id).collect();
    let coord = |key: &str| num(js_num(body.get(key)).filter(|f| *f != 0.0).unwrap_or(0.0).clamp(-100000.0, 100000.0));
    let payload = json!({
        "documentId": body["documentId"],
        "userId": user_id,
        "displayName": display_name,
        "avatarUrl": avatar_url,
        "area": body["area"],
        "x": coord("x"),
        "y": coord("y"),
        "at": now_ms(),
    });
    emit_users(&state, &audience, "document:cursor", &payload).await;
    ack_ok(ack, true);
}

// ------------------------------------------------------------------ abonelik / anlık görüntü

async fn subscribe(socket: SocketRef, Data(body): Data<Value>, ack: AckSender, State(state): State<AppState>) {
    let organization = jstr(body.get("organizationId"));
    let Some(user_id) = user_of(&socket).filter(|_| !organization.is_empty()) else { return ack_ok(ack, false) };
    if !member_exists(&state, &organization, &user_id).await {
        return ack_ok(ack, false);
    }
    socket.join(format!("org:{organization}"));
    replay_presence(&socket, &state, &user_id, &organization).await;
    ack_ok(ack, true);
}

/// Anlık görüntü yalnız isteyen sokete gider: ekibin o anki etkinlikleri ve aktif ses odaları.
async fn replay_presence(socket: &SocketRef, state: &AppState, user_id: &str, organization_id: &str) {
    let activities: Vec<(String, Value)> = state.presence.lock().unwrap().activities.clone();
    let others: Vec<String> = activities.iter().map(|(id, _)| id.clone()).collect();
    let peers = crate::perm::peers_among(&state.db, user_id, &others).await.unwrap_or_default();
    for (id, activity) in activities {
        if id != user_id && peers.contains(&id) {
            let _ = socket.emit("presence:activity", &json!({ "userId": id, "activity": activity }));
        }
    }
    crate::calls::replay_voice(socket, state, user_id, organization_id).await;
}

async fn snapshot(socket: SocketRef, Data(body): Data<Value>, ack: AckSender, State(state): State<AppState>) {
    let organization = jstr(body.get("organizationId"));
    let joined = socket.rooms().iter().any(|room| room.as_ref() == format!("org:{organization}"));
    let Some(user_id) = user_of(&socket).filter(|_| !organization.is_empty() && joined) else { return ack_ok(ack, false) };
    replay_presence(&socket, &state, &user_id, &organization).await;
    ack_ok(ack, true);
}

// ------------------------------------------------------------------ yayın yardımcıları

impl Presence {
    pub fn connected(&self) -> Vec<String> {
        self.online.iter().filter(|(_, n)| **n > 0).map(|(id, _)| id.clone()).collect()
    }
    pub fn is_online(&self, user_id: &str) -> bool {
        self.online.get(user_id).copied().unwrap_or(0) > 0
    }
}

/// Bireysel olaylar yalnız ilgili kişilerin `user:` odalarına gider.
pub async fn emit_users(state: &AppState, user_ids: &[String], event: &str, payload: &Value) {
    let Some(ns) = state.io.get().and_then(|io| io.of("/stream")) else { return };
    let mut seen = HashSet::new();
    for id in user_ids {
        if seen.insert(id) {
            let _ = ns.clone().within(format!("user:{id}")).emit(event, payload).await;
        }
    }
}

pub async fn emit_org(state: &AppState, organization_id: &str, event: &str, payload: &Value) {
    let Some(ns) = state.io.get().and_then(|io| io.of("/stream")) else { return };
    let _ = ns.within(format!("org:{organization_id}")).emit(event, payload).await;
}

/// Kuruluştan bağımsız duyuru: bağlı herkese.
pub async fn broadcast(state: &AppState, event: &str, payload: &Value) {
    let Some(ns) = state.io.get().and_then(|io| io.of("/stream")) else { return };
    let _ = ns.emit(event, payload).await;
}

/// Kanal olayları yalnız kanalı görebilen bağlı kişilere; yetki sorgusu düşerse olay düşer.
pub async fn emit_channel(state: &AppState, organization_id: &str, channel_id: &str, event: &str, payload: &Value) {
    let connected = state.presence.lock().unwrap().connected();
    if connected.is_empty() {
        return;
    }
    if let Ok(audience) = crate::perm::channel_audience(&state.db, organization_id, channel_id, &connected).await {
        emit_users(state, &audience, event, payload).await;
    }
}

pub async fn emit_project(state: &AppState, organization_id: &str, project_id: &str, event: &str, payload: &Value) {
    let connected = state.presence.lock().unwrap().connected();
    if connected.is_empty() {
        return;
    }
    if let Ok(audience) = crate::perm::project_audience(&state.db, organization_id, project_id, &connected).await {
        emit_users(state, &audience, event, payload).await;
    }
}

/// Çevrimiçi/durum olayları: yalnız kişiyle en az bir projeyi paylaşan bağlı kişilere (ve kendisine).
pub async fn emit_peers(state: &AppState, user_id: &str, event: &str, payload: &Value) {
    let connected = state.presence.lock().unwrap().connected();
    if connected.is_empty() {
        return;
    }
    if let Ok(audience) = crate::perm::peers_among(&state.db, user_id, &connected).await {
        emit_users(state, &audience, event, payload).await;
    }
}
