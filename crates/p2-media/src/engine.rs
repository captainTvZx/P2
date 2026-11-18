//! Arama motoru — CallProvider.tsx'in medya yarısı. Kendi iş parçacığında (kendi tokio
//! çalışma ortamıyla) yaşar; arayüz komut gönderir, motor olay döndürür. Sinyalleşmeyi
//! (socket.io) motor yapmaz: giden `Signal` olaylarını arayüz `call:signal` olarak iletir,
//! gelenleri `Command::Signal` ile verir.
//!
//! Her eşle bir RTCPeerConnection (ağ: tam örgü), kusursuz müzakere (küçük kullanıcı
//! kimliği "kibar"), `{description}` / `{candidate}` mesajları, Opus SDP ayarı — Spike 2'de
//! Electron 0.9.9 ile birlikte çalıştığı doğrulanan protokol.
//!
//! Ses yolu 10 ms'lik saatle döner:
//!   uzak izler → karıştır (+ zil/işaret sesleri) → APM ters akış (yankı referansı) → hoparlör
//!   mikrofon → APM (yankı/gürültü/kazanç) → sustur/bas-konuş kapısı → yerel iz

use std::borrow::Cow;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::StreamExt;
use futures::channel::mpsc as fmpsc;
use libwebrtc::audio_source::native::NativeAudioSource;
use libwebrtc::audio_stream::native::NativeAudioStream;
use libwebrtc::native::apm::AudioProcessingModule;
use libwebrtc::peer_connection_factory::native::PeerConnectionFactoryExt;
use libwebrtc::prelude::*;
use serde_json::{Value, json};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::audio::{AudioIo, FRAME, RATE, level_db, to_i16};
use crate::synth::{Cue, Ring, Synth};
use crate::camera::{Camera, CameraSource};
use crate::video::{Frames, LOCAL_CAMERA, LOCAL_SCREEN, Preview, ScreenSource, Sharer, spawn_receiver};
use libwebrtc::video_source::native::NativeVideoSource;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Debug)]
pub struct IceConfig {
    pub urls: Vec<String>,
    pub username: String,
    pub password: String,
}

/// settings.ts karşılıkları (`inputDeviceId`, `echoCancellation`, `voiceBitrate`…).
#[derive(Clone, Debug)]
pub struct MediaConfig {
    pub input: Option<String>,
    pub output: Option<String>,
    pub echo_cancellation: bool,
    pub noise_suppression: bool,
    pub auto_gain: bool,
    /// Opus hedef bit hızı (kbps, 32–128).
    pub voice_kbps: u32,
    pub ice: Vec<IceConfig>,
    /// 0–2 (1 = değiştirme).
    pub input_volume: f32,
    pub output_volume: f32,
    pub cue_volume: f32,
    /// Testler: mikrofon yerine sentetik ses (gerçek mikrofon açılmaz).
    pub test_tone: bool,
    /// Testler: hoparlör açılmaz (ses yine karıştırılır, seviyeler ölçülür).
    pub silent_output: bool,
    /// Testler (`P2_NO_AUDIO_DEVICE`): hiçbir ses aygıtı açılmaz (mikrofon, hoparlör, loopback);
    /// bilgisayar sesi yerine sentetik halka kullanılır.
    pub no_devices: bool,
    /// Yalnız TURN röle adayları (settings.ts `forceRelay`; TURN tanımlıysa).
    pub force_relay: bool,
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            input: None,
            output: None,
            echo_cancellation: true,
            noise_suppression: true,
            auto_gain: true,
            voice_kbps: 96,
            ice: vec![IceConfig { urls: vec!["stun:stun.l.google.com:19302".into()], username: String::new(), password: String::new() }],
            input_volume: 1.0,
            output_volume: 1.0,
            cue_volume: 0.7,
            test_tone: false,
            silent_output: false,
            force_relay: false,
            no_devices: false,
        }
    }
}

pub enum Command {
    /// Aramanın güncel katılımcıları (`call:state`): eksik eşler kurulur, gidenler kapanır.
    Sync { call_id: String, me: String, participants: Vec<String> },
    /// Gelen `call:signal` verisi.
    Signal { call_id: String, from: String, data: Value },
    /// Aramadan çık: bütün eşler kapanır (motor ve aygıtlar açık kalır).
    Leave,
    SetMuted(bool),
    SetDeafened(bool),
    /// Bas-konuş kapısı: false iken mikrofon gönderilmez.
    SetTalking(bool),
    SetVolume { user: String, volume: f32 },
    /// "Sesimi bu kişiye gönderme" (CallProvider `togglePeerDeafen`): o eşe giden mikrofon izi
    /// sökülür (`send: false`) ya da geri takılır.
    SetPeerSend { user: String, send: bool },
    Ring(Ring),
    Cue(Cue),
    /// Ayarlardaki "kendini duy": mikrofon (işlenmiş) hoparlöre de verilir.
    Loopback(bool),
    Reconfigure(MediaConfig),
    /// Ekran paylaşımını başlat: kare hızı ve kalite ön ayarı (`1080p`, `720p`…; screenShare.ts).
    /// `audio`: bilgisayar sesi de paylaşılır (loopback, mikrofonla karıştırılır).
    StartScreen { source: ScreenSource, fps: u32, quality: String, audio: bool },
    /// Süren paylaşımın ayarlarını canlı değiştir (yayıncının kendi yayın çubuğu): kare hızı, kalite
    /// ön ayarı, bilgisayar sesi. `None` olan değişmez.
    UpdateShare { fps: Option<u32>, quality: Option<String>, audio: Option<bool> },
    StartCamera(CameraSource),
    StopCamera,
    /// İzleyici olarak bu kişinin yayınını hangi kalitede almak istiyorum (`auto`, `720p`…).
    RequestQuality { user: String, level: String },
    StopScreen,
    Stop,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// Arayüz `call:signal { callId, to, data }` olarak gönderir.
    Signal { call_id: String, to: String, data: Value },
    PeerState { user: String, state: String },
    /// ~10/sn: mikrofon (işlenmiş, kapıdan önce) ve eşlerin seviyesi (dBFS).
    Levels { mic: f32, peers: Vec<(String, f32)> },
    Devices { input: Option<String>, output: Option<String> },
    Error(String),
    /// Bu kullanıcıdan yeni görüntü karesi var (`Engine::frames()`'ten alınır).
    Video { user: String },
    /// Görüntü izi bitti.
    VideoGone { user: String },
    /// Kendi yayınım başladı / durdu.
    Screen { on: bool },
    Camera { on: bool },
    /// Bilgisayar sesi paylaşımı açıldı (`mode`: "işlem", "uç nokta", "sentetik") ya da açılamadı.
    ShareAudio { on: bool, mode: String, error: Option<String> },
    /// Bir izleyici yayınımı şu kalitede istiyor.
    ViewerQuality { user: String, level: String },
    /// Tanı satırı (istemci kayda yazar): aygıt açılış süresi, yavaş sinyalleşme, aygıt hataları.
    Trace(String),
}

