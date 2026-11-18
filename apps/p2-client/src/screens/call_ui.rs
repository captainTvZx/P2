//! Arama arayüzü (I ajanı, Electron birebir) — karşılıklar:
//!  - CallOverlay.tsx: arama paneli `.call-dock` (+ `.expanded` sahne, `.call-invite`), gelen arama
//!    `.incoming-layer/.incoming-call`, `.call-error-toast`.
//!  - ChannelContent.tsx `VoiceRoomStage`: sesli oda sahnesi (boş durum, kişi kartları, katıl düğmesi,
//!    izlenen yayınlar `.featured`), kart sağ tık menüsü `.voice-user-menu` (ses kaydırıcıları).
//!  - StreamStage/StreamPlayer.tsx: izlenen yayın oynatıcıları, kalem araçları `.pen-tools`.
//! Stiller call.css + channel-content.css. Boyutlar CSS px; yazılar `fs()` (yazı ölçeği ,85).
//! Çalışma alanının alt modülü (alanlarına doğrudan erişir).

use std::cell::Cell;
use p2_ui::P2Fx as _; // backdrop_blur
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::AnimationExt as _;
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, Bounds, Context, Entity, Hsla, ImageSource, MouseButton, ObjectFit, Pixels, SharedString, StyleRefinement, Subscription, Window, canvas, div, img, px};
use p2_client_core::{CallParticipant, CallState, Channel, ChannelType};
use p2_ui::P2Text as _;
use p2_ui::components::{avatar_hue, over, user_initials};
use p2_ui::icons::icon;
use p2_ui::tokens::{Palette, css_rgba, fs, shadows};
use p2_ui::{Entrance, P2Motion as _};

use super::WorkspaceScreen;
use super::super::chat::{ChatPane, ChatSubject};
use super::screen_picker::{PickerEvent, QUALITIES, ScreenPicker, quality_el, quality_height};

/// Eski "arama sahnesi" yolu (artık kullanılmıyor; Electron'da arama paneli büyür).
#[allow(dead_code)]
pub(super) const CALL_STAGE: &str = "@arama";

/// Çalışma alanındaki arama arayüzü durumu (Electron bileşen state'leri).
#[derive(Default)]
pub(super) struct CallUi {
    /// Bireysel / grup aramasının arama görünümü (sesli oda gibi ana alanı kaplar: sahne, alt kontrol
    /// çubuğu, sohbet yan paneli). Aramanın sohbeti açıkken görünür; başka yere geçince arama panelde
    /// sürer, sohbete dönünce görünüm geri gelir.
    expanded: bool,
    /// Arama başladı ama sohbeti açık değil: bir sonraki çizimde sohbete geçilir (pencere gerekir).
    pending_open: bool,
    /// Görünümü en son hangi arama için ayarladım (arama başlayınca aç, bitince kapat).
    tracked: Option<String>,
    /// "Kişi ekle" listesi (`invitePicker`).
    invite: bool,
    /// "Sessize al" denen gelen arama (`dismissedCallId`).
    dismissed: Option<String>,
    /// Sesli oda kart menüsü (kişi, pencere konumu).
    menu: Option<(String, f32, f32)>,
    /// VoiceRoomStage `cardsOpen` tersi.
    cards_hidden: bool,
    /// Sesli oda / arama "Sohbeti göster" (`voiceChatOpen`): varsayılan kapalı, oturum boyunca korunur.
    voice_chat: bool,
    /// Sunum düzeninde büyük gösterilmek üzere seçilen paylaşım (şeritteki kartına tıklanan).
    pinned: Option<String>,
    /// Ekran paylaşımlarının başlama sırası (en son başlayan sonda; seçim yoksa o büyür).
    share_order: std::cell::RefCell<Vec<String>>,
    /// Büyüyünce kendiliğinden "izle" sinyali gönderilen son yayın (bir kez; sonra kullanıcıya kalır).
    auto_watched: std::cell::RefCell<Option<String>>,
    /// Sesli oda sohbetinin başlığı çalışma alanında çizildiği için sohbet değişince yenile.
    chat_watch: Option<Subscription>,
    /// Sahnenin ölçülen boyutu (ızgara ve radyal zeminler bununla hesaplanır).
    stage_size: Rc<Cell<(f32, f32)>>,
    /// Ölçüm anındaki tahmini sahne boyutu (pencere − panel genişlikleri): bu karenin boyutu ölçüm +
    /// tahmin farkıdır (boyutlandırmada bir kare gecikmeli kart/zemin olmasın).
    stage_est: Rc<Cell<(f32, f32)>>,
    /// Üzerinde durulan kart / yayın (izle düğmesi, yayın çubuğu).
    hover: Option<String>,
    /// Sürüklenen kaydırıcı: (kimlik, iz sınırları).
    sliders: Rc<std::cell::RefCell<std::collections::HashMap<String, Bounds<Pixels>>>>,
    /// Açık açılır liste (yayın kalitesi / kare hızı / izleyici kalitesi): (seçenek anahtarı, tetikleyici kimliği).
    dropdown: Option<(String, String)>,
    /// Açılır liste tetikleyicilerinin ölçülen pencere sınırları (kimlik → sınır): liste tetikleyicinin
    /// hemen altına (yer yoksa üstüne) açılır; fare konumu kullanılmaz.
    dropdown_bounds: Rc<std::cell::RefCell<std::collections::HashMap<String, Bounds<Pixels>>>>,
    /// × ile kapatılan arama paneli (o aramanın kimliği; sonraki aramada yine görünür).
    dock_hidden: Option<String>,
    /// Arama panelinin konumu (sol, alt; pencere px) — sürüklenir, `layout.json`'a yazılır.
    dock_pos: Rc<Cell<Option<(f32, f32)>>>,
    /// Sürükleme başlangıcı: (fare x, fare y, sol, alt).
    dock_drag: Rc<Cell<Option<(f32, f32, f32, f32)>>>,
    /// Panelin ölçülen yüksekliği (pencere içinde tutmak için).
    dock_h: Rc<Cell<f32>>,
    /// Basılı tutulup sürüklenen kaydırıcının kimliği (pencere genelinde fare izlenir; imleç
    /// kaydırıcının ince şeridinden çıksa da değer güncellenir).
    pub(super) slider_drag: Rc<std::cell::RefCell<Option<SharedString>>>,
    /// Tam ekran izlenen yayın: (kişi, etiket, kendi yayınım mı).
    fullscreen: Option<(String, String, bool)>,
    /// Tam ekrana girerken pencereyi işletim sistemi tam ekranına ben aldım (çıkışta geri alınır).
    os_fullscreen: bool,
    /// Tam ekran katmanının odağı (Esc çalışma alanının tuş dinleyicisine ulaşsın).
    fs_focus: Option<gpui_kit::FocusHandle>,
}

/// Arama paneli konumu (`layout.json` anahtarları; varsayılan `left: 78px; bottom: 12px`).
const DOCK_LEFT: crate::resize::Spec = crate::resize::Spec { key: "p2:call-dock-left", initial: 78., min: 0., max: 20000., direction: 1., load: crate::resize::Load::Clamp };
const DOCK_BOTTOM: crate::resize::Spec = crate::resize::Spec { key: "p2:call-dock-bottom", initial: 12., min: 0., max: 20000., direction: 1., load: crate::resize::Load::Any };
const DOCK_W: f32 = 274.;
/// Arama görünümünün sohbet paneli en az / sahnenin bıraktığı en az genişlik.
const CHAT_MIN_W: f32 = 260.;
const STAGE_MIN_W: f32 = 280.;

// ------------------------------------------------------------------------------------------ yardımcılar

/// CallIcons.tsx: dolgulu tek renk ikon (`fill: currentColor; stroke: none`).
/// Arama görünümünü aç simgesi (köşe okları).
const ICON_EXPAND: &str = r#"<path d="M9 4H4v5M15 4h5v5M9 20H4v-5M15 20h5v-5"/>"#;

/// Sahnenin alt kontrol çubuğu yüksekliği (düğmeler 40 + iç boşluk).
const BAR_H: f32 = 54.;

/// Yayının kare hızları — yayıncının canlı ayarı (kalite seviyeleri: screen_picker `QUALITIES`).
const SHARE_FPS: [u32; 4] = [5, 15, 30, 60];

/// İzleyicinin seçebileceği kaliteler (QualityMenu.tsx `LEVELS`): Otomatik + yayıncının gönderdiği
/// kaliteye (`call:signal {shareQuality}`) kadar olan seviyeler; yayıncı duyurmadıysa (eski istemci) 1080p.
fn view_levels(stream_quality: Option<&str>) -> Vec<&'static str> {
    let cap = stream_quality.map(quality_height).unwrap_or(1080);
    std::iter::once("auto").chain(QUALITIES.iter().copied().filter(|q| quality_height(q) <= cap)).collect()
}

/// İzleyicinin istediği ve gerçekte alacağı seviye: yayıncının kalitesinin üstündeki istek tavana iner.
fn view_level_shown(wanted: &str, stream_quality: Option<&str>) -> String {
    match stream_quality {
        Some(cap) if wanted != "auto" && quality_height(wanted) > quality_height(cap) => cap.to_string(),
        _ => wanted.to_string(),
    }
}

/// Sahnede gösterilen oda: sesli oda kanalı ya da bireysel / grup araması.
pub(super) struct Room {
    name: String,
    people: Vec<CallParticipant>,
    /// Aranan ama henüz katılmamış kişiler (bireysel/grup araması; "çalıyor").
    pending: Vec<CallParticipant>,
    joined: bool,
    /// Sesli odaysa kanal (katıl düğmesi); aramada `None`.
    channel: Option<Channel>,
}

/// Katılımcının durum simgeleri (sesli oda satırı, arama kutucuğu, kişi listesi): YAYINDA rozeti,
/// kamera, kapalı mikrofon, kapalı kulaklık. Nötr çizgi ikonlar (text-3); yalnız susturma hafif kırmızı.
pub(super) fn status_icons(part: &CallParticipant, size: f32, p: &Palette) -> gpui_kit::Div {
    let glyph = |name: &str, color: Hsla, tip_key: &str| {
        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .child(icon(&format!("composer:{name}")).stroke(1.9).el().size(px(size)).text_color(color))
            .id(SharedString::from(format!("st-{name}-{}", part.user_id)))
            .tooltip(tip(p2_i18n::t(tip_key)))
    };
    let muted_ink = alpha(p.danger_text, 0.85);
    div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(4.))
        .when(part.screen, |s| s.child(div().flex_none().px(px(5.)).py(px(1.)).rounded(px(5.)).bg(p.danger).text_color(gpui_kit::white()).text_size(fs(8.)).css_weight(800.).tracking(0.04).child(p2_i18n::t("callui.liveTag"))))
        .when(part.video, |s| s.child(glyph("camera", p.text_3, "callui.status.camera")))
        .when(part.muted, |s| s.child(glyph("micOff", muted_ink, "callui.status.muted")))
        .when(part.deafened, |s| s.child(glyph("headphonesOff", p.text_3, "callui.status.deafened")))
}

/// "Aramada" simgesi (sohbet satırları): yeşil ahize.
fn in_call_icon(user_id: &str, p: &Palette) -> gpui_kit::Stateful<gpui_kit::Div> {
    div()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .child(icon("composer:call").stroke(1.9).el().size(px(13.)).text_color(p.success_text))
        .id(SharedString::from(format!("st-incall-{user_id}")))
        .tooltip(tip(p2_i18n::t("callui.status.inCall")))
}

