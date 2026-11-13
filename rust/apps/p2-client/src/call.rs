//! Arama denetçisi — CallProvider.tsx'in durum yarısı: aramayı başlat/kabul et/reddet/ayrıl,
//! sustur/sağırlaştır/bas-konuş, gelen arama ve zil, sesli oda varlığı, konuşan göstergesi.
//! Medya `p2-media` motorunda; sinyaller socket.io üzerinden (`RtSender`) taşınır.
//!
//! Motor yalnız gerektiğinde açılır (arama, zil ya da mikrofon testi): boşta WebRTC ve ses
//! aygıtları kapalı kalır.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use futures::StreamExt as _;
use gpui_kit::{Context, Window};
use p2_client_core::realtime::{RtEvent, RtSender};
use p2_client_core::store::{self, MediaSettings};
use p2_client_core::{CallParticipant, CallState};
use std::sync::Arc;

use gpui_kit::RenderImage;
use p2_media::{Command, Cue, Engine, Event, IceConfig, MediaConfig, Ring, ScreenSource};
use serde_json::json;

/// Konuşuyor eşiği (dBFS, ~100 ms tepe RMS).
const SPEAKING_DB: f32 = -48.0;

pub struct Incoming {
    pub call: CallState,
    pub from_id: String,
    pub from_name: String,
    pub silent: bool,
}

pub struct CallController {
    me: String,
    org: String,
    rt: RtSender,
    pub call: Option<CallState>,
    pub incoming: Option<Incoming>,
    engine: Option<Engine>,
    /// Motor kapatılacaksa bu sürümle (geç gelen kapatma yeni motoru düşürmesin).
    engine_epoch: u64,
    pub muted: bool,
    pub deafened: bool,
    /// Bas-konuş tuşu basılı mı.
    pub talking: bool,
    pub speaking: HashSet<String>,
    pub mic_level: f32,
    pub peer_states: HashMap<String, String>,
    pub connected_at: Option<Instant>,
    pub voice_rooms: HashMap<String, Vec<CallParticipant>>,
    pub error: Option<(String, usize)>,
    pub mic_test: bool,
    pub devices: (Option<String>, Option<String>),
    pub settings: MediaSettings,
    trace_peak: (f32, HashMap<String, f32>),
    trace_ticks: u32,
    /// Kendi ekran paylaşımım açık mı.
    pub screen_on: bool,
    pub camera_on: bool,
    /// Kişi başına dinleme sesi (0–2).
    pub volumes: HashMap<String, f32>,
    /// İzlediğim yayınlar için istediğim kalite.
    pub viewer_quality: HashMap<String, String>,
    /// Gelen görüntülerin son karesi (kullanıcı → GPUI görüntüsü, genişlik, yükseklik).
    pub videos: HashMap<String, (Arc<RenderImage>, u32, u32)>,
    video_counts: HashMap<String, u32>,
    video_trace_at: Option<Instant>,
    /// Yayın çizimleri: hedef (yayını yapan) → çizgiler ve imleçler.
    pub boards: HashMap<String, crate::annotate::Board>,
    prune_running: bool,
    /// Süren paylaşımın kaynağı (kaplama yalnız ekran paylaşımında açılır).
    share_source: Option<ScreenSource>,
    /// Bilgisayar sesi paylaşılıyorsa yakalama kipi ("işlem", "uç nokta", "sentetik").
    pub share_audio: Option<String>,
    /// Masaüstü çizim kaplaması (kendi yayınımın üstünde).
    overlay: Option<gpui_kit::AnyWindowHandle>,
    /// Aramaya katıldığım an (balondaki "Çalıyor… 00:12").
    pub started_at: Option<Instant>,
    /// Ana pencere (balondan "ayrıl": motor görevleri ana pencereye bağlı).
    main_window: Option<gpui_kit::AnyWindowHandle>,
    /// Arama balonu ve "bu seferlik gizle" denilen arama.
    widget: Option<gpui_kit::WindowHandle<crate::call_widget::CallWidget>>,
    widget_skipped: Option<String>,
    widget_watch: bool,
    widget_opening: bool,
    /// I ajanı (arama arayüzü eşdeğerliği) — CallProvider.tsx durumları:
    /// kişi başına ses seviyesi 0..1 (`levels`; konuşma halkası, kart parlaması).
    pub levels: HashMap<String, f32>,
    /// İzlediğim yayınlar (`watching`; yayıncıya `call:signal {watch}` gider).
    pub watching: Vec<String>,
    /// "Kendim için sustur" (`locallyMuted`).
    pub locally_muted: HashSet<String>,
    /// "Sesimi bu kişiye gönderme" (`deafenedFor`).
    pub deafened_for: HashSet<String>,
    /// Gelen aramada "Kabul et"e basıldı (`joining`: düğme "Bağlanıyor…").
    pub joining: bool,
}