/// Motor tanıtıcısı. Düşürülünce motor durur ve aygıtlar kapanır.
pub struct Engine {
    tx: UnboundedSender<Command>,
    frames: Frames,
}

impl Engine {
    pub fn start(config: MediaConfig) -> (Self, fmpsc::UnboundedReceiver<Event>) {
        let (tx, rx) = unbounded_channel();
        let frames: Frames = Arc::default();
        let shared = frames.clone();
        let (etx, erx) = fmpsc::unbounded();
        std::thread::Builder::new()
            .name("p2-media".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("tokio");
                runtime.block_on(run(config, rx, etx, shared));
            })
            .expect("medya iş parçacığı");
        (Self { tx, frames }, erx)
    }

    pub fn send(&self, command: Command) {
        let _ = self.tx.send(command);
    }

    /// Gelen görüntülerin son kareleri (kullanıcı kimliğine göre).
    pub fn frames(&self) -> Frames {
        self.frames.clone()
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.tx.send(Command::Stop);
    }
}

/// CallProvider.tsx `withOpusTuning`: giden SDP'de Opus fmtp parametreleri.
pub fn with_opus_tuning(sdp: &str, kbps: u32) -> String {
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
                        ("maxaveragebitrate", (kbps.clamp(32, 128) * 1000).to_string()),
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

enum Internal {
    Ice(String, IceCandidate),
    Negotiate(String),
    Connection(String, PeerConnectionState),
    Track(String, MediaStreamTrack),
}

struct Peer {
    pc: PeerConnection,
    polite: bool,
    pending: Vec<IceCandidate>,
    /// Yayın göndericisi: yalnız ekran paylaşılırken var (iz eklenir/sökülür, yeniden pazarlık).
    video_sender: Option<RtpSender>,
    /// Mikrofon göndericisi (kişiye özel susturma için iz sökülüp takılır).
    audio_sender: Option<RtpSender>,
    /// Kusursuz müzakere: kendi teklifimiz yolda mı.
    making_offer: bool,
}

/// Yerel medya: fabrika ve mikrofon izi ilk aramada kurulur (zil ya da mikrofon testi için
/// WebRTC hiç başlatılmaz — boştaki bellek düşük kalsın).
struct Rtc {
    factory: PeerConnectionFactory,
    source: NativeAudioSource,
    track: RtcAudioTrack,
}

type Jitter = Arc<Mutex<VecDeque<i16>>>;

struct State {
    config: MediaConfig,
    events: fmpsc::UnboundedSender<Event>,
    internal: UnboundedSender<Internal>,
    rtc: Option<Rtc>,
    call_id: Option<String>,
    me: String,
    peers: HashMap<String, Peer>,
    remote: HashMap<String, Jitter>,
    volumes: HashMap<String, f32>,
    /// Kendi call:state'imizden önce gelen sinyaller.
    early: Vec<(String, String, Value)>,
    muted: bool,
    deafened: bool,
    talking: bool,
    loopback: bool,
    frames: Frames,
    receivers: HashMap<String, Arc<AtomicBool>>,
    sharer: Option<Sharer>,
    /// Yayın kaynağı ve izi: her paylaşımda yeniden kurulur. (NativeVideoSource ilk gerçek
    /// kareye kadar 10 fps siyah "keepalive" basar; baştan kurulsaydı karşı tarafta siyah kutu olurdu.)
    screen: Option<(NativeVideoSource, RtcVideoTrack)>,
    camera: Option<(Camera, RtcVideoTrack)>,
    /// Süren paylaşımın kare hızı ve kalitesi.
    share: Option<(u32, String)>,
    /// İzleyicilerin istediği kalite (`call:signal {quality}`).
    viewer_levels: HashMap<String, String>,
    /// Eşlere giden görüntü: "screen" | "camera" | yok.
    sending: Option<&'static str>,
    /// Yayında bilgisayar sesi (gerçek loopback ya da testte sentetik halka).
    system_audio: Option<SystemAudio>,
}

impl State {
    fn emit(&self, event: Event) {
        let _ = self.events.unbounded_send(event);
    }

    fn rtc(&mut self) -> &Rtc {
        self.rtc.get_or_insert_with(|| {
            let factory = PeerConnectionFactory::default();
            let source = NativeAudioSource::new(AudioSourceOptions::default(), RATE, 1, 100);
            let track = factory.create_audio_track("p2-mikrofon", source.clone());
            Rtc { factory, source, track }
        })
    }

    fn new_peer(&mut self, other: &str) -> Result<(), String> {
        let ice = self.config.ice.clone();
        let polite = self.me.as_str() < other; // CallProvider.tsx: createPeer(..., me < other)
        let internal = self.internal.clone();
        let screen_track = self.active_track();
        let relay = self.config.force_relay && ice.iter().any(|s| s.urls.iter().any(|u| u.starts_with("turn")));
        let rtc = self.rtc();
        let mut config = RtcConfiguration::default();
        config.ice_servers = ice.into_iter().map(|s| IceServer { urls: s.urls, username: s.username, password: s.password }).collect();
        if relay {
            config.ice_transport_type = libwebrtc::peer_connection_factory::IceTransportsType::Relay;
        }
        let pc = rtc.factory.create_peer_connection(config).map_err(|e| format!("{e:?}"))?;
        let track = rtc.track.clone();
        let id = other.to_string();
        let (t, i) = (internal.clone(), id.clone());
        pc.on_ice_candidate(Some(Box::new(move |c| {
            let _ = t.send(Internal::Ice(i.clone(), c));
        })));
        let (t, i) = (internal.clone(), id.clone());
        pc.on_negotiation_needed(Some(Box::new(move |_| {
            let _ = t.send(Internal::Negotiate(i.clone()));
        })));
        let (t, i) = (internal.clone(), id.clone());
        pc.on_connection_state_change(Some(Box::new(move |s| {
            let _ = t.send(Internal::Connection(i.clone(), s));
        })));
        let (t, i) = (internal.clone(), id.clone());
        pc.on_track(Some(Box::new(move |e| {
            let _ = t.send(Internal::Track(i.clone(), e.track));
        })));
        // İz, geri çağrılar bağlandıktan SONRA eklenir: "negotiation needed" add_track
        // içinde tetiklenir; önce eklenirse kaba tarafın teklifi hiç gönderilmez.
        let audio_sender = Some(pc.add_track(MediaStreamTrack::Audio(track), &["p2"]).map_err(|e| format!("{e:?}"))?);
        // Yayın sürerken katılan da görüntüyü ilk andan alır.
        let video_sender = screen_track.and_then(|t| pc.add_track(MediaStreamTrack::Video(t), &["p2"]).ok());
        self.peers.insert(id.clone(), Peer { pc, polite, pending: Vec::new(), video_sender, audio_sender, making_offer: false });
        if !polite {
            // Yine de kaçırılırsa (tetiklenme zamanlaması sürüme göre değişebilir) kaba taraf teklif eder.
            let _ = internal.send(Internal::Negotiate(id));
        }
        Ok(())
    }

    /// Eşlere giden görüntü: yayın varsa o, yoksa kamera (CallProvider.tsx `replaceVideo`).
    fn active_track(&self) -> Option<RtcVideoTrack> {
        self.screen.as_ref().map(|s| s.1.clone()).or_else(|| self.camera.as_ref().map(|c| c.1.clone()))
    }

    fn active_kind(&self) -> Option<&'static str> {
        if self.screen.is_some() {
            Some("screen")
        } else if self.camera.is_some() {
            Some("camera")
        } else {
            None
        }
    }

    /// Giden görüntü değiştiyse her eşte iz sökülüp yenisi eklenir (yeniden pazarlık).
    fn swap_video(&mut self) {
        let kind = self.active_kind();
        if kind == self.sending {
            return;
        }
        self.sending = kind;
        let track = self.active_track();
        for peer in self.peers.values_mut() {
            if let Some(sender) = peer.video_sender.take() {
                let _ = peer.pc.remove_track(sender);
            }
            if let Some(t) = &track {
                peer.video_sender = peer.pc.add_track(MediaStreamTrack::Video(t.clone()), &["p2"]).ok();
            }
        }
        // Kodlayıcı sınırları yeniden pazarlık bitince uygulanır (bkz. signal).
    }

    /// Bu eşe giden görüntünün kodlayıcı sınırları (CallProvider.tsx `applyEncoding`):
    /// yayında paylaşanın kalitesi tavan, izleyicinin istediği daha düşükse o.
    fn apply_encoding(&self, id: &str) {
        let Some(sender) = self.peers.get(id).and_then(|p| p.video_sender.as_ref()) else { return };
        let mut params = sender.parameters();
        if params.encodings.is_empty() {
            params.encodings.push(RtpEncodingParameters::default());
        }
        let enc = &mut params.encodings[0];
        match (&self.share, self.screen.is_some()) {
            (Some((fps, quality)), true) => {
                let (share_h, _) = quality_preset(quality);
                let wanted = self.viewer_levels.get(id).map(String::as_str).filter(|l| *l != "auto");
                let level = match wanted {
                    Some(l) if quality_preset(l).0 < share_h => l,
                    _ => quality.as_str(),
                };
                let (h, _) = quality_preset(level);
                enc.max_bitrate = Some(bitrate_for(level, *fps));
                enc.max_framerate = Some(*fps as f64);
                // Kaynağın gerçek yüksekliğinden istenen kaliteye ölçeklenir (yayın sırasında kalite
                // değişince çözünürlük de değişsin); kaynak henüz bilinmiyorsa paylaşım kalitesi esas.
                let source_h = self.sharer.as_ref().map(|s| s.height()).filter(|h| *h > 0).unwrap_or(share_h);
                enc.scale_resolution_down_by = Some((source_h as f64 / h as f64).max(1.0));
            }
            _ => {
                enc.max_bitrate = Some(1_200_000);
                enc.max_framerate = Some(30.0);
                enc.scale_resolution_down_by = Some(1.0);
            }
        }
        let ok = sender.set_parameters(params).is_ok();
        if std::env::var_os("P2_TRACE_RTC").is_some() {
            eprintln!("[rtc] {id} kodlayıcı sınırları {} ({:?})", if ok { "uygulandı" } else { "uygulanamadı" }, self.viewer_levels.get(id));
        }
    }

    fn close_peer(&mut self, user: &str) {
        if let Some(alive) = self.receivers.remove(user) {
            alive.store(false, Ordering::Relaxed);
        }
        if let Some(p) = self.peers.remove(user) {
            p.pc.close();
        }
        self.remote.remove(user);
    }

    async fn sync(&mut self, call_id: String, me: String, participants: Vec<String>) {
        if self.call_id.as_deref() != Some(call_id.as_str()) {
            // Başka bir aramaya geçildi: eski eşler kapanır.
            for user in self.peers.keys().cloned().collect::<Vec<_>>() {
                self.close_peer(&user);
            }
            self.call_id = Some(call_id.clone());
        }
        self.me = me;
        let me = self.me.clone();
        for user in participants.iter().filter(|u| **u != me) {
            if !self.peers.contains_key(user) {
                if let Err(e) = self.new_peer(user) {
                    self.emit(Event::Error(p2_i18n::tv("media2.connect", &[("error", &e.to_string())])));
                }
            }
        }
        let gone: Vec<String> = self.peers.keys().filter(|u| !participants.contains(u)).cloned().collect();
        for user in gone {
            self.close_peer(&user);
        }
        // Erken gelen sinyaller sırayla işlenir.
        let early = std::mem::take(&mut self.early);
        for (cid, from, data) in early {
            if cid == call_id {
                self.signal(cid, from, data).await;
            }
        }
    }

    async fn signal(&mut self, call_id: String, from: String, data: Value) {
        if self.call_id.as_deref() != Some(call_id.as_str()) {
            if self.early.len() < 40 {
                self.early.push((call_id, from, data));
            }
            return;
        }
        if !self.peers.contains_key(&from) {
            if let Err(e) = self.new_peer(&from) {
                self.emit(Event::Error(p2_i18n::tv("media2.connect", &[("error", &e.to_string())])));
                return;
            }
        }
        if let Some(level) = data.get("quality").and_then(|q| q.as_str()) {
            if std::env::var_os("P2_TRACE_RTC").is_some() { eprintln!("[rtc] {from} yayını {level} istiyor"); }
            self.viewer_levels.insert(from.clone(), level.to_string());
            self.apply_encoding(&from);
            let _ = self.events.unbounded_send(Event::ViewerQuality { user: from, level: level.to_string() });
            return;
        }
        let kbps = self.config.voice_kbps;
        let peer = self.peers.get_mut(&from).expect("eş");
        if let Some(desc) = data.get("description").filter(|d| !d.is_null()) {
            let kind = desc["type"].as_str().unwrap_or_default();
            let sdp = desc["sdp"].as_str().unwrap_or_default();
            let Ok(sdp_type) = kind.parse::<SdpType>() else { return };
            // Kusursuz müzakere (CallProvider.tsx): kaba taraf çakışan teklifi yok sayar; kibar taraf
            // kendi teklifini geri alıp karşınınkini uygular.
            let collision = sdp_type == SdpType::Offer && (peer.making_offer || peer.pc.signaling_state() != SignalingState::Stable);
            if !peer.polite && collision {
                return;
            }
            if collision && peer.pc.signaling_state() == SignalingState::HaveLocalOffer {
                if let Ok(rollback) = SessionDescription::parse("", SdpType::Rollback) {
                    let _ = peer.pc.set_local_description(rollback).await;
                }
                peer.making_offer = false;
            }
            if std::env::var_os("P2_TRACE_RTC").is_some() {
                let _ = std::fs::write(format!("C:/p2-pg/rtc-{kind}.sdp"), sdp);
            }
            let Ok(remote) = SessionDescription::parse(sdp, sdp_type) else { return };
            if peer.pc.set_remote_description(remote).await.is_err() {
                return;
            }
            if sdp_type == SdpType::Offer {
                if let Ok(answer) = peer.pc.create_answer(AnswerOptions::default()).await {
                    let text = answer.to_string();
                    if peer.pc.set_local_description(answer).await.is_ok() {
                        let data = json!({ "description": { "type": "answer", "sdp": with_opus_tuning(&text, kbps) } });
                        let _ = self.events.unbounded_send(Event::Signal { call_id: call_id.clone(), to: from.clone(), data });
                    }
                }
            }
            for cand in peer.pending.drain(..) {
                let _ = peer.pc.add_ice_candidate(cand).await;
            }
            // Pazarlık bitti: kodlayıcı sınırları artık güvenle uygulanır (iz eklendikten hemen
            // sonra set_parameters imza iş parçacığında saniyelerce bekliyordu).
            if sdp_type == SdpType::Answer || sdp_type == SdpType::Offer {
                self.apply_encoding(&from);
            }
        } else if let Some(c) = data.get("candidate").filter(|c| !c.is_null()) {
            let sdp = c["candidate"].as_str().unwrap_or_default();
            if sdp.is_empty() {
                return;
            }
            if let Ok(cand) = IceCandidate::parse(c["sdpMid"].as_str().unwrap_or("0"), c["sdpMLineIndex"].as_i64().unwrap_or(0) as i32, sdp) {
                if peer.pc.current_remote_description().is_some() {
                    let _ = peer.pc.add_ice_candidate(cand).await;
                } else {
                    peer.pending.push(cand);
                }
            }
        }
        // `watch`, `quality`, `draw`, `cursor`: yayın ve çizim (Dilim 4b).
    }

    async fn internal(&mut self, ev: Internal) {
        let Some(call_id) = self.call_id.clone() else { return };
        match ev {
            Internal::Ice(to, cand) => {
                let data = json!({ "candidate": { "candidate": cand.candidate(), "sdpMid": cand.sdp_mid(), "sdpMLineIndex": cand.sdp_mline_index() } });
                self.emit(Event::Signal { call_id, to, data });
            }
            Internal::Negotiate(to) => {
                // İlk bağlantıda yalnız kaba taraf teklif eder (çakışma azalsın); sonradan (yayın
                // açma/kapama) iki taraf da teklif edebilir — çakışmayı kibar taraf geri alır.
                let kbps = self.config.voice_kbps;
                let Some(peer) = self.peers.get_mut(&to) else { return };
                let first = peer.pc.current_remote_description().is_none();
                if (first && peer.polite) || peer.making_offer || peer.pc.signaling_state() != SignalingState::Stable {
                    return;
                }
                peer.making_offer = true;
                // Not: OfferOptions::default() offer_to_receive_* = Some(false) verir; Unified Plan'da bu
                // ses alıcı-vericisini "sendonly" yapar ve karşı taraf hiç ses göndermez.
                if let Ok(offer) = peer.pc.create_offer(OfferOptions { ice_restart: false, offer_to_receive_audio: None, offer_to_receive_video: None }).await {
                    let sdp = offer.to_string();
                    if std::env::var_os("P2_TRACE_RTC").is_some() { let _ = std::fs::write("C:/p2-pg/rtc-local-offer.sdp", &sdp); }
                    if peer.pc.set_local_description(offer).await.is_ok() {
                        let _ = self.events.unbounded_send(Event::Signal { call_id, to, data: json!({ "description": { "type": "offer", "sdp": with_opus_tuning(&sdp, kbps) } }) });
                    }
                }
                peer.making_offer = false;
            }
            Internal::Connection(user, state) => {
                let state = match state {
                    PeerConnectionState::New => "new",
                    PeerConnectionState::Connecting => "connecting",
                    PeerConnectionState::Connected => "connected",
                    PeerConnectionState::Disconnected => "disconnected",
                    PeerConnectionState::Failed => "failed",
                    PeerConnectionState::Closed => "closed",
                };
                if state == "connected" {
                    self.apply_encoding(&user);
                }
                self.emit(Event::PeerState { user, state: state.into() });
            }
            Internal::Track(user, MediaStreamTrack::Audio(track)) => {
                if std::env::var_os("P2_TRACE_RTC").is_some() { eprintln!("[rtc] ses izi geldi {user}"); }
                let jitter: Jitter = Arc::default();
                self.remote.insert(user, jitter.clone());
                tokio::spawn(async move {
                    let mut stream = NativeAudioStream::new(track, RATE as i32, 1);
                    while let Some(frame) = stream.next().await {
                        let mut q = jitter.lock().unwrap();
                        q.extend(frame.data.iter().copied());
                        // 200 ms'den fazla birikme gecikme demek: en eskisi atılır.
                        let excess = q.len().saturating_sub(RATE as usize / 5);
                        q.drain(..excess);
                    }
                });
            }
            Internal::Track(user, MediaStreamTrack::Video(track)) => {
                let alive = spawn_receiver(user.clone(), track, self.frames.clone(), self.events.clone());
                if let Some(old) = self.receivers.insert(user, alive) {
                    old.store(false, Ordering::Relaxed);
                }
            }
        }
    }

    fn leave(&mut self) {
        for user in self.peers.keys().cloned().collect::<Vec<_>>() {
            self.close_peer(&user);
        }
        self.call_id = None;
        self.early.clear();
    }
}

