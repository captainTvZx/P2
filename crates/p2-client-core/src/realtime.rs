//! Gerçek zamanlı bağlantı — `src/realtime.ts` karşılığı.
//!
//! socket.io `/stream` namespace'ine erişim jetonuyla bağlanır, bağlanınca kuruluş odasına
//! `subscribe` olur. Kendi iş parçacığında (kendi tokio çalışma ortamıyla) yaşar; olaylar
//! arayüze `futures` kanalıyla tipli olarak akar. Bağlantı koparsa (sunucu yeniden
//! başladı, jetonun süresi doldu) jeton gerekiyorsa yenilenip yeniden bağlanılır.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures::FutureExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use rust_socketio::asynchronous::{Client, ClientBuilder};
use rust_socketio::{Event, Payload, TransportType};
use serde_json::{Value, json};

use crate::api::Api;
use crate::models::{CallParticipant, CallState, ChatGroup, Message, Scope};

/// Mesaj olayları kanal, DM ve grup için ortaktır; hangi sohbete ait olduğu `scope`'ta.
/// Mesajlardaki tepkiler DM/grupta ham gelir: arayüz `Message::normalize` ile özetler.
#[derive(Clone, Debug)]
pub enum RtEvent {
    Connected,
    Disconnected,
    MessageCreated { scope: Scope, parent_id: Option<String>, message: Message },
    MessageUpdated { scope: Scope, parent_id: Option<String>, message: Message },
    MessageDeleted { scope: Scope, message_id: String, parent_id: Option<String> },
    /// Ham (emoji, userId) satırları; istemci kendi kimliğine göre özetler.
    Reactions { scope: Scope, message_id: String, rows: Vec<(String, String)> },
    Presence { user_id: String, status: String },
    /// Kendi durumum (tercih + görünen): `presence:prefer` sonrası ve bağlanınca.
    PresenceSelf { preference: String, status: String },
    /// "yazıyor": kanal (`channelId`) ya da DM (`conversationId`) için.
    Typing { scope: Scope, user_id: String, display_name: String, typing: bool },
    /// Yeni DM açıldı (karşı taraf başlattı) — liste yenilenir.
    /// `direct:conversation`: sohbet açıldı ya da (`hidden` doluysa) listemden kaldırıldı / geri geldi.
    ConversationOpened { conversation_id: String, project_id: Option<String>, hidden: Option<bool> },
    /// Grup oluşturuldu / üyeleri ya da adı değişti.
    GroupChanged { group: ChatGroup },
    /// Bu kullanıcı gruptan çıktı ya da çıkarıldı.
    GroupLeft { group_id: String },
    /// Gelen arama (çalar; `silent` ise sessiz bildirim).
    CallIncoming { call: CallState, from_id: String, from_name: String, silent: bool },
    /// Aramanın güncel hali (katılımcılar, sustur/yayın bayrakları).
    CallState { call: CallState },
    /// WebRTC sinyali (`description` / `candidate` / `watch`…).
    CallSignal { call_id: String, from: String, data: Value },
    /// Arama bitti; `reason` = "switched" ise başka odaya geçildi.
    CallEnded { call_id: String, reason: String },
    CallDeclined { call_id: String, user_id: String },
    /// Moderatör susturdu.
    CallForceMute { call_id: String },
    /// Sesli kanaldakiler (kanal listesinde gösterilir).
    VoicePresence { channel_id: String, participants: Vec<CallParticipant> },
    /// Görev panosu: board:updated|deleted, sprint:updated|deleted, task:updated|deleted|moved|comment…
    /// (ham yük; pano ekranı kendisi ayrıştırır).
    Board { event: String, payload: Value },
    /// Henüz tipli karşılığı olmayan olaylar (sonraki dilimler).
    Other { event: String, payload: Value },
}

