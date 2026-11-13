//! Faz 0 / Spike 2 — Rust WebRTC katılımcısı ("arama botu").
//!
//! Bugünkü P2 sunucusunun (NestJS) arama sinyalleşmesine bağlanır ve P2 istemcisiyle
//! (Electron) aynı protokolle konuşur: `call:incoming/accept/state/signal/media`,
//! kusursuz müzakere (küçük kullanıcı kimliği "kibar"), `{description}` / `{candidate}`
//! mesajları, Opus SDP ayarları (`CallProvider.tsx` → `withOpusTuning`).
//!
//! Gönderir: 440 Hz bip (ses) + hareketli test deseni 1280×720@15 (ekran paylaşımı).
//! Alır ve ölçer: karşı tarafın sesi (dBFS) ve görüntüsü (kare/sn, çözünürlük).
//!
//! Ortam değişkenleri:
//!   P2_API         (varsayılan http://127.0.0.1:4110)
//!   P2_BOT_USER / P2_BOT_PASS   botun hesabı
//!   P2_CALL_TARGET kullanıcı adı verilirse bot onu arar; yoksa gelen aramaları kabul eder.
//!   Parite ekranları için (varsayılan davranış değişmez): P2_BOT_SILENT=1 bip yok (sessiz ses),
//!   P2_BOT_NO_SCREEN=1 ekran paylaşımı duyurulmaz, P2_BOT_MUTED=1 / P2_BOT_DEAFENED=1 durum olarak bildirilir.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use futures_util::{FutureExt, StreamExt};
use libwebrtc::audio_source::native::NativeAudioSource;
use libwebrtc::audio_stream::native::NativeAudioStream;
use libwebrtc::peer_connection_factory::native::PeerConnectionFactoryExt;
use libwebrtc::prelude::*;
use libwebrtc::video_source::native::NativeVideoSource;
use libwebrtc::video_stream::native::NativeVideoStream;
use rust_socketio::Payload;
use rust_socketio::asynchronous::{Client, ClientBuilder};
use serde_json::{Value, json};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

const SAMPLE_RATE: u32 = 48_000;
const VOICE_KBPS: u32 = 96; // settings.ts varsayılanı
const WIDTH: u32 = 1280;
const HEIGHT: u32 = 720;