/// Mikrofon kapısı (CallProvider `applyMicrophone`, susturma ayrı): "open" her zaman açık,
/// "ptt" yalnız tuş basılıyken, "ptm" (bas-sustur) tuş basılıyken kapalı.
pub fn talking_for(mode: &str, holding: bool) -> bool {
    match mode {
        "ptt" => holding,
        "ptm" => !holding,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::talking_for;

    #[test]
    fn voice_modes_match_electron() {
        // open: tuş etkisiz
        assert!(talking_for("open", false) && talking_for("open", true));
        // ptt: boşta kapalı, basılıyken açık
        assert!(!talking_for("ptt", false) && talking_for("ptt", true));
        // ptm: boşta açık, basılıyken kapalı
        assert!(talking_for("ptm", false) && !talking_for("ptm", true));
    }
}

pub fn media_config(s: &MediaSettings) -> MediaConfig {
    let mut ice = vec![IceConfig { urls: vec![s.stun_url.clone()], username: String::new(), password: String::new() }];
    if !s.turn_url.trim().is_empty() {
        ice.push(IceConfig { urls: vec![s.turn_url.trim().to_string()], username: s.turn_username.clone(), password: s.turn_password.clone() });
    }
    MediaConfig {
        input: s.input_device.clone(),
        output: s.output_device.clone(),
        echo_cancellation: s.echo_cancellation,
        noise_suppression: s.noise_suppression,
        auto_gain: s.auto_gain,
        voice_kbps: s.voice_bitrate,
        ice,
        input_volume: s.input_volume as f32 / 100.0,
        output_volume: s.output_volume as f32 / 100.0,
        cue_volume: s.ringtone_volume as f32 / 100.0,
        // Testler (otomatik pilot): gerçek mikrofon/hoparlör kullanılmaz.
        test_tone: std::env::var_os("P2_TEST_TONE").is_some(),
        silent_output: std::env::var_os("P2_SILENT_AUDIO").is_some(),
        force_relay: s.force_relay,
        // Testler: ses aygıt katmanı hiç açılmaz (P2_NO_AUDIO_DEVICE).
        no_devices: std::env::var_os("P2_NO_AUDIO_DEVICE").is_some(),
    }
}

impl CallController {
    pub fn new(me: String, org: String, rt: RtSender) -> Self {
        Self {
            me,
            org,
            rt,
            call: None,
            incoming: None,
            engine: None,
            engine_epoch: 0,
            muted: false,
            deafened: false,
            talking: true,
            speaking: HashSet::new(),
            mic_level: -100.0,
            peer_states: HashMap::new(),
            connected_at: None,
            voice_rooms: HashMap::new(),
            error: None,
            mic_test: false,
            devices: (None, None),
            settings: store::load_settings().media,
            trace_peak: (-100.0, HashMap::new()),
            trace_ticks: 0,
            screen_on: false,
            camera_on: false,
            volumes: HashMap::new(),
            viewer_quality: HashMap::new(),
            videos: HashMap::new(),
            video_counts: HashMap::new(),
            video_trace_at: None,
            boards: HashMap::new(),
            prune_running: false,
            share_source: None,
            share_audio: None,
            overlay: None,
            started_at: None,
            main_window: None,
            widget: None,
            widget_skipped: None,
            widget_watch: false,
            widget_opening: false,
            levels: HashMap::new(),
            watching: Vec::new(),
            locally_muted: HashSet::new(),
            deafened_for: HashSet::new(),
            joining: false,
        }
    }

    /// Boştaki (tuş basılı değilken) mikrofon kapısı kapalı mı: yalnız "ptt" kipinde.
    /// ("ptm" bas-sustur: boştayken açık; `talking = !self.ptt()` iki kip için de doğru.)
    fn ptt(&self) -> bool {
        !talking_for(&self.settings.voice_mode, false)
    }

    /// Tuşlu kip mi ("ptt" bas-konuş ya da "ptm" bas-sustur).
    fn key_mode(&self) -> bool {
        matches!(self.settings.voice_mode.as_str(), "ptt" | "ptm")
    }

    fn set_error(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        let epoch = self.error.as_ref().map_or(0, |e| e.1 + 1);
        self.error = Some((message.into(), epoch));
        crate::trace(&format!("arama: hata {}", self.error.as_ref().unwrap().0));
        cx.notify();
    }

    fn send(&self, command: Command) {
        if let Some(engine) = &self.engine {
            engine.send(command);
        }
    }

    fn ensure_engine(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.engine_epoch += 1;
        self.main_window.get_or_insert(window.window_handle());
        if self.engine.is_some() {
            return;
        }
        let (engine, mut events) = Engine::start(media_config(&self.settings));
        engine.send(Command::SetMuted(self.muted));
        engine.send(Command::SetDeafened(self.deafened));
        self.talking = !self.ptt();
        engine.send(Command::SetTalking(self.talking));
        self.engine = Some(engine);
        cx.spawn_in(window, async move |this, cx| {
            while let Some(event) = events.next().await {
                if this.update(cx, |this, cx| this.on_engine(event, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    /// Arama, zil ve mikrofon testi bittiyse motoru kısa bir gecikmeyle kapatır (ayrılma
    /// işaret sesi çalabilsin).
    fn release_engine(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.call.is_some() || self.incoming.is_some() || self.mic_test {
            return;
        }
        let epoch = self.engine_epoch;
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(700)).await;
            let _ = this.update(cx, |this, cx| {
                if this.engine_epoch == epoch && this.call.is_none() && this.incoming.is_none() && !this.mic_test {
                    this.engine = None;
                    this.speaking.clear();
                    cx.notify();
                }
            });
        })
        .detach();
    }

    // ------------------------------------------------------------ eylemler

    pub fn start_direct(&mut self, user_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.error = None;
        self.ensure_engine(window, cx);
        crate::trace("arama: bireysel arama başlatıldı");
        self.rt.emit("call:start", json!({ "organizationId": self.org, "kind": "direct", "targetUserId": user_id }));
    }

    pub fn join_channel(&mut self, channel_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.call.as_ref().and_then(|c| c.channel_id.as_deref()) == Some(channel_id) {
            return;
        }
        self.error = None;
        self.ensure_engine(window, cx);
        crate::trace("arama: sesli kanala katılınıyor");
        self.rt.emit("call:start", json!({ "organizationId": self.org, "kind": "channel", "channelId": channel_id, "ring": false }));
    }

    pub fn join_group(&mut self, group_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.error = None;
        self.ensure_engine(window, cx);
        crate::trace("arama: grup araması başlatıldı");
        self.rt.emit("call:start", json!({ "organizationId": self.org, "kind": "group", "groupId": group_id, "ring": true }));
    }

    pub fn accept(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(incoming) = &self.incoming else { return };
        let id = incoming.call.id.clone();
        self.ensure_engine(window, cx);
        self.send(Command::Ring(Ring::None));
        crate::trace("arama: kabul edildi");
        self.joining = true;
        self.rt.emit("call:accept", json!({ "callId": id }));
        cx.notify();
    }

    pub fn decline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(incoming) = self.incoming.take() else { return };
        self.send(Command::Ring(Ring::None));
        crate::trace("arama: reddedildi");
        self.rt.emit("call:decline", json!({ "callId": incoming.call.id }));
        self.release_engine(window, cx);
        cx.notify();
    }

    pub fn leave(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(call) = self.call.take() else { return };
        self.rt.emit("call:leave", json!({ "callId": call.id }));
        crate::trace("arama: ayrıldın");
        self.teardown(window, cx);
    }

    fn teardown(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.screen_on {
            self.send(Command::StopScreen);
            self.screen_on = false;
        }
        if self.camera_on {
            self.send(Command::StopCamera);
            self.camera_on = false;
        }
        self.close_overlay(cx);
        self.share_audio = None;
        self.started_at = None;
        self.share_source = None;
        self.boards.clear();
        self.viewer_quality.clear();
        self.watching.clear();
        self.levels.clear();
        self.deafened_for.clear();
        self.joining = false;
        for (_, (image, _, _)) in self.videos.drain() {
            cx.drop_image(image, None);
        }
        self.call = None;
        self.connected_at = None;
        self.peer_states.clear();
        self.speaking.clear();
        self.send(Command::Leave);
        self.send(Command::Ring(Ring::None));
        self.send(Command::Cue(Cue::Leave));
        self.release_engine(window, cx);
        cx.notify();
    }

    pub fn toggle_mute(&mut self, cx: &mut Context<Self>) {
        self.muted = !self.muted;
        if !self.muted && self.deafened {
            // Sesi açmak sağırlığı da kaldırır (Discord/Electron davranışı).
            self.deafened = false;
            self.send(Command::SetDeafened(false));
        }
        self.send(Command::SetMuted(self.muted));
        if let Some(call) = &self.call {
            self.rt.emit("call:media", json!({ "callId": call.id, "muted": self.muted, "deafened": self.deafened }));
        }
        crate::trace(&format!("arama: mikrofon {}", if self.muted { "kapalı" } else { "açık" }));
        cx.notify();
    }

    pub fn toggle_deafen(&mut self, cx: &mut Context<Self>) {
        self.deafened = !self.deafened;
        if self.deafened {
            self.muted = true;
        }
        self.send(Command::SetDeafened(self.deafened));
        self.send(Command::SetMuted(self.muted));
        if let Some(call) = &self.call {
            self.rt.emit("call:media", json!({ "callId": call.id, "deafened": self.deafened, "muted": self.muted }));
        }
        crate::trace(&format!("arama: kulaklık {}", if self.deafened { "kapalı" } else { "açık" }));
        cx.notify();
    }

    /// Kip tuşu ("ptt": basılıyken konuş; "ptm": basılıyken sustur — CallProvider `applyMicrophone`).
    pub fn push_to_talk(&mut self, down: bool, cx: &mut Context<Self>) {
        if !self.key_mode() {
            return;
        }
        let talking = talking_for(&self.settings.voice_mode, down);
        if self.talking == talking {
            return;
        }
        self.talking = talking;
        self.send(Command::SetTalking(talking));
        cx.notify();
    }

    pub fn ptt_key(&self) -> Option<String> {
        self.key_mode().then(|| self.settings.voice_key.to_lowercase())
    }

    /// Ayarlar değişti: motor açıksa yeniden yapılandırılır.
    pub fn apply_settings(&mut self, settings: MediaSettings, cx: &mut Context<Self>) {
        self.settings = settings;
        self.talking = !self.ptt();
        self.send(Command::Reconfigure(media_config(&self.settings)));
        self.send(Command::SetTalking(self.talking));
        cx.notify();
    }

    /// Ayarlardaki mikrofon testi (seviye çubuğu; `hear` = kendini duy).
    pub fn set_mic_test(&mut self, on: bool, hear: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.mic_test = on;
        if on {
            self.ensure_engine(window, cx);
            self.send(Command::SetTalking(true));
            self.send(Command::Loopback(hear));
        } else {
            self.send(Command::Loopback(false));
            self.send(Command::SetTalking(!self.ptt()));
            self.release_engine(window, cx);
        }
        cx.notify();
    }

    pub fn play_cue(&mut self, cue: Cue, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure_engine(window, cx);
        self.send(Command::Cue(cue));
        self.release_engine(window, cx);
    }

    // ------------------------------------------------------------ olaylar

    fn ring_for(&self, call: &CallState) -> Ring {
        // Bireysel aramada karşı taraf katılana kadar "çalıyor" sesi.
        if call.kind == "direct" && call.participants.len() < 2 && call.caller.as_deref() == Some(self.me.as_str()) { Ring::Outgoing } else { Ring::None }
    }

    pub fn on_rt(&mut self, event: &RtEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            RtEvent::Connected => {
                if let Some(call) = &self.call {
                    self.rt.emit("call:sync", json!({ "callId": call.id }));
                }
            }
            RtEvent::CallIncoming { call, from_id, from_name, silent } => {
                if self.call.as_ref().is_some_and(|c| c.id == call.id) {
                    return;
                }
                crate::trace(&format!("arama: gelen arama {from_name}"));
                self.incoming = Some(Incoming { call: call.clone(), from_id: from_id.clone(), from_name: from_name.clone(), silent: *silent });
                if !silent {
                    self.ensure_engine(window, cx);
                    self.send(Command::Ring(Ring::Incoming));
                }
                // CallOverlay: gelen arama 45 sn yanıtlanmazsa kendiliğinden reddedilir.
                let id = call.id.clone();
                cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(Duration::from_secs(45)).await;
                    let _ = this.update_in(cx, |this, window, cx| {
                        if this.incoming.as_ref().is_some_and(|i| i.call.id == id) {
                            this.decline(window, cx);
                        }
                    });
                })
                .detach();
                cx.notify();
            }
            RtEvent::CallState { call } => {
                if call.has(&self.me) {
                    let before = self.call.take();
                    match &before {
                        Some(b) if b.id == call.id => {
                            for p in call.participants.iter().filter(|p| p.user_id != self.me) {
                                match b.participants.iter().find(|x| x.user_id == p.user_id) {
                                    None => self.send(Command::Cue(Cue::Join)),
                                    Some(prev) if p.screen && !prev.screen => self.send(Command::Cue(Cue::StreamStart)),
                                    Some(prev) if !p.screen && prev.screen => self.send(Command::Cue(Cue::StreamStop)),
                                    _ => {}
                                }
                            }
                            if b.participants.iter().any(|p| p.user_id != self.me && !call.has(&p.user_id)) {
                                self.send(Command::Cue(Cue::Leave));
                            }
                        }
                        _ => {
                            self.ensure_engine(window, cx);
                            self.send(Command::Cue(Cue::Join));
                            self.connected_at = None;
                            self.started_at = Some(Instant::now());
                            self.start_widget_watch(cx);
                            crate::trace(&format!("arama: katıldın ({}, {} kişi)", call.kind, call.participants.len()));
                            // Otomatik pilot: N sn sonra test deseniyle yayın aç, M sn sonra kapat.
                            if let Some(after) = std::env::var("P2_AUTOPILOT_SCREEN").ok().and_then(|s| s.parse::<u64>().ok()) {
                                let stop = std::env::var("P2_AUTOPILOT_SCREEN_STOP").ok().and_then(|s| s.parse::<u64>().ok());
                                cx.spawn_in(window, async move |this, cx| {
                                    cx.background_executor().timer(Duration::from_secs(after)).await;
                                    let _ = this.update(cx, |this, cx| this.start_screen(ScreenSource::TestPattern, 15, "720p", std::env::var_os("P2_AUTOPILOT_SCREEN_AUDIO").is_some(), cx));
                                    if let Some(stop) = stop {
                                        cx.background_executor().timer(Duration::from_secs(stop)).await;
                                        let _ = this.update(cx, |this, cx| this.stop_screen(cx));
                                    }
                                })
                                .detach();
                            }
                            self.autopilot_extras(window, cx);
                        }
                    }
                    if self.incoming.as_ref().is_some_and(|i| i.call.id == call.id) {
                        self.incoming = None;
                    }
                    self.joining = false;
                    if self.connected_at.is_none() && (call.kind == "channel" || call.participants.len() > 1) {
                        self.connected_at = Some(Instant::now());
                    }
                    // Sunucudaki kendi bayraklarım (moderatör ya da başka oturum değiştirmiş olabilir).
                    if let Some(mine) = call.participants.iter().find(|p| p.user_id == self.me) {
                        if mine.muted != self.muted && !mine.muted && before.is_none() {
                            // Yeni katılımda yerel tercih geçerli: sunucuya bildir.
                            self.rt.emit("call:media", json!({ "callId": call.id, "muted": self.muted, "deafened": self.deafened }));
                        }
                    }
                    self.send(Command::Ring(self.ring_for(call)));
                    self.send(Command::Sync { call_id: call.id.clone(), me: self.me.clone(), participants: call.participants.iter().map(|p| p.user_id.clone()).collect() });
                    if let Some(channel) = &call.channel_id {
                        self.voice_rooms.insert(channel.clone(), call.participants.clone());
                    }
                    self.call = Some(call.clone());
                    cx.notify();
                } else if self.call.as_ref().is_some_and(|c| c.id == call.id) {
                    // Artık katılımcı değilim (başka oturumdan ayrıldım ya da atıldım).
                    self.teardown(window, cx);
                } else if let Some(incoming) = self.incoming.as_mut().filter(|i| i.call.id == call.id) {
                    incoming.call = call.clone();
                    cx.notify();
                }
            }
            RtEvent::CallSignal { call_id, from, data } => {
                // Çizim ve kalem imleci arayüzde kalır (CallProvider.tsx handleSignal: medya motoruna gitmez).
                if data.get("draw").is_some_and(|d| d.is_object()) || data.get("cursor").is_some_and(|d| d.is_object()) {
                    self.on_draw_signal(from, data, cx);
                    return;
                }
                self.send(Command::Signal { call_id: call_id.clone(), from: from.clone(), data: data.clone() });
            }
            RtEvent::CallEnded { call_id, reason } => {
                if self.incoming.as_ref().is_some_and(|i| &i.call.id == call_id) {
                    self.incoming = None;
                    self.send(Command::Ring(Ring::None));
                    self.release_engine(window, cx);
                    cx.notify();
                }
                if self.call.as_ref().is_some_and(|c| &c.id == call_id) && reason != "switched" {
                    crate::trace(&format!("arama: bitti ({reason})"));
                    self.teardown(window, cx);
                }
            }
            RtEvent::CallDeclined { call_id, .. } => {
                if self.call.as_ref().is_some_and(|c| &c.id == call_id) {
                    self.send(Command::Ring(Ring::None));
                    self.set_error("Arama reddedildi.", cx);
                }
            }
            RtEvent::CallForceMute { call_id } => {
                if self.call.as_ref().is_some_and(|c| &c.id == call_id) && !self.muted {
                    self.toggle_mute(cx);
                }
            }
            RtEvent::VoicePresence { channel_id, participants } => {
                self.voice_rooms.insert(channel_id.clone(), participants.clone());
                cx.notify();
            }
            _ => {}
        }
    }

    fn on_engine(&mut self, event: Event, cx: &mut Context<Self>) {
        match event {
            Event::Signal { call_id, to, data } => {
                self.rt.emit("call:signal", json!({ "callId": call_id, "to": to, "data": data }));
            }
            Event::PeerState { user, state } => {
                crate::trace(&format!("arama: eş {} {state}", &user[..8.min(user.len())]));
                self.peer_states.insert(user, state);
                cx.notify();
            }
            Event::Levels { mic, peers } => {
                self.mic_level = mic;
                let mut speaking = HashSet::new();
                if mic > SPEAKING_DB && !self.muted && self.talking && self.call.is_some() {
                    speaking.insert(self.me.clone());
                }
                for (user, db) in &peers {
                    if *db > SPEAKING_DB {
                        speaking.insert(user.clone());
                    }
                }
                // Testler: gelen sesin ölçüldüğünü izle (saniyede bir, pencerenin en yüksek değeri).
                if std::env::var_os("P2_TRACE_AUDIO").is_some() {
                    self.trace_peak.0 = self.trace_peak.0.max(mic);
                    for (u, db) in &peers {
                        let e = self.trace_peak.1.entry(u.clone()).or_insert(-100.0);
                        *e = e.max(*db);
                    }
                    self.trace_ticks += 1;
                    if self.trace_ticks % 10 == 0 {
                        let list: Vec<String> = self.trace_peak.1.drain().map(|(u, db)| format!("{}={db:.0}", &u[..8.min(u.len())])).collect();
                        crate::trace(&format!("ses: mikrofon {:.0} dBFS, gelen [{}]", self.trace_peak.0, list.join(", ")));
                        self.trace_peak.0 = -100.0;
                    }
                }
                // Seviye (0..1, 0,05 adım): CallProvider `levels` karşılığı; yalnız değişince çizilir.
                let level = |db: f32| (((db + 60.0) / 45.0).clamp(0.0, 1.0) * 20.0).round() / 20.0;
                let mut levels: HashMap<String, f32> = peers.iter().map(|(u, db)| (u.clone(), level(*db))).filter(|(_, l)| *l > 0.0).collect();
                if !self.muted && self.talking && self.call.is_some() && level(mic) > 0.0 {
                    levels.insert(self.me.clone(), level(mic));
                }
                let levels_changed = levels != self.levels;
                self.levels = levels;
                if levels_changed && speaking == self.speaking {
                    cx.notify();
                }
                if speaking != self.speaking || self.mic_test {
                    self.speaking = speaking;
                    cx.notify();
                }
            }
            Event::Devices { input, output } => {
                crate::trace(&format!("arama: aygıtlar giriş={} çıkış={}", input.as_deref().unwrap_or("-"), output.as_deref().unwrap_or("-")));
                self.devices = (input, output);
                cx.notify();
            }
            Event::Error(message) => self.set_error(message, cx),
            Event::Video { user } => self.take_frame(user, cx),
            Event::VideoGone { user } => {
                if let Some((image, _, _)) = self.videos.remove(&user) {
                    cx.drop_image(image, None);
                }
                cx.notify();
            }
            Event::Screen { on } => {
                self.screen_on = on;
                self.send(Command::Cue(if on { Cue::StreamStart } else { Cue::StreamStop }));
                if let Some(call) = &self.call {
                    self.rt.emit("call:media", json!({ "callId": call.id, "screen": on }));
                }
                crate::trace(&format!("arama: ekran paylaşımı {}", if on { "açık" } else { "kapalı" }));
                if on {
                    // Çizim kaplaması yalnız ekran paylaşımında (testte: P2_TEST_OVERLAY ile desen de).
                    let screen = match &self.share_source {
                        Some(ScreenSource::Screen(id)) => Some(Some(*id)),
                        _ if std::env::var_os("P2_TEST_OVERLAY").is_some() => Some(None),
                        _ => None,
                    };
                    if let Some(screen) = screen.filter(|_| self.overlay.is_none()) {
                        // Pencere açılırken hemen bir kez çizilir ve CallController'ı okur: bu
                        // güncellemenin içinde açılmaz, döngünün sonuna ertelenir.
                        let (calls, me) = (cx.entity(), self.me.clone());
                        cx.defer(move |cx| {
                            let handle = crate::overlay_draw::open(calls.clone(), me, screen, cx);
                            calls.update(cx, |c, _| c.overlay = handle);
                        });
                    }
                } else {
                    self.close_overlay(cx);
                    self.share_source = None;
                    self.boards.remove(&self.me);
                }
                cx.notify();
            }
            Event::Camera { on } => {
                self.camera_on = on;
                if let Some(call) = &self.call {
                    self.rt.emit("call:media", json!({ "callId": call.id, "video": on }));
                }
                crate::trace(&format!("arama: kamera {}", if on { "açık" } else { "kapalı" }));
                cx.notify();
            }
            Event::ShareAudio { on, mode, error } => {
                self.share_audio = on.then(|| mode.clone());
                match error {
                    // CallProvider.tsx: "(hata) Yayın sessiz başladı."
                    Some(e) => self.set_error(format!("{} Yayın sessiz başladı.", if e.is_empty() { "Bilgisayar sesi alınamadı.".to_string() } else { e }), cx),
                    None => crate::trace(&format!("arama: bilgisayar sesi {}", if on { format!("açık ({mode})") } else { "kapalı".into() })),
                }
                cx.notify();
            }
            Event::ViewerQuality { user, level } => {
                crate::trace(&format!("arama: {} yayınımı {level} istiyor", &user[..8.min(user.len())]));
            }
        }
    }

    // ------------------------------------------------------------ görüntü

    /// Testler: kamera (`P2_AUTOPILOT_CAMERA=sn[,kapat sn]`), kişi sesi (`P2_AUTOPILOT_VOLUME=0.5`),
    /// moderasyon (`P2_AUTOPILOT_MODERATE=sn`), izleyici kalitesi (`P2_AUTOPILOT_QUALITY=480p:sn`).
    fn autopilot_extras(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let env = |k: &str| std::env::var(k).ok();
        let secs = |s: &str| s.trim().parse::<u64>().ok();
        if let Some(spec) = env("P2_AUTOPILOT_CAMERA") {
            let mut parts = spec.split(',');
            let (on, off) = (parts.next().and_then(secs).unwrap_or(2), parts.next().and_then(secs));
            cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(Duration::from_secs(on)).await;
                let _ = this.update(cx, |this, cx| this.toggle_camera(cx));
                if let Some(off) = off {
                    cx.background_executor().timer(Duration::from_secs(off)).await;
                    let _ = this.update(cx, |this, cx| this.toggle_camera(cx));
                }
            })
            .detach();
        }
        let others = move |this: &Self| -> Vec<String> { this.call.as_ref().map(|c| c.participants.iter().map(|p| p.user_id.clone()).filter(|u| *u != this.me).collect()).unwrap_or_default() };
        if let Some(volume) = env("P2_AUTOPILOT_VOLUME").and_then(|v| v.parse::<f32>().ok()) {
            cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(Duration::from_secs(3)).await;
                let _ = this.update(cx, |this, cx| {
                    for user in others(this) {
                        this.set_volume(&user, volume, cx);
                    }
                });
            })
            .detach();
        }
        // "sn" ya da "sn:Görünen Ad" (o kişi; yoksa ilk diğer katılımcı).
        if let Some((after, name)) = env("P2_AUTOPILOT_MODERATE").map(|s| match s.split_once(':') {
            Some((a, n)) => (secs(a).unwrap_or(5), Some(n.to_string())),
            None => (secs(&s).unwrap_or(5), None),
        }) {
            cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(Duration::from_secs(after)).await;
                let _ = this.update(cx, |this, cx| {
                    let target = match &name {
                        Some(n) => this.call.as_ref().and_then(|c| c.participants.iter().find(|p| &p.display_name == n).map(|p| p.user_id.clone())),
                        None => others(this).first().cloned(),
                    };
                    if let Some(user) = target {
                        this.force_mute(&user, cx);
                    }
                });
            })
            .detach();
        }
        if let Some((level, after)) = env("P2_AUTOPILOT_QUALITY").and_then(|s| s.split_once(':').map(|(l, a)| (l.to_string(), secs(a).unwrap_or(3)))) {
            cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(Duration::from_secs(after)).await;
                let _ = this.update(cx, |this, cx| {
                    for user in others(this) {
                        this.request_quality(&user, &level, cx);
                    }
                });
            })
            .detach();
        }
    }

    /// Kamera aç/kapat (test: `P2_TEST_CAMERA` gerçek kamera yerine desen).
    pub fn toggle_camera(&mut self, cx: &mut Context<Self>) {
        if self.call.is_none() {
            return self.set_error("Kamera için önce bir aramaya katıl.", cx);
        }
        if self.camera_on {
            self.send(Command::StopCamera);
        } else {
            let source = if std::env::var_os("P2_TEST_CAMERA").is_some() { p2_media::CameraSource::TestPattern } else { p2_media::CameraSource::Device(self.settings.camera_device.clone()) };
            self.send(Command::StartCamera(source));
        }
    }

    /// İzlediğim yayını hangi kalitede alayım (QualityMenu.tsx): gönderene `call:signal {quality}`.
    pub fn request_quality(&mut self, user: &str, level: &str, cx: &mut Context<Self>) {
        self.viewer_quality.insert(user.to_string(), level.to_string());
        self.send(Command::RequestQuality { user: user.to_string(), level: level.to_string() });
        crate::trace(&format!("arama: {} yayını {level} istendi", &user[..8.min(user.len())]));
        cx.notify();
    }

    /// Kişi başına dinleme sesi (0–2; 1 = olduğu gibi).
    pub fn set_volume(&mut self, user: &str, volume: f32, cx: &mut Context<Self>) {
        let volume = volume.clamp(0.0, 2.0);
        self.volumes.insert(user.to_string(), volume);
        self.send(Command::SetVolume { user: user.to_string(), volume });
        crate::trace(&format!("arama: {} sesi %{:.0}", &user[..8.min(user.len())], volume * 100.0));
        cx.notify();
    }

    /// "Sessize al" (CallOverlay `dismissedCallId`): kart gizlenir, zil susar; arama bekler.
    pub fn silence_incoming(&mut self, cx: &mut Context<Self>) {
        if self.incoming.is_some() {
            self.send(Command::Ring(Ring::None));
            crate::trace("arama: gelen arama sessize alındı");
        }
        cx.notify();
    }

    /// Kişi ekle (CallOverlay `invite`).
    pub fn invite(&mut self, user: &str, cx: &mut Context<Self>) {
        if let Some(call) = &self.call {
            self.rt.emit("call:invite", json!({ "callId": call.id, "userId": user }));
            crate::trace(&format!("arama: {} davet edildi", &user[..8.min(user.len())]));
        }
        cx.notify();
    }

    /// Yayını izle / bırak (CallProvider `watch`); `None` hepsini bırakır.
    pub fn watch(&mut self, user: Option<&str>, cx: &mut Context<Self>) {
        let call_id = self.call.as_ref().map(|c| c.id.clone());
        let signal = |rt: &RtSender, to: &str, on: bool| {
            if let Some(id) = &call_id {
                rt.emit("call:signal", json!({ "callId": id, "to": to, "data": { "watch": on } }));
            }
        };
        match user {
            None => {
                for u in std::mem::take(&mut self.watching) {
                    signal(&self.rt, &u, false);
                }
            }
            Some(u) => {
                let on = !self.watching.iter().any(|w| w == u);
                signal(&self.rt, u, on);
                if on {
                    self.watching.push(u.to_string());
                } else {
                    self.watching.retain(|w| w != u);
                }
                crate::trace(&format!("arama: {} yayını {}", &u[..8.min(u.len())], if on { "izleniyor" } else { "bırakıldı" }));
            }
        }
        cx.notify();
    }

    /// Kişi ses seviyesi (0–100, Electron `userVolumes`); motora 0–1 gider.
    pub fn user_volume(&self, user: &str) -> u32 {
        (self.volumes.get(user).copied().unwrap_or(1.0).min(1.0) * 100.0).round() as u32
    }

    pub fn set_user_volume(&mut self, user: &str, volume: u32, cx: &mut Context<Self>) {
        let v = volume.min(100) as f32 / 100.0;
        self.volumes.insert(user.to_string(), v);
        if !self.locally_muted.contains(user) {
            self.send(Command::SetVolume { user: user.to_string(), volume: v });
        }
        cx.notify();
    }

    /// "Kendim için sustur" (`toggleLocalMute`): o kişinin sesi bende 0.
    pub fn toggle_local_mute(&mut self, user: &str, cx: &mut Context<Self>) {
        let muted = !self.locally_muted.remove(user);
        if muted {
            self.locally_muted.insert(user.to_string());
        }
        let v = if muted { 0.0 } else { self.volumes.get(user).copied().unwrap_or(1.0) };
        self.send(Command::SetVolume { user: user.to_string(), volume: v });
        crate::trace(&format!("arama: {} kendim için {}", &user[..8.min(user.len())], if muted { "susturuldu" } else { "açıldı" }));
        cx.notify();
    }

    /// "Sesimi bu kişiye gönderme" (`togglePeerDeafen`): o eşe giden mikrofon izi sökülür.
    pub fn toggle_peer_deafen(&mut self, user: &str, cx: &mut Context<Self>) {
        let blocked = !self.deafened_for.remove(user);
        if blocked {
            self.deafened_for.insert(user.to_string());
        }
        self.send(Command::SetPeerSend { user: user.to_string(), send: !blocked });
        crate::trace(&format!("arama: {} kişisine ses {}", &user[..8.min(user.len())], if blocked { "gönderilmiyor" } else { "gönderiliyor" }));
        cx.notify();
    }

    /// Yönetici: katılımcıyı sustur (`call:moderate`; sunucu sahip/yönetici rolünü denetler).
    pub fn force_mute(&mut self, user: &str, cx: &mut Context<Self>) {
        if let Some(call) = &self.call {
            self.rt.emit("call:moderate", json!({ "callId": call.id, "userId": user, "muted": true }));
            crate::trace(&format!("arama: {} susturuldu (moderasyon)", &user[..8.min(user.len())]));
        }
        cx.notify();
    }

    /// Ekran paylaşımını başlat (ScreenShareModal seçimi). Aramada değilsen bir şey yapmaz.
    pub fn start_screen(&mut self, source: ScreenSource, fps: u32, quality: &str, audio: bool, cx: &mut Context<Self>) {
        if self.call.is_none() {
            return self.set_error("Ekran paylaşmak için önce bir aramaya katıl.", cx);
        }
        self.share_source = Some(source.clone());
        self.send(Command::StartScreen { source, fps, quality: quality.to_string(), audio });
    }

    // ------------------------------------------------------------ çizim

    fn close_overlay(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.overlay.take() {
            crate::overlay_draw::close(handle, cx);
        }
    }

    /// Gelen `{draw}` / `{cursor}`: ilgili yayının tahtasına işlenir. Hedef bensem ve yayın
    /// sürüyorsa kaplama penceresi (aynı tahtayı izler) masaüstümde de çizer.
    fn on_draw_signal(&mut self, from: &str, data: &serde_json::Value, cx: &mut Context<Self>) {
        let now = Instant::now();
        if let Some(d) = data.get("draw") {
            let target = d["target"].as_str().unwrap_or_default().to_string();
            let points = crate::annotate::parse_points(&d["points"]);
            let (id, color) = (d["id"].as_str().unwrap_or_default(), d["color"].as_str().unwrap_or("#ffffff"));
            self.boards.entry(target.clone()).or_default().add_stroke(id, color, d["width"].as_f64().unwrap_or(4.0) as f32, &points, now);
            crate::trace(&format!("çizim: {} → {} çizgi {id} ({} nokta){}", &from[..8.min(from.len())], &target[..8.min(target.len())], points.len(), if target == self.me && self.overlay.is_some() { " · kaplamada" } else { "" }));
        } else if let Some(c) = data.get("cursor") {
            let target = c["target"].as_str().unwrap_or_default().to_string();
            let cursor = (c["gone"].as_bool() != Some(true)).then(|| crate::annotate::Cursor {
                x: c["x"].as_f64().unwrap_or(0.0) as f32,
                y: c["y"].as_f64().unwrap_or(0.0) as f32,
                name: c["name"].as_str().unwrap_or_default().to_string(),
                color: c["color"].as_str().unwrap_or("#ffffff").to_string(),
                at: now,
            });
            let first = !self.boards.get(&target).is_some_and(|b| b.cursors.contains_key(from));
            if first && cursor.is_some() {
                crate::trace(&format!("çizim: {} imleci ({}) → {}", &from[..8.min(from.len())], c["name"].as_str().unwrap_or_default(), &target[..8.min(target.len())]));
            }
            self.boards.entry(target).or_default().set_cursor(from, cursor);
        }
        self.schedule_prune(cx);
        cx.notify();
    }

    /// Kendi çizgim: hemen kendi tahtama, sonra aramadaki herkese (CallProvider.tsx `sendStroke`).
    pub fn send_stroke(&mut self, target: &str, id: &str, color: &str, width: f32, points: &[(f32, f32)], cx: &mut Context<Self>) {
        self.boards.entry(target.to_string()).or_default().add_stroke(id, color, width, points, Instant::now());
        let data = crate::annotate::draw_payload(target, id, color, width, points);
        self.emit_to_others(data);
        crate::trace(&format!("çizim: gönderildi → {} çizgi {id} ({} nokta)", &target[..8.min(target.len())], points.len()));
        self.schedule_prune(cx);
        cx.notify();
    }

    /// Kalem imleci: yalnız diğerlerine (kendime göstermem gerekmez).
    pub fn send_pointer(&mut self, target: &str, name: &str, color: &str, x: f32, y: f32, gone: bool) {
        self.emit_to_others(crate::annotate::cursor_payload(target, name, color, x, y, gone));
    }

    fn emit_to_others(&self, data: serde_json::Value) {
        let Some(call) = &self.call else { return };
        for p in call.participants.iter().filter(|p| p.user_id != self.me) {
            self.rt.emit("call:signal", json!({ "callId": call.id, "to": p.user_id, "data": data }));
        }
    }

    /// Solan çizgiler ve kaybolan imleçler bellekte kalmasın.
    fn schedule_prune(&mut self, cx: &mut Context<Self>) {
        if self.prune_running {
            return;
        }
        self.prune_running = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(500)).await;
                let done = this
                    .update(cx, |this, cx| {
                        let now = Instant::now();
                        this.boards.retain(|_, b| !b.prune(now));
                        if this.boards.is_empty() {
                            this.prune_running = false;
                            cx.notify();
                            true
                        } else {
                            false
                        }
                    })
                    .unwrap_or(true);
                if done {
                    break;
                }
            }
        })
        .detach();
    }

    // ------------------------------------------------------------ arama balonu

    /// Arama sürdükçe ana pencerenin durumunu izler (main.cjs: minimize/hide/restore olayları).
    fn start_widget_watch(&mut self, cx: &mut Context<Self>) {
        if self.widget_watch {
            return;
        }
        self.widget_watch = true;
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(Duration::from_millis(400)).await;
            let go = this
                .update(cx, |this, cx| {
                    this.sync_widget(cx);
                    let alive = this.call.is_some() || this.widget.is_some();
                    if !alive {
                        this.widget_watch = false;
                    }
                    alive
                })
                .unwrap_or(false);
            if !go {
                break;
            }
        })
        .detach();
    }

    /// main.cjs `syncCallWidget`: aktif arama + ana pencere gizli/simge + "bir daha gösterme"
    /// denmemiş + bu arama için "bu seferlik gizle" denmemiş → balon görünür.
    pub fn sync_widget(&mut self, cx: &mut Context<Self>) {
        let call_id = self.call.as_ref().map(|c| c.id.clone());
        let show = call_id.is_some() && crate::call_widget::main_hidden() && !crate::call_widget::load_state().hidden && self.widget_skipped != call_id;
        if !show {
            if let Some(w) = &self.widget {
                if w.update(cx, |_, window, _| window.remove_window()).is_ok() {
                    self.widget = None;
                    crate::trace("balon: kapandı");
                }
            }
            return;
        }
        match &self.widget {
            Some(w) => crate::call_widget::remember_position(w, cx),
            None if !self.widget_opening => {
                self.widget_opening = true;
                let calls = cx.entity();
                cx.defer(move |cx| {
                    let handle = crate::call_widget::open(calls.clone(), cx);
                    calls.update(cx, |c, _| {
                        c.widget = handle;
                        c.widget_opening = false;
                    });
                });
            }
            None => {}
        }
    }

    pub fn widget_skip_this_call(&mut self, cx: &mut Context<Self>) {
        self.widget_skipped = self.call.as_ref().map(|c| c.id.clone());
        self.sync_widget(cx);
    }

    /// Balondaki düğmeler (main.cjs `widget:action` → CallOverlay.tsx `onWidgetAction`).
    pub fn widget_action(&mut self, action: &str, cx: &mut Context<Self>) {
        match action {
            "mute" => self.toggle_mute(cx),
            "deafen" => self.toggle_deafen(cx),
            "open" => {
                // Yalnız pencereyi öne getirir; kullanıcı bıraktığı ekranda kalır.
                let hwnd = windows::Win32::Foundation::HWND(crate::MAIN_HWND.load(std::sync::atomic::Ordering::Relaxed) as _);
                if !hwnd.is_invalid() {
                    crate::shell::show(hwnd);
                }
            }
            _ => {}
        }
    }

    pub fn main_window(&self) -> Option<gpui_kit::AnyWindowHandle> {
        self.main_window
    }

    pub fn stop_screen(&mut self, _cx: &mut Context<Self>) {
        self.send(Command::StopScreen);
    }

    /// Motordan en yeni kareyi al, GPUI görüntüsüne çevir; öncekinin dokusu bırakılır.
    fn take_frame(&mut self, user: String, cx: &mut Context<Self>) {
        let Some(engine) = &self.engine else { return };
        let Some(frame) = engine.frames().lock().unwrap().remove(&user) else { return };
        let (w, h) = (frame.width, frame.height);
        let Some(buffer) = image::RgbaImage::from_raw(w, h, frame.bgra) else { return };
        let image = Arc::new(RenderImage::new(vec![image::Frame::new(buffer)]));
        if let Some((old, _, _)) = self.videos.insert(user.clone(), (image, w, h)) {
            cx.drop_image(old, None);
        }
        // Testler: kullanıcı başına saniyede bir kare hızı ve boyut.
        if std::env::var_os("P2_TRACE_AUDIO").is_some() {
            *self.video_counts.entry(user).or_default() += 1;
            let at = *self.video_trace_at.get_or_insert_with(Instant::now);
            if at.elapsed() >= Duration::from_secs(2) {
                for (u, n) in self.video_counts.drain() {
                    let size = self.videos.get(&u).map(|v| format!("{}x{}", v.1, v.2)).unwrap_or_default();
                    crate::trace(&format!("görüntü: {} {size} {:.0} kare/sn", &u[..8.min(u.len())], n as f32 / at.elapsed().as_secs_f32()));
                }
                self.video_trace_at = Some(Instant::now());
            }
        }
        cx.notify();
    }

    /// Arama başlığı: "#ekip-odası", "Rust Bot", grup adı (çağıran tamamlar).
    pub fn elapsed(&self) -> Option<String> {
        self.connected_at.map(|at| {
            let s = at.elapsed().as_secs();
            format!("{:02}:{:02}", s / 60, s % 60)
        })
    }
}