fn open_audio(config: &MediaConfig, state_events: &fmpsc::UnboundedSender<Event>) -> (AudioIo, AudioProcessingModule) {
    let opened = Instant::now();
    // `no_devices`: mikrofon da hoparlör de açılmaz (testler; sentetik ses, ölçüm sürer).
    let (audio, errors) = AudioIo::open(config.input.as_deref(), config.output.as_deref(), !config.test_tone && !config.no_devices, !config.silent_output && !config.no_devices);
    for e in errors {
        let _ = state_events.unbounded_send(Event::Error(e));
    }
    let _ = state_events.unbounded_send(Event::Devices { input: audio.input_name.clone(), output: audio.output_name.clone() });
    let apm = AudioProcessingModule::new(config.echo_cancellation, config.auto_gain, true, config.noise_suppression);
    let _ = state_events.unbounded_send(Event::Trace(format!("aygıtlar açıldı {} ms (giriş={:?}, çıkış={:?})", opened.elapsed().as_millis(), audio.input_name, audio.output_name)));
    (audio, apm)
}

/// Ses döngüsünü 100 ms'den uzun bekleten adımı tanı kaydına yazar.
fn slow(state: &State, what: &str, started: Instant) {
    let ms = started.elapsed().as_millis();
    if ms > 100 {
        state.emit(Event::Trace(format!("yavaş {what}: {ms} ms")));
    }
}