const EVENTS: &[&str] = &[
    "message:created",
    "message:updated",
    "message:deleted",
    "message:reactions",
    "direct:created",
    "direct:updated",
    "direct:deleted",
    "direct:reactions",
    "direct:conversation",
    "group:message",
    "group:message:updated",
    "group:message:deleted",
    "group:reactions",
    "group:reply",
    "group:created",
    "group:updated",
    "group:left",
    "presence:update",
    "presence:self",
    // Kişinin etkinliği (oynuyor/indiriyor…) ve "X oyunu başlattı" duyurusu (Steam tarzı köşe kartı).
    "presence:activity",
    "library:playing",
    "typing",
    "channels:changed",
    "members:changed",
    "activity:new",
    "call:incoming",
    "call:state",
    "call:signal",
    "call:ended",
    "call:declined",
    "call:force-mute",
    "call:refused",
    "board:updated",
    "board:deleted",
    "sprint:updated",
    "sprint:deleted",
    "task:updated",
    "task:deleted",
    "task:moved",
    "task:comment",
    "task:comment:deleted",
    "task:comment:reactions",
    "document:created",
    "document:updated",
    "document:deleted",
    "document:access",
    "document:revoked",
    "voice:presence",
    // Kütüphane: yeni sürüm yayınlandı / canlıya alındı (otomatik güncelleme).
    "release:published",
    "release:live",
    // İstemci güncellemesi yayınlandı (realtime.ts → güncelleyici denetler).
    "update:published",
    // Proje toplulukları (docs/proje-bazli-yapi.md): çıkarıldım/yasaklandım, proje silindi/kalıcı
    // silindi, projeye katılan/ayrılan kişi, proje başına yetki değişikliği.
    "project:created",
    "project:removed",
    "project:deleted",
    "project:purged",
    "project:member:joined",
    "project:member:removed",
    "project:permissions",
    "permissions:changed",
    // Sunucu yönetimi: sunucu yetkilerim değişti / sunucudan yasaklandım.
    "server:permissions",
    "server:banned",
    // Arkadaşlar (docs/arkadaslar.md): istek, kabul, çıkarma/ret/iptal, engel.
    "friend:request",
    "friend:accepted",
    "friend:removed",
    "friend:blocked",
    "friend:unblocked",
];

fn rows(v: &Value) -> Vec<(String, String)> {
    v["rows"]
        .as_array()
        .map(|rows| rows.iter().map(|r| (r["emoji"].as_str().unwrap_or_default().to_string(), r["userId"].as_str().unwrap_or_default().to_string())).collect())
        .unwrap_or_default()
}

fn parse(event: &str, v: Value) -> RtEvent {
    let s = |k: &str| v[k].as_str().unwrap_or_default().to_string();
    let parent = || v["parentId"].as_str().map(str::to_string);
    let msg = || serde_json::from_value::<Message>(v["message"].clone()).ok();
    let scope = match event.split(':').next() {
        Some("direct") => Scope::Direct(s("conversationId")),
        Some("group") => Scope::Group(s("groupId")),
        _ => Scope::Channel(s("channelId")),
    };
    let other = |v: Value| RtEvent::Other { event: event.into(), payload: v };
    let call = || serde_json::from_value::<CallState>(v["call"].clone()).ok();
    if ["board:", "sprint:", "task:"].iter().any(|p| event.starts_with(p)) {
        return RtEvent::Board { event: event.into(), payload: v };
    }
    match event {
        "call:incoming" => match call() {
            Some(call) => RtEvent::CallIncoming { call, from_id: v["from"]["userId"].as_str().unwrap_or_default().into(), from_name: v["from"]["displayName"].as_str().unwrap_or_default().into(), silent: v["silent"].as_bool().unwrap_or(false) },
            None => other(v),
        },
        "call:state" => match call() {
            Some(call) => RtEvent::CallState { call },
            None => other(v),
        },
        "call:signal" => RtEvent::CallSignal { call_id: s("callId"), from: s("from"), data: v["data"].clone() },
        "call:ended" => RtEvent::CallEnded { call_id: s("callId"), reason: s("reason") },
        "call:declined" => RtEvent::CallDeclined { call_id: s("callId"), user_id: s("userId") },
        "call:force-mute" => RtEvent::CallForceMute { call_id: s("callId") },
        "voice:presence" => RtEvent::VoicePresence { channel_id: s("channelId"), participants: serde_json::from_value(v["participants"].clone()).unwrap_or_default() },
        "message:created" | "direct:created" | "group:message" | "group:reply" => match msg() {
            Some(message) => RtEvent::MessageCreated { scope, parent_id: parent(), message },
            None => other(v),
        },
        "message:updated" | "direct:updated" | "group:message:updated" => match msg() {
            Some(message) => RtEvent::MessageUpdated { scope, parent_id: parent(), message },
            None => other(v),
        },
        "message:deleted" | "direct:deleted" | "group:message:deleted" => RtEvent::MessageDeleted { scope, message_id: s("messageId"), parent_id: parent() },
        "message:reactions" | "direct:reactions" | "group:reactions" => RtEvent::Reactions { scope, message_id: s("messageId"), rows: rows(&v) },
        "direct:conversation" => RtEvent::ConversationOpened { conversation_id: s("conversationId"), project_id: v["projectId"].as_str().map(str::to_string), hidden: v["hidden"].as_bool() },
        "group:created" | "group:updated" => match serde_json::from_value::<ChatGroup>(v["group"].clone()) {
            Ok(group) => RtEvent::GroupChanged { group },
            Err(_) => other(v),
        },
        "group:left" => RtEvent::GroupLeft { group_id: s("groupId") },
        "presence:update" => RtEvent::Presence { user_id: s("userId"), status: s("status") },
        "presence:self" => RtEvent::PresenceSelf { preference: s("preference"), status: s("status") },
        "typing" => RtEvent::Typing {
            scope: match v["conversationId"].as_str() {
                Some(c) => Scope::Direct(c.to_string()),
                None => Scope::Channel(s("channelId")),
            },
            user_id: s("userId"),
            display_name: s("displayName"),
            typing: v["typing"].as_bool().unwrap_or(true),
        },
        _ => other(v),
    }
}