enum Ev {
    Socket(&'static str, Value),
    Ice(String, IceCandidate),
    Negotiate(String),
    Connection(String, PeerConnectionState),
    Track(String, MediaStreamTrack),
}

struct Peer {
    pc: PeerConnection,
    polite: bool,
    pending: Vec<IceCandidate>,
}

/// Gelen medya ölçümleri (peer başına).
#[derive(Default)]
struct Meter {
    audio_frames: AtomicU64,
    audio_peak_db: AtomicU64, // f64 bitleri
    video_frames: AtomicU64,
    video_size: AtomicU64, // (w << 32) | h
}

fn log(msg: impl AsRef<str>) {
    println!("[{}] {}", chrono_like_now(), msg.as_ref());
}

fn chrono_like_now() -> String {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let s = t.as_secs() % 86_400;
    format!("{:02}:{:02}:{:02}", (s / 3600 + 3) % 24, (s / 60) % 60, s % 60)
}

/// CallProvider.tsx `withOpusTuning`: karşı tarafa giden SDP'de Opus fmtp parametreleri.
fn with_opus_tuning(sdp: &str) -> String {
    let payloads: Vec<String> = sdp
        .split("\r\n")
        .filter_map(|l| l.strip_prefix("a=rtpmap:"))
        .filter(|rest| rest.to_ascii_lowercase().contains(" opus/"))
        .filter_map(|rest| rest.split(' ').next().map(str::to_string))
        .collect();
    sdp.split("\r\n")
        .map(|line| {
            for p in &payloads {
                let prefix = format!("a=fmtp:{p} ");
                if let Some(rest) = line.strip_prefix(&prefix) {
                    let mut params: Vec<(String, String)> = rest
                        .split(';')
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(|kv| match kv.split_once('=') {
                            Some((k, v)) => (k.to_string(), v.to_string()),
                            None => (kv.to_string(), String::new()),
                        })
                        .collect();
                    for (k, v) in [
                        ("maxaveragebitrate", (VOICE_KBPS * 1000).to_string()),
                        ("stereo", "0".into()),
                        ("sprop-stereo", "0".into()),
                        ("useinbandfec", "1".into()),
                        ("usedtx", "0".into()),
                        ("maxplaybackrate", "48000".into()),
                    ] {
                        match params.iter_mut().find(|(key, _)| key == k) {
                            Some(entry) => entry.1 = v,
                            None => params.push((k.to_string(), v)),
                        }
                    }
                    let joined: Vec<String> = params.iter().map(|(k, v)| if v.is_empty() { k.clone() } else { format!("{k}={v}") }).collect();
                    return format!("{prefix}{}", joined.join(";"));
                }
            }
            line.to_string()
        })
        .collect::<Vec<_>>()
        .join("\r\n")
}

fn first_arg(payload: Payload) -> Value {
    match payload {
        Payload::Text(mut values) if !values.is_empty() => values.swap_remove(0),
        _ => Value::Null,
    }
}

fn forward(client: ClientBuilder, event: &'static str, tx: UnboundedSender<Ev>) -> ClientBuilder {
    client.on(event, move |payload: Payload, _client: Client| {
        let tx = tx.clone();
        async move {
            let _ = tx.send(Ev::Socket(event, first_arg(payload)));
        }
        .boxed()
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let api = std::env::var("P2_API").unwrap_or_else(|_| "http://127.0.0.1:4110".into());
    let user = std::env::var("P2_BOT_USER")?;
    let pass = std::env::var("P2_BOT_PASS")?;
    let target = std::env::var("P2_CALL_TARGET").ok().filter(|t| !t.is_empty());

    // 1) Giriş
    let http = reqwest::Client::new();
    let login: Value = http
        .post(format!("{api}/auth/login"))
        .header("x-p2-client-version", "rust-callbot")
        .json(&json!({ "identifier": user, "password": pass }))
        .send()
        .await?
        .json()
        .await?;
    let token = login["accessToken"].as_str().ok_or_else(|| anyhow::anyhow!("giriş başarısız: {login}"))?.to_string();
    let me = login["user"]["id"].as_str().unwrap_or_default().to_string();
    log(format!("giriş yapıldı: {} ({me})", login["user"]["displayName"]));

    // 2) Medya kaynakları (tüm eşlerle paylaşılır)
    let factory = PeerConnectionFactory::default();
    let audio_source = NativeAudioSource::new(AudioSourceOptions::default(), SAMPLE_RATE, 1, 100);
    let audio_track = factory.create_audio_track("p2bot-ses", audio_source.clone());
    let video_source = NativeVideoSource::new(VideoResolution { width: WIDTH, height: HEIGHT }, true);
    let video_track = factory.create_video_track("p2bot-ekran", video_source.clone());
    spawn_tone(audio_source, std::env::var_os("P2_BOT_SILENT").is_some());
    spawn_pattern(video_source);

    // 3) socket.io /stream
    let (tx, mut rx) = unbounded_channel::<Ev>();
    let mut builder = ClientBuilder::new(api.clone()).namespace("/stream").auth(json!({ "token": token })).reconnect(false);
    for event in ["call:incoming", "call:state", "call:signal", "call:ended", "call:declined", "presence:self"] {
        builder = forward(builder, event, tx.clone());
    }
    let socket = builder.connect().await?;
    log("socket.io /stream bağlandı");

    // 4) İstenirse ara
    if let Some(target) = &target {
        let auth = format!("Bearer {token}");
        let orgs: Value = http.get(format!("{api}/organizations")).header("authorization", &auth).send().await?.json().await?;
        let org = orgs[0]["id"].as_str().ok_or_else(|| anyhow::anyhow!("botun kuruluşu yok"))?.to_string();
        let members: Value = http.get(format!("{api}/organizations/{org}/members")).header("authorization", &auth).send().await?.json().await?;
        let target_id = members
            .as_array()
            .and_then(|m| m.iter().find(|u| u["username"].as_str() == Some(target.as_str())))
            .and_then(|u| u["id"].as_str())
            .ok_or_else(|| anyhow::anyhow!("{target} kuruluşta yok"))?
            .to_string();
        socket.emit("call:start", json!({ "organizationId": org, "kind": "direct", "targetUserId": target_id })).await?;
        log(format!("{target} aranıyor..."));
    } else if let Ok(channel) = std::env::var("P2_CALL_CHANNEL") {
        // Sesli odaya katıl (kanal araması, zil yok).
        let auth = format!("Bearer {token}");
        let orgs: Value = http.get(format!("{api}/organizations")).header("authorization", &auth).send().await?.json().await?;
        let org = orgs[0]["id"].as_str().ok_or_else(|| anyhow::anyhow!("botun kuruluşu yok"))?.to_string();
        let projects: Value = http.get(format!("{api}/organizations/{org}/projects")).header("authorization", &auth).send().await?.json().await?;
        let project = projects[0]["id"].as_str().unwrap_or_default().to_string();
        let cats: Value = http.get(format!("{api}/organizations/{org}/projects/{project}/channels")).header("authorization", &auth).send().await?.json().await?;
        let id = cats
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|c| c["channels"].as_array().cloned().unwrap_or_default())
            .find(|c| c["name"].as_str() == Some(channel.as_str()))
            .and_then(|c| c["id"].as_str().map(str::to_string))
            .ok_or_else(|| anyhow::anyhow!("{channel} kanalı yok"))?;
        socket.emit("call:start", json!({ "organizationId": org, "kind": "channel", "channelId": id, "ring": false })).await?;
        log(format!("#{channel} sesli odasına katılınıyor..."));
    } else {
        log("gelen arama bekleniyor (P2'den bu kullanıcıyı ara)");
    }

    // 5) Olay döngüsü
    let mut call_id: Option<String> = None;
    let mut peers: HashMap<String, Peer> = HashMap::new();
    let mut meters: HashMap<String, Arc<Meter>> = HashMap::new();
    let mut announced_screen = false;
    let mut report = tokio::time::interval(Duration::from_secs(3));
    // P2_BOT_DRAW: ekran paylaşan katılımcının yayınına kalemle çiz (call:signal {draw}/{cursor}).
    let draw = std::env::var_os("P2_BOT_DRAW").is_some();
    let mut draw_tick = tokio::time::interval(Duration::from_millis(150));
    let mut sharers: Vec<String> = Vec::new();
    let mut draw_step: u32 = 0;
    let bot_name = login["user"]["displayName"].as_str().unwrap_or("Bot").to_string();

    loop {
        tokio::select! {
            _ = draw_tick.tick(), if draw && !sharers.is_empty() && call_id.is_some() => {
                // Her 20 adımda (3 sn) bir daire: 17 nokta çizgi, imleç de gezinir.
                let id = call_id.clone().unwrap();
                let (cycle, k) = (draw_step / 20, draw_step % 20);
                draw_step += 1;
                let a = k as f64 / 16.0 * std::f64::consts::TAU;
                let (x, y) = (0.5 + 0.2 * a.cos(), 0.5 + 0.25 * a.sin());
                for target in &sharers {
                    for to in peers.keys() {
                        if k < 17 {
                            let data = json!({ "draw": { "target": target, "id": format!("bot-{cycle}"), "color": "#69db7c", "width": 6, "points": [{ "x": x, "y": y }] } });
                            socket.emit("call:signal", json!({ "callId": id, "to": to, "data": data })).await.ok();
                        }
                        let data = json!({ "cursor": { "target": target, "name": bot_name, "color": "#69db7c", "x": x, "y": y } });
                        socket.emit("call:signal", json!({ "callId": id, "to": to, "data": data })).await.ok();
                    }
                }
                if k == 0 {
                    log(format!("çiziyor: {} yayınına daire bot-{cycle}", sharers.iter().map(|s| &s[..8]).collect::<Vec<_>>().join(",")));
                }
            }
            _ = report.tick() => {
                for (id, m) in &meters {
                    let a = m.audio_frames.swap(0, Ordering::Relaxed);
                    let peak = f64::from_bits(m.audio_peak_db.swap((-120f64).to_bits(), Ordering::Relaxed));
                    let v = m.video_frames.swap(0, Ordering::Relaxed);
                    let size = m.video_size.load(Ordering::Relaxed);
                    let state = peers.get(id).map(|p| format!("{:?}", p.pc.connection_state())).unwrap_or("-".into());
                    log(format!(
                        "← {}: bağlantı={state}  ses: {} kare/sn, tepe {:.1} dBFS  görüntü: {:.1} kare/sn {}x{}",
                        &id[..8.min(id.len())], a / 3, peak, v as f64 / 3., size >> 32, size & 0xffff_ffff
                    ));
                }
            }
            Some(ev) = rx.recv() => match ev {
                Ev::Socket("call:incoming", body) => {
                    let id = body["call"]["id"].as_str().unwrap_or_default().to_string();
                    log(format!("gelen arama: {} — kabul ediliyor", body["from"]["displayName"]));
                    socket.emit("call:accept", json!({ "callId": id })).await.ok();
                }
                Ev::Socket("call:state", body) => {
                    let call = &body["call"];
                    let participants: Vec<String> = call["participants"].as_array().into_iter().flatten()
                        .filter_map(|p| p["userId"].as_str().map(str::to_string)).collect();
                    if !participants.contains(&me) { continue; }
                    let id = call["id"].as_str().unwrap_or_default().to_string();
                    sharers = call["participants"].as_array().into_iter().flatten().filter(|p| p["screen"].as_bool() == Some(true) && p["userId"].as_str() != Some(me.as_str())).filter_map(|p| p["userId"].as_str().map(str::to_string)).collect();
                    if call_id.as_deref() != Some(&id) {
                        log(format!("aramaya katıldı: {id}"));
                        call_id = Some(id.clone());
                        announced_screen = false;
                    }
                    for other in participants.iter().filter(|p| **p != me) {
                        if !peers.contains_key(other) {
                            let peer = new_peer(&factory, other, &me, &audio_track, &video_track, &tx)?;
                            meters.insert(other.clone(), Arc::default());
                            peers.insert(other.clone(), peer);
                        }
                    }
                    peers.retain(|id, peer| { let keep = participants.contains(id); if !keep { peer.pc.close(); } keep });
                    if !announced_screen && participants.len() > 1 {
                        announced_screen = true;
                        let screen = std::env::var_os("P2_BOT_NO_SCREEN").is_none();
                        let (muted, deafened) = (std::env::var_os("P2_BOT_MUTED").is_some(), std::env::var_os("P2_BOT_DEAFENED").is_some());
                        socket.emit("call:media", json!({ "callId": id, "screen": screen, "muted": muted, "deafened": deafened })).await.ok();
                        log(if screen { "ekran paylaşımı (test deseni) duyuruldu" } else { "durum duyuruldu (ekran yok)" });
                    }
                }
                Ev::Socket("call:signal", body) => {
                    let from = body["from"].as_str().unwrap_or_default().to_string();
                    if call_id.as_deref() != body["callId"].as_str() { continue; }
                    if !peers.contains_key(&from) {
                        let peer = new_peer(&factory, &from, &me, &audio_track, &video_track, &tx)?;
                        meters.insert(from.clone(), Arc::default());
                        peers.insert(from.clone(), peer);
                    }
                    let peer = peers.get_mut(&from).unwrap();
                    let data = &body["data"];
                    if let Some(desc) = data.get("description").filter(|d| !d.is_null()) {
                        handle_description(&socket, call_id.as_deref().unwrap(), &from, peer, desc).await;
                    } else if let Some(c) = data.get("candidate").filter(|c| !c.is_null()) {
                        let sdp = c["candidate"].as_str().unwrap_or_default();
                        if sdp.is_empty() { continue; }
                        match IceCandidate::parse(c["sdpMid"].as_str().unwrap_or("0"), c["sdpMLineIndex"].as_i64().unwrap_or(0) as i32, sdp) {
                            Ok(cand) if peer.pc.current_remote_description().is_some() => { let _ = peer.pc.add_ice_candidate(cand).await; }
                            Ok(cand) => peer.pending.push(cand),
                            Err(e) => log(format!("ICE adayı ayrıştırılamadı: {e:?}")),
                        }
                    } else if let Some(d) = data.get("draw") {
                        log(format!("çizim alındı: {} → {} {} ({} nokta, {}, {}px)", &from[..8], &d["target"].as_str().unwrap_or("")[..8.min(d["target"].as_str().unwrap_or("").len())], d["id"].as_str().unwrap_or(""), d["points"].as_array().map(|a| a.len()).unwrap_or(0), d["color"].as_str().unwrap_or(""), d["width"]));
                    } else if let Some(c) = data.get("cursor") {
                        log(format!("imleç alındı: {} {} ({:.2},{:.2}){}", &from[..8], c["name"].as_str().unwrap_or(""), c["x"].as_f64().unwrap_or(0.), c["y"].as_f64().unwrap_or(0.), if c["gone"].as_bool() == Some(true) { " gitti" } else { "" }));
                    } else if let Some(w) = data.get("watch") {
                        log(format!("{} yayınımı {}", &from[..8], if w.as_bool() == Some(true) { "izlemeye başladı" } else { "izlemeyi bıraktı" }));
                    }
                }
                Ev::Socket("call:ended", body) => {
                    log(format!("arama bitti ({})", body["reason"]));
                    for (_, p) in peers.drain() { p.pc.close(); }
                    meters.clear();
                    call_id = None;
                    if target.is_some() { break; }
                }
                Ev::Socket("call:declined", _) => { log("arama reddedildi"); }
                Ev::Socket(_, _) => {}
                Ev::Ice(to, cand) => {
                    if let Some(id) = &call_id {
                        let data = json!({ "candidate": { "candidate": cand.candidate(), "sdpMid": cand.sdp_mid(), "sdpMLineIndex": cand.sdp_mline_index() } });
                        socket.emit("call:signal", json!({ "callId": id, "to": to, "data": data })).await.ok();
                    }
                }
                Ev::Negotiate(to) => {
                    // Yalnız kaba taraf kendiliğinden teklif yapar; kibar taraf karşı teklifi bekler.
                    let (Some(id), Some(peer)) = (&call_id, peers.get(&to)) else { continue };
                    if peer.polite || peer.pc.signaling_state() != SignalingState::Stable { continue; }
                    match peer.pc.create_offer(OfferOptions { ice_restart: false, offer_to_receive_audio: None, offer_to_receive_video: None }).await {
                        Ok(offer) => {
                            let sdp = offer.to_string();
                            if peer.pc.set_local_description(offer).await.is_ok() {
                                socket.emit("call:signal", json!({ "callId": id, "to": to, "data": { "description": { "type": "offer", "sdp": with_opus_tuning(&sdp) } } })).await.ok();
                                log(format!("teklif gönderildi → {}", &to[..8]));
                            }
                        }
                        Err(e) => log(format!("teklif oluşturulamadı: {e:?}")),
                    }
                }
                Ev::Connection(id, state) => log(format!("{}: bağlantı durumu {state:?}", &id[..8.min(id.len())])),
                Ev::Track(id, track) => {
                    let meter = meters.entry(id.clone()).or_default().clone();
                    match track {
                        MediaStreamTrack::Audio(t) => { log(format!("{}: ses izi geldi", &id[..8])); spawn_audio_meter(t, meter); }
                        MediaStreamTrack::Video(t) => { log(format!("{}: görüntü izi geldi", &id[..8])); spawn_video_meter(t, meter); }
                    }
                }
            }
        }
    }
    socket.disconnect().await.ok();
    Ok(())
}

fn new_peer(
    factory: &PeerConnectionFactory,
    other: &str,
    me: &str,
    audio: &RtcAudioTrack,
    video: &RtcVideoTrack,
    tx: &UnboundedSender<Ev>,
) -> anyhow::Result<Peer> {
    // settings.ts varsayılanıyla aynı STUN; RtcConfiguration #[non_exhaustive].
    let mut config = RtcConfiguration::default();
    config.ice_servers = vec![IceServer { urls: vec!["stun:stun.l.google.com:19302".into()], username: String::new(), password: String::new() }];
    let pc = factory.create_peer_connection(config).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    // İki iz de add_track ile eklenir: karşı teklif gelince bu alıcı-vericiler yeniden kullanılır.
    pc.add_track(MediaStreamTrack::Audio(audio.clone()), &["p2bot"]).map_err(|e| anyhow::anyhow!("{e:?}"))?;
    pc.add_track(MediaStreamTrack::Video(video.clone()), &["p2bot"]).map_err(|e| anyhow::anyhow!("{e:?}"))?;

    let id = other.to_string();
    let t = tx.clone();
    let i = id.clone();
    pc.on_ice_candidate(Some(Box::new(move |c| {
        let _ = t.send(Ev::Ice(i.clone(), c));
    })));
    let t = tx.clone();
    let i = id.clone();
    pc.on_negotiation_needed(Some(Box::new(move |_| {
        let _ = t.send(Ev::Negotiate(i.clone()));
    })));
    let t = tx.clone();
    let i = id.clone();
    pc.on_connection_state_change(Some(Box::new(move |s| {
        let _ = t.send(Ev::Connection(i.clone(), s));
    })));
    let t = tx.clone();
    let i = id.clone();
    pc.on_track(Some(Box::new(move |e| {
        let _ = t.send(Ev::Track(i.clone(), e.track));
    })));
    let polite = me < other; // CallProvider.tsx: createPeer(..., me < other)
    log(format!("eş oluşturuldu → {} ({})", &other[..8.min(other.len())], if polite { "kibar" } else { "kaba" }));
    Ok(Peer { pc, polite, pending: Vec::new() })
}

async fn handle_description(socket: &Client, call_id: &str, from: &str, peer: &mut Peer, desc: &Value) {
    let kind = desc["type"].as_str().unwrap_or_default();
    let sdp = desc["sdp"].as_str().unwrap_or_default();
    let Ok(sdp_type) = kind.parse::<SdpType>() else { return };
    let collision = sdp_type == SdpType::Offer && peer.pc.signaling_state() != SignalingState::Stable;
    if !peer.polite && collision {
        log("çakışan teklif yok sayıldı (kaba taraf)");
        return;
    }
    let Ok(remote) = SessionDescription::parse(sdp, sdp_type) else {
        log("SDP ayrıştırılamadı");
        return;
    };
    if let Err(e) = peer.pc.set_remote_description(remote).await {
        log(format!("uzak açıklama uygulanamadı: {e:?}"));
        return;
    }
    if sdp_type == SdpType::Offer {
        match peer.pc.create_answer(AnswerOptions::default()).await {
            Ok(answer) => {
                let text = answer.to_string();
                if peer.pc.set_local_description(answer).await.is_ok() {
                    socket
                        .emit("call:signal", json!({ "callId": call_id, "to": from, "data": { "description": { "type": "answer", "sdp": with_opus_tuning(&text) } } }))
                        .await
                        .ok();
                    log(format!("cevap gönderildi → {}", &from[..8]));
                }
            }
            Err(e) => log(format!("cevap oluşturulamadı: {e:?}")),
        }
    }
    for cand in peer.pending.drain(..) {
        let _ = peer.pc.add_ice_candidate(cand).await;
    }
}

/// 440 Hz bip: her 1,5 sn'de 300 ms, -12 dBFS. 10 ms'lik kareler.
fn spawn_tone(source: NativeAudioSource, silent: bool) {
    tokio::spawn(async move {
        let per_frame = (SAMPLE_RATE / 100) as usize;
        let mut n: u64 = 0;
        let mut tick = tokio::time::interval(Duration::from_millis(10));
        loop {
            tick.tick().await;
            let samples: Vec<i16> = (0..per_frame)
                .map(|_| {
                    let t = n as f64 / SAMPLE_RATE as f64;
                    n += 1;
                    if !silent && t % 1.5 < 0.3 { ((t * 440.0 * std::f64::consts::TAU).sin() * 0.25 * i16::MAX as f64) as i16 } else { 0 }
                })
                .collect();
            let mut frame = AudioFrame::new(SAMPLE_RATE, 1, per_frame as u32);
            frame.data = Cow::Owned(samples);
            let _ = source.capture_frame(&frame).await;
        }
    });
}

/// Hareketli test deseni: kayan renkli şeritler + sekerek ilerleyen beyaz kutu.
fn spawn_pattern(source: NativeVideoSource) {
    tokio::spawn(async move {
        let start = Instant::now();
        let mut tick = tokio::time::interval(Duration::from_millis(1000 / 15));
        let mut frame_no: u32 = 0;
        loop {
            tick.tick().await;
            let mut buffer = I420Buffer::new(WIDTH, HEIGHT);
            let (sy, su, sv) = buffer.strides();
            let (y, u, v) = buffer.data_mut();
            let shift = frame_no * 6;
            for row in 0..HEIGHT {
                for col in 0..WIDTH {
                    let band = ((col + shift) / 160) % 8;
                    y[(row * sy + col) as usize] = 60 + (band * 22) as u8;
                }
            }
            for row in 0..HEIGHT / 2 {
                for col in 0..WIDTH / 2 {
                    let band = ((col * 2 + shift) / 160) % 8;
                    u[(row * su + col) as usize] = (64 + band * 18) as u8;
                    v[(row * sv + col) as usize] = (200 - band * 14) as u8;
                }
            }
            // Beyaz kutu (hareket algısı için)
            let bx = (frame_no * 12) % (WIDTH - 160);
            let by = ((frame_no * 7) % (2 * (HEIGHT - 160))).abs_diff(HEIGHT - 160).min(HEIGHT - 160);
            for row in by..by + 160 {
                for col in bx..bx + 160 {
                    y[(row * sy + col) as usize] = 235;
                }
            }
            let frame = VideoFrame {
                rotation: VideoRotation::VideoRotation0,
                timestamp_us: start.elapsed().as_micros() as i64,
                frame_metadata: None,
                buffer,
            };
            source.capture_frame(&frame);
            frame_no += 1;
        }
    });
}

fn spawn_audio_meter(track: RtcAudioTrack, meter: Arc<Meter>) {
    tokio::spawn(async move {
        let mut stream = NativeAudioStream::new(track, SAMPLE_RATE as i32, 1);
        while let Some(frame) = stream.next().await {
            meter.audio_frames.fetch_add(1, Ordering::Relaxed);
            let peak = frame.data.iter().map(|s| (*s as i32).unsigned_abs()).max().unwrap_or(0) as f64 / i16::MAX as f64;
            let db = if peak > 0. { 20. * peak.log10() } else { -120. };
            let prev = f64::from_bits(meter.audio_peak_db.load(Ordering::Relaxed));
            if db > prev || prev == 0.0 {
                meter.audio_peak_db.store(db.to_bits(), Ordering::Relaxed);
            }
        }
    });
}

fn spawn_video_meter(track: RtcVideoTrack, meter: Arc<Meter>) {
    tokio::spawn(async move {
        let mut stream = NativeVideoStream::new(track);
        while let Some(frame) = stream.next().await {
            meter.video_frames.fetch_add(1, Ordering::Relaxed);
            let (w, h) = (frame.buffer.width() as u64, frame.buffer.height() as u64);
            meter.video_size.store((w << 32) | h, Ordering::Relaxed);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::with_opus_tuning;

    #[test]
    fn opus_fmtp_is_tuned_like_callprovider() {
        let sdp = "v=0\r\na=rtpmap:111 opus/48000/2\r\na=fmtp:111 minptime=10;useinbandfec=0\r\na=rtpmap:96 VP8/90000";
        let out = with_opus_tuning(sdp);
        assert!(out.contains("a=fmtp:111 minptime=10;useinbandfec=1;maxaveragebitrate=96000;stereo=0;sprop-stereo=0;usedtx=0;maxplaybackrate=48000"), "{out}");
        assert!(out.contains("a=rtpmap:96 VP8/90000"));
    }
}