/// Bir tikte üretilecek en fazla kare (200 ms).
const MAX_CATCH_UP: u64 = 20;

async fn run(config: MediaConfig, mut commands: UnboundedReceiver<Command>, events: fmpsc::UnboundedSender<Event>, frames: Frames) {
    let (itx, mut irx) = unbounded_channel::<Internal>();
    let (mut audio, mut apm) = open_audio(&config, &events);
    let mut state = State {
        config,
        events,
        internal: itx,
        rtc: None,
        call_id: None,
        me: String::new(),
        peers: HashMap::new(),
        remote: HashMap::new(),
        volumes: HashMap::new(),
        early: Vec::new(),
        muted: false,
        deafened: false,
        talking: true,
        loopback: false,
        frames,
        receivers: HashMap::new(),
        sharer: None,
        screen: None,
        camera: None,
        share: None,
        viewer_levels: HashMap::new(),
        sending: None,
        system_audio: None,
    };
    let mut screen_tick: Option<tokio::time::Interval> = None;
    let mut synth = Synth::default();
    let mut tick = tokio::time::interval(Duration::from_millis(10));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut ticks: u64 = 0;
    let mut mic_peak = -100f32;
    let mut peer_peak: HashMap<String, f32> = HashMap::new();
    let mut last_mic = [0i16; FRAME];
    let mut tone_phase = 0u64;
    // Ses saati: kare sayısı duvar saatinden hesaplanır. tokio zamanlayıcısı Windows'ta ~15,6 ms
    // çözünürlüklüdür ve sinyalleşme (sync/signal) sırasında döngü bekler; "tik başına bir kare"
    // saniyede 100 yerine ~64 kare üretip hoparlörü aç bırakıyordu (takılan, titreyen ses).
    let clock_start = Instant::now();
    let mut frames_done: u64 = 0;

    loop {
        tokio::select! {
            command = commands.recv() => match command {
                None | Some(Command::Stop) => break,
                Some(Command::Sync { call_id, me, participants }) => { let t = Instant::now(); state.sync(call_id, me, participants).await; slow(&state, "sync", t); }
                Some(Command::Signal { call_id, from, data }) => { let t = Instant::now(); state.signal(call_id, from, data).await; slow(&state, "signal", t); }
                Some(Command::Leave) => state.leave(),
                Some(Command::SetMuted(m)) => state.muted = m,
                Some(Command::SetDeafened(d)) => state.deafened = d,
                Some(Command::SetTalking(t)) => state.talking = t,
                Some(Command::SetVolume { user, volume }) => {
                    if std::env::var_os("P2_TRACE_RTC").is_some() { eprintln!("[rtc] {user} sesi {volume:.2} (iz {})", if state.remote.contains_key(&user) { "var" } else { "henüz yok" }); }
                    state.volumes.insert(user, volume.clamp(0.0, 2.0));
                }
                Some(Command::SetPeerSend { user, send }) => {
                    let track = state.rtc.as_ref().map(|r| r.track.clone());
                    if let (Some(peer), Some(track)) = (state.peers.get(&user), track) {
                        let _ = peer.audio_sender.as_ref().map(|s| s.set_track(send.then(|| MediaStreamTrack::Audio(track))));
                    }
                }
                Some(Command::Ring(r)) => synth.ring(r),
                Some(Command::Cue(c)) => synth.cue(c),
                Some(Command::Loopback(on)) => state.loopback = on,
                Some(Command::StartScreen { source, fps, quality, audio: with_audio }) => {
                    if state.sharer.is_some() {
                        continue;
                    }
                    let video = NativeVideoSource::new(VideoResolution { width: 1920, height: 1080 }, true);
                    let preview = Preview::new(LOCAL_SCREEN, state.frames.clone(), state.events.clone(), 10);
                    match Sharer::start(source, video.clone(), Some(preview)) {
                        Ok(mut sharer) => {
                            // İlk kare hemen: kaynağın siyah "keepalive" döngüsü ilk gerçek karede durur.
                            sharer.tick();
                            let track = state.rtc().factory.create_video_track("p2-ekran", video.clone());
                            state.screen = Some((video, track));
                            state.sharer = Some(sharer);
                            state.share = Some((fps.clamp(1, 60), quality));
                            // İz eklenince her eşte "negotiation needed" tetiklenir (yeniden pazarlık).
                            state.swap_video();
                            let mut t = tokio::time::interval(Duration::from_millis(1000 / fps.clamp(1, 60) as u64));
                            t.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                            screen_tick = Some(t);
                            state.emit(Event::Screen { on: true });
                            // Bilgisayar sesi (CallProvider.tsx startScreen): açılamazsa yayın sessiz sürer.
                            if with_audio {
                                match SystemAudio::start(state.config.no_devices) {
                                    Ok((sa, mode)) => {
                                        state.system_audio = Some(sa);
                                        state.emit(Event::ShareAudio { on: true, mode, error: None });
                                    }
                                    Err(e) => state.emit(Event::ShareAudio { on: false, mode: String::new(), error: Some(e) }),
                                }
                            }
                        }
                        Err(e) => state.emit(Event::Error(e)),
                    }
                }
                Some(Command::UpdateShare { fps, quality, audio: with_audio }) => {
                    let Some(share) = state.share.as_mut() else { continue };
                    if let Some(fps) = fps {
                        share.0 = fps.clamp(1, 60);
                        let mut t = tokio::time::interval(Duration::from_millis(1000 / share.0 as u64));
                        t.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                        screen_tick = Some(t);
                    }
                    if let Some(quality) = quality {
                        share.1 = quality;
                    }
                    for id in state.peers.keys().cloned().collect::<Vec<_>>() {
                        state.apply_encoding(&id);
                    }
                    match with_audio {
                        Some(true) if state.system_audio.is_none() => match SystemAudio::start(state.config.no_devices) {
                            Ok((sa, mode)) => {
                                state.system_audio = Some(sa);
                                state.emit(Event::ShareAudio { on: true, mode, error: None });
                            }
                            Err(e) => state.emit(Event::ShareAudio { on: false, mode: String::new(), error: Some(e) }),
                        },
                        Some(false) if state.system_audio.take().is_some() => state.emit(Event::ShareAudio { on: false, mode: String::new(), error: None }),
                        _ => {}
                    }
                }
                Some(Command::StopScreen) => {
                    // Önizleme, yakalayıcı kapandıktan SONRA silinir (geç kare kalmasın).
                    let preview = state.sharer.take().and_then(|s| s.preview().cloned());
                    if let Some(p) = preview {
                        p.clear();
                    }
                    state.screen = None;
                    state.share = None;
                    if state.system_audio.take().is_some() {
                        state.emit(Event::ShareAudio { on: false, mode: String::new(), error: None });
                    }
                    // Kamera açıksa eşlere yeniden kamera gider.
                    state.swap_video();
                    screen_tick = None;
                    state.emit(Event::Screen { on: false });
                }
                Some(Command::StartCamera(source)) => {
                    if state.camera.is_some() {
                        continue;
                    }
                    let video = NativeVideoSource::new(VideoResolution { width: 640, height: 480 }, false);
                    let preview = Preview::new(LOCAL_CAMERA, state.frames.clone(), state.events.clone(), 15);
                    match Camera::start(source, video.clone(), Some(preview)) {
                        Ok(camera) => {
                            let track = state.rtc().factory.create_video_track("p2-kamera", video);
                            state.camera = Some((camera, track));
                            state.swap_video();
                            state.emit(Event::Camera { on: true });
                        }
                        Err(e) => state.emit(Event::Error(e)),
                    }
                }
                Some(Command::StopCamera) => {
                    if state.camera.take().is_some() {
                        // Kamera iş parçacığı bitti (Drop bekler): son önizleme karesi silinir.
                        Preview::new(LOCAL_CAMERA, state.frames.clone(), state.events.clone(), 1).clear();
                        state.swap_video();
                        state.emit(Event::Camera { on: false });
                    }
                }
                Some(Command::RequestQuality { user, level }) => {
                    if let Some(call_id) = state.call_id.clone() {
                        state.emit(Event::Signal { call_id, to: user, data: json!({ "quality": level }) });
                    }
                }
                Some(Command::Reconfigure(config)) => {
                    let reopen = config.input != state.config.input || config.output != state.config.output
                        || config.test_tone != state.config.test_tone || config.silent_output != state.config.silent_output
                        || config.echo_cancellation != state.config.echo_cancellation || config.noise_suppression != state.config.noise_suppression
                        || config.auto_gain != state.config.auto_gain || config.no_devices != state.config.no_devices;
                    state.config = config;
                    if reopen {
                        drop(audio);
                        (audio, apm) = open_audio(&state.config, &state.events);
                    }
                }
            },
            Some(ev) = irx.recv() => { let t = Instant::now(); state.internal(ev).await; slow(&state, "internal", t); }
            _ = async { match screen_tick.as_mut() { Some(t) => { t.tick().await; } None => std::future::pending::<()>().await } } => {
                if let Some(sharer) = state.sharer.as_mut() {
                    sharer.tick();
                }
            }
            _ = tick.tick() => {
                let target = (clock_start.elapsed().as_micros() / 10_000) as u64;
                let mut due = target.saturating_sub(frames_done);
                // Uzun bir duraklamadan sonra en fazla 200 ms telafi edilir; fazlası atlanır.
                if due > MAX_CATCH_UP { frames_done += due - MAX_CATCH_UP; due = MAX_CATCH_UP; }
                frames_done += due;
                for e in audio.drain_errors() { state.emit(Event::Trace(format!("aygıt hatası: {e}"))); }
                for _ in 0..due {
                ticks += 1;
                // 1) Uzak sesler + yerel işaret sesleri → hoparlör
                let mut mix = [0f32; FRAME];
                let mut buf = [0i16; FRAME];
                for (user, jitter) in &state.remote {
                    {
                        let mut q = jitter.lock().unwrap();
                        for slot in buf.iter_mut() {
                            *slot = q.pop_front().unwrap_or(0);
                        }
                    }
                    let db = level_db(&buf);
                    let peak = peer_peak.entry(user.clone()).or_insert(-100.0);
                    *peak = peak.max(db);
                    mix_peer(&mut mix, &buf, peer_volume(&state.volumes, user));
                }
                let out_volume = state.config.output_volume;
                if state.deafened {
                    mix = [0.0; FRAME];
                } else {
                    mix.iter_mut().for_each(|m| *m *= out_volume);
                }
                if state.loopback {
                    for (m, s) in mix.iter_mut().zip(last_mic.iter()) {
                        *m += *s as f32 / i16::MAX as f32;
                    }
                }
                synth.render(&mut mix, state.config.cue_volume);
                // Kişi sesi %200'e kadar yükseltilebilir: sert kırpma yerine yumuşak sınır.
                mix.iter_mut().for_each(|m| *m = soft_limit(*m));
                let mut render: Vec<i16> = mix.iter().map(|v| to_i16(*v)).collect();
                if state.config.echo_cancellation {
                    let _ = apm.process_reverse_stream(&mut render, RATE as i32, 1);
                }
                audio.play(&mix);

                // 2) Mikrofon → APM → kapı → yerel iz
                let mut mic = [0f32; FRAME];
                if state.config.test_tone {
                    // Test sesi: 0,6 sn'de bir 200 ms'lik 523 Hz (do), −18 dBFS.
                    for slot in mic.iter_mut() {
                        let t = tone_phase as f64 / RATE as f64;
                        tone_phase += 1;
                        *slot = if t % 0.6 < 0.2 { ((t * 523.25 * std::f64::consts::TAU).sin() * 0.18) as f32 } else { 0.0 };
                    }
                } else {
                    audio.take_mic(&mut mic);
                }
                let gain = state.config.input_volume;
                let mut pcm: Vec<i16> = mic.iter().map(|v| to_i16(*v * gain)).collect();
                // Test sesi APM'den geçmez (gürültü bastırıcı sentetik tonu siler).
                if !state.config.test_tone {
                    let _ = apm.process_stream(&mut pcm, RATE as i32, 1);
                }
                last_mic.copy_from_slice(&pcm);
                mic_peak = mic_peak.max(level_db(&pcm));
                if state.muted || !state.talking {
                    pcm.iter_mut().for_each(|s| *s = 0);
                }
                // Bilgisayar sesi kapıdan SONRA eklenir: mikrofon kapalıyken de yayının sesi gider
                // (Electron: mikrofon izi devre dışı, karışımdaki sistem sesi akmaya devam eder).
                if let Some(sa) = state.system_audio.as_mut() {
                    sa.mix(&mut pcm);
                }
                if let Some(rtc) = &state.rtc {
                    let mut frame = AudioFrame::new(RATE, 1, FRAME as u32);
                    frame.data = Cow::Owned(pcm);
                    let _ = rtc.source.capture_frame(&frame).await;
                }

                // 3) Seviyeler ~10/sn
                if ticks % 10 == 0 {
                    let peers = peer_peak.drain().collect();
                    state.emit(Event::Levels { mic: mic_peak, peers });
                    mic_peak = -100.0;
                }
                }
            }
        }
    }
    state.leave();
}