/// `m:ss`.
fn clock(secs: f32) -> String {
    let s = secs.max(0.).round() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Bireysel aramada karşı taraf.
fn counterpart(call: &CallState, me: &str) -> Option<String> {
    if call.caller.as_deref() == Some(me) { call.callee.clone() } else { call.caller.clone() }
}

fn call_icon(name: &str, size: f32, color: Hsla) -> gpui_kit::Svg {
    icon(&format!("call:{name}")).stroke(0.).filled().el().size(px(size)).text_color(color)
}

/// `rgba(7,11,20,.72)` — kart/kutucuk etiket zemini.
fn chip_bg(alpha: f32) -> Hsla {
    css_rgba(7, 11, 20, alpha)
}

fn tip(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> gpui_kit::AnyView + 'static {
    let text: SharedString = text.into();
    move |window, cx| gpui_kit::component::tooltip::Tooltip::new(text.clone()).build(window, cx)
}

/// `color-mix(in srgb, c p%, transparent)`.
fn alpha(c: Hsla, a: f32) -> Hsla {
    Hsla { a: c.a * a, ..c }
}

/// `color-mix(in srgb, c, white 8%)`.
fn lighten(c: Hsla, white: f32) -> Hsla {
    let r: gpui_kit::Rgba = c.into();
    gpui_kit::Rgba { r: r.r + (1. - r.r) * white, g: r.g + (1. - r.g) * white, b: r.b + (1. - r.b) * white, a: r.a }.into()
}

fn mmss(secs: u64) -> String {
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

/// SVG rengi (#rrggbb, opaklık).
fn svg_color(c: Hsla) -> (String, f32) {
    let r: gpui_kit::Rgba = c.into();
    (format!("#{:02x}{:02x}{:02x}", (r.r * 255.).round() as u8, (r.g * 255.).round() as u8, (r.b * 255.).round() as u8), r.a)
}

/// `.voice-room-stage.empty` zemini: `radial-gradient(80% 70% at 50% 100%, accent-soft-2, transparent 72%),
/// linear-gradient(145deg, bg-1 0%, blue-900 52%, bg-0 100%)` (resvg ile; GPUI'de elips gradyan yok).
///
/// Boyutlandırmada titremesin diye iki görsel de boyuttan bağımsız, bir kez rasterlenir (eskiden SVG
/// sahne boyutuyla değişiyor, her karede arka planda rasterlenene dek boş kalıp zemin titriyordu):
/// - Doğrusal kısım: `p2_ui::components::linear_background` (sabit kare görsel, eşit ölçek).
/// - Elips kısım: yarıçaplar ve merkez boyutla orantılı; sabit görsel sahneye esnetilir (birebir).
fn empty_room_background(w: f32, h: f32, p: &Palette) -> gpui_kit::Div {
    const S: f32 = 512.;
    let (glow, ga) = svg_color(p.accent_soft_2);
    // Elips: merkez (S/2, S), yarıçaplar 0.8S × 0.7S — sahneye esnetilince 80% × 70% at 50% 100%.
    let (rx, ry) = (S * 0.8, S * 0.7);
    let ellipse = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{S}" height="{S}" viewBox="0 0 {S} {S}"><defs><radialGradient id="r" gradientUnits="userSpaceOnUse" cx="{gx}" cy="{S}" r="{rx}" gradientTransform="translate({gx} {S}) scale(1 {sy}) translate(-{gx} -{S})"><stop offset="0" stop-color="{glow}" stop-opacity="{ga}"/><stop offset=".72" stop-color="{glow}" stop-opacity="0"/></radialGradient></defs><rect width="{S}" height="{S}" fill="url(#r)"/></svg>"#,
        gx = S / 2.,
        sy = ry / rx,
    );
    div()
        .w(px(w))
        .h(px(h))
        .child(p2_ui::components::linear_background(w, h, 145., &[(p.bg_1, 0.), (p.blue_900, 0.52), (p.bg_0, 1.)]).absolute().top_0().left_0())
        .child(p2_ui::icons::svg_image(ellipse).object_fit(ObjectFit::Fill).absolute().top_0().left_0().w(px(w)).h(px(h)))
}

/// Yerel `input[type=range]` (accent-color): 8px iz (dolu kısım vurgu), 16px topuz. `value` 0..1.
fn range_track(value: f32, accent: Hsla, w: f32, p: &Palette) -> gpui_kit::Div {
    let v = value.clamp(0., 1.);
    let thumb = 16.;
    let x = (w - thumb) * v;
    let edge = over(alpha(gpui_kit::white(), 0.25), accent);
    div()
        .relative()
        .w(px(w))
        .h(px(16.))
        .flex_none()
        .child(div().absolute().left(px(1.)).right(px(1.)).top(px(4.)).h(px(8.)).rounded(px(4.)).bg(if p.dark { css_rgba(59, 59, 59, 1.) } else { css_rgba(239, 239, 239, 1.) }).border_1().border_color(if p.dark { css_rgba(133, 133, 133, 1.) } else { css_rgba(118, 118, 118, 1.) }))
        .child(div().absolute().left(px(1.)).top(px(4.)).h(px(8.)).w(px((x + thumb / 2.).max(4.))).rounded(px(4.)).bg(accent).border_1().border_color(edge))
        .child(div().absolute().left(px(x)).top_0().size(px(thumb)).rounded_full().bg(accent))
}

// ------------------------------------------------------------------------------------------ çalışma alanı

impl WorkspaceScreen {
    /// Sohbet satırı (proje SOHBETLER + Ana sayfa): kişi benimle aynı bireysel / grup aramasındaysa
    /// sesli oda satırındaki durum simgeleri — aramada, YAYINDA, kamera, kapalı mikrofon / kulaklık.
    pub(super) fn dm_call_status(&self, user_id: &str, p: &Palette, cx: &App) -> Option<gpui_kit::Div> {
        let call = self.calls.read(cx).call.as_ref()?;
        let me = self.auth.user.id.as_str();
        if call.kind == "channel" || user_id == me || !call.participants.iter().any(|x| x.user_id == me) {
            return None;
        }
        let part = call.participants.iter().find(|x| x.user_id == user_id)?;
        Some(div().flex_none().flex().items_center().gap(px(4.)).child(in_call_icon(user_id, p)).child(status_icons(part, 14., p)))
    }

    /// Grup sohbeti satırı: o grubun aramasındaysam "aramada" simgesi.
    pub(super) fn group_call_status(&self, group_id: &str, p: &Palette, cx: &App) -> Option<gpui_kit::Stateful<gpui_kit::Div>> {
        let call = self.calls.read(cx).call.as_ref()?;
        (call.kind == "group" && call.group_id.as_deref() == Some(group_id)).then(|| in_call_icon(group_id, p))
    }

    /// Otomatik pilot (arama testleri): gerçek zamanlı bağlantı ve proje kişileri hazır
    /// olunca bir kez çalışır. P2_AUTOPILOT_CALL=<kullanıcı> arar, P2_AUTOPILOT_VOICE=<kanal>
    /// sesli odayı açıp katılır; P2_AUTOPILOT_MUTE_AFTER / _LEAVE_AFTER (sn) sustur / ayrıl.
    /// Görünüm kancaları (veri yazmaz): P2_AUTOPILOT_VOICE_VIEW=<kanal> (katılmadan aç),
    /// P2_AUTOPILOT_VOICE_MENU=<kullanıcı adı> (kart menüsü), P2_AUTOPILOT_CALL_EXPAND=1,
    /// P2_AUTOPILOT_SCREEN_PICKER=1, P2_AUTOPILOT_WATCH=1 (ilk yayını izle).
    pub(super) fn autopilot_call(&mut self, window: &mut gpui_kit::Window, cx: &mut Context<Self>) {
        if self.call_autopilot_done || !self.connected || self.members.is_empty() {
            return;
        }
        self.call_autopilot_done = true;
        self.autopilot_draw(window, cx);
        if let Ok(user) = std::env::var("P2_AUTOPILOT_CALL") {
            if let Some(id) = self.members.iter().find(|m| m.username == user).map(|m| m.id.clone()) {
                // Proje içinden arama: o projenin sohbeti kapsamında (arama kaydı proje sohbetine düşer).
                let scope = self.dm_scope();
                self.start_direct_call(&id, scope, window, cx);
                if std::env::var_os("P2_AUTOPILOT_STAGE").is_some() {
                    self.call_ui.expanded = true;
                }
            }
        }
        let find = |this: &Self, name: &str| this.categories.iter().flat_map(|c| c.channels.iter()).find(|c| c.name == name).cloned();
        if let Some(channel) = std::env::var("P2_AUTOPILOT_VOICE_VIEW").ok().and_then(|n| find(self, &n)) {
            self.select_channel(channel, window, cx);
            // Boyutlandırma testi (yalnız görünüm): `P2_AUTOPILOT_VOICE_CHAT=1` oda sohbetini de açar.
            if std::env::var_os("P2_AUTOPILOT_VOICE_CHAT").is_some() {
                self.call_ui.voice_chat = true;
            }
            crate::trace("arama: sesli oda görünümü açıldı");
        }
        if let Some(channel) = std::env::var("P2_AUTOPILOT_VOICE").ok().and_then(|n| find(self, &n)) {
            let id = channel.id.clone();
            self.select_channel(channel, window, cx);
            self.calls.update(cx, |c, cx| c.join_channel(&id, window, cx));
        }
        for (var, mute) in [("P2_AUTOPILOT_MUTE_AFTER", true), ("P2_AUTOPILOT_LEAVE_AFTER", false)] {
            if let Some(secs) = std::env::var(var).ok().and_then(|s| s.parse::<u64>().ok()) {
                let calls = self.calls.clone();
                cx.spawn_in(window, async move |_, cx| {
                    cx.background_executor().timer(std::time::Duration::from_secs(secs)).await;
                    let _ = cx.update(|window, cx| {
                        calls.update(cx, |c, cx| if mute { c.toggle_mute(cx) } else { c.leave(window, cx) });
                    });
                })
                .detach();
            }
        }
        // Görünüm kancaları: arama kurulunca (katılımcılar gelince) bir kez.
        let menu = std::env::var("P2_AUTOPILOT_VOICE_MENU").ok();
        let expand = std::env::var_os("P2_AUTOPILOT_CALL_EXPAND").is_some();
        let picker = std::env::var_os("P2_AUTOPILOT_SCREEN_PICKER").is_some();
        let watch = std::env::var_os("P2_AUTOPILOT_WATCH").is_some();
        if menu.is_some() || expand || picker || watch {
            cx.spawn_in(window, async move |this, cx| {
                for _ in 0..80 {
                    cx.background_executor().timer(Duration::from_millis(250)).await;
                    let done = this
                        .update_in(cx, |this, window, cx| {
                            let (in_call, ready) = {
                                let calls = this.calls.read(cx);
                                let ready = calls.call.as_ref().is_some_and(|c| c.participants.len() > 1);
                                (calls.call.is_some(), ready)
                            };
                            if !in_call || !ready {
                                return false;
                            }
                            if let Some(name) = &menu {
                                let Some(user) = this.members.iter().chain(std::iter::once(&this.me_as_member())).find(|m| &m.username == name).map(|m| m.id.clone()) else { return true };
                                let (w, h) = this.call_ui.stage_size.get();
                                if w < 10. {
                                    cx.notify();
                                    return false;
                                }
                                // Kartın ortası (pencere koordinatı; sahne 324,96'dan başlar).
                                let ix = this.voice_room_people(cx).iter().position(|p| p.user_id == user).unwrap_or(0);
                                let (x, y) = this.card_center(ix, w, h, cx);
                                this.call_ui.menu = Some((user, x, y));
                                crate::trace(&format!("arama: kişi menüsü {name}"));
                            }
                            if expand {
                                this.open_call_view(window, cx);
                            }
                            if picker {
                                this.toggle_screen(window, cx);
                                crate::trace("arama: ekran paylaşımı penceresi");
                            }
                            if watch {
                                let me = this.auth.user.id.clone();
                                let target = this.calls.read(cx).call.as_ref().and_then(|c| c.participants.iter().find(|p| p.user_id != me && (p.screen || p.video)).map(|p| p.user_id.clone()));
                                let Some(target) = target else { return false };
                                if !this.calls.read(cx).videos.contains_key(&target) {
                                    return false;
                                }
                                this.calls.update(cx, |c, cx| c.watch(Some(&target), cx));
                            }
                            cx.notify();
                            true
                        })
                        .unwrap_or(true);
                    if done {
                        break;
                    }
                }
            })
            .detach();
        }
    }

    /// Arama görünümünü aç (paneldeki düğme, otomatik pilot): sesli odada odanın kanalı, bireysel /
    /// grup aramasında aramanın sohbeti açılır ve sahne ana alanı kaplar.
    pub(super) fn open_call_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(call) = self.calls.read(cx).call.clone() else { return };
        if call.kind == "channel" {
            let channel = call.channel_id.as_ref().and_then(|id| self.categories.iter().flat_map(|c| c.channels.iter()).find(|c| &c.id == id).cloned());
            if let Some(channel) = channel {
                if self.voice_channel(cx).map(|c| c.id) != Some(channel.id.clone()) {
                    self.select_channel(channel, window, cx);
                }
            }
            return;
        }
        self.call_ui.expanded = true;
        self.board = None;
        self.documents = None;
        self.library = None;
        if !self.call_chat_matches(&call, cx) {
            match call.kind.as_str() {
                "group" => {
                    if let Some(group) = call.group_id.as_deref().and_then(|id| self.group_by_id(id)).cloned() {
                        self.select_group(group, window, cx);
                    }
                }
                _ => {
                    let me = self.auth.user.id.clone();
                    let member = counterpart(&call, &me).and_then(|id| {
                        self.find_member(&id).or_else(|| {
                            call.participants.iter().find(|p| p.user_id == id).map(|p| p2_client_core::Member { id: p.user_id.clone(), username: String::new(), display_name: p.display_name.clone(), title: None, avatar_url: p.avatar_url.clone(), status: String::new(), role: String::new(), email: None })
                        })
                    });
                    if let Some(member) = member {
                        self.open_direct_in(member, call.project_id.clone(), window, cx);
                    }
                }
            }
        }
        crate::trace(&format!("arama: arama görünümü açıldı ({})", call.kind));
        cx.notify();
    }

    /// Otomatik pilot: arama görünümünü aç / kapat (kapatınca sohbet kendi görünümüne döner, arama
    /// panelde sürer).
    pub(super) fn call_ui_expand(&mut self, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        if on {
            self.open_call_view(window, cx);
        } else {
            self.call_ui.expanded = false;
        }
        crate::trace(if on { "arama: panel büyütüldü" } else { "arama: panel küçültüldü" });
        cx.notify();
    }

    /// Açık sohbet bu bireysel / grup aramasının sohbeti mi.
    fn call_chat_matches(&self, call: &CallState, cx: &App) -> bool {
        let Some(chat) = &self.chat else { return false };
        match (&chat.read(cx).subject, call.kind.as_str()) {
            (ChatSubject::Direct(m), "direct") => counterpart(call, &self.auth.user.id).as_deref() == Some(m.id.as_str()),
            (ChatSubject::Group(g), "group") => call.group_id.as_deref() == Some(g.id.as_str()),
            _ => false,
        }
    }

    /// Ana alanda gösterilen bireysel / grup araması (görünüm açık ve aramanın sohbeti açıkken).
    fn call_room(&self, cx: &App) -> Option<CallState> {
        if !self.call_ui.expanded {
            return None;
        }
        let call = self.calls.read(cx).call.clone().filter(|c| c.kind != "channel")?;
        self.call_chat_matches(&call, cx).then_some(call)
    }

    /// Sahnedeki oda: açık sesli oda ya da arama görünümü.
    fn current_room(&self, cx: &App) -> Option<Room> {
        if let Some(channel) = self.voice_channel(cx) {
            let calls = self.calls.read(cx);
            return Some(Room {
                name: channel.name.clone(),
                people: calls.voice_rooms.get(&channel.id).cloned().unwrap_or_default(),
                pending: Vec::new(),
                joined: calls.call.as_ref().and_then(|c| c.channel_id.as_deref()) == Some(channel.id.as_str()),
                channel: Some(channel),
            });
        }
        let call = self.call_room(cx)?;
        let me = &self.auth.user.id;
        let pending = call
            .invited
            .iter()
            .filter(|u| *u != me && !call.has(u))
            .map(|u| CallParticipant { user_id: u.clone(), display_name: self.person_name(u), ..Default::default() })
            .collect();
        // Bireysel aramada karşı taraf henüz katılmadıysa o da "çalıyor".
        let mut pending: Vec<CallParticipant> = pending;
        if call.kind == "direct" {
            if let Some(other) = counterpart(&call, me).filter(|o| !call.has(o) && !pending.iter().any(|p| &p.user_id == o)) {
                pending.push(CallParticipant { display_name: self.person_name(&other), user_id: other, ..Default::default() });
            }
        }
        Some(Room { name: self.call_title(&call), people: call.participants.clone(), pending, joined: true, channel: None })
    }

    /// Arama başladı / bitti (arama denetçisi her değiştiğinde çağrılır). Bireysel ve grup aramasında
    /// arama görünümü kendiliğinden açılır (sesli oda gibi ana alanı kaplar): aramanın sohbeti açık
    /// değilse ona geçilir. Kullanıcı başka yere geçebilir; arama panelde sürer.
    pub(super) fn call_ui_track(&mut self, cx: &mut Context<Self>) {
        let current = self.calls.read(cx).call.as_ref().map(|c| (c.id.clone(), c.kind.clone()));
        let id = current.as_ref().map(|c| c.0.clone());
        if id == self.call_ui.tracked {
            return;
        }
        self.call_ui.tracked = id;
        match current {
            Some((_, kind)) if kind != "channel" => {
                self.call_ui.expanded = true;
                self.call_ui.pending_open = true;
                cx.notify();
            }
            _ => {
                self.call_ui.expanded = false;
                self.call_ui.pending_open = false;
                self.call_ui.invite = false;
            }
        }
    }

    /// Aramanın görünen adı: kanal adı, karşı taraf ya da grup (overlay_ui.rs kullanır).
    pub(super) fn call_title(&self, call: &CallState) -> String {
        match call.kind.as_str() {
            "channel" => {
                let id = call.channel_id.clone().unwrap_or_default();
                let name = self.categories.iter().flat_map(|c| c.channels.iter()).find(|c| c.id == id).map(|c| c.name.clone()).unwrap_or_else(|| p2_i18n::t("callui.voiceRoomFallback"));
                format!("🔊 {name}")
            }
            "group" => {
                let id = call.group_id.clone().unwrap_or_default();
                self.group_by_id(&id).map(|g| format!("👥 {}", g.name)).unwrap_or_else(|| p2_i18n::t("callui.groupCallFallback"))
            }
            _ => {
                let me = &self.auth.user.id;
                let other = call.participants.iter().find(|p| &p.user_id != me).map(|p| p.display_name.clone()).or_else(|| {
                    let id = if call.caller.as_deref() == Some(me.as_str()) { call.callee.clone() } else { call.caller.clone() };
                    id.and_then(|id| self.find_member(&id)).map(|m| m.display_name)
                });
                other.unwrap_or_else(|| p2_i18n::t("callui.directCall"))
            }
        }
    }

    /// Kişinin görünen adı (proje kişileri; yoksa "Katılımcı").
    fn person_name(&self, user: &str) -> String {
        self.find_member(user).map(|m| m.display_name).unwrap_or_else(|| p2_i18n::t("callui.participant"))
    }

    // ------------------------------------------------------------ sohbet bağlantıları

    /// Sohbet açıldı: sesli odaysa başlığı çalışma alanı çizdiği için sohbet değişince yenile.
    pub(super) fn call_ui_chat_opened(&mut self, cx: &mut Context<Self>) {
        self.call_ui.chat_watch = None;
        self.call_ui.menu = None;
        self.call_ui.cards_hidden = false;
        // Sohbet yan paneli varsayılan kapalı; açık/kapalı seçimi oturum boyunca korunur (başlıktaki
        // "Sohbeti göster/gizle").
        self.sync_room_chat(cx);
    }

    /// Sohbet sesli odada / arama görünümünde gömülü (başlığı çalışma alanı çizer); değilse kendi
    /// başlığıyla. Arama başlayıp bitince aynı sohbet için değişir.
    fn sync_room_chat(&mut self, cx: &mut Context<Self>) {
        let Some(chat) = self.chat.clone() else { return };
        let embedded = self.voice_channel(cx).is_some() || self.call_room(cx).is_some();
        if chat.read(cx).embedded != embedded {
            chat.update(cx, |c, cx| {
                c.embedded = embedded;
                cx.notify();
            });
        }
        if embedded && self.call_ui.chat_watch.is_none() {
            self.call_ui.chat_watch = Some(cx.observe(&chat, |_, _, cx| cx.notify()));
        } else if !embedded {
            self.call_ui.chat_watch = None;
        }
    }

    pub(super) fn toggle_voice_chat(&mut self, cx: &mut Context<Self>) {
        self.call_ui.voice_chat = !self.call_ui.voice_chat;
        cx.notify();
    }

    /// Esc: açılır liste → kişi menüsü → kalem (çizimi bitir) → kişi ekle listesi.
    pub(super) fn call_ui_escape(&mut self, cx: &mut Context<Self>) -> bool {
        if self.picker.take().is_some() {
            return true;
        }
        if self.call_ui.dropdown.take().is_some() || self.call_ui.menu.take().is_some() {
            return true;
        }
        if let Some(target) = self.call_pen.as_ref().map(|p| p.target.clone()) {
            self.pen_up(cx);
            self.toggle_pen(&target, cx);
            return true;
        }
        std::mem::take(&mut self.call_ui.invite)
    }

    /// Yayını tam ekran aç / kapat: yayın pencerenin tamamını kaplar ve pencere işletim sistemi tam
    /// ekranına alınır (zaten tam ekransa dokunulmaz). Düğme, çift tık ya da Esc kapatır.
    pub(super) fn toggle_stream_fullscreen(&mut self, user: &str, own: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.call_ui.fullscreen.as_ref().is_some_and(|f| f.0 == user) {
            self.exit_stream_fullscreen(window, cx);
            return;
        }
        let screen = self.voice_room_people(cx).iter().find(|x| x.user_id == user).map(|x| x.screen).unwrap_or(true);
        let label = match (own, screen) {
            (true, true) => p2_i18n::t("callui.youScreen"),
            (true, false) => p2_i18n::t("callui.you"),
            (false, true) => p2_i18n::tv("callui.screenLabel", &[("name", &self.person_name(user))]),
            (false, false) => self.person_name(user),
        };
        self.call_ui.fullscreen = Some((user.to_string(), label, own));
        let focus = self.call_ui.fs_focus.get_or_insert_with(|| cx.focus_handle()).clone();
        window.focus(&focus, cx);
        if !window.is_fullscreen() {
            window.toggle_fullscreen();
            self.call_ui.os_fullscreen = true;
        }
        self.call_ui.hover = None;
        crate::trace(&format!("arama: {} yayını tam ekran", &user[..8.min(user.len())]));
        cx.notify();
    }

    /// Esc: açık açılır liste / kişi menüsü / kalem yoksa tam ekran yayından çıkar.
    pub(super) fn call_ui_fullscreen_escape(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.call_ui.fullscreen.is_none() || self.picker.is_some() || self.call_ui.dropdown.is_some() || self.call_ui.menu.is_some() || self.call_pen.is_some() {
            return false;
        }
        self.exit_stream_fullscreen(window, cx)
    }

    /// Tam ekrandan çık (pencereyi tam ekrana ben aldıysam geri alınır). Tam ekran yoksa `false`.
    pub(super) fn exit_stream_fullscreen(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.call_ui.fullscreen.take().is_none() {
            return false;
        }
        if std::mem::take(&mut self.call_ui.os_fullscreen) && window.is_fullscreen() {
            window.toggle_fullscreen();
        }
        self.call_ui.hover = None;
        crate::trace("arama: tam ekrandan çıkıldı");
        cx.notify();
        true
    }

    /// Tam ekran yayın katmanı (çalışma alanının tamamı; kontroller üzerine gelince alttaki çubukta).
    /// Yayın bittiyse / izlenmiyorsa kendiliğinden kapanır.
    pub(super) fn stream_fullscreen_view(&self, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (user, label, own) = self.call_ui.fullscreen.clone()?;
        let alive = {
            let calls = self.calls.read(cx);
            calls.videos.contains_key(&user) || calls.rewinds.contains_key(&user) || (own && calls.screen_on)
        };
        if !alive {
            cx.defer_in(window, |this, window, cx| {
                this.exit_stream_fullscreen(window, cx);
            });
            return None;
        }
        let size = window.viewport_size();
        let (w, h) = (f32::from(size.width), (f32::from(size.height) - super::super::chat::view::ROOT_TOP).max(100.));
        Some(
            div()
                .id("stream-fullscreen")
                .when_some(self.call_ui.fs_focus.as_ref(), |s, f| s.track_focus(f))
                .absolute()
                .inset_0()
                .bg(gpui_kit::black())
                .occlude()
                .child(self.stream_player(&user, &label, own, w, h, true, p, cx))
                .into_any_element(),
        )
    }

    /// Açık sohbet bir sesli odaysa kanalı.
    fn voice_channel(&self, cx: &App) -> Option<Channel> {
        let chat = self.chat.as_ref()?;
        match &chat.read(cx).subject {
            ChatSubject::Channel(c) if c.kind == ChannelType::Voice => Some(c.clone()),
            _ => None,
        }
    }

    /// Başlığın arama durumu (`.tool.call-active`, sohbet düğmesi).
    fn sync_call_head(&self, cx: &mut Context<Self>) {
        let Some(chat) = self.chat.clone() else { return };
        // DM / grup sohbetinde de (arama görünümü) sohbet düğmesinin durumu eşitlenir.
        let channel = match &chat.read(cx).subject {
            ChatSubject::Channel(c) => Some(c.id.clone()),
            _ => None,
        };
        let in_call = channel.is_some() && self.calls.read(cx).call.as_ref().and_then(|c| c.channel_id.as_deref()) == channel.as_deref();
        let voice_chat = self.call_ui.voice_chat;
        chat.update(cx, |c, cx| {
            if c.head.in_call != in_call || c.head.voice_chat != voice_chat {
                c.head.in_call = in_call;
                c.head.voice_chat = voice_chat;
                cx.notify();
            }
        });
    }

    /// Ana alan: sesli oda açıksa başlık + `.voice-room-shell` (sahne [+ oda sohbeti]).
    pub(super) fn voice_main(&mut self, chat_style: &StyleRefinement, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.sync_call_head(cx);
        // Arama yeni başladı, sohbeti açık değil: aramanın sohbetine geç (pencere burada var).
        if std::mem::take(&mut self.call_ui.pending_open) {
            cx.defer_in(window, |this, window, cx| this.open_call_view(window, cx));
        }
        self.sync_room_chat(cx);
        let room = self.current_room(cx)?;
        let chat: Entity<ChatPane> = self.chat.clone()?;
        let pp = p.clone();
        let header = chat.update(cx, |c, cx| c.header_element(&pp, cx));
        let stage = self.voice_stage(&room, p, window, cx);
        let in_call = room.channel.is_none();
        let chat_w = self.voice_chat_width(window);
        let aside = self.call_ui.voice_chat.then(|| {
            let name = if in_call { room.name.clone() } else { format!("#{}", room.name) };
            let (soft, strong) = (p.accent_soft, p.text_strong);
            // ChannelContent.tsx `useResizable('p2:voice-chat-width',430,300,980,-1)`. Dar pencerede panel
            // sahneye yer bırakacak kadar daralır (bkz. `voice_chat_width`); hiçbir şey ekran dışına taşmaz.
            div()
                .relative()
                .w(px(chat_w))
                .min_w(px(CHAT_MIN_W.min(chat_w)))
                .max_w(gpui_kit::relative(0.8))
                .h_full()
                .flex_none()
                .flex()
                .flex_col()
                .border_l_1()
                .border_color(p.border_subtle)
                .bg(p.bg_1)
                .p2_enter("voice-room-chat", Entrance::SlideRight, Duration::ZERO)
                .child(
                    div()
                        .h(px(48.))
                        .flex_none()
                        .pl(px(15.))
                        .pr(px(12.))
                        .flex()
                        .items_center()
                        .justify_between()
                        .border_b_1()
                        .border_color(p.border_subtle)
                        .child(div().flex().flex_col().gap(px(1.)).child(div().text_size(fs(13.)).css_weight(700.).text_color(p.text_strong).child(p2_i18n::t(if in_call { "callui.callChat" } else { "callui.roomChat" }))).child(div().text_size(fs(10.)).text_color(p.text_3).child(name)))
                        .child(
                            div()
                                .id("voice-chat-close")
                                .size(px(30.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(6.))
                                .text_size(fs(19.))
                                .text_color(p.text_3)
                                .cursor_pointer()
                                .hover(move |s| s.bg(soft).text_color(strong))
                                .child("×")
                                .on_click(cx.listener(|this, _, _, cx| this.toggle_voice_chat(cx))),
                        ),
                )
                .child(div().flex_1().min_h_0().flex().flex_col().child(chat.clone().cached(chat_style.clone())).child(chat.read(cx).composer_bar(p, chat.downgrade(), window, cx)))
                .child(crate::resize::handle("voice-chat-resize", crate::resize::VOICE_CHAT, crate::resize::Edge::Left, p2_i18n::t("callui.roomChatWidth"), p))
        });
        Some(
            div()
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .flex_col()
                .bg(p.bg_0)
                .child(header)
                .child(div().flex_1().min_h_0().flex().overflow_hidden().bg(p.bg_0).child(stage).children(aside))
                .into_any_element(),
        )
    }

    /// Sohbet yan panelinin genişliği: kayıtlı (sürüklenen) genişlik, ama sahneye en az
    /// `STAGE_MIN_W` kalacak şekilde sınırlı (en az `CHAT_MIN_W`).
    fn voice_chat_width(&self, window: &Window) -> f32 {
        let avail = f32::from(window.viewport_size().width) - 72. - crate::resize::value(&crate::resize::SIDEBAR);
        let want = crate::resize::value(&crate::resize::VOICE_CHAT);
        want.min(avail - STAGE_MIN_W).max(CHAT_MIN_W).min((avail * 0.8).max(160.))
    }

    // ------------------------------------------------------------ sesli oda sahnesi

    /// Sahnedeki kişiler (açık sesli oda ya da arama görünümü).
    fn voice_room_people(&self, cx: &App) -> Vec<CallParticipant> {
        self.current_room(cx).map(|r| r.people).unwrap_or_default()
    }

    /// Izgara: `repeat(auto-fit, minmax(260px,1fr))`, `grid-auto-rows: minmax(190px,1fr)`, aralık 12.
    /// Dönüş: (sütun, sütun genişliği, satır yüksekliği).
    fn room_grid(n: usize, w: f32, h: f32) -> (usize, f32, f32) {
        let gap = 12.;
        let fit = (((w + gap) / (260. + gap)).floor() as usize).max(1);
        let cols = n.clamp(1, fit);
        let rows = n.div_ceil(cols).max(1);
        let col_w = (w - gap * (cols as f32 - 1.)) / cols as f32;
        let row_h = ((h - gap * (rows as f32 - 1.)) / rows as f32).max(190.);
        (cols, col_w, row_h)
    }

    /// Kartın pencere koordinatındaki ortası (otomatik pilot menüsü için).
    fn card_center(&self, ix: usize, w: f32, h: f32, cx: &App) -> (f32, f32) {
        let n = self.voice_room_people(cx).len();
        let joined = self.voice_joined(cx);
        let (inner_w, inner_h) = (w - 56., h - 56. - 16. - if joined { BAR_H } else { 39. });
        let (cols, col_w, row_h) = Self::room_grid(n, inner_w, inner_h);
        let (c, r) = (ix % cols, ix / cols);
        let x = 28. + c as f32 * (col_w + 12.) + col_w / 2.;
        let y = 28. + r as f32 * (row_h + 12.) + row_h / 2.;
        let origin = self.call_ui_stage_origin();
        (origin.0 + x, origin.1 + y)
    }

    /// Sahnenin pencere içi sol üstü (ray 72 + kenar çubuğu `p2:sidebar-width`, başlık 38 + 58).
    fn call_ui_stage_origin(&self) -> (f32, f32) {
        (72. + crate::resize::value(&crate::resize::SIDEBAR), 96.)
    }

    fn voice_joined(&self, cx: &App) -> bool {
        self.current_room(cx).is_some_and(|r| r.joined)
    }

    /// `VoiceRoomStage` (sesli oda ve bireysel / grup aramasının görünümü).
    fn voice_stage(&self, room: &Room, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let calls = self.calls.read(cx);
        let people = room.people.clone();
        let joined = room.joined;
        let can_connect = room.channel.as_ref().is_none_or(|c| self.my_perms.can_in(&c.id, "voice.connect"));
        let me = self.auth.user.id.clone();
        let size = self.call_ui.stage_size.clone();
        let measured_est = self.call_ui.stage_est.clone();
        // Sahne boyutunun pencere + panel genişliklerinden tahmini (aynı karede hesaplanır).
        let est = {
            let vp = window.viewport_size();
            (f32::from(vp.width) - 72. - crate::resize::value(&crate::resize::SIDEBAR) - if self.call_ui.voice_chat { self.voice_chat_width(window) } else { 0. }, f32::from(vp.height) - 96.)
        };
        let (sw, sh) = {
            let (w, h) = size.get();
            if w > 10. {
                // Sahne (flex-1, h-full) pencere ve panel genişlikleriyle birebir değişir; ölçüm bir önceki
                // kareden geldiği için tahmindeki farkı ekle. Yoksa boyutlandırmada her kare önce eski
                // boyutla çizilip ardından düzeliyor (kartlar ve zemin titriyor).
                let (ew, eh) = measured_est.get();
                ((w + est.0 - ew).max(120.), (h + est.1 - eh).max(1.))
            } else {
                est
            }
        };
        // Sahnenin boyutu ölçülür; bu karede kullanılan boyuttan farklıysa yeniden çizilir (ızgara + radyal zeminler).
        let measure = canvas(
            move |bounds, window, _| {
                let s = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                size.set(s);
                measured_est.set(est);
                if (s.0 - sw).abs() > 0.5 || (s.1 - sh).abs() > 0.5 {
                    window.refresh();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0();
        let join_id = room.channel.as_ref().map(|c| c.id.clone()).unwrap_or_default();
        let join = move |p: &Palette, cx: &mut Context<Self>| {
            let id = join_id.clone();
            div()
                .id("voice-room-join")
                .flex_none()
                .h(px(39.))
                .px(px(18.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(8.))
                .bg(p.accent_grad.background())
                .shadow(shadows(&[p.accent_glow_strong, p.inset_top]))
                .text_color(p.text_on_accent)
                .text_size(fs(12.))
                .css_weight(750.)
                .cursor_pointer()
                .hover(|s| s.top(px(-1.)))
                .when(!can_connect, |s| s.opacity(0.55))
                .child(p2_i18n::t("callui.joinRoom"))
                .on_click(cx.listener(move |this, _, window, cx| this.calls.update(cx, |c, cx| c.join_channel(&id, window, cx))))
        };

        if people.is_empty() {
            return div()
                .relative()
                .flex_1()
                .min_w_0()
                .h_full()
                .overflow_hidden()
                .child(empty_room_background(sw.max(1.), sh.max(1.), p).absolute().top_0().left_0())
                .child(measure)
                .child(
                    div().absolute().inset_0().p(px(24.)).flex().items_center().justify_center().child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(9.))
                            .p2_enter("voice-room-empty", Entrance::Rise, Duration::ZERO)
                            .child(div().size(px(42.)).flex().items_center().justify_center().child(icon("channel:voice").stroke(1.8).el().size(px(27.)).text_color(p.text_accent)))
                            .child(div().text_size(fs(24.)).css_weight(700.).text_color(p.text_strong).child(room.name.clone()))
                            .child(div().mb(px(10.)).text_size(fs(12.)).text_color(p.text_3).child(p2_i18n::t("callui.roomEmpty")))
                            .when(!joined, |s| s.child(join(p, cx))),
                    ),
                )
                .into_any_element();
        }

        // Sunum düzeni (Meet): biri ekran paylaşıyorsa (kendi yayınım da, yerel önizleme) paylaşım ana
        // alanı alır, kişiler alttaki kompakt şeritte. Birden çok paylaşımda seçilen (kartına tıklanan)
        // ya da en son başlayan büyük, diğerleri şeritte. İzlenen kamera da paylaşım yokken büyür.
        let sharing_of = |x: &CallParticipant| x.screen || (x.user_id == me && calls.screen_on);
        let sharers: Vec<String> = people.iter().filter(|x| sharing_of(x)).map(|x| x.user_id.clone()).collect();
        let latest = {
            let mut order = self.call_ui.share_order.borrow_mut();
            order.retain(|u| sharers.contains(u));
            for u in &sharers {
                if !order.contains(u) {
                    order.push(u.clone());
                }
            }
            order.last().cloned()
        };
        let watched_cam = people
            .iter()
            .find(|x| !sharing_of(x) && calls.watching.contains(&x.user_id) && (x.video || x.user_id == me) && calls.videos.contains_key(&x.user_id))
            .map(|x| x.user_id.clone());
        let pinned = self.call_ui.pinned.clone().filter(|u| sharers.contains(u));
        let main: Option<(String, String, bool, (f32, f32))> = pinned.or(latest).or(watched_cam).and_then(|u| {
            let x = people.iter().find(|x| x.user_id == u)?;
            let own = u == me;
            let label = match (own, sharing_of(x)) {
                (true, true) => p2_i18n::t("callui.youScreen"),
                (true, false) => p2_i18n::t("callui.you"),
                (false, true) => p2_i18n::tv("callui.screenLabel", &[("name", &x.display_name)]),
                (false, false) => x.display_name.clone(),
            };
            // Yayının gerçek en-boy oranı (geri sarılıyorsa kayıttaki kare); kare yoksa 16:9.
            let dims = calls.rewinds.get(&u).and_then(|v| v.image.as_ref().map(|i| (i.1, i.2))).or_else(|| calls.videos.get(&u).map(|v| (v.1, v.2))).filter(|d| d.0 > 0 && d.1 > 0).map(|d| (d.0 as f32, d.1 as f32)).unwrap_or((16., 9.));
            Some((u, label, own, dims))
        });
        // Büyüyen başkasının yayınıysa yayıncıya "izliyorum" gider (tam kalite); bir kez, sonra kullanıcıya kalır.
        if let Some((u, _, own, _)) = &main {
            let mut auto = self.call_ui.auto_watched.borrow_mut();
            if !own && auto.as_deref() != Some(u.as_str()) {
                *auto = Some(u.clone());
                if !calls.watching.contains(u) {
                    let u = u.clone();
                    cx.defer_in(window, move |this, _, cx| {
                        if !this.calls.read(cx).watching.contains(&u) {
                            this.calls.update(cx, |c, cx| c.watch(Some(&u), cx));
                        }
                    });
                }
            }
        }
        let featured = main.is_some();
        let cards_open = !featured || !self.call_ui.cards_hidden;
        // Kartlar: katılımcılar + çalan (henüz katılmamış) kişiler.
        let cards: Vec<(CallParticipant, bool)> = people.iter().map(|x| (x.clone(), false)).chain(room.pending.iter().map(|x| (x.clone(), true))).collect();

        // Dar sahnede kenar boşlukları küçülür.
        let pad = if featured || sw < 520. { 12. } else { 28. };
        let (inner_w, inner_h) = (sw - pad * 2., sh - pad * 2.);
        // Alt çubuk: dar sahnede yayın ayarları ikinci satıra iner, düğmeler küçülür.
        let (bar, bar_h) = if joined { self.call_bar(room.channel.is_none(), inner_w, p, cx) } else { (div().into_any_element(), 39.) };
        // `grid-template-rows: minmax(0,1fr) auto` + 16 aralık: ikinci satır katılmadıysa 39px düğme,
        // katıldıysa alt kontrol çubuğu.
        let grid_h = inner_h - 16. - bar_h;
        // Sunum şeridi: tek sıra 16:9 küçük kutucuklar (sığmazsa yatay kaydırılır).
        const STRIP_W: f32 = 150.;
        const STRIP_H: f32 = 84.;
        let (cols, col_w, row_h) = if featured { (cards.len().max(1), STRIP_W, STRIP_H) } else { Self::room_grid(cards.len(), inner_w, grid_h) };
        let gap = if featured { 7. } else { 12. };

        let mut grid = div().flex().flex_col().gap(px(gap)).flex_none();
        if featured {
            grid = grid.items_center();
        }
        for row in cards.chunks(cols) {
            let mut r = div().flex().gap(px(gap));
            for (part, pending) in row {
                r = r.child(self.voice_card(part, col_w, row_h, featured, *pending, p, cx));
            }
            grid = grid.child(r);
        }

        let mut body = div().relative().flex_1().min_w_0().h_full().flex().flex_col().p(px(pad)).gap(px(if featured { 8. } else { 16. })).overflow_hidden();
        if let Some((user, label, own, (vw, vh))) = &main {
            // Ana alan: kalan yükseklik (şerit düğmesi 26, şerit, alt çubuk ve aralıklar düşülür). Yayın
            // kutusu yayının oranıyla bu alana en büyük haliyle sığar, ortalanır (kenarlar boş kalır).
            let area_h = (inner_h - 26. - 8. - if cards_open { STRIP_H + 8. } else { 0. } - bar_h - 8.).max(60.);
            let area_w = inner_w.max(60.);
            let ratio = vw / vh;
            let bw = area_w.min(area_h * ratio).floor();
            let bh = (bw / ratio).floor();
            body = body.child(div().flex_1().min_h_0().flex().items_center().justify_center().child(self.stream_player(user, label, *own, bw, bh, false, p, cx)));
            let n = cards.len();
            let open = cards_open;
            body = body.child(
                div().flex_none().flex().justify_center().child(
                    div()
                        .id("cards-toggle")
                        .h(px(26.))
                        .px(px(12.))
                        .flex()
                        .items_center()
                        .gap(px(7.))
                        .rounded(px(99.))
                        .border_1()
                        .border_color(p.border)
                        .bg(p.bg_3)
                        .text_color(p.text_2)
                        .text_size(fs(10.5))
                        .css_weight(650.)
                        .cursor_pointer()
                        .hover({
                            let (bg, fg) = (p.bg_hover, p.text_strong);
                            move |s| s.bg(bg).text_color(fg)
                        })
                        .child(chevron(open, p.text_2))
                        .child(if open { p2_i18n::t("callui.hideParticipants") } else { p2_i18n::tv("callui.participantsCount", &[("n", &n.to_string())]) })
                        .tooltip(tip(if open { p2_i18n::t("callui.hideParticipants") } else { p2_i18n::t("callui.showParticipants") }))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.call_ui.cards_hidden = !this.call_ui.cards_hidden;
                            cx.notify();
                        })),
                ),
            );
            if cards_open {
                body = body.child(div().flex_none().w(px(inner_w)).flex().justify_center().child(div().id("call-strip").max_w(px(inner_w)).overflow_x_scroll().child(grid)));
            }
        } else {
            // Kutucuklar sığmazsa (dar/alçak pencere) alt alta dizilir ve dikey kaydırılır.
            let rows = cards.len().div_ceil(cols.max(1)) as f32;
            let fits = rows * row_h + (rows - 1.).max(0.) * gap <= grid_h + 0.5;
            body = body.child(div().id("call-grid").h(px(grid_h.max(0.))).flex_none().flex().flex_col().when(fits, |s| s.justify_center()).overflow_y_scroll().child(grid));
        }
        if joined {
            body = body.child(bar);
        } else {
            body = body.child(div().flex_none().flex().justify_center().child(join(p, cx)));
        }
        div().relative().flex_1().min_w_0().h_full().child(measure).child(body).into_any_element()
    }

    /// Sahnenin alt kontrol çubuğu (Meet tarzı): mikrofon, kulaklık, kamera, ekran (+ yayın ayarları),
    /// kişi ekle (aramada), ayrıl (sohbeti göster/gizle başlıkta). `avail`: sahnenin iç genişliği —
    /// sığmazsa yayın ayarları ikinci satıra iner, düğmeler küçülür. Dönüş: (çubuk, yükseklik).
    fn call_bar(&self, in_call: bool, avail: f32, p: &Palette, cx: &mut Context<Self>) -> (AnyElement, f32) {
        let calls = self.calls.read(cx);
        let (muted, deafened, camera, screen) = (calls.muted, calls.deafened, calls.camera_on, calls.screen_on);
        let invite_open = self.call_ui.invite;
        let n = 4. + if in_call { 1. } else { 0. };
        let base_w = n * 44. + 58. + n * 8.;
        let compact = avail < base_w;
        let (bw, hw, gap) = if compact { (36., 44., 4.) } else { (44., 58., 8.) };
        let button = move |id: &'static str, name: &'static str, tip_text: String, state: u8, p: &Palette| {
            // state: 0 normal, 1 etkin, 2 kırmızı (kapalı mikrofon / ayrıl)
            let (bg, border, fg) = match state {
                1 => (p.accent_soft_2, p.border_accent, p.text_strong),
                2 => (p.danger_soft, p.danger_border, p.danger_text),
                _ => (p.bg_3, p.border, p.text_2),
            };
            let hangup = id == "bar-hangup";
            let (hbg, hborder, hfg) = if hangup { (p.danger, p.danger, gpui_kit::white()) } else if state == 0 { (p.bg_hover, p.border_strong, p.text_strong) } else { (bg, border, fg) };
            // "takım:ad" çizgi ikonu (sohbet), değilse arama ikonu.
            let base = if name.contains(':') { icon(name).stroke(1.8).el().size(px(19.)).text_color(fg) } else { call_icon(name, 19., fg) };
            let svg = base.group_hover(id, move |s| s.text_color(hfg));
            let svg = if hangup { svg.with_transformation(gpui_kit::Transformation::rotate(gpui_kit::radians(133f32.to_radians()))) } else { svg };
            div()
                .id(id)
                .group(id)
                .flex_none()
                .w(px(if hangup { hw } else { bw }))
                .h(px(40.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(12.))
                .border_1()
                .border_color(border)
                .bg(if hangup { p.danger } else { bg })
                .cursor_pointer()
                .hover(move |s| s.bg(hbg).border_color(hborder).top(px(-1.)))
                .child(if hangup { call_icon(name, 19., gpui_kit::white()).with_transformation(gpui_kit::Transformation::rotate(gpui_kit::radians(133f32.to_radians()))) } else { svg })
                .tooltip(tip(tip_text))
        };
        let mut bar = div()
            .id("call-bar")
            .flex_none()
            .h(px(BAR_H))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(gap))
            .child(button("bar-mute", if muted { "micOff" } else { "mic" }, if muted { p2_i18n::t("callui.unmute") } else { p2_i18n::t("callui.mute") }, if muted { 2 } else { 0 }, p).on_click(cx.listener(|this, _, _, cx| this.calls.update(cx, |c, cx| c.toggle_mute(cx)))))
            .child(button("bar-deafen", if deafened { "headphonesOff" } else { "headphones" }, if deafened { p2_i18n::t("callui.undeafen") } else { p2_i18n::t("callui.deafen") }, if deafened { 2 } else { 0 }, p).on_click(cx.listener(|this, _, _, cx| this.calls.update(cx, |c, cx| c.toggle_deafen(cx)))))
            .child(button("bar-camera", if camera { "camera" } else { "cameraOff" }, if camera { p2_i18n::t("callui.cameraOff") } else { p2_i18n::t("callui.cameraOn") }, if camera { 1 } else { 0 }, p).on_click(cx.listener(|this, _, _, cx| this.calls.update(cx, |c, cx| c.toggle_camera(cx)))))
            .child(button("bar-screen", if screen { "screen" } else { "screenOff" }, if screen { p2_i18n::t("callui.stopShare") } else { p2_i18n::t("callui.shareScreen") }, if screen { 1 } else { 0 }, p).on_click(cx.listener(|this, _, window, cx| this.toggle_screen(window, cx))));
        // Yayın ayarları (kalite, kare hızı, bilgisayar sesi, durdur) alt çubukta değil, yayıncının
        // kendi önizlemesinin çubuğunda; çubukta yalnız paylaşım düğmesi kalır.
        if in_call {
            bar = bar.child(button("bar-invite", "personAdd", p2_i18n::t("callui.addPerson"), if invite_open { 1 } else { 0 }, p).on_click(cx.listener(|this, _, _, cx| {
                this.call_ui.invite = !this.call_ui.invite;
                cx.notify();
            })));
        }
        // Sohbeti göster/gizle başlıkta (arama düğmesinin yanında); çubukta tekrar yok.
        let bar = bar.child(button("bar-hangup", "hangup", p2_i18n::t("callui.leave"), 2, p).on_click(cx.listener(|this, _, window, cx| this.calls.update(cx, |c, cx| c.leave(window, cx)))));
        (bar.into_any_element(), BAR_H)
    }

    /// Açılır liste tetikleyicisi (seçim penceresinin `select_box` görünümü, alt çubuk boyunda):
    /// tıklanınca liste kutunun hemen altında açılır, yeniden tıklanınca kapanır.
    fn dropdown_trigger(&self, id: String, key: String, label: impl IntoElement, width: f32, tip_text: String, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let open = self.call_ui.dropdown.as_ref().is_some_and(|d| d.1 == id);
        let (bounds_map, bounds_id, anchor_id) = (self.call_ui.dropdown_bounds.clone(), id.clone(), id.clone());
        super::screen_picker::select_box(SharedString::from(id), label, width, p)
            // Kutunun pencere içindeki sınırları her çizimde ölçülür (liste buna göre konumlanır).
            .child(canvas(move |bounds, _, _| { bounds_map.borrow_mut().insert(bounds_id.clone(), bounds); }, |_, _, _, _| {}).absolute().inset_0())
            .h(px(28.))
            .text_size(fs(10.5))
            .css_weight(650.)
            .when(open, |s| s.border_color(p.border_accent).bg(p.accent_soft_2))
            .hover({
                let (accent, strong) = (p.border_accent, p.text_strong);
                move |s| s.border_color(accent).text_color(strong)
            })
            .tooltip(tip(tip_text))
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &gpui_kit::MouseDownEvent, _, cx| {
                cx.stop_propagation();
                let _ = ev;
                this.call_ui.dropdown = if this.call_ui.dropdown.as_ref().is_some_and(|d| d.1 == anchor_id) { None } else { Some((key.clone(), anchor_id.clone())) };
                cx.notify();
            }))
            .into_any_element()
    }

    /// Açık açılır listenin seçenekleri: (etiket, seçili mi, seçim).
    fn dropdown_items(&self, key: &str, p: &Palette, cx: &App) -> Vec<(AnyElement, bool, String)> {
        let calls = self.calls.read(cx);
        let (fps, quality) = calls.share_settings.clone().unwrap_or((30, "1080p".into()));
        match key {
            "share-quality" => QUALITIES.iter().map(|q| (quality_el(q, p).into_any_element(), *q == quality, q.to_string())).collect(),
            "share-fps" => SHARE_FPS.iter().map(|f| (format!("{f} FPS").into_any_element(), *f == fps, f.to_string())).collect(),
            k => match k.strip_prefix("view-quality:") {
                Some(user) => {
                    let cap = calls.stream_quality.get(user).map(String::as_str);
                    let current = view_level_shown(calls.viewer_quality.get(user).map(String::as_str).unwrap_or("auto"), cap);
                    view_levels(cap).into_iter().map(|q| (quality_el(q, p).into_any_element(), q == current, q.to_string())).collect()
                }
                None => Vec::new(),
            },
        }
    }

    fn dropdown_pick(&mut self, key: &str, value: &str, cx: &mut Context<Self>) {
        match key {
            "share-quality" => {
                if let Some(q) = QUALITIES.iter().copied().find(|q| *q == value) {
                    self.save_media(|m| m.share_quality = q.to_string(), cx);
                    self.calls.update(cx, |c, cx| c.update_share(None, Some(q), None, cx));
                }
            }
            "share-fps" => {
                if let Ok(f) = value.parse::<u32>() {
                    self.save_media(|m| m.share_fps = f, cx);
                    self.calls.update(cx, |c, cx| c.update_share(Some(f), None, None, cx));
                }
            }
            k => {
                if let Some(user) = k.strip_prefix("view-quality:") {
                    let user = user.to_string();
                    let cap = self.calls.read(cx).stream_quality.get(&user).cloned();
                    if let Some(level) = view_levels(cap.as_deref()).into_iter().find(|q| *q == value) {
                        self.calls.update(cx, |c, cx| c.request_quality(&user, level, cx));
                    }
                }
            }
        }
        self.call_ui.dropdown = None;
        cx.notify();
    }

    /// Açık açılır liste (seçim penceresindeki listeyle aynı görünüm), kök düzeyde çizilir.
    pub(super) fn call_dropdown_view(&self, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (key, anchor_id) = self.call_ui.dropdown.clone()?;
        let items = self.dropdown_items(&key, p, cx);
        let anchor = self.call_ui.dropdown_bounds.borrow().get(&anchor_id).copied();
        let (Some(anchor), false) = (anchor, items.is_empty()) else { return None };
        let vp = window.viewport_size();
        let (vw, vh) = (f32::from(vp.width), f32::from(vp.height));
        let (ax, ay, aw, ah) = (f32::from(anchor.origin.x), f32::from(anchor.origin.y), f32::from(anchor.size.width), f32::from(anchor.size.height));
        let (w, h) = (aw.max(118.), items.len() as f32 * 29. + 12.);
        // Liste kutunun hemen altına, sol kenarı hizalı açılır; altta yer yoksa hemen üstüne.
        let (gap, edge) = (4., 8.);
        let below = ay + ah + gap;
        let top = if below + h <= vh - edge || ay - gap - h < edge { below.min(vh - h - edge).max(edge) } else { ay - gap - h };
        let left = ax.min(vw - w - edge).max(edge);
        let mut list = div().w(px(w)).p(px(5.)).flex().flex_col().gap(px(1.)).rounded(px(11.)).border_1().border_color(p.border_strong).bg(p.glass.background()).backdrop_blur(p2_ui::fx::BLUR).shadow(p.shadow_lg()).occlude();
        for (i, (label, active, value)) in items.into_iter().enumerate() {
            let (soft, strong) = (p.accent_soft, p.text_strong);
            let k = key.clone();
            list = list.child(
                div()
                    .id(SharedString::from(format!("call-opt-{i}")))
                    .h(px(28.))
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .rounded(px(6.))
                    .text_size(fs(11.))
                    .text_color(p.text)
                    .when(active, |s| s.bg(p.bg_selected).text_color(p.text_strong))
                    .hover(move |s| s.bg(soft).text_color(strong))
                    .cursor_pointer()
                    .child(label)
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |this, _, _, cx| this.dropdown_pick(&k, &value, cx))),
            );
        }
        Some(
            div()
                .id("call-dropdown-layer")
                .absolute()
                .inset_0()
                // Dışarı tıklama kapatır; tetikleyicinin üstündeki tıklamayı tetikleyici kendisi işler
                // (yoksa aynı tıklama listeyi hem kapatıp hem yeniden açardı).
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &gpui_kit::MouseDownEvent, _, cx| {
                    if anchor.contains(&ev.position) {
                        return;
                    }
                    this.call_ui.dropdown = None;
                    cx.notify();
                }))
                // Pencere koordinatında konum (üst öğelerin kaymasından bağımsız).
                .child(gpui_kit::anchored().position(gpui_kit::point(px(left), px(top))).child(list.p2_enter(SharedString::from(format!("call-dropdown-in-{anchor_id}")), Entrance::ScaleIn, Duration::ZERO)))
                .into_any_element(),
        )
    }

    /// Yayıncının canlı yayın ayarları: kalite ve kare hızı (açılır listeler), bilgisayar sesi ve durdur.
    /// Seçimler ayarlara da yazılır (sonraki yayın aynı ayarla başlar).
    fn share_controls(&self, prefix: &str, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let calls = self.calls.read(cx);
        let (fps, quality) = calls.share_settings.clone().unwrap_or((30, "1080p".into()));
        let audio = calls.share_audio.is_some();
        let (soft, danger) = (p.danger_soft, p.danger_text);
        let quality_sel = self.dropdown_trigger(format!("{prefix}-share-quality"), "share-quality".into(), quality_el(&quality, p), 84., p2_i18n::t("callui.share.qualityTip"), p, cx);
        let fps_sel = self.dropdown_trigger(format!("{prefix}-share-fps"), "share-fps".into(), format!("{fps} FPS"), 72., p2_i18n::t("callui.share.fpsTip"), p, cx);
        div()
            .flex_none()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(5.))
            .px(px(6.))
            .py(px(4.))
            .rounded(px(10.))
            .border_1()
            .border_color(p.border_subtle)
            .bg(p.bg_2)
            .child(quality_sel)
            .child(fps_sel)
            .child(self.share_audio_toggle(format!("{prefix}-share-audio"), audio, p, cx))
            .child(
                div()
                    .id(SharedString::from(format!("{prefix}-share-stop")))
                    .flex_none()
                    .h(px(28.))
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(p.danger_border)
                    .bg(p.danger_soft)
                    .text_color(p.danger_text)
                    .text_size(fs(10.))
                    .css_weight(750.)
                    .whitespace_nowrap()
                    .cursor_pointer()
                    .hover(move |s| s.bg(soft).text_color(danger).top(px(-1.)))
                    .child(p2_i18n::t("callui.stopShare"))
                    .on_click(cx.listener(|this, _, _, cx| this.calls.update(cx, |c, cx| c.stop_screen(cx)))),
            )
            .into_any_element()
    }

    /// "Bilgisayar sesi" anahtarı (yayıncı): ikon + etiket + `.p2-toggle` görünümünde küçük hap; açılır
    /// listelerle aynı 28px yükseklik. Açıkken vurgu zemini ve kenarlığı.
    fn share_audio_toggle(&self, id: String, on: bool, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let (accent, strong) = (p.border_accent, p.text_strong);
        let knob_shadow = shadows(&[p2_ui::tokens::Shadow::new(0., 1., 3., 0., css_rgba(0, 0, 0, 0.4))]);
        let pill = div()
            .relative()
            .flex_none()
            .w(px(28.))
            .h(px(16.))
            .rounded_full()
            .map(|s| if on { s.bg(p.accent_grad.background()) } else { s.bg(p.navy_500) })
            .child(div().absolute().top(px(2.)).left(px(if on { 14. } else { 2. })).size(px(12.)).rounded_full().bg(gpui_kit::white()).shadow(knob_shadow));
        div()
            .id(SharedString::from(id))
            .flex_none()
            .h(px(28.))
            .pl(px(9.))
            .pr(px(6.))
            .flex()
            .items_center()
            .gap(px(7.))
            .rounded(px(8.))
            .border_1()
            .border_color(if on { p.border_accent } else { p.border })
            .bg(if on { p.accent_soft_2 } else { p.bg_3 })
            .text_color(if on { p.text_strong } else { p.text_2 })
            .text_size(fs(10.5))
            .css_weight(650.)
            .whitespace_nowrap()
            .cursor_pointer()
            .hover(move |s| s.border_color(accent).text_color(strong))
            .child(call_icon(if on { "volume" } else { "volumeOff" }, 14., if on { p.text_accent } else { p.text_3 }))
            .child(p2_i18n::t("callui.share.audio"))
            .child(pill)
            .tooltip(tip(p2_i18n::t(if on { "callui.share.audioOff" } else { "callui.share.audioOn" })))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, _, cx| {
                this.save_media(|m| m.share_audio = !on, cx);
                this.calls.update(cx, |c, cx| c.update_share(None, None, Some(!on), cx));
            }))
            .into_any_element()
    }

    /// `.voice-room-card` (normal ya da `.featured` küçük kart).
    /// `pending`: aranan ama henüz katılmamış kişi ("çalıyor", soluk).
    fn voice_card(&self, part: &CallParticipant, w: f32, h: f32, small: bool, pending: bool, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let calls = self.calls.read(cx);
        let me = &self.auth.user.id;
        let is_me = &part.user_id == me;
        let hue = p.hues[avatar_hue(&part.user_id)];
        let level = if part.muted { 0. } else { calls.levels.get(&part.user_id).copied().unwrap_or(0.) };
        let speaking = level > 0.08;
        // Kutucuktaki görüntü: kamera (kaplar) ya da yayın (tamamı sığar). Kamera kapanınca kare
        // silinir ve kutucuk hemen avatara döner (call.rs `prune_media` / yerel önizleme).
        let sharing = part.screen || (is_me && calls.screen_on);
        let video = calls.videos.get(&part.user_id).filter(|_| part.video || part.screen || (is_me && (calls.camera_on || calls.screen_on))).map(|v| v.0.clone());
        let watchable = video.is_some();
        let watched = calls.watching.contains(&part.user_id);
        let hovered = self.call_ui.hover.as_deref() == Some(part.user_id.as_str());
        let border = if watched { p.accent_hover } else if speaking { hue.fg } else { alpha(hue.fg, 0.14 + level * 0.76) };
        let radius = if small { 9. } else { 14. };
        // background-clip: border-box — gradyan kenarlığın altına da uzanır (yarı saydam kenarlık onun üstünde).
        let bg = radial(w, h, hue.bg, p).rounded(px(radius));
        let uid = part.user_id.clone();
        let (hover_id, menu_id) = (uid.clone(), uid.clone());
        let av = if small { 32. } else { 84. };
        let mut card = div()
            .id(SharedString::from(format!("voice-card-{}", part.user_id)))
            .relative()
            .w(px(w))
            .h(px(h))
            .flex_none()
            .rounded(px(radius))
            .border_2()
            .border_color(border)
            .text_color(hue.fg)
            .cursor_pointer()
            .when(level > 0., |s| {
                s.shadow(vec![gpui_kit::BoxShadow { color: alpha(hue.bg, 0.68), offset: gpui_kit::point(px(0.), px(0.)), blur_radius: px(level * 22.), spread_radius: px(0.), inset: false }])
            })
            .on_hover(cx.listener(move |this, on: &bool, _, cx| {
                let next = on.then(|| hover_id.clone());
                if this.call_ui.hover != next && (*on || this.call_ui.hover.as_deref() == Some(hover_id.as_str())) {
                    this.call_ui.hover = next;
                    cx.notify();
                }
            }))
            .on_mouse_down(MouseButton::Right, cx.listener(move |this, ev: &gpui_kit::MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.open_participant_menu(&menu_id, f32::from(ev.position.x), f32::from(ev.position.y), cx);
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                if sharing {
                    // Sunum düzeni: tıklanan paylaşım büyük gösterilir.
                    this.call_ui.pinned = Some(uid.clone());
                    cx.notify();
                } else if watchable {
                    let target = uid.clone();
                    this.calls.update(cx, |c, cx| c.watch(Some(&target), cx));
                } else {
                    this.open_profile_by_id(&uid, cx);
                }
            }))
            .child(bg.absolute().top(px(-2.)).left(px(-2.)))
            .when(pending, |s| s.opacity(0.62))
            .when_some(video, |s, image| {
                // Görüntü kutucuğun iç sınırlarına kesin boyutla yerleşir (boyutsuz `img` kendi piksel
                // boyutunu alıp sol üste yapışıyordu). Kamera `cover` (ortadan kırpılır, Meet gibi),
                // yayın `contain`; köşeler `img`'in kendi yuvarlaklığıyla kırpılır.
                let (iw, ih) = ((w - 4.).max(1.), (h - 4.).max(1.));
                s.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .w(px(iw))
                        .h(px(ih))
                        .overflow_hidden()
                        .rounded(px(radius - 2.))
                        .bg(p.bg_0)
                        .child(
                            img(ImageSource::Render(image))
                                .absolute()
                                .top_0()
                                .left_0()
                                .w(px(iw))
                                .h(px(ih))
                                .rounded(px(radius - 2.))
                                .object_fit(if sharing { ObjectFit::Contain } else { ObjectFit::Cover }),
                        ),
                )
            })
            .when(!watchable, |s| s.child(
                div().absolute().inset_0().flex().items_center().justify_center().child(
                    div()
                        .size(px(av))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(if small { 11. } else { 28. }))
                        .bg(hue.bg)
                        .text_color(hue.fg)
                        .text_size(fs(if small { 11. } else { 25. }))
                        .line_height(px(av))
                        .css_weight(850.)
                        .shadow(shadows(&[p2_ui::tokens::Shadow { x: 0., y: 10., blur: 28., spread: 0., color: css_rgba(0, 0, 0, 0.25), inset: false }]))
                        .child(p2_ui::components::initials_label(user_initials(&part.display_name))),
                ),
            ));
        // Alt bilgi: ad (+ SEN, YAYINDA) ve susturulmuş simgesi.
        let mut tag = div()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(7.))
            .px(px(if small { 5. } else { 8. }))
            .py(px(if small { 3. } else { 5. }))
            .rounded(px(7.))
            .bg(chip_bg(0.72))
            .child(div().min_w_0().truncate().text_size(fs(if small { 9. } else { 13. })).css_weight(700.).child(if pending { p2_i18n::tv("callui.ringingLabel", &[("name", &part.display_name)]) } else { part.display_name.clone() }));
        if &part.user_id == me {
            tag = tag.child(div().flex_none().text_size(fs(8.)).css_weight(800.).text_color(p.text_accent).child(p2_i18n::t("callui.youTag")));
        }
        // Durum simgeleri (yayında, kamera, mikrofon, kulaklık) ayrı küçük etikette; kendi yayınım da sayılır.
        let shown = CallParticipant { screen: sharing, video: part.video || (is_me && calls.camera_on), ..part.clone() };
        let has_status = shown.screen || shown.video || shown.muted || shown.deafened;
        card = card.child(
            div()
                .absolute()
                .left(px(if small { 6. } else { 11. }))
                .right(px(if small { 6. } else { 11. }))
                .bottom(px(if small { 4. } else { 10. }))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(if small { 4. } else { 10. }))
                .child(tag)
                .when(has_status, |s| s.child(div().flex_none().px(px(if small { 4. } else { 6. })).py(px(if small { 3. } else { 5. })).rounded(px(7.)).bg(chip_bg(0.72)).child(status_icons(&shown, if small { 11. } else { 15. }, p)))),
        );
        // Paylaşımın kartı tıklanınca büyür (sunum düzeni); "İzle" düğmesi yalnız kameralar için.
        if watchable && !sharing && (hovered || watched) {
            let target = part.user_id.clone();
            let (accent, white) = (p.accent, gpui_kit::white());
            card = card.child(
                div().absolute().left_0().right_0().top(px(h / 2. - 2. - 15.)).flex().justify_center().child(
                    div()
                        .id(SharedString::from(format!("watch-{}", part.user_id)))
                        .h(px(30.))
                        .px(px(14.))
                        .flex()
                        .items_center()
                        .rounded(px(11.))
                        .border_1()
                        .border_color(p.border_accent)
                        .bg(css_rgba(14, 22, 38, 0.92))
                        .text_color(p.text_strong)
                        .text_size(fs(11.))
                        .css_weight(700.)
                        .hover(move |s| s.bg(accent).text_color(white))
                        .child(if watched { p2_i18n::t("callui.stopWatching") } else { p2_i18n::t("callui.watch") })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            let target = target.clone();
                            this.calls.update(cx, |c, cx| c.watch(Some(&target), cx));
                        })),
                ),
            );
        }
        card.p2_enter(SharedString::from(format!("voice-card-in-{}", part.user_id)), Entrance::ScaleIn, Duration::ZERO).into_any_element()
    }

    // ------------------------------------------------------------ sunum düzeninin ana yayını

    /// `.stream-player` (StreamPlayer.tsx): görüntü, etiket, çizimler, çubuk (üzerine gelince).
    /// Kutu (`w`×`h`) yayının oranıyla hesaplanır; görüntü kutunun iç sınırlarına kesin boyutla oturur.
    #[allow(clippy::too_many_arguments)]
    /// `full`: tam ekran katmanındaki oynatıcı (kenarlıksız, siyah zemin; kimlikler ayrı).
    #[allow(clippy::too_many_arguments)]
    fn stream_player(&self, user: &str, label: &str, own: bool, w: f32, h: f32, full: bool, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let calls = self.calls.read(cx);
        // Geri sarılıyorsa kayıttan çözülen kare, değilse canlı kare.
        let rewound = calls.rewinds.get(user).and_then(|v| v.image.clone());
        let behind = calls.rewind_info(user).map(|i| i.2).filter(|b| *b > 0.5 || calls.rewinds.contains_key(user));
        let video = rewound.or_else(|| calls.videos.get(user).map(|v| (v.0.clone(), v.1, v.2)));
        let source = video.as_ref().map(|v| (v.1, v.2));
        let pen_here = self.call_pen.as_ref().is_some_and(|pen| pen.target == user);
        let hover_key = if full { format!("stream-full:{user}") } else { format!("stream:{user}") };
        let hovered = self.call_ui.hover.as_deref() == Some(hover_key.as_str());
        let (dbl_user, dbl_own) = (user.to_string(), own);
        // Tek üzerine gelme dinleyicisi (GPUI öğe başına bir tane tutar): çubuğu göster/gizle ve kalem
        // açıksa çıkınca imleci diğerlerinden kaldır. Eskiden kalem dinleyicisi bunu eziyordu, çubuk hiç
        // görünmüyor ve kalemden çıkılamıyordu.
        let mut player = div()
            .id(SharedString::from(if full { format!("stream-full-{user}") } else { format!("stream-{user}") }))
            .relative()
            .flex_none()
            .w(px(w))
            .h(px(h))
            .overflow_hidden()
            .map(|s| if full { s.bg(gpui_kit::black()) } else { s.rounded(px(11.)).border_1().border_color(p.border).bg(p.bg_0) })
            .on_hover(cx.listener(move |this, on: &bool, _, cx| {
                let next = on.then(|| hover_key.clone());
                if !*on && pen_here {
                    this.pen_gone(cx);
                }
                if this.call_ui.hover != next {
                    this.call_ui.hover = next;
                    cx.notify();
                }
            }));
        if let Some((image, _, _)) = video {
            // Görüntü kutunun iç sınırlarına kesin boyutla mutlak yerleşir (boyutsuz `img` kendi piksel
            // boyutunu alıp sol üste yapışıyordu); `contain`: hiç kırpılmaz/ezilmez, oran farkı kenarda kalır.
            let inset = if full { 0. } else { 2. };
            let (iw, ih) = ((w - inset).max(1.), (h - inset).max(1.));
            player = player.child(img(ImageSource::Render(image)).absolute().top_0().left_0().w(px(iw)).h(px(ih)).when(!full, |s| s.rounded(px(10.))).object_fit(ObjectFit::Contain));
        } else if own {
            // Kendi önizlememin ilk karesi henüz gelmedi.
            player = player.child(
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(6.))
                    .bg(p.bg_1)
                    .text_color(p.text_3)
                    .child(call_icon("screen", 26., alpha(p.text_3, 0.7)))
                    .child(div().text_size(fs(12.5)).css_weight(700.).text_color(p.text).child(p2_i18n::t("callui.sharingScreen")))
                    .child(div().text_size(fs(10.)).child(p2_i18n::t("callui.previewOff"))),
            );
        }
        // Çift tık: tam ekran aç / kapat (kalem açıkken çift tık çizimdir).
        if !pen_here {
            player = player.on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &gpui_kit::MouseDownEvent, window, cx| {
                if ev.click_count == 2 {
                    cx.stop_propagation();
                    this.toggle_stream_fullscreen(&dbl_user, dbl_own, window, cx);
                }
            }));
        }
        // Çizim alanı: kalem açıkken fare çizer (StreamPlayer `.stream-canvas.drawing`).
        if pen_here {
            let area = self.call_pen.as_ref().map(|pen| pen.area.clone()).unwrap_or_default();
            player = player
                .cursor_crosshair()
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &gpui_kit::MouseDownEvent, _, cx| this.pen_down(ev.position, source, cx)))
                .on_mouse_move(cx.listener(move |this, ev: &gpui_kit::MouseMoveEvent, _, cx| this.pen_move(ev.position, ev.pressed_button == Some(MouseButton::Left), source, cx)))
                .on_mouse_up(MouseButton::Left, cx.listener(|this, _: &gpui_kit::MouseUpEvent, _, cx| this.pen_up(cx)))
                .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _: &gpui_kit::MouseUpEvent, _, cx| this.pen_up(cx)))
                .child(canvas(move |bounds, _, _| area.set(bounds), |_, _, _, _| {}).absolute().inset_0());
            // Kalem açıkken her zaman görünen "Çizimi bitir" (sağ üst): çubuk gizliyken de çıkılabilir.
            let target = user.to_string();
            let (danger, white) = (p.danger, gpui_kit::white());
            player = player.child(
                div().absolute().top(px(10.)).right(px(12.)).child(
                    div()
                        .id(SharedString::from(format!("pen-stop-{user}")))
                        .h(px(28.))
                        .px(px(10.))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .rounded(px(8.))
                        .border_1()
                        .border_color(p.border_accent)
                        .bg(chip_bg(0.86))
                        .text_color(p.text_strong)
                        .text_size(fs(10.5))
                        .css_weight(700.)
                        .cursor_pointer()
                        .hover(move |s| s.bg(danger).border_color(danger).text_color(white))
                        .child(call_icon("pen", 13., p.text_strong))
                        .child(p2_i18n::t("callui.pen.stop"))
                        .tooltip(tip(p2_i18n::t("callui.pen.stopTip")))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let t = target.clone();
                            this.toggle_pen(&t, cx);
                        })),
                ),
            );
        }
        if let Some(board) = calls.boards.get(user).filter(|b| !b.is_empty()) {
            player = player.child(crate::annotate::strokes_canvas(board.clone(), source));
            let area = self.call_pen.as_ref().filter(|pen| pen.target == user).map(|pen| pen.area.get());
            if let Some(a) = area.filter(|a| f32::from(a.size.width) > 0.) {
                let local = crate::annotate::content_box(Bounds { origin: gpui_kit::point(px(0.), px(0.)), size: a.size }, source);
                let rect = (f32::from(local.origin.x), f32::from(local.origin.y), f32::from(local.size.width), f32::from(local.size.height));
                player = player.children(crate::annotate::cursor_layer(board, rect, crate::annotate::TILE_LOOK));
            }
        }
        // `.stream-label` (sol üst) + geri sarılıyorsa "canlının gerisi" rozeti.
        player = player.child(
            div()
                .absolute()
                .left(px(12.))
                .top(px(10.))
                .flex()
                .gap(px(6.))
                .child(div().px(px(9.)).py(px(4.)).rounded(px(8.)).bg(chip_bg(0.74)).text_color(p.text_strong).text_size(fs(12.)).css_weight(650.).child(label.to_string()))
                .when_some(behind, |s, b| s.child(div().px(px(8.)).py(px(4.)).rounded(px(8.)).bg(chip_bg(0.74)).text_color(p.text_accent).text_size(fs(11.)).css_weight(750.).child(p2_i18n::tv("callui.rewind.behind", &[("time", &clock(b))])))),
        );
        // `.stream-bar`: yalnız üzerine gelince (kalemle çizerken gizli).
        if hovered && !(pen_here && self.call_pen.as_ref().is_some_and(|pen| pen.drawing.is_some())) {
            player = player.child(self.stream_bar(user, own, pen_here, p, cx));
        }
        player.into_any_element()
    }

    /// `.stream-bar` (izleyici: duraklat, zaman çizgisi, CANLI, ses, kalite, kalem, tam ekran, kapat).
    fn stream_bar(&self, user: &str, own: bool, pen_here: bool, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let (volume, locally, call_channel) = {
            let calls = self.calls.read(cx);
            (calls.user_volume(user), calls.locally_muted.contains(user), calls.call.as_ref().and_then(|c| c.channel_id.clone()))
        };
        let full = self.call_ui.fullscreen.as_ref().is_some_and(|f| f.0 == user);
        let icon_btn = |id: String, name: &'static str, tip_text: String, active: bool, color: Option<Hsla>, p: &Palette| {
            let (hover_bg, strong) = (p.accent_soft_2, p.text_strong);
            let fg = color.unwrap_or(if active { p.text_strong } else { p.text_2 });
            div()
                .id(SharedString::from(id))
                .size(px(30.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(8.))
                .bg(if active { p.accent_soft_2 } else { p.bg_3 })
                .text_color(fg)
                .cursor_pointer()
                .hover(move |s| s.bg(hover_bg).text_color(strong))
                .child(call_icon(name, 15., fg))
                .tooltip(tip(tip_text))
        };
        let mut bar = div()
            .absolute()
            .left(px(10.))
            .right(px(10.))
            .bottom(px(10.))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .px(px(9.))
            .py(px(7.))
            .rounded(px(11.))
            .border_1()
            .border_color(p.border_strong)
            .bg(p.glass.background()).backdrop_blur(p2_ui::fx::BLUR)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        let target = user.to_string();
        if !own {
            bar = bar
                .child(self.rewind_controls(user, p, cx))
                // Yayıncının sesi (kişi sesi, %0–200): eskiden yalnız görüntüydü, sürüklemek bir şey yapmıyordu.
                .child({
                    let (t1, t2) = (target.clone(), target.clone());
                    let shown = if locally { 0 } else { volume };
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_color(p.text_2)
                        .child(
                            div()
                                .id(SharedString::from(format!("stream-mute-{user}")))
                                .flex()
                                .cursor_pointer()
                                .child(call_icon(if shown > 0 { "volume" } else { "volumeOff" }, 15., if locally { p.danger_text } else { p.text_2 }))
                                .tooltip(tip(if locally { p2_i18n::t("callui.menu.unmuteLocal") } else { p2_i18n::t("callui.menu.muteLocal") }))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    let t = t1.clone();
                                    this.calls.update(cx, |c, cx| c.toggle_local_mute(&t, cx));
                                })),
                        )
                        .child(self.drag_slider(SharedString::from(format!("stream-vol-{user}")), volume, 200, 72., p.accent_hover, p, Rc::new(move |this, v, cx| this.set_user_volume(&t2, v, cx)), cx).tooltip(tip(format!("{} · {volume}%", p2_i18n::t("callui.menu.userVolume")))))
                })
                .child(self.quality_menu(user, p, cx));
        } else if self.calls.read(cx).screen_on {
            // Yayıncı kendi yayınını izliyor: kalite / kare hızı / ses / durdur burada da.
            bar = bar.child(div().flex_1()).child(self.share_controls("own", p, cx));
        }
        // Kalem: renkler + kalınlık + önizleme; "Ekrana çiz" / "Kalemi bırak".
        if pen_here {
            bar = bar.child(self.pen_tools(p, cx));
        }
        let ink = self.pen_ink(cx).0;
        let pen_target = target.clone();
        if !own && self.can_annotate(call_channel.as_deref()) {
            bar = bar.child(
                icon_btn(format!("pen-{user}"), "pen", if pen_here { p2_i18n::t("callui.penOff") } else { p2_i18n::t("callui.penOn") }, pen_here, pen_here.then(|| crate::annotate::color(&ink)), p)
                    .when(pen_here, |s| s.border_1().border_color(p.border_accent))
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_pen(&pen_target, cx))),
            );
        }
        {
            // Tam ekran (izleyici ve yayıncının kendi önizlemesi); tam ekrandayken "küçült".
            let (fs_user, fs_own) = (target.clone(), own);
            bar = bar.child(
                icon_btn(format!("full-{user}"), if full { "shrink" } else { "expand" }, if full { p2_i18n::t("callui.exitFullscreen") } else { p2_i18n::t("callui.fullscreen") }, full, None, p)
                    .on_click(cx.listener(move |this, _, window, cx| this.toggle_stream_fullscreen(&fs_user, fs_own, window, cx))),
            );
            if full {
                // Tam ekranda "izlemeyi bırak" yerine yalnız çıkış (yukarıdaki düğme / Esc).
                return bar.into_any_element();
            }
            let close = target.clone();
            let (soft, danger) = (p.danger_soft, p.danger_text);
            bar = bar.child(
                div()
                    .id(SharedString::from(format!("close-{user}")))
                    .size(px(30.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.))
                    .bg(p.bg_3)
                    .text_color(p.text_2)
                    .text_size(px(16.))
                    .cursor_pointer()
                    .hover(move |s| s.bg(soft).text_color(danger))
                    .child("×")
                    .tooltip(tip(p2_i18n::t("callui.stopWatching")))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let user = close.clone();
                        this.calls.update(cx, |c, cx| c.watch(Some(&user), cx));
                    })),
            );
        }
        bar.into_any_element()
    }

    /// Geri sarma (izleyici): duraklat/oynat, −10 sn, son N saniyelik kaydırma çubuğu, zaman, CANLI.
    /// Yayıncı geçmiş açmadıysa yalnız CANLI ve "geçmiş kapalı".
    fn rewind_controls(&self, user: &str, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let info = self.calls.read(cx).rewind_info(user);
        let live_label = |active: bool, p: &Palette| {
            div()
                .flex()
                .items_center()
                .gap(px(5.))
                .px(px(6.))
                .h(px(24.))
                .rounded(px(7.))
                .text_color(if active { p.danger_text } else { p.text_3 })
                .text_size(fs(9.5))
                .css_weight(800.)
                .tracking(0.06)
                .child(div().size(px(6.)).rounded_full().bg(if active { p.danger } else { p.text_4 }))
                .child(p2_i18n::t("callui.live"))
        };
        let Some((_window, available, behind, paused)) = info else {
            return div()
                .flex_1()
                .min_w(px(60.))
                .flex()
                .items_center()
                .gap(px(8.))
                .child(live_label(true, p))
                .child(div().text_size(fs(9.5)).text_color(p.text_3).child(p2_i18n::t("callui.rewind.off")))
                .into_any_element();
        };
        let at_live = behind <= 0.5 && !paused;
        let (hover_bg, strong) = (p.accent_soft_2, p.text_strong);
        let small_btn = |id: String, p: &Palette| {
            div()
                .id(SharedString::from(id))
                .h(px(30.))
                .min_w(px(30.))
                .px(px(6.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(8.))
                .bg(p.bg_3)
                .text_color(p.text_2)
                .text_size(fs(10.))
                .css_weight(750.)
                .cursor_pointer()
                .hover(move |s| s.bg(hover_bg).text_color(strong))
        };
        let (t1, t2, t3, t4, t5) = (user.to_string(), user.to_string(), user.to_string(), user.to_string(), user.to_string());
        let key = format!("rewind-{user}");
        let (k1, k2, k3) = (key.clone(), key.clone(), key.clone());
        let sliders = self.call_ui.sliders.clone();
        let (s1, s2) = (sliders.clone(), sliders.clone());
        // Çubuk [canlı − kayıttaki geçmiş, canlı] aralığını gösterir; tıklanan yer o kadar geride.
        let pick = move |b: Bounds<Pixels>, x: Pixels| (1. - ((f32::from(x) - f32::from(b.origin.x)) / f32::from(b.size.width).max(1.)).clamp(0., 1.)) * available;
        let played = if available > 0. { ((available - behind) / available).clamp(0., 1.) } else { 1. };
        let track = div()
            .id(SharedString::from(format!("rewind-track-{user}")))
            .relative()
            .flex_1()
            .min_w(px(80.))
            .h(px(16.))
            .cursor_pointer()
            .child(div().absolute().left_0().right_0().top(px(6.)).h(px(4.)).rounded(px(2.)).bg(p.bg_3))
            .child(div().absolute().left_0().top(px(6.)).h(px(4.)).w(gpui_kit::relative(played)).rounded(px(2.)).bg(if at_live { p.danger } else { p.accent }))
            .child(div().absolute().top(px(2.)).left(gpui_kit::relative(played)).ml(px(-6.)).size(px(12.)).rounded_full().bg(if at_live { p.danger } else { p.accent_hover }).border_2().border_color(gpui_kit::white()))
            .child(canvas(move |b, _, _| { sliders.borrow_mut().insert(k1.clone(), b); }, |_, _, _, _| {}).absolute().inset_0())
            .tooltip(tip(p2_i18n::tv("callui.rewind.trackTip", &[("time", &clock(available))])))
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &gpui_kit::MouseDownEvent, _, cx| {
                if let Some(b) = s1.borrow().get(&k2).copied() {
                    let behind = pick(b, ev.position.x);
                    this.calls.update(cx, |c, cx| c.rewind_to(&t1, behind, cx));
                }
            }))
            .on_mouse_move(cx.listener(move |this, ev: &gpui_kit::MouseMoveEvent, _, cx| {
                if ev.pressed_button == Some(MouseButton::Left) {
                    if let Some(b) = s2.borrow().get(&k3).copied() {
                        let behind = pick(b, ev.position.x);
                        this.calls.update(cx, |c, cx| c.rewind_to(&t2, behind, cx));
                    }
                }
            }));
        let (soft, danger) = (p.danger_soft, p.danger_text);
        div()
            .flex_1()
            .min_w(px(220.))
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                small_btn(format!("rewind-pause-{user}"), p)
                    .child(call_icon(if paused { "play" } else { "pause" }, 15., p.text_2))
                    .tooltip(tip(p2_i18n::t(if paused { "callui.rewind.play" } else { "callui.pause" })))
                    .on_click(cx.listener(move |this, _, _, cx| this.calls.update(cx, |c, cx| c.rewind_pause(&t3, cx)))),
            )
            .child(
                small_btn(format!("rewind-back-{user}"), p)
                    .child(p2_i18n::t("callui.rewind.back10"))
                    .tooltip(tip(p2_i18n::t("callui.rewind.back10Tip")))
                    .on_click(cx.listener(move |this, _, _, cx| this.calls.update(cx, |c, cx| c.rewind_back(&t4, 10., cx)))),
            )
            .child(track)
            .child(div().flex_none().min_w(px(38.)).text_size(fs(9.5)).css_weight(700.).text_color(if at_live { p.text_3 } else { p.text_accent }).child(if at_live { clock(available) } else { format!("−{}", clock(behind)) }))
            .child(
                div()
                    .id(SharedString::from(format!("rewind-live-{user}")))
                    .flex_none()
                    .rounded(px(7.))
                    .when(!at_live, move |s| s.cursor_pointer().hover(move |s| s.bg(soft).text_color(danger)))
                    .child(live_label(at_live, p))
                    .tooltip(tip(p2_i18n::t("callui.rewind.goLive")))
                    .on_click(cx.listener(move |this, _, _, cx| this.calls.update(cx, |c, cx| c.rewind_live(&t5, cx)))),
            )
            .into_any_element()
    }

    /// QualityMenu.tsx: izleyicinin alacağı kalite (açılır liste).
    fn quality_menu(&self, user: &str, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let current = {
            let calls = self.calls.read(cx);
            view_level_shown(calls.viewer_quality.get(user).map(String::as_str).unwrap_or("auto"), calls.stream_quality.get(user).map(String::as_str))
        };
        self.dropdown_trigger(format!("quality-{user}"), format!("view-quality:{user}"), quality_el(&current, p), 92., p2_i18n::t("callui.quality.tip"), p, cx)
    }

    // ------------------------------------------------------------ kişi menüsü (.voice-user-menu)

    /// Katılımcıya sağ tık (kutucuk, sesli oda satırı, arama paneli kişisi): kişi menüsünü açar.
    pub(super) fn open_participant_menu(&mut self, user: &str, x: f32, y: f32, cx: &mut Context<Self>) {
        self.call_ui.menu = Some((user.to_string(), x, y));
        cx.notify();
    }

    pub(super) fn voice_menu_view(&self, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (user, x, y) = self.call_ui.menu.clone()?;
        // Kişi: açık sahne, sürmekte olan arama ya da kenar çubuğundaki herhangi bir sesli oda.
        let part = {
            let calls = self.calls.read(cx);
            self.voice_room_people(cx)
                .into_iter()
                .find(|x| x.user_id == user)
                .or_else(|| calls.call.as_ref().and_then(|c| c.participants.iter().find(|x| x.user_id == user).cloned()))
                .or_else(|| calls.voice_rooms.values().flat_map(|v| v.iter()).find(|x| x.user_id == user).cloned())?
        };
        let me = self.auth.user.id.clone();
        let is_me = user == me;
        let (live, watched, settings, muted, deafened, locally, blocked, volume) = {
            let calls = self.calls.read(cx);
            (
                (part.screen || part.video) && calls.videos.contains_key(&user),
                calls.watching.contains(&user),
                calls.settings.clone(),
                calls.muted,
                calls.deafened,
                calls.locally_muted.contains(&user),
                calls.deafened_for.contains(&user),
                calls.user_volume(&user),
            )
        };
        let hue = p.hues[avatar_hue(&user)];
        let height = 49. + if is_me { 102. + 11. + 68. } else { 51. + 11. + 68. + if live { 34. + 11. } else { 0. } } + 12.;
        let (left, top) = super::super::chat::view::place_menu(x, y, y, false, 230., height, window);
        let top = top - super::super::chat::view::ROOT_TOP;
        let item = |id: &'static str, name: &'static str, label: String, p: &Palette| {
            let (soft, strong) = (p.accent_soft, p.text_strong);
            div()
                .id(id)
                .group(id)
                .h(px(34.))
                .px(px(9.))
                .flex()
                .items_center()
                .gap(px(9.))
                .rounded(px(6.))
                .text_size(fs(11.))
                .css_weight(650.)
                .text_color(p.text)
                .cursor_pointer()
                .hover(move |s| s.bg(soft).text_color(strong))
                .child(call_icon(name, 15., p.text).group_hover(id, move |s| s.text_color(strong)))
                .child(label)
        };
        // `max`: kaydırıcının üst sınırı (%; kişi sesi 200'e kadar, kendi seviyelerim 100).
        let slider = |id: &'static str, label: String, value: u32, max: u32, p: &Palette, on: Rc<dyn Fn(&mut Self, u32, &mut Context<Self>)>, cx: &mut Context<Self>| {
            self.level_slider(id, label, value, max, 206., p, on, cx)
        };
        // p2_enter öğeyi relative yapar: konum dış sarmalayıcıda.
        let mut menu = super::super::chat::view::menu_panel("voice-user-menu", 230., 6., p)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(9.))
                    .pt(px(5.))
                    .px(px(5.))
                    .pb(px(10.))
                    .child(
                        div()
                            .size(px(34.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(11.))
                            .bg(gpui_kit::linear_gradient(145., gpui_kit::linear_color_stop(hue.bg, 0.), gpui_kit::linear_color_stop(hue.bg2, 1.)))
                            .text_color(hue.fg)
                            .text_size(fs(11.))
                            .line_height(px(34.))
                            .css_weight(800.)
                            .tracking(-0.02)
                            .child(p2_ui::components::initials_label(user_initials(&part.display_name))),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(div().text_size(fs(12.)).css_weight(700.).text_color(p.text).child(part.display_name.clone()))
                            .child(div().text_size(fs(9.)).text_color(p.text_3).child(if is_me { p2_i18n::t("callui.menu.ownSub") } else { p2_i18n::t("callui.menu.userSub") })),
                    ),
            );
        let close = |this: &mut Self, cx: &mut Context<Self>| {
            this.call_ui.menu = None;
            cx.notify();
        };
        if !is_me && live {
            let target = user.clone();
            menu = menu
                .child(item("vm-watch", "screen", if watched { p2_i18n::t("callui.stopWatching") } else { p2_i18n::t("callui.watch") }, p).on_click(cx.listener(move |this, _, _, cx| {
                    let t = target.clone();
                    this.calls.update(cx, |c, cx| c.watch(Some(&t), cx));
                    close(this, cx);
                })))
                .child(super::super::chat::view::menu_hr(p));
        }
        if is_me {
            let s = settings.clone();
            menu = menu
                .child(slider("vm-in", p2_i18n::t("callui.menu.micLevel"), s.input_volume.min(100), 100, p, Rc::new(|this, v, cx| this.set_media_volume(true, v, cx)), cx))
                .child(slider("vm-out", p2_i18n::t("callui.menu.speakerLevel"), s.output_volume.min(100), 100, p, Rc::new(|this, v, cx| this.set_media_volume(false, v, cx)), cx))
                .child(super::super::chat::view::menu_hr(p))
                .child(item("vm-mute", if muted { "mic" } else { "micOff" }, if muted { p2_i18n::t("callui.menu.unmuteMe") } else { p2_i18n::t("callui.menu.muteMe") }, p).on_click(cx.listener(move |this, _, _, cx| {
                    this.calls.update(cx, |c, cx| c.toggle_mute(cx));
                    close(this, cx);
                })))
                .child(item("vm-deafen", "headphones", if deafened { p2_i18n::t("callui.menu.undeafenMe") } else { p2_i18n::t("callui.menu.deafenMe") }, p).on_click(cx.listener(move |this, _, _, cx| {
                    this.calls.update(cx, |c, cx| c.toggle_deafen(cx));
                    close(this, cx);
                })));
        } else {
            let (t1, t2, t3) = (user.clone(), user.clone(), user.clone());
            menu = menu
                .child(slider("vm-user", p2_i18n::t("callui.menu.userVolume"), volume, 200, p, Rc::new(move |this, v, cx| this.set_user_volume(&t1, v, cx)), cx))
                .child(super::super::chat::view::menu_hr(p))
                .child(item("vm-local", if locally { "volume" } else { "volumeOff" }, if locally { p2_i18n::t("callui.menu.unmuteLocal") } else { p2_i18n::t("callui.menu.muteLocal") }, p).on_click(cx.listener(move |this, _, _, cx| {
                    let t = t2.clone();
                    this.calls.update(cx, |c, cx| c.toggle_local_mute(&t, cx));
                    close(this, cx);
                })))
                .child(item("vm-send", "headphones", if blocked { p2_i18n::t("callui.menu.resumeSend") } else { p2_i18n::t("callui.menu.stopSend") }, p).on_click(cx.listener(move |this, _, _, cx| {
                    let t = t3.clone();
                    this.calls.update(cx, |c, cx| c.toggle_peer_deafen(&t, cx));
                    close(this, cx);
                })));
        }
        Some(
            div()
                .id("voice-menu-layer")
                .absolute()
                .inset_0()
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    this.call_ui.menu = None;
                    cx.notify();
                }))
                .on_mouse_down(MouseButton::Right, cx.listener(|this, _, _, cx| {
                    this.call_ui.menu = None;
                    cx.notify();
                }))
                .child(div().absolute().left(px(left)).top(px(top)).child(menu.p2_enter("voice-user-menu-in", Entrance::ScaleIn, Duration::ZERO)))
                .into_any_element(),
        )
    }

    /// Sürüklenebilir kaydırıcı (`range_track` görünümü, 0–`max`): basınca değer alınır, fare basılı
    /// kaldıkça pencerenin her yerinde izlenir (imleç 16px'lik şeritten çıksa da), bırakınca biter.
    /// Eskiden yalnız şeridin üzerindeyken hareket dinleniyordu: sürüklerken birkaç piksel kayınca
    /// değer takılı kalıyordu.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn drag_slider(&self, id: SharedString, value: u32, max: u32, w: f32, accent: Hsla, p: &Palette, on: Rc<dyn Fn(&mut Self, u32, &mut Context<Self>)>, cx: &mut Context<Self>) -> gpui_kit::Stateful<gpui_kit::Div> {
        let bounds: Rc<Cell<Bounds<Pixels>>> = Rc::new(Cell::new(Bounds::default()));
        let drag = self.call_ui.slider_drag.clone();
        let entity = cx.entity().downgrade();
        let pick = move |b: Bounds<Pixels>, x: Pixels| (((f32::from(x) - f32::from(b.origin.x) - 8.) / (f32::from(b.size.width) - 16.).max(1.)).clamp(0., 1.) * max as f32).round() as u32;
        let (b0, b1, b2) = (bounds.clone(), bounds.clone(), bounds);
        let (d1, d2) = (drag.clone(), drag);
        let (k1, k2) = (id.clone(), id.clone());
        let on_down = on.clone();
        div()
            .id(id)
            .relative()
            .flex_none()
            .w(px(w))
            .h(px(16.))
            .cursor_pointer()
            .child(range_track(value as f32 / max.max(1) as f32, accent, w, p))
            .child(
                canvas(
                    move |b, _, _| b1.set(b),
                    move |_, _, window, _| {
                        let (d, k, b) = (d2.clone(), k2.clone(), b2.clone());
                        window.on_mouse_event(move |ev: &gpui_kit::MouseMoveEvent, phase, _, cx| {
                            if phase != gpui_kit::DispatchPhase::Bubble || d.borrow().as_ref() != Some(&k) {
                                return;
                            }
                            if ev.pressed_button != Some(MouseButton::Left) {
                                *d.borrow_mut() = None;
                                return;
                            }
                            let v = pick(b.get(), ev.position.x);
                            let on = on.clone();
                            let _ = entity.update(cx, |this, cx| {
                                on(this, v, cx);
                                cx.notify();
                            });
                        });
                        let (d, k) = (d2, k2);
                        window.on_mouse_event(move |_: &gpui_kit::MouseUpEvent, phase, _, _| {
                            if phase == gpui_kit::DispatchPhase::Bubble && d.borrow().as_ref() == Some(&k) {
                                *d.borrow_mut() = None;
                            }
                        });
                    },
                )
                .absolute()
                .inset_0(),
            )
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &gpui_kit::MouseDownEvent, _, cx| {
                cx.stop_propagation();
                *d1.borrow_mut() = Some(k1.clone());
                on_down(this, pick(b0.get(), ev.position.x), cx);
                cx.notify();
            }))
    }

    /// Menü kaydırıcısı: üstte etiket ve yüzde, altta sürüklenebilir şerit (`.voice-user-menu`).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn level_slider(&self, id: &'static str, label: String, value: u32, max: u32, w: f32, p: &Palette, on: Rc<dyn Fn(&mut Self, u32, &mut Context<Self>)>, cx: &mut Context<Self>) -> gpui_kit::Div {
        div()
            .px(px(5.))
            .py(px(7.))
            .flex()
            .flex_col()
            .gap(px(7.))
            .text_size(fs(10.))
            .text_color(p.text_2)
            .child(div().flex().justify_between().child(label).child(div().text_size(fs(9.)).css_weight(700.).text_color(p.text_accent).child(format!("{value}%"))))
            .child(div().m(px(2.)).child(self.drag_slider(SharedString::from(id), value, max, w, p.accent, p, on, cx)))
    }

    /// Kişi sesi + "kendim için sustur" (kenar çubuğu kişi menüleri; arama dışında da — ses kişiye
    /// göre kayıtlı, sonraki aramada uygulanır).
    pub(super) fn person_volume_items(&self, user: &str, p: &Palette, cx: &mut Context<Self>) -> Vec<AnyElement> {
        if user == self.auth.user.id {
            return Vec::new();
        }
        let (volume, locally) = {
            let calls = self.calls.read(cx);
            (calls.user_volume(user), calls.locally_muted.contains(user))
        };
        let (t1, t2) = (user.to_string(), user.to_string());
        let (soft, strong) = (p.accent_soft, p.text_strong);
        let mute = div()
            .id("nm-person-local-mute")
            .group("nm-person-local-mute")
            .w_full()
            .h(px(32.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(6.))
            .text_color(p.text)
            .text_size(fs(12.))
            .css_weight(600.)
            .cursor_pointer()
            .hover(move |s| s.bg(soft).text_color(strong))
            .child(call_icon(if locally { "volume" } else { "volumeOff" }, 14., p.text_2).group_hover("nm-person-local-mute", move |s| s.text_color(strong)))
            .child(if locally { p2_i18n::t("callui.menu.unmuteLocal") } else { p2_i18n::t("callui.menu.muteLocal") })
            .on_click(cx.listener(move |this, _, _, cx| {
                let t = t2.clone();
                this.calls.update(cx, |c, cx| c.toggle_local_mute(&t, cx));
                cx.notify();
            }));
        vec![
            self.level_slider("nm-person-volume", p2_i18n::t("callui.menu.userVolume"), volume, 200, 162., p, Rc::new(move |this, v, cx| this.set_user_volume(&t1, v, cx)), cx).into_any_element(),
            mute.into_any_element(),
        ]
    }

    /// Kişi başına dinleme sesi (%0–200): motora gider ve kişiye göre ayarlara yazılır.
    pub(super) fn set_user_volume(&mut self, user: &str, value: u32, cx: &mut Context<Self>) {
        let value = value.min(200);
        self.calls.update(cx, |c, cx| c.set_user_volume(user, value, cx));
        let user = user.to_string();
        self.save_media(move |m| {
            if value == 100 {
                m.user_volumes.remove(&user);
            } else {
                m.user_volumes.insert(user, value as f32 / 100.);
            }
        }, cx);
    }

    /// Kendi mikrofon/hoparlör seviyem (settings.ts `inputVolume`/`outputVolume`; motor yeniden ayarlanır).
    fn set_media_volume(&mut self, input: bool, value: u32, cx: &mut Context<Self>) {
        self.save_media(|m| if input { m.input_volume = value } else { m.output_volume = value }, cx);
        let settings = self.calls.read(cx).settings.clone();
        self.calls.update(cx, |c, cx| c.apply_settings(settings, cx));
    }

    // ------------------------------------------------------------ arama paneli (.call-dock)

    pub(super) fn call_dock(&self, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let calls = self.calls.read(cx);
        let call = calls.call.clone()?;
        // × ile bu arama için kapatıldı (sonraki aramada yine görünür).
        if self.call_ui.dock_hidden.as_deref() == Some(call.id.as_str()) {
            return None;
        }
        let call_id = call.id.clone();
        let speaking = calls.speaking.clone();
        let peer_states = calls.peer_states.clone();
        let (connected_at, started_at) = (calls.connected_at, calls.started_at);
        let me = self.auth.user.id.clone();
        // Arama görünümü (sesli oda ya da arama sahnesi) zaten ana alandaysa "aç" düğmesi gizli.
        let view_open = match call.kind.as_str() {
            "channel" => self.voice_channel(cx).is_some_and(|c| Some(&c.id) == call.channel_id.as_ref()),
            _ => self.call_room(cx).is_some() && self.board.is_none() && self.documents.is_none() && self.library.is_none(),
        };
        let others: Vec<CallParticipant> = call.participants.iter().filter(|x| x.user_id != me).cloned().collect();
        // Panelde kendi avatarım değil, konuştuğum/aradığım kişiler (CallOverlay `roster`).
        let waiting: Vec<CallParticipant> = call
            .invited
            .iter()
            .filter(|u| **u != me)
            .map(|u| CallParticipant { user_id: u.clone(), display_name: self.person_name(u), ..Default::default() })
            .collect();
        let roster: Vec<(CallParticipant, bool)> = if call.kind == "direct" {
            let counterpart = if call.caller.as_deref() == Some(me.as_str()) { call.callee.clone() } else { call.caller.clone() };
            match others.iter().find(|x| Some(&x.user_id) == counterpart.as_ref()).or(others.first()) {
                Some(x) => vec![(x.clone(), false)],
                None => waiting.iter().find(|x| Some(&x.user_id) == counterpart.as_ref()).or(waiting.first()).map(|x| vec![(x.clone(), true)]).unwrap_or_default(),
            }
        } else {
            others.iter().map(|x| (x.clone(), false)).chain(waiting.iter().map(|x| (x.clone(), true))).collect()
        };
        // Bireysel aramada karşı tarafın durumu panel başlığında da görünür (YAYINDA, kamera, mikrofon, kulaklık).
        let dock_status: Option<CallParticipant> = (call.kind == "direct").then(|| roster.first().filter(|r| !r.1).map(|r| r.0.clone())).flatten();
        let ringing = connected_at.is_none();
        let headline = if call.kind == "channel" { p2_i18n::t("callui.voiceRoom") } else { roster.first().map(|r| r.0.display_name.clone()).unwrap_or_else(|| p2_i18n::t("callui.voiceCall")) };
        let since = |at: Option<std::time::Instant>| at.map(|t| t.elapsed().as_secs()).unwrap_or(0);
        let subtitle: AnyElement = if ringing {
            div()
                .text_color(p.text_accent)
                .child(p2_i18n::tv("callui.ringing", &[("time", &mmss(since(started_at)))]))
                .with_animation("ringing-fade", gpui_kit::Animation::new(Duration::from_millis(1600)).repeat(), |el, t| el.opacity(0.55 + 0.45 * (1. - (t * std::f32::consts::TAU).cos()) / 2.))
                .into_any_element()
        } else {
            div().child(p2_i18n::tv("callui.durationPeople", &[("time", &mmss(since(connected_at))), ("n", &call.participants.len().to_string())])).into_any_element()
        };
        // Bağlantı notu (CallOverlay `linkNote`).
        let link_note: Option<(String, bool)> = if ringing {
            None
        } else {
            let pending: Vec<&CallParticipant> = others.iter().filter(|x| peer_states.get(&x.user_id).map(String::as_str) != Some("connected")).collect();
            if pending.is_empty() {
                None
            } else if let Some(broken) = pending.iter().find(|x| matches!(peer_states.get(&x.user_id).map(String::as_str), Some("failed" | "disconnected"))) {
                Some((p2_i18n::tv("callui.linkFailed", &[("name", &broken.display_name)]), true))
            } else {
                Some((p2_i18n::tv("callui.linkConnecting", &[("states", &pending.iter().map(|x| peer_states.get(&x.user_id).cloned().unwrap_or_else(|| p2_i18n::t("callui.peerNew"))).collect::<Vec<_>>().join(", "))]), false))
            }
        };
        let (muted, deafened, camera, screen) = (calls.muted, calls.deafened, calls.camera_on, calls.screen_on);
        let error = calls.error.as_ref().map(|e| e.0.clone());

        let control = |id: &'static str, name: &'static str, tip_text: String, state: u8, p: &Palette| {
            // state: 0 normal, 1 .active, 2 .danger-on / .hangup
            let (bg, border, fg) = match state {
                1 => (p.accent_soft_2, p.border_accent, p.text_strong),
                2 => (p.danger_soft, p.danger_border, p.danger_text),
                _ => (p.bg_3, p.border, p.text_2),
            };
            let hangup = id == "dock-hangup";
            let (hbg, hborder, hfg) = if hangup { (p.danger, p.danger, gpui_kit::white()) } else if state == 0 { (p.bg_hover, p.border_strong, p.text_strong) } else { (bg, border, fg) };
            let svg = call_icon(name, 19., fg).group_hover(id, move |s| s.text_color(hfg));
            let svg = if hangup { svg.with_transformation(gpui_kit::Transformation::rotate(gpui_kit::radians(133f32.to_radians()))) } else { svg };
            div()
                .id(id)
                .group(id)
                .flex_1()
                .h(px(36.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(11.))
                .border_1()
                .border_color(border)
                .bg(bg)
                .cursor_pointer()
                .hover(move |s| s.bg(hbg).border_color(hborder).top(px(-1.)))
                .child(svg)
                .tooltip(tip(tip_text))
        };
        let invite_open = self.call_ui.invite;
        let controls = div()
            .flex()
            .items_center()
            .gap(px(5.))
            .child(control("dock-mute", if muted { "micOff" } else { "mic" }, if muted { p2_i18n::t("callui.unmute") } else { p2_i18n::t("callui.mute") }, if muted { 2 } else { 0 }, p).on_click(cx.listener(|this, _, _, cx| this.calls.update(cx, |c, cx| c.toggle_mute(cx)))))
            .child(control("dock-deafen", if deafened { "headphonesOff" } else { "headphones" }, if deafened { p2_i18n::t("callui.undeafen") } else { p2_i18n::t("callui.deafen") }, if deafened { 2 } else { 0 }, p).on_click(cx.listener(|this, _, _, cx| this.calls.update(cx, |c, cx| c.toggle_deafen(cx)))))
            .child(control("dock-camera", if camera { "camera" } else { "cameraOff" }, if camera { p2_i18n::t("callui.cameraOff") } else { p2_i18n::t("callui.cameraOn") }, if camera { 1 } else { 0 }, p).on_click(cx.listener(|this, _, _, cx| this.calls.update(cx, |c, cx| c.toggle_camera(cx)))))
            .child(control("dock-screen", if screen { "screen" } else { "screenOff" }, if screen { p2_i18n::t("callui.stopShare") } else { p2_i18n::t("callui.shareScreen") }, if screen { 1 } else { 0 }, p).on_click(cx.listener(|this, _, window, cx| this.toggle_screen(window, cx))))
            .child(control("dock-invite", "personAdd", p2_i18n::t("callui.addPerson"), if invite_open { 1 } else { 0 }, p).on_click(cx.listener(|this, _, _, cx| {
                this.call_ui.invite = !this.call_ui.invite;
                cx.notify();
            })))
            .child(control("dock-hangup", "hangup", p2_i18n::t("callui.leave"), 2, p).on_click(cx.listener(|this, _, window, cx| this.calls.update(cx, |c, cx| c.leave(window, cx)))));

        let mut dock = div()
            .id("call-dock")
            .flex()
            .flex_col()
            .gap(px(9.))
            .p(px(10.))
            .rounded(px(14.))
            .border_1()
            .border_color(p.border_strong)
            .bg(p.glass.background()).backdrop_blur(p2_ui::fx::BLUR)
            .shadow(p.shadow_lg())
            .occlude()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(9.))
                    .child(
                        div().w(px(10.)).flex_none().child(
                            div()
                                .size(px(9.))
                                .rounded_full()
                                .bg(p.success)
                                .with_animation("call-pulse", gpui_kit::Animation::new(Duration::from_millis(2200)).repeat(), |el, t| {
                                    let k = (1. - (t * std::f32::consts::TAU).cos()) / 2.;
                                    el.shadow(vec![gpui_kit::BoxShadow { color: css_rgba(61, 220, 132, 0.45 * (1. - k)), offset: gpui_kit::point(px(0.), px(0.)), blur_radius: px(0.), spread_radius: px(5. * k), inset: false }])
                                }),
                        ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(div().truncate().text_size(fs(11.5)).css_weight(700.).text_color(p.text_strong).child(headline))
                            .child(div().text_size(fs(9.5)).text_color(p.text_3).child(subtitle)),
                    )
                    .when_some(dock_status, |s, part| s.child(status_icons(&part, 14., p)))
                    // Arama görünümünü aç (sesli oda / arama sahnesi ana alanda).
                    .when(!view_open, |s| s.child({
                        let (soft, strong, border) = (p.accent_soft, p.text_strong, p.border);
                        div()
                            .id("dock-expand")
                            .group("dock-expand")
                            .flex_none()
                            .size(px(26.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(8.))
                            .border_1()
                            .border_color(gpui_kit::transparent_black())
                            .cursor_pointer()
                            .hover(move |s| s.bg(soft).border_color(border))
                            .child(p2_ui::icons::Icon::raw(ICON_EXPAND).stroke(1.9).el().size(px(15.)).text_color(p.text_3).group_hover("dock-expand", move |s| s.text_color(strong)))
                            .tooltip(tip(p2_i18n::t("call.expand")))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_call_view(window, cx);
                                cx.notify();
                            }))
                    }))
                    // × paneli bu arama için gizler.
                    .child({
                        let (soft, strong) = (p.accent_soft, p.text_strong);
                        let id = call_id.clone();
                        div()
                            .id("dock-close")
                            .flex_none()
                            .size(px(24.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(7.))
                            .text_size(fs(16.))
                            .text_color(p.text_3)
                            .cursor_pointer()
                            .hover(move |s| s.bg(soft).text_color(strong))
                            .child("×")
                            .tooltip(tip(p2_i18n::t("callui.dock.close")))
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.call_ui.dock_hidden = Some(id.clone());
                                this.call_ui.invite = false;
                                crate::trace("arama: panel bu arama için kapatıldı");
                                cx.notify();
                            }))
                    })
                    // Başlık satırından tutulup pencerede istenen yere sürüklenir.
                    .id("dock-head")
                    .cursor(gpui_kit::CursorStyle::OpenHand)
                    .tooltip(tip(p2_i18n::t("callui.dock.drag")))
                    .on_mouse_down(MouseButton::Left, {
                        let (drag, pos) = (self.call_ui.dock_drag.clone(), self.call_ui.dock_pos.clone());
                        move |ev: &gpui_kit::MouseDownEvent, window, cx| {
                            let (l, b) = pos.get().unwrap_or((DOCK_LEFT.initial, DOCK_BOTTOM.initial));
                            drag.set(Some((f32::from(ev.position.x), f32::from(ev.position.y), l, b)));
                            window.prevent_default();
                            cx.stop_propagation();
                            window.refresh();
                        }
                    }),
            );
        if let Some((text, bad)) = link_note {
            dock = dock.child(
                div()
                    .px(px(8.))
                    .py(px(6.))
                    .rounded(px(8.))
                    .border_1()
                    .border_color(if bad { p.danger_border } else { p.border })
                    .bg(if bad { p.danger_soft } else { p.accent_soft })
                    .text_color(if bad { p.danger_text } else { p.text_accent })
                    .text_size(fs(10.))
                    .child(text),
            );
        }
        {
            let mut chips = div().flex().flex_wrap().gap(px(5.));
            for (part, pending) in &roster {
                let hue = p.hues[avatar_hue(&part.user_id)];
                let speaking = speaking.contains(&part.user_id);
                let uid = part.user_id.clone();
                chips = chips.child(
                    div()
                        .id(SharedString::from(format!("call-chip-{}", part.user_id)))
                        .size(px(28.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(10.))
                        .border_2()
                        .border_color(if speaking { p.success } else if *pending { p.border } else { gpui_kit::transparent_black() })
                        .bg(gpui_kit::linear_gradient(145., gpui_kit::linear_color_stop(hue.bg, 0.), gpui_kit::linear_color_stop(hue.bg2, 1.)))
                        .text_color(hue.fg)
                        .text_size(fs(9.))
                        .line_height(px(24.))
                        .css_weight(800.)
                        .when(*pending, |s| s.opacity(0.72))
                        .when(speaking, |s| s.shadow(vec![gpui_kit::BoxShadow { color: css_rgba(61, 220, 132, 0.45), offset: gpui_kit::point(px(0.), px(0.)), blur_radius: px(10.), spread_radius: px(0.), inset: false }]))
                        .cursor_pointer()
                        .hover(|s| s.top(px(-1.)))
                        .child(p2_ui::components::initials_label(user_initials(&part.display_name)))
                        .tooltip(tip(if *pending { p2_i18n::tv("callui.chipTipPending", &[("name", &part.display_name)]) } else { p2_i18n::tv("callui.chipTip", &[("name", &part.display_name)]) }))
                        .when(!*pending, |s| {
                            let menu_id = part.user_id.clone();
                            s.on_mouse_down(MouseButton::Right, cx.listener(move |this, ev: &gpui_kit::MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                this.open_participant_menu(&menu_id, f32::from(ev.position.x), f32::from(ev.position.y), cx);
                            }))
                        })
                        .on_click(cx.listener(move |this, _, _, cx| this.open_profile_by_id(&uid, cx))),
                );
            }
            if roster.is_empty() {
                chips = chips.child(div().text_size(fs(11.3)).text_color(p.text).child(p2_i18n::t("callui.alone")));
            }
            dock = dock.child(chips);
        }
        dock = dock.child(controls);
        if invite_open {
            let invitable: Vec<_> = self.members.iter().filter(|m| m.id != me && !call.participants.iter().any(|x| x.user_id == m.id) && !call.invited.contains(&m.id)).cloned().collect();
            let mut list = div().id("call-invite-list").flex().flex_col().gap(px(2.)).max_h(px(170.)).overflow_y_scroll();
            for person in &invitable {
                let hue = p.hues[avatar_hue(&person.id)];
                let id = person.id.clone();
                let (soft, strong) = (p.accent_soft, p.text_strong);
                list = list.child(
                    div()
                        .id(SharedString::from(format!("invite-{}", person.id)))
                        .w_full()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .px(px(6.))
                        .py(px(4.))
                        .rounded(px(6.))
                        .text_color(p.text)
                        .text_size(fs(11.))
                        .cursor_pointer()
                        .hover(move |s| s.bg(soft).text_color(strong))
                        .child(div().size(px(28.)).flex().items_center().justify_center().rounded(px(10.)).border_2().border_color(gpui_kit::transparent_black()).bg(gpui_kit::linear_gradient(145., gpui_kit::linear_color_stop(hue.bg, 0.), gpui_kit::linear_color_stop(hue.bg2, 1.))).text_color(hue.fg).text_size(fs(9.)).line_height(px(24.)).css_weight(800.).child(p2_ui::components::initials_label(user_initials(&person.display_name))))
                        .child(person.display_name.clone())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let u = id.clone();
                            this.calls.update(cx, |c, cx| c.invite(&u, cx));
                            this.call_ui.invite = false;
                            cx.notify();
                        })),
                );
            }
            if invitable.is_empty() {
                list = list.child(div().text_size(fs(10.)).text_color(p.text_3).child(p2_i18n::t("callui.noOneToAdd")));
            }
            dock = dock.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .pt(px(8.))
                    .border_t_1()
                    .border_color(p.border_subtle)
                    .child(div().text_size(fs(9.)).css_weight(800.).tracking(0.18).text_color(p.text_accent).child(p2_i18n::t("callui.addToCall")))
                    .child(list),
            );
        }
        if let Some(e) = error {
            dock = dock.child(div().text_size(fs(10.)).text_color(p.danger_text).child(e));
        }
        // Konum: kayıtlı (sürüklenen) sol/alt, varsayılan `left: 78px; bottom: 12px`; pencere küçülünce
        // panel hep pencerenin içinde kalır.
        let pos = self.call_ui.dock_pos.clone();
        let (l, b) = pos.get().unwrap_or_else(|| {
            let v = (crate::resize::value(&DOCK_LEFT), crate::resize::value(&DOCK_BOTTOM));
            pos.set(Some(v));
            v
        });
        let vp = window.viewport_size();
        let (vw, vh) = (f32::from(vp.width), f32::from(vp.height) - super::super::chat::view::ROOT_TOP);
        let dock_h = self.call_ui.dock_h.clone();
        let h = dock_h.get().max(60.);
        let left = l.min(vw - DOCK_W - 6.).max(6.);
        let bottom = b.min(vh - h - 6.).max(6.);
        let measure = canvas(move |bounds, _, _| dock_h.set(f32::from(bounds.size.height)), |_, _, _, _| {}).absolute().inset_0();
        let drag = self.call_ui.dock_drag.clone();
        let dragging = drag.get().is_some();
        let (d1, d2, p1) = (drag.clone(), drag, pos.clone());
        let tracker = crate::resize::tracker(
            dragging,
            gpui_kit::CursorStyle::ClosedHand,
            move |at, _, _| {
                if let Some((sx, sy, l0, b0)) = d1.get() {
                    p1.set(Some((l0 + f32::from(at.x) - sx, b0 - (f32::from(at.y) - sy))));
                }
            },
            move |_, _| {
                if d2.take().is_some() {
                    if let Some((l, b)) = pos.get() {
                        // Pencere içine sıkıştırılmış hâli kaydedilir.
                        let (l, b) = (l.min(vw - DOCK_W - 6.).max(6.), b.min(vh - h - 6.).max(6.));
                        pos.set(Some((l, b)));
                        p2_client_core::store::save_layout_value(DOCK_LEFT.key, l);
                        p2_client_core::store::save_layout_value(DOCK_BOTTOM.key, b);
                        crate::trace(&format!("arama: panel taşındı ({l:.0}, {b:.0})"));
                    }
                }
            },
        );
        Some(div().absolute().left(px(left)).bottom(px(bottom)).w(px(DOCK_W)).child(dock.child(measure).child(tracker).p2_enter("call-dock", Entrance::PopElastic, Duration::ZERO)).into_any_element())
    }

    // ------------------------------------------------------------ gelen arama

    /// `.incoming-layer` + `.incoming-call` (zil motor tarafında çalar).
    pub(super) fn incoming_card(&self, p: &Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        let calls = self.calls.read(cx);
        let incoming = calls.incoming.as_ref()?;
        if incoming.silent || self.call_ui.dismissed.as_deref() == Some(incoming.call.id.as_str()) {
            return None;
        }
        let hue = p.hues[avatar_hue(&incoming.from_id)];
        let joining = calls.joining;
        let call_id = incoming.call.id.clone();
        let button = |id: &'static str, label: String, _p: &Palette| div().id(id).flex_1().h(px(38.)).flex().items_center().justify_center().rounded(px(11.)).border_1().text_size(fs(12.)).css_weight(700.).cursor_pointer().child(label);
        let (danger, danger_border, white) = (p.danger, p.danger, gpui_kit::white());
        let (hover_bg, strong) = (p.bg_hover, p.text_strong);
        let card = div()
            .relative()
            .w(px(320.))
            .overflow_hidden()
            .pt(px(26.))
            .px(px(22.))
            .pb(px(20.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(4.))
            .rounded(px(18.))
            .border_1()
            .border_color(p.border_strong)
            .bg(p.glass_strong.background())
            .shadow(p.shadow_lg())
            .occlude()
            .child(super::super::setup_ui::shine("incoming-shine", 320., 250))
            .child(
                div()
                    .size(px(82.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(27.))
                    .bg(gpui_kit::linear_gradient(145., gpui_kit::linear_color_stop(hue.bg, 0.), gpui_kit::linear_color_stop(hue.bg2, 1.)))
                    .text_color(hue.fg)
                    .text_size(fs(26.))
                    .line_height(px(82.))
                    .css_weight(800.)
                    .tracking(-0.02)
                    .shadow(p.shadow_md())
                    .child(p2_ui::components::initials_label(user_initials(&incoming.from_name))),
            )
            .child(div().mt(px(12.)).text_size(fs(17.)).css_weight(700.).text_color(p.text_strong).child(incoming.from_name.clone()))
            .child(div().text_size(fs(11.)).text_color(p.text_3).child(if incoming.call.kind == "channel" { p2_i18n::t("callui.incoming.channel") } else { p2_i18n::t("callui.incoming.direct") }))
            .child(
                div()
                    .w_full()
                    .mt(px(18.))
                    .flex()
                    .gap(px(7.))
                    .child(button("incoming-decline", p2_i18n::t("callui.decline"), p).border_color(p.danger_border).bg(p.danger_soft).text_color(p.danger_text).hover(move |s| s.bg(danger).border_color(danger_border).text_color(white)).on_click(cx.listener(|this, _, window, cx| this.calls.update(cx, |c, cx| c.decline(window, cx)))))
                    .child(button("incoming-silence", p2_i18n::t("callui.silence"), p).border_color(p.border).bg(p.bg_3).text_color(p.text_2).hover(move |s| s.bg(hover_bg).text_color(strong)).on_click(cx.listener(move |this, _, _, cx| {
                        this.call_ui.dismissed = Some(call_id.clone());
                        this.calls.update(cx, |c, cx| c.silence_incoming(cx));
                        cx.notify();
                    })))
                    .child({
                        let accept = button("incoming-accept", if joining { p2_i18n::t("callui.connecting") } else { p2_i18n::t("callui.accept") }, p)
                            .border_color(gpui_kit::transparent_black())
                            .bg(gpui_kit::linear_gradient(135., gpui_kit::linear_color_stop(p.success, 0.), gpui_kit::linear_color_stop(p.success_mid, 1.)))
                            .text_color(p.text_on_success)
                            .on_click(cx.listener(|this, _, window, cx| this.calls.update(cx, |c, cx| c.accept(window, cx))));
                        if joining {
                            accept.opacity(0.6).into_any_element()
                        } else {
                            // `call-pulse 2.2s` + `0 6px 18px rgba(61,220,132,.35)`.
                            accept
                                .with_animation("accept-pulse", gpui_kit::Animation::new(Duration::from_millis(2200)).repeat(), |el, t| {
                                    let k = (1. - (t * std::f32::consts::TAU).cos()) / 2.;
                                    el.shadow(vec![
                                        gpui_kit::BoxShadow { color: css_rgba(61, 220, 132, 0.35), offset: gpui_kit::point(px(0.), px(6.)), blur_radius: px(18.), spread_radius: px(0.), inset: false },
                                        gpui_kit::BoxShadow { color: css_rgba(61, 220, 132, 0.45 * (1. - k)), offset: gpui_kit::point(px(0.), px(0.)), blur_radius: px(0.), spread_radius: px(5. * k), inset: false },
                                    ])
                                })
                                .into_any_element()
                        }
                    }),
            )
            .p2_enter(("incoming", incoming.call.id.len()), Entrance::PopElastic, Duration::ZERO);
        Some(
            // .incoming-layer (position: fixed; inset: 0; z-index 130) başlık çubuğunun (.titlebar z-index 1000) ALTINDA kalır:
            // Electron görüntüsünde başlık çubuğu bulanık/karartılmış değil. Kart yine pencerenin ortasında (inset 0),
            // karartma/bulanıklık yalnız başlık çubuğunun altında.
            div().absolute().top(px(-38.)).left_0().right_0().bottom_0().child(div()
                .id("incoming-layer")
                .relative()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(div().id("incoming-dim").absolute().top(px(38.)).left_0().right_0().bottom_0().bg(p.overlay).backdrop_blur(p2_ui::fx::BLUR_SM).occlude())
                .child(card)
                .p2_enter("incoming-layer-in", Entrance::Fade, Duration::ZERO))
                .into_any_element(),
        )
    }

    /// Ekran paylaşımı düğmesi: açıksa durdurur, değilse kaynak seçiciyi açar.
    pub(super) fn toggle_screen(&mut self, window: &mut gpui_kit::Window, cx: &mut Context<Self>) {
        if self.calls.read(cx).screen_on {
            self.calls.update(cx, |c, cx| c.stop_screen(cx));
            return;
        }
        let picker = cx.new(|cx| ScreenPicker::new(window, cx));
        let sub = cx.subscribe_in(&picker, window, |this, _, event: &PickerEvent, _, cx| {
            this.picker = None;
            if let PickerEvent::Picked { source, fps, quality, audio, rewind } = event {
                let (source, fps, quality, audio, rewind) = (source.clone(), *fps, quality.clone(), *audio, *rewind);
                // Son seçimler kalıcı: sonraki yayın penceresi bunlarla açılır.
                let q = quality.clone();
                this.save_media(
                    move |m| {
                        m.share_audio = audio;
                        m.share_quality = q;
                        m.share_fps = fps;
                        m.share_rewind = rewind;
                    },
                    cx,
                );
                crate::trace(&format!("ekran paylaşımı: {quality} {fps} fps, geçmiş {rewind} sn"));
                this.calls.update(cx, |c, cx| c.start_screen(source, fps, &quality, audio, rewind, cx));
            }
            cx.notify();
        });
        self.picker = Some((picker, sub));
        cx.notify();
    }

    // ------------------------------------------------------------ kalem (yayına çizim)

    /// Ses ayarlarındaki küçük tercihleri (paylaşım sesi, kalem) kaydeder; motor yeniden kurulmaz.
    pub(super) fn save_media(&mut self, change: impl FnOnce(&mut p2_client_core::store::MediaSettings), cx: &mut Context<Self>) {
        let mut media = self.calls.read(cx).settings.clone();
        change(&mut media);
        let saved = media.clone();
        let _ = self.app.update(cx, |app, _| {
            app.settings.media = saved;
            p2_client_core::store::save_settings(&app.settings);
        });
        self.calls.update(cx, |c, _| c.settings = media);
    }

    fn can_annotate(&self, channel: Option<&str>) -> bool {
        match channel {
            Some(ch) => self.my_perms.can_in(ch, "screen.annotate"),
            None => self.my_perms.can("screen.annotate"),
        }
    }

    /// Kalem rengi (ayar boşsa kişiye özgü) ve kalınlığı.
    fn pen_ink(&self, cx: &App) -> (String, f32) {
        let s = &self.calls.read(cx).settings;
        let ink = if s.pen_ink.is_empty() { crate::annotate::pen_color(&self.auth.user.id).to_string() } else { s.pen_ink.clone() };
        (ink, s.pen_width.clamp(2, 14) as f32)
    }

    fn my_name(&self, cx: &App) -> String {
        let me = &self.auth.user.id;
        self.calls.read(cx).call.as_ref().and_then(|c| c.participants.iter().find(|p| &p.user_id == me).map(|p| p.display_name.clone())).unwrap_or_else(|| p2_i18n::t("callui.you"))
    }

    pub(super) fn toggle_pen(&mut self, target: &str, cx: &mut Context<Self>) {
        let was = self.call_pen.as_ref().map(|p| p.target.clone());
        if was.is_some() {
            // Yarım çizgi gönderilir, imleç diğerlerinden kalkar; fare normale döner.
            self.pen_up(cx);
            self.pen_gone(cx);
        }
        self.call_pen = if was.as_deref() == Some(target) { None } else { Some(crate::annotate::Pen::new(target)) };
        crate::trace(&format!("kalem: {} ({})", if self.call_pen.is_some() { "açık" } else { "kapalı" }, &target[..8.min(target.len())]));
        cx.notify();
    }

    fn pen_down(&mut self, position: gpui_kit::Point<Pixels>, source: Option<(u32, u32)>, cx: &mut Context<Self>) {
        let me = self.auth.user.id.clone();
        let Some(pen) = self.call_pen.as_mut() else { return };
        let point = pen.normalize(position, source);
        pen.drawing = Some((crate::annotate::stroke_id(&me), vec![point], std::time::Instant::now()));
        self.pen_flush(cx);
    }

    fn pen_move(&mut self, position: gpui_kit::Point<Pixels>, pressed: bool, source: Option<(u32, u32)>, cx: &mut Context<Self>) {
        let (ink, _) = self.pen_ink(cx);
        let name = self.my_name(cx);
        let Some(pen) = self.call_pen.as_mut() else { return };
        let (x, y) = pen.normalize(position, source);
        // İmleç ~55 ms'de bir (StreamPlayer.tsx).
        if pen.pointer_sent.elapsed().as_millis() > 55 {
            pen.pointer_sent = std::time::Instant::now();
            let target = pen.target.clone();
            self.calls.update(cx, |c, _| c.send_pointer(&target, &name, &ink, x, y, false));
        }
        let Some(pen) = self.call_pen.as_mut() else { return };
        let Some(drawing) = pen.drawing.as_mut() else { return };
        if !pressed {
            return;
        }
        drawing.1.push((x, y));
        if drawing.2.elapsed().as_millis() > 60 {
            self.pen_flush(cx);
        }
    }

    fn pen_up(&mut self, cx: &mut Context<Self>) {
        if self.call_pen.as_ref().is_some_and(|p| p.drawing.is_some()) {
            self.pen_flush(cx);
            if let Some(pen) = self.call_pen.as_mut() {
                pen.drawing = None;
            }
        }
    }

    /// Biriken noktaları gönder (60 ms'de bir ve bırakınca).
    fn pen_flush(&mut self, cx: &mut Context<Self>) {
        let (ink, width) = self.pen_ink(cx);
        let Some(pen) = self.call_pen.as_mut() else { return };
        let target = pen.target.clone();
        let Some((id, buffer, sent)) = pen.drawing.as_mut() else { return };
        if buffer.is_empty() {
            return;
        }
        let points: Vec<(f32, f32)> = buffer.drain(..).collect();
        *sent = std::time::Instant::now();
        let id = id.clone();
        self.calls.update(cx, |c, cx| c.send_stroke(&target, &id, &ink, width, &points, cx));
    }

    /// İmleç yayından çıktı / kalem bırakıldı: diğerlerinde imleç kaybolsun.
    fn pen_gone(&mut self, cx: &mut Context<Self>) {
        let Some(target) = self.call_pen.as_ref().map(|p| p.target.clone()) else { return };
        let (ink, _) = self.pen_ink(cx);
        let name = self.my_name(cx);
        self.calls.update(cx, |c, _| c.send_pointer(&target, &name, &ink, 0.0, 0.0, true));
    }

    /// `.pen-tools`: kendi rengin + palet (15px daireler), kalınlık kaydırıcısı (2–14), önizleme.
    fn pen_tools(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let (ink, width) = self.pen_ink(cx);
        let own = crate::annotate::pen_color(&self.auth.user.id);
        let mut colors = div().flex().items_center().gap(px(4.));
        for (ix, color) in std::iter::once(own).chain(crate::annotate::PEN_PALETTE.iter().copied().filter(|c| *c != own)).enumerate() {
            let active = color == ink;
            let tip_text = if color == own { p2_i18n::t("callui.pen.ownColor") } else { p2_i18n::t("callui.pen.color") };
            colors = colors.child(
                div()
                    .id(SharedString::from(format!("pen-ink-{ix}")))
                    .size(px(15.))
                    .rounded_full()
                    .bg(crate::annotate::color(color))
                    .border_2()
                    .border_color(if active { gpui_kit::white() } else { gpui_kit::transparent_black() })
                    .shadow(vec![gpui_kit::BoxShadow { color: css_rgba(0, 0, 0, 0.45), offset: gpui_kit::point(px(0.), px(0.)), blur_radius: px(0.), spread_radius: px(1.), inset: false }])
                    .cursor_pointer()
                    .hover(|s| s.border_color(css_rgba(255, 255, 255, 0.45)))
                    .tooltip(tip(tip_text))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.save_media(|m| m.pen_ink = color.to_string(), cx);
                        cx.notify();
                    })),
            );
        }
        let sliders = self.call_ui.sliders.clone();
        let (s1, s2) = (sliders.clone(), sliders.clone());
        let pick = |b: Bounds<Pixels>, x: Pixels| (2. + ((f32::from(x) - f32::from(b.origin.x) - 8.) / (f32::from(b.size.width) - 16.)).clamp(0., 1.) * 12.).round() as u32;
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .pl(px(4.))
            .pr(px(8.))
            .py(px(3.))
            .rounded(px(9.))
            .border_1()
            .border_color(p.border)
            .bg(p.bg_2)
            .child(colors)
            .child(
                div()
                    .id("pen-width")
                    .relative()
                    .cursor_pointer()
                    .child(range_track((width - 2.) / 12., p.text_2, 64., p))
                    .child(canvas(move |b, _, _| { sliders.borrow_mut().insert("pen-width".into(), b); }, |_, _, _, _| {}).absolute().inset_0())
                    .tooltip(tip(p2_i18n::tv("callui.pen.width", &[("w", &format!("{width:.0}"))])))
                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &gpui_kit::MouseDownEvent, _, cx| {
                        if let Some(b) = s1.borrow().get("pen-width").copied() {
                            let v = pick(b, ev.position.x);
                            this.save_media(|m| m.pen_width = v, cx);
                            cx.notify();
                        }
                    }))
                    .on_mouse_move(cx.listener(move |this, ev: &gpui_kit::MouseMoveEvent, _, cx| {
                        if ev.pressed_button == Some(MouseButton::Left) {
                            if let Some(b) = s2.borrow().get("pen-width").copied() {
                                let v = pick(b, ev.position.x);
                                this.save_media(|m| m.pen_width = v, cx);
                                cx.notify();
                            }
                        }
                    })),
            )
            .child(div().size(px(width)).rounded_full().bg(crate::annotate::color(&ink)).shadow(vec![gpui_kit::BoxShadow { color: css_rgba(255, 255, 255, 0.25), offset: gpui_kit::point(px(0.), px(0.)), blur_radius: px(0.), spread_radius: px(1.), inset: false }]))
            .into_any_element()
    }

    /// Otomatik pilot (`P2_AUTOPILOT_DRAW=1`): ilk canlı yayını sesli odada izler, kalemi açar ve
    /// çizim alanını (mantıksal px + ölçek + kaynak boyutu) izine yazar; test kendi penceresine
    /// PostMessage ile fare sürükler (gerçek fare yolu).
    pub(super) fn autopilot_draw(&mut self, window: &mut gpui_kit::Window, cx: &mut Context<Self>) {
        if std::env::var_os("P2_AUTOPILOT_DRAW").is_none() {
            return;
        }
        cx.spawn_in(window, async move |this, cx| {
            for _ in 0..120 {
                cx.background_executor().timer(std::time::Duration::from_millis(500)).await;
                let done = this
                    .update_in(cx, |this, window, cx| {
                        let me = this.auth.user.id.clone();
                        let (target, channel) = {
                            let calls = this.calls.read(cx);
                            let t = calls.call.as_ref().and_then(|c| c.participants.iter().find(|p| p.user_id != me && p.screen && calls.videos.contains_key(&p.user_id)).map(|p| p.user_id.clone()));
                            (t, calls.call.as_ref().and_then(|c| c.channel_id.clone()))
                        };
                        let Some(target) = target else { return false };
                        if this.call_pen.is_none() {
                            // Electron: sesli odada yayını izle (StreamStage) ve kalemi aç.
                            if let Some(ch) = channel.and_then(|id| this.categories.iter().flat_map(|c| c.channels.iter()).find(|c| c.id == id).cloned()) {
                                if this.voice_channel(cx).map(|c| c.id) != Some(ch.id.clone()) {
                                    this.select_channel(ch, window, cx);
                                }
                            } else {
                                this.open_call_view(window, cx);
                            }
                            if !this.calls.read(cx).watching.contains(&target) {
                                let t = target.clone();
                                this.calls.update(cx, |c, cx| c.watch(Some(&t), cx));
                            }
                            this.toggle_pen(&target, cx);
                            return false;
                        }
                        let area = this.call_pen.as_ref().map(|p| p.area.get()).unwrap_or_default();
                        if f32::from(area.size.width) < 10.0 {
                            cx.notify();
                            return false;
                        }
                        let source = this.calls.read(cx).videos.get(&target).map(|v| (v.1, v.2)).unwrap_or((0, 0));
                        crate::trace(&format!(
                            "kalem: alan {:.0},{:.0},{:.0},{:.0} ölçek {:.2} kaynak {}x{}",
                            f32::from(area.origin.x),
                            f32::from(area.origin.y),
                            f32::from(area.size.width),
                            f32::from(area.size.height),
                            window.scale_factor(),
                            source.0,
                            source.1
                        ));
                        true
                    })
                    .unwrap_or(true);
                if done {
                    break;
                }
            }
        })
        .detach();
    }
}

/// `.voice-room-card` zemini: `radial-gradient(circle at 50% 35%, mix(bg, white 8%), bg 42%, bg-0 100%)`.
fn radial(w: f32, h: f32, bg: Hsla, p: &Palette) -> gpui_kit::Img {
    p2_ui::components::radial_background(w.max(1.), h.max(1.), (0.5, 0.35), &[(lighten(bg, 0.08), 0.), (bg, 0.42), (p.bg_0, 1.)], p.bg_0)
}

/// `.cards-toggle .chevron` (CSS kenarlık üçgeni: 8px geniş, 5px yüksek).
fn chevron(down: bool, color: Hsla) -> gpui_kit::Svg {
    let body: &'static str = if down { r#"<path d="M0 7H24L12 22Z"/>"# } else { r#"<path d="M0 22H24L12 7Z"/>"# };
    p2_ui::icons::Icon::raw(body).stroke(0.).filled().el().size(px(8.)).text_color(color)
}