/// Bağlantı tanıtıcısı: düşürülünce (drop) iş parçacığı kapanır.
pub struct Realtime {
    stop: Arc<AtomicBool>,
    outbox: UnboundedSender<(String, Value)>,
}

impl Realtime {
    /// Sunucuya olay gönder (yazıyor bildirimi gibi). Bağlantı yoksa düşer.
    pub fn emit(&self, event: &str, payload: Value) {
        let _ = self.outbox.unbounded_send((event.to_string(), payload));
    }

    /// Başka bileşenlerin (arama denetçisi) olay gönderebilmesi için kopyalanabilir uç.
    pub fn sender(&self) -> RtSender {
        RtSender(self.outbox.clone())
    }
}

/// Kopyalanabilir gönderici; bağlantının yaşam süresine sahip olmaz.
#[derive(Clone)]
pub struct RtSender(UnboundedSender<(String, Value)>);

impl RtSender {
    pub fn emit(&self, event: &str, payload: Value) {
        let _ = self.0.unbounded_send((event.to_string(), payload));
    }
}

impl Drop for Realtime {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.outbox.close_channel();
    }
}

pub fn connect(api: Arc<Api>, organization_id: String) -> (Realtime, UnboundedReceiver<RtEvent>) {
    let (tx, rx) = unbounded();
    let (out_tx, out_rx) = unbounded::<(String, Value)>();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();
    std::thread::Builder::new()
        .name("p2-realtime".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("tokio");
            runtime.block_on(run(api, organization_id, tx, out_rx, stop_thread));
        })
        .expect("gerçek zamanlı iş parçacığı");
    (Realtime { stop, outbox: out_tx }, rx)
}

fn rt_trace(line: &str) {
    if std::env::var_os("P2_TRACE_SOCKET").is_some() {
        eprintln!("[soket] {line}");
    }
}