#[cfg(test)]
mod tests {
    use super::with_opus_tuning;

    #[test]
    fn opus_fmtp_is_tuned_like_callprovider() {
        let sdp = "v=0\r\na=rtpmap:111 opus/48000/2\r\na=fmtp:111 minptime=10;useinbandfec=0\r\na=rtpmap:96 VP8/90000";
        let out = with_opus_tuning(sdp, 64);
        assert!(out.contains("a=fmtp:111 minptime=10;useinbandfec=1;maxaveragebitrate=64000;stereo=0;sprop-stereo=0;usedtx=0;maxplaybackrate=48000"), "{out}");
        assert!(out.contains("a=rtpmap:96 VP8/90000"));
    }
}

/// screenShare.ts `QUALITIES`: (yükseklik, bit hızı).
pub fn quality_preset(level: &str) -> (u32, u64) {
    match level {
        "2160p" => (2160, 16_000_000),
        "1440p" => (1440, 9_000_000),
        "720p" => (720, 2_500_000),
        "480p" => (480, 1_000_000),
        "360p" => (360, 500_000),
        _ => (1080, 4_500_000),
    }
}

/// Kare hızı arttıkça aynı çözünürlük daha fazla bant genişliği ister (screenShare.ts `bitrateFor`).
pub fn bitrate_for(level: &str, fps: u32) -> u64 {
    let base = quality_preset(level).1 as f64;
    (base * if fps >= 60 { 1.5 } else if fps >= 30 { 1.0 } else { 0.7 }).round() as u64
}

#[cfg(test)]
mod quality_tests {
    use super::*;

    #[test]
    fn presets_match_js() {
        assert_eq!(bitrate_for("1080p", 30), 4_500_000);
        assert_eq!(bitrate_for("720p", 60), 3_750_000);
        assert_eq!(bitrate_for("480p", 15), 700_000);
        assert_eq!(quality_preset("bilinmeyen").0, 1080);
    }
}

/// Yayının bilgisayar sesi: gerçek WASAPI loopback ya da (`no_devices` testlerinde) aygıtsız
/// sentetik halka — 880 Hz, −20 dBFS. İkisi de aynı halka + `mix_into` yolundan karışır.
enum SystemAudio {
    Real(crate::loopback::Loopback),
    Synthetic { ring: crate::audio::Ring, phase: u64 },
}

impl SystemAudio {
    fn start(no_devices: bool) -> Result<(Self, String), String> {
        if no_devices {
            return Ok((Self::Synthetic { ring: Arc::default(), phase: 0 }, "sentetik".into()));
        }
        let lb = crate::loopback::Loopback::start()?;
        let mode = lb.mode.clone();
        Ok((Self::Real(lb), mode))
    }

    /// 10 ms: (gerekirse sentetik örnek üret) ve kareye ekle.
    fn mix(&mut self, pcm: &mut [i16]) -> usize {
        match self {
            Self::Real(lb) => lb.mix_into(pcm),
            Self::Synthetic { ring, phase } => {
                let stereo: Vec<f32> = (0..pcm.len())
                    .flat_map(|_| {
                        let t = *phase as f64 / RATE as f64;
                        *phase += 1;
                        let v = ((t * 880.0 * std::f64::consts::TAU).sin() * 0.1) as f32;
                        [v, v]
                    })
                    .collect();
                crate::loopback::push_stereo(ring, &stereo);
                crate::loopback::mix_into(pcm, ring)
            }
        }
    }
}