async fn run(api: Arc<Api>, organization_id: String, tx: UnboundedSender<RtEvent>, mut outbox: futures::channel::mpsc::UnboundedReceiver<(String, Value)>, stop: Arc<AtomicBool>) {
    use futures::StreamExt;
    let base = api.socket_base();
    let mut delay = Duration::from_millis(700);
    while !stop.load(Ordering::SeqCst) {
        // Jeton yoksa ya da süresi dolmuşsa yenilet (me() 401 alırsa Api kendisi yeniler).
        let token = {
            let api = api.clone();
            tokio::task::spawn_blocking(move || {
                let _ = api.me();
                api.access_token()
            })
            .await
            .unwrap_or_default()
        };
        rt_trace(&format!("jeton hazır ({} karakter)", token.len()));
        let (closed_tx, mut closed_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
        let mut builder = ClientBuilder::new(base.clone()).transport_type(TransportType::Websocket).namespace("/stream").auth(json!({ "token": token })).reconnect(false);
        for event in EVENTS {
            let tx = tx.clone();
            builder = builder.on(*event, move |payload: Payload, _client: Client| {
                let tx = tx.clone();
                async move {
                    let value = match payload {
                        Payload::Text(mut values) if !values.is_empty() => values.swap_remove(0),
                        _ => Value::Null,
                    };
                    let _ = tx.unbounded_send(parse(event, value));
                }
                .boxed()
            });
        }
        if std::env::var_os("P2_TRACE_SOCKET").is_some() {
            builder = builder.on_any(|event: Event, payload: Payload, _client: Client| {
                async move {
                    let text = match payload {
                        Payload::Text(v) => serde_json::to_string(&v).unwrap_or_default(),
                        other => format!("{other:?}"),
                    };
                    eprintln!("[soket] {event:?} {}", text.chars().take(120).collect::<String>());
                }
                .boxed()
            });
        }
        {
            let closed = closed_tx.clone();
            builder = builder.on(Event::Close, move |_, _| {
                let closed = closed.clone();
                async move {
                    let _ = closed.send(());
                }
                .boxed()
            });
        }
        let connected = builder.connect().await;
        rt_trace(&format!("bağlantı: {:?}", connected.as_ref().err()));
        match connected {
            Ok(client) => {
                delay = Duration::from_millis(700);
                // Sunucu jetonu handleConnection'da eşzamansız doğrular; abonelik o bitmeden
                // gelirse `{ok:false}` döner ve soket kuruluş odasına girmez. Onay gelene dek dene.
                // rust_socketio'da ilk bağlantı ara sıra hiç paket almadan "sağır" kalıyor;
                // onay gelmezse bu soket bırakılıp hemen yenisi açılır (yeniden deneme beklemesi yok).
                let subscribed = subscribe(&client, &organization_id).await;
                rt_trace(&format!("abonelik: {subscribed}"));
                if !subscribed {
                    let _ = client.disconnect().await;
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    continue;
                }
                let _ = tx.unbounded_send(RtEvent::Connected);
                loop {
                    tokio::select! {
                        _ = closed_rx.recv() => break,
                        out = outbox.next() => match out {
                            Some((event, payload)) => { let _ = client.emit(event, payload).await; }
                            None => { let _ = client.disconnect().await; return; }
                        },
                        _ = tokio::time::sleep(Duration::from_millis(500)) => {
                            if stop.load(Ordering::SeqCst) { let _ = client.disconnect().await; return; }
                        }
                    }
                }
                let _ = tx.unbounded_send(RtEvent::Disconnected);
            }
            Err(_) => {}
        }
        // realtime.ts: reconnectionDelay 700 ms, en çok 4 sn.
        tokio::time::sleep(delay).await;
        delay = (delay * 2).min(Duration::from_secs(4));
    }
}

/// `subscribe` olayını onaylı gönderir; `{ok:true}` gelene dek birkaç kez dener.
async fn subscribe(client: &Client, organization_id: &str) -> bool {
    for attempt in 0..3u64 {
        let (ok_tx, ok_rx) = tokio::sync::oneshot::channel::<bool>();
        let ok_tx = std::sync::Mutex::new(Some(ok_tx));
        let sent = client
            .emit_with_ack("subscribe", json!({ "organizationId": organization_id }), Duration::from_millis(800), move |payload: Payload, _c: Client| {
                let ok = matches!(&payload, Payload::Text(v) if v.iter().any(|x| x["ok"] == Value::Bool(true) || x[0]["ok"] == Value::Bool(true)));
                if let Some(tx) = ok_tx.lock().unwrap().take() {
                    let _ = tx.send(ok);
                }
                async {}.boxed()
            })
            .await;
        if sent.is_ok() && matches!(tokio::time::timeout(Duration::from_millis(800), ok_rx).await, Ok(Ok(true))) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100 * (attempt + 1))).await;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_message_events() {
        let v = json!({ "projectId": "p", "channelId": "c", "parentId": null, "message": { "id": "m", "content": "selam", "createdAt": "2026-11-12T10:00:00.000Z", "author": { "id": "u", "username": "cem", "displayName": "Cem" }, "_count": { "replies": 0 } } });
        match parse("message:created", v) {
            RtEvent::MessageCreated { scope, parent_id, message } => {
                assert_eq!((scope, parent_id, message.content.as_str()), (Scope::Channel("c".into()), None, "selam"));
            }
            other => panic!("{other:?}"),
        }
        match parse("message:reactions", json!({ "channelId": "c", "messageId": "m", "rows": [{ "emoji": "👍", "userId": "u" }] })) {
            RtEvent::Reactions { rows, .. } => assert_eq!(rows, vec![("👍".to_string(), "u".to_string())]),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn parses_direct_and_group_events() {
        // DM mesajında tepkiler ham satırlar; normalize ile özetlenir.
        let v = json!({ "conversationId": "d", "message": { "id": "m", "content": "selam", "createdAt": "2026-11-12T10:00:00.000Z", "author": { "id": "u", "username": "cem", "displayName": "Cem" }, "reactions": [{ "emoji": "👍", "userId": "u" }, { "emoji": "👍", "userId": "ben" }] } });
        match parse("direct:created", v) {
            RtEvent::MessageCreated { scope, mut message, .. } => {
                assert_eq!(scope, Scope::Direct("d".into()));
                message.normalize("ben");
                assert_eq!((message.reactions.len(), message.reactions[0].count, message.reactions[0].reacted), (1, 2, true));
            }
            other => panic!("{other:?}"),
        }
        match parse("group:message:deleted", json!({ "groupId": "g", "messageId": "m" })) {
            RtEvent::MessageDeleted { scope, message_id, .. } => assert_eq!((scope, message_id.as_str()), (Scope::Group("g".into()), "m")),
            other => panic!("{other:?}"),
        }
        match parse("typing", json!({ "userId": "u", "displayName": "Cem", "channelId": null, "conversationId": "d", "typing": true })) {
            RtEvent::Typing { scope, typing, .. } => assert_eq!((scope, typing), (Scope::Direct("d".into()), true)),
            other => panic!("{other:?}"),
        }
    }
}