/// Kişi başına dinleme sesi (kayıt yoksa 1 = olduğu gibi; 0–2 aralığında).
fn peer_volume(volumes: &HashMap<String, f32>, user: &str) -> f32 {
    volumes.get(user).copied().unwrap_or(1.0).clamp(0.0, 2.0)
}

/// Bir eşin 10 ms'lik karesini kişi sesiyle çarpıp karışıma ekler.
fn mix_peer(mix: &mut [f32], frame: &[i16], volume: f32) {
    for (m, s) in mix.iter_mut().zip(frame.iter()) {
        *m += *s as f32 / i16::MAX as f32 * volume;
    }
}

/// Yumuşak sınırlayıcı: |x| ≤ 0,8 olduğu gibi geçer, üstü 1'e yumuşakça yaklaşır (yükseltilmiş ses
/// kare dalgaya dönüp cızırdamasın).
fn soft_limit(x: f32) -> f32 {
    const KNEE: f32 = 0.8;
    let a = x.abs();
    if a <= KNEE {
        x
    } else {
        let over = (a - KNEE) / (1.0 - KNEE);
        x.signum() * (KNEE + (1.0 - KNEE) * over.tanh())
    }
}

#[cfg(test)]
mod mix_tests {
    use super::*;

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
    }

    /// Kişi sesi karışıma uygulanır: %0 sessiz, %50 yarı, %200 iki kat (anahtar kullanıcı kimliği).
    #[test]
    fn peer_volume_scales_mix() {
        let frame: Vec<i16> = (0..FRAME).map(|i| ((i as f32 * 0.07).sin() * 8000.0) as i16).collect();
        let mut volumes = HashMap::new();
        let gain = |volumes: &HashMap<String, f32>| {
            let mut mix = [0f32; FRAME];
            mix_peer(&mut mix, &frame, peer_volume(volumes, "kullanici-1"));
            rms(&mix)
        };
        let unity = gain(&volumes);
        assert!(unity > 0.1, "{unity}");
        volumes.insert("kullanici-1".to_string(), 0.0);
        assert_eq!(gain(&volumes), 0.0);
        volumes.insert("kullanici-1".to_string(), 0.5);
        assert!((gain(&volumes) / unity - 0.5).abs() < 0.01);
        volumes.insert("kullanici-1".to_string(), 2.0);
        assert!((gain(&volumes) / unity - 2.0).abs() < 0.01);
        // Başka kişinin ayarı bu kişiyi etkilemez; aralık dışı değer sınırlanır.
        volumes.insert("kullanici-1".to_string(), 5.0);
        volumes.insert("kullanici-2".to_string(), 0.0);
        assert!((gain(&volumes) / unity - 2.0).abs() < 0.01);
    }

    /// Sınırlayıcı küçük sinyale dokunmaz, büyüğü 1'in altında tutar ve sırayı korur.
    #[test]
    fn soft_limit_keeps_boost_bounded() {
        assert_eq!(soft_limit(0.5), 0.5);
        assert_eq!(soft_limit(-0.8), -0.8);
        assert!(soft_limit(1.6) < 1.0 && soft_limit(1.6) > soft_limit(1.0));
        assert!(soft_limit(-3.0) >= -1.0 && soft_limit(-3.0) < -0.8);
    }
}

#[cfg(test)]
mod system_audio_tests {
    use super::*;

    /// Sentetik halka enjeksiyonu: susturulmuş (sıfır) mikrofon karesine bilgisayar sesi
    /// eklenir — CallProvider.tsx'te mikrofon izi kapalıyken de karışımdaki sistem sesi gider.
    #[test]
    fn synthetic_ring_reaches_muted_frame() {
        let (mut sa, mode) = SystemAudio::start(true).unwrap();
        assert_eq!(mode, "sentetik");
        let mut level = -100.0;
        for _ in 0..5 {
            let mut pcm = [0i16; FRAME];
            assert_eq!(sa.mix(&mut pcm), FRAME);
            level = level_db(&pcm);
        }
        // 0,1 genlikli sinüs: RMS ≈ −23 dBFS.
        assert!((level - -23.0).abs() < 1.0, "{level}");
    }
}

#[cfg(test)]
mod clock_tests {
    use super::*;

    /// Ses saati duvar saatine bağlı: aygıt açılmadan (no_devices) 2 sn'de ~200 kare → ~20 seviye
    /// olayı (her 10 karede bir). Eski "tik başına bir kare" Windows'ta ~13 olay veriyordu.
    #[test]
    fn engine_produces_100_frames_per_second() {
        let config = MediaConfig { no_devices: true, ..Default::default() };
        let (engine, mut events) = Engine::start(config);
        std::thread::sleep(Duration::from_millis(300));
        while let Ok(Some(_)) = events.try_next() {}
        let start = Instant::now();
        let mut levels = 0;
        while start.elapsed() < Duration::from_secs(2) {
            while let Ok(Some(ev)) = events.try_next() {
                if matches!(ev, Event::Levels { .. }) {
                    levels += 1;
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        drop(engine);
        assert!((18..=22).contains(&levels), "2 sn'de {levels} seviye olayı (beklenen ~20)");
    }
}
