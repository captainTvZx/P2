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
use super::screen_picker::{PickerEvent, ScreenPicker};

/// Eski "arama sahnesi" yolu (artık kullanılmıyor; Electron'da arama paneli büyür).
#[allow(dead_code)]
pub(super) const CALL_STAGE: &str = "@arama";

/// Çalışma alanındaki arama arayüzü durumu (Electron bileşen state'leri).
#[derive(Default)]
pub(super) struct CallUi {
    /// `.call-dock.expanded` (CallOverlay `stage`).
    expanded: bool,
    /// "Kişi ekle" listesi (`invitePicker`).
    invite: bool,
    /// "Sessize al" denen gelen arama (`dismissedCallId`).
    dismissed: Option<String>,
    /// Sesli oda kart menüsü (kişi, pencere konumu).
    menu: Option<(String, f32, f32)>,
    /// VoiceRoomStage `cardsOpen` tersi.
    cards_hidden: bool,
    /// Sesli oda "Sohbeti göster" (`voiceChatOpen`).
    voice_chat: bool,
    /// Sesli oda sohbetinin başlığı çalışma alanında çizildiği için sohbet değişince yenile.
    chat_watch: Option<Subscription>,
    /// Sahnenin ölçülen boyutu (ızgara ve radyal zeminler bununla hesaplanır).
    stage_size: Rc<Cell<(f32, f32)>>,
    /// Üzerinde durulan kart / yayın (izle düğmesi, yayın çubuğu).
    hover: Option<String>,
    /// Sürüklenen kaydırıcı: (kimlik, iz sınırları).
    sliders: Rc<std::cell::RefCell<std::collections::HashMap<String, Bounds<Pixels>>>>,
}

// ------------------------------------------------------------------------------------------ yardımcılar

/// CallIcons.tsx: dolgulu tek renk ikon (`fill: currentColor; stroke: none`).
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
fn empty_room_background(w: f32, h: f32, p: &Palette) -> gpui_kit::Img {
    let a = 145f32.to_radians();
    let len = (w * a.sin()).abs() + (h * a.cos()).abs();
    let (dx, dy) = (a.sin() * len / 2., -a.cos() * len / 2.);
    let (cx, cy) = (w / 2., h / 2.);
    let (c0, _) = svg_color(p.bg_1);
    let (c1, _) = svg_color(p.blue_900);
    let (c2, _) = svg_color(p.bg_0);
    let (glow, ga) = svg_color(p.accent_soft_2);
    let (rx, ry) = (w * 0.8, h * 0.7);
    let markup = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><defs><linearGradient id="l" gradientUnits="userSpaceOnUse" x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}"><stop offset="0" stop-color="{c0}"/><stop offset=".52" stop-color="{c1}"/><stop offset="1" stop-color="{c2}"/></linearGradient><radialGradient id="r" gradientUnits="userSpaceOnUse" cx="{gx}" cy="{h}" r="{rx}" gradientTransform="translate({gx} {h}) scale(1 {sy}) translate(-{gx} -{h})"><stop offset="0" stop-color="{glow}" stop-opacity="{ga}"/><stop offset=".72" stop-color="{glow}" stop-opacity="0"/></radialGradient></defs><rect width="{w}" height="{h}" fill="url(#l)"/><rect width="{w}" height="{h}" fill="url(#r)"/></svg>"#,
        x1 = cx - dx,
        y1 = cy - dy,
        x2 = cx + dx,
        y2 = cy + dy,
        gx = w / 2.,
        sy = ry / rx,
    );
    p2_ui::icons::svg_image(markup).w(px(w)).h(px(h))
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
                self.calls.update(cx, |c, cx| c.start_direct(&id, window, cx));
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
                                this.call_ui.expanded = true;
                                crate::trace("arama: panel büyütüldü");
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

    /// Aramanın görünen adı: kanal adı, karşı taraf ya da grup (overlay_ui.rs kullanır).
    pub(super) fn call_title(&self, call: &CallState) -> String {
        match call.kind.as_str() {
            "channel" => {
                let id = call.channel_id.clone().unwrap_or_default();
                let name = self.categories.iter().flat_map(|c| c.channels.iter()).find(|c| c.id == id).map(|c| c.name.clone()).unwrap_or_else(|| "sesli oda".into());
                format!("🔊 {name}")
            }
            "group" => {
                let id = call.group_id.clone().unwrap_or_default();
                self.groups.iter().find(|g| g.id == id).map(|g| format!("👥 {}", g.name)).unwrap_or_else(|| "👥 Grup araması".into())
            }
            _ => {
                let me = &self.auth.user.id;
                let other = call.participants.iter().find(|p| &p.user_id != me).map(|p| p.display_name.clone()).or_else(|| {
                    let id = if call.caller.as_deref() == Some(me.as_str()) { call.callee.clone() } else { call.caller.clone() };
                    id.and_then(|id| self.find_member(&id)).map(|m| m.display_name)
                });
                other.unwrap_or_else(|| "Bireysel arama".into())
            }
        }
    }

    /// Kişinin görünen adı (proje kişileri; yoksa "Katılımcı").
    fn person_name(&self, user: &str) -> String {
        self.find_member(user).map(|m| m.display_name).unwrap_or_else(|| "Katılımcı".into())
    }

    // ------------------------------------------------------------ sohbet bağlantıları

    /// Sohbet açıldı: sesli odaysa başlığı çalışma alanı çizdiği için sohbet değişince yenile.
    pub(super) fn call_ui_chat_opened(&mut self, cx: &mut Context<Self>) {
        self.call_ui.chat_watch = None;
        self.call_ui.menu = None;
        self.call_ui.cards_hidden = false;
        self.call_ui.voice_chat = false;
        let Some(chat) = self.chat.clone() else { return };
        if self.voice_channel(cx).is_some() {
            chat.update(cx, |c, _| c.embedded = true);
            self.call_ui.chat_watch = Some(cx.observe(&chat, |_, _, cx| cx.notify()));
        }
    }

    pub(super) fn toggle_voice_chat(&mut self, cx: &mut Context<Self>) {
        self.call_ui.voice_chat = !self.call_ui.voice_chat;
        cx.notify();
    }

    /// Esc: kişi menüsü → büyütülmüş panel (ekran paylaşımı penceresi kendi Esc'ini işler).
    pub(super) fn call_ui_escape(&mut self, cx: &mut Context<Self>) -> bool {
        if self.picker.take().is_some() {
            return true;
        }
        if self.call_ui.menu.take().is_some() || std::mem::take(&mut self.call_ui.invite) {
            return true;
        }
        if self.calls.read(cx).call.is_some() && std::mem::take(&mut self.call_ui.expanded) {
            return true;
        }
        false
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
        let channel = match &chat.read(cx).subject {
            ChatSubject::Channel(c) => c.id.clone(),
            _ => return,
        };
        let in_call = self.calls.read(cx).call.as_ref().and_then(|c| c.channel_id.as_deref()) == Some(channel.as_str());
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
        let channel = self.voice_channel(cx)?;
        let chat: Entity<ChatPane> = self.chat.clone()?;
        let pp = p.clone();
        let header = chat.update(cx, |c, cx| c.header_element(&pp, cx));
        let stage = self.voice_stage(&channel, p, window, cx);
        let aside = self.call_ui.voice_chat.then(|| {
            let name = channel.name.clone();
            let (soft, strong) = (p.accent_soft, p.text_strong);
            // ChannelContent.tsx `useResizable('p2:voice-chat-width',430,300,980,-1)`; CSS `min-width: 300px; max-width: 80%`.
            div()
                .relative()
                .w(px(crate::resize::value(&crate::resize::VOICE_CHAT)))
                .min_w(px(300.))
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
                        .child(div().flex().flex_col().gap(px(1.)).child(div().text_size(fs(13.)).css_weight(700.).text_color(p.text_strong).child("Oda sohbeti")).child(div().text_size(fs(10.)).text_color(p.text_3).child(format!("#{name}"))))
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
                .child(crate::resize::handle("voice-chat-resize", crate::resize::VOICE_CHAT, crate::resize::Edge::Left, "Oda sohbeti genişliği", p))
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

    // ------------------------------------------------------------ sesli oda sahnesi

    /// Sesli odadaki kişiler (açık sesli oda kanalı).
    fn voice_room_people(&self, cx: &App) -> Vec<CallParticipant> {
        let Some(channel) = self.voice_channel(cx) else { return Vec::new() };
        self.calls.read(cx).voice_rooms.get(&channel.id).cloned().unwrap_or_default()
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
        let (inner_w, inner_h) = (w - 56., h - 56. - 16. - if joined { 0. } else { 39. });
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
        let Some(channel) = self.voice_channel(cx) else { return false };
        self.calls.read(cx).call.as_ref().and_then(|c| c.channel_id.as_deref()) == Some(channel.id.as_str())
    }

    /// `VoiceRoomStage`.
    fn voice_stage(&self, channel: &Channel, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let calls = self.calls.read(cx);
        let people = calls.voice_rooms.get(&channel.id).cloned().unwrap_or_default();
        let joined = calls.call.as_ref().and_then(|c| c.channel_id.as_deref()) == Some(channel.id.as_str());
        let can_connect = self.my_perms.can_in(&channel.id, "voice.connect");
        let me = self.auth.user.id.clone();
        let size = self.call_ui.stage_size.clone();
        let (sw, sh) = {
            let (w, h) = size.get();
            if w > 10. {
                (w, h)
            } else {
                let vp = window.viewport_size();
                (f32::from(vp.width) - 72. - crate::resize::value(&crate::resize::SIDEBAR) - if self.call_ui.voice_chat { crate::resize::value(&crate::resize::VOICE_CHAT) } else { 0. }, f32::from(vp.height) - 96.)
            }
        };
        // Sahnenin boyutu ölçülür; değişince yeniden çizilir (ızgara + radyal zeminler).
        let measure = canvas(
            move |bounds, window, _| {
                let s = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                if size.get() != s {
                    size.set(s);
                    window.refresh();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0();
        let join_id = channel.id.clone();
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
                .child("Sesli odaya katıl")
                .on_click(cx.listener(move |this, _, window, cx| this.calls.update(cx, |c, cx| c.join_channel(&id, window, cx))))
        };

        if people.is_empty() {
            return div()
                .relative()
                .flex_1()
                .min_w(px(300.))
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
                            .child(div().text_size(fs(24.)).css_weight(700.).text_color(p.text_strong).child(channel.name.clone()))
                            .child(div().mb(px(10.)).text_size(fs(12.)).text_color(p.text_3).child("Şu an sesli sohbette kimse yok"))
                            .when(!joined, |s| s.child(join(p, cx))),
                    ),
                )
                .into_any_element();
        }

        // İzlenen yayınlar (StreamStage): başkalarının izlediğim canlı yayınları + kendi paylaşımım.
        let live = |part: &CallParticipant| (part.screen || part.video) && calls.videos.contains_key(&part.user_id);
        let mut stage_items: Vec<(String, String, bool)> = people
            .iter()
            .filter(|x| x.user_id != me && calls.watching.contains(&x.user_id) && live(x))
            .map(|x| (x.user_id.clone(), format!("{}{}", x.display_name, if x.screen { " · ekran" } else { "" }), false))
            .collect();
        if people.iter().any(|x| x.user_id == me) && (calls.screen_on || calls.camera_on) {
            stage_items.push((me.clone(), if calls.screen_on { "Sen · ekran".into() } else { "Sen".into() }, true));
        }
        let featured = !stage_items.is_empty();
        let cards_open = !featured || !self.call_ui.cards_hidden;

        let pad = if featured { 12. } else { 28. };
        let (inner_w, inner_h) = (sw - pad * 2., sh - pad * 2.);
        // `grid-template-rows: minmax(0,1fr) auto` + 16 aralık: katılmadıysa ikinci satır 39px düğme.
        let grid_h = inner_h - 16. - if joined { 0. } else { 39. };
        let (cols, col_w, row_h) = if featured {
            // `.featured .voice-room-grid`: minmax(118px,150px) sütunlar, 64px satırlar, ortalı.
            let fit = (((inner_w + 7.) / (118. + 7.)).floor() as usize).max(1);
            let cols = people.len().clamp(1, fit);
            let used = cols as f32 * 150. + 7. * (cols as f32 - 1.);
            let w = if used <= inner_w { 150. } else { (inner_w - 7. * (cols as f32 - 1.)) / cols as f32 };
            (cols, w, 64.)
        } else {
            Self::room_grid(people.len(), inner_w, grid_h)
        };
        let gap = if featured { 7. } else { 12. };

        let mut grid = div().flex().flex_col().gap(px(gap)).flex_none();
        if featured {
            grid = grid.items_center();
        }
        for row in people.chunks(cols) {
            let mut r = div().flex().gap(px(gap));
            for part in row {
                r = r.child(self.voice_card(part, col_w, row_h, featured, p, cx));
            }
            grid = grid.child(r);
        }

        let mut body = div().relative().flex_1().min_w(px(300.)).h_full().flex().flex_col().p(px(pad)).gap(px(if featured { 8. } else { 16. })).overflow_hidden();
        if featured {
            body = body.child(div().flex_1().min_h_0().child(self.stream_stage(&stage_items, p, cx)));
            let n = people.len();
            let open = cards_open;
            body = body.child(
                div().flex().justify_center().child(
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
                        .child(if open { "Katılımcıları gizle".to_string() } else { format!("Katılımcılar ({n})") })
                        .tooltip(tip(if open { "Katılımcıları gizle" } else { "Katılımcıları göster" }))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.call_ui.cards_hidden = !this.call_ui.cards_hidden;
                            cx.notify();
                        })),
                ),
            );
            if cards_open {
                body = body.child(grid);
            }
        } else {
            body = body.child(div().h(px(grid_h)).flex_none().flex().flex_col().justify_center().child(grid));
        }
        if !joined {
            body = body.child(div().flex().justify_center().child(join(p, cx)));
        }
        div().relative().flex_1().min_w(px(300.)).h_full().child(measure).child(body).into_any_element()
    }

    /// `.voice-room-card` (normal ya da `.featured` küçük kart).
    fn voice_card(&self, part: &CallParticipant, w: f32, h: f32, small: bool, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let calls = self.calls.read(cx);
        let me = &self.auth.user.id;
        let hue = p.hues[avatar_hue(&part.user_id)];
        let level = if part.muted { 0. } else { calls.levels.get(&part.user_id).copied().unwrap_or(0.) };
        let speaking = level > 0.08;
        let watchable = &part.user_id != me && (part.screen || part.video) && calls.videos.contains_key(&part.user_id);
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
                this.call_ui.menu = Some((menu_id.clone(), f32::from(ev.position.x), f32::from(ev.position.y)));
                cx.stop_propagation();
                cx.notify();
            }))
            .on_click(cx.listener(move |this, _, _, cx| {
                if watchable {
                    let target = uid.clone();
                    this.calls.update(cx, |c, cx| c.watch(Some(&target), cx));
                } else {
                    this.open_profile_by_id(&uid, cx);
                }
            }))
            .child(bg.absolute().top(px(-2.)).left(px(-2.)))
            .child(
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
                        .child(user_initials(&part.display_name)),
                ),
            );
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
            .child(div().min_w_0().truncate().text_size(fs(if small { 9. } else { 13. })).css_weight(700.).child(part.display_name.clone()));
        if &part.user_id == me {
            tag = tag.child(div().flex_none().text_size(fs(8.)).css_weight(800.).text_color(p.text_accent).child("SEN"));
        }
        if part.screen {
            tag = tag.child(div().flex_none().ml(px(6.)).px(px(5.)).py(px(1.)).rounded(px(5.)).bg(p.danger).text_color(gpui_kit::white()).text_size(fs(8.)).css_weight(800.).tracking(0.04).child("YAYINDA"));
        }
        card = card.child(
            div()
                .absolute()
                .left(px(if small { 6. } else { 11. }))
                .right(px(if small { 6. } else { 11. }))
                .bottom(px(if small { 4. } else { 10. }))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(10.))
                .child(tag)
                .when(part.muted, |s| s.child(div().flex_none().p(px(5.)).rounded(px(7.)).bg(chip_bg(0.72)).child(call_icon("micOff", 17., p.danger_text)))),
        );
        if watchable && (hovered || watched) {
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
                        .child(if watched { "Yayını kapat" } else { "Yayını izle" })
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

    // ------------------------------------------------------------ izlenen yayınlar (StreamStage)

    fn stream_stage(&self, items: &[(String, String, bool)], p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let count = items.len().min(4);
        let mut rows: Vec<Vec<&(String, String, bool)>> = Vec::new();
        if count <= 2 {
            rows.push(items.iter().collect());
        } else {
            rows.push(items.iter().take(2).collect());
            rows.push(items.iter().skip(2).take(2).collect());
        }
        let mut stage = div().size_full().flex().flex_col().gap(px(8.));
        for row in rows {
            let mut r = div().flex_1().min_h_0().flex().gap(px(8.));
            for (user, label, own) in row {
                r = r.child(self.stream_player(user, label, *own, p, cx));
            }
            stage = stage.child(r);
        }
        stage.into_any_element()
    }

    /// `.stream-player` (StreamPlayer.tsx): görüntü, etiket, çizimler, çubuk (üzerine gelince).
    fn stream_player(&self, user: &str, label: &str, own: bool, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let calls = self.calls.read(cx);
        let video = calls.videos.get(user).map(|v| (v.0.clone(), v.1, v.2));
        let source = video.as_ref().map(|v| (v.1, v.2));
        let pen_here = self.call_pen.as_ref().is_some_and(|pen| pen.target == user);
        let hover_key = format!("stream:{user}");
        let hovered = self.call_ui.hover.as_deref() == Some(hover_key.as_str());
        let mut player = div()
            .id(SharedString::from(format!("stream-{user}")))
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .rounded(px(11.))
            .border_1()
            .border_color(p.border)
            .bg(p.bg_0)
            .on_hover(cx.listener(move |this, on: &bool, _, cx| {
                let next = on.then(|| hover_key.clone());
                if this.call_ui.hover != next {
                    this.call_ui.hover = next;
                    cx.notify();
                }
            }));
        if own {
            // Rust'ta kendi yayınımın önizleme karesi yok: Electron'daki "önizleme kapalı" kartı.
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
                    .child(div().text_size(fs(12.5)).css_weight(700.).text_color(p.text).child("Ekranını paylaşıyorsun"))
                    .child(div().text_size(fs(10.)).child("Önizleme kapalı")),
            );
        } else if let Some((image, _, _)) = video {
            player = player.child(img(ImageSource::Render(image)).size_full().object_fit(ObjectFit::Contain));
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
                .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                    if !*hovered {
                        this.pen_gone(cx)
                    }
                }))
                .child(canvas(move |bounds, _, _| area.set(bounds), |_, _, _, _| {}).absolute().inset_0());
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
        // `.stream-label` (sol üst).
        player = player.child(div().absolute().left(px(12.)).top(px(10.)).px(px(9.)).py(px(4.)).rounded(px(8.)).bg(chip_bg(0.74)).text_color(p.text_strong).text_size(fs(12.)).css_weight(650.).child(label.to_string()));
        // `.stream-bar`: yalnız üzerine gelince (kalemle çizerken gizli).
        if hovered && !(pen_here && self.call_pen.as_ref().is_some_and(|pen| pen.drawing.is_some())) {
            player = player.child(self.stream_bar(user, own, pen_here, p, cx));
        }
        player.into_any_element()
    }

    /// `.stream-bar` (izleyici: duraklat, zaman çizgisi, CANLI, ses, kalite, kalem, tam ekran, kapat).
    fn stream_bar(&self, user: &str, own: bool, pen_here: bool, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let (volume, call_channel) = {
            let calls = self.calls.read(cx);
            (calls.user_volume(user), calls.call.as_ref().and_then(|c| c.channel_id.clone()))
        };
        let icon_btn = |id: String, name: &'static str, tip_text: &'static str, active: bool, color: Option<Hsla>, p: &Palette| {
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
                .child(icon_btn(format!("pause-{user}"), "pause", "Duraklat", false, None, p))
                .child(div().flex_1().min_w(px(60.)).h(px(4.)).rounded(px(2.)).bg(p.accent).opacity(0.4))
                .child(div().flex().items_center().gap(px(5.)).px(px(2.)).text_color(p.danger_text).text_size(fs(9.5)).css_weight(800.).tracking(0.06).child(div().size(px(6.)).rounded_full().bg(p.danger)).child("CANLI"))
                .child(div().flex().items_center().gap(px(6.)).text_color(p.text_2).child(call_icon(if volume > 0 { "volume" } else { "volumeOff" }, 15., p.text_2)).child(range_track(volume as f32 / 100., p.accent_hover, 64., p)))
                .child(self.quality_menu(user, p, cx));
        }
        // Kalem: renkler + kalınlık + önizleme; "Ekrana çiz" / "Kalemi bırak".
        if pen_here {
            bar = bar.child(self.pen_tools(p, cx));
        }
        let ink = self.pen_ink(cx).0;
        let pen_target = target.clone();
        if !own && self.can_annotate(call_channel.as_deref()) {
            bar = bar.child(
                icon_btn(format!("pen-{user}"), "pen", if pen_here { "Kalemi bırak" } else { "Ekrana çiz" }, pen_here, pen_here.then(|| crate::annotate::color(&ink)), p)
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_pen(&pen_target, cx))),
            );
        }
        if own {
            bar = bar.child(icon_btn(format!("preview-{user}"), "camera", "Önizlemeyi göster", false, None, p));
        } else {
            bar = bar.child(icon_btn(format!("full-{user}"), "expand", "Tam ekran", false, None, p));
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
                    .tooltip(tip("Yayını kapat"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let user = close.clone();
                        this.calls.update(cx, |c, cx| c.watch(Some(&user), cx));
                    })),
            );
        }
        bar.into_any_element()
    }

    /// QualityMenu.tsx `.quality-trigger` (kapalı hâli; seçenekler tıklayınca sırayla).
    fn quality_menu(&self, user: &str, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        const LEVELS: [(&str, &str); 5] = [("auto", "Otomatik"), ("1080p", "1080p"), ("720p", "720p"), ("480p", "480p"), ("360p", "360p")];
        let current = self.calls.read(cx).viewer_quality.get(user).cloned().unwrap_or_else(|| "auto".into());
        let ix = LEVELS.iter().position(|l| l.0 == current).unwrap_or(0);
        let next = LEVELS[(ix + 1) % LEVELS.len()].0;
        let target = user.to_string();
        let (accent, bg2, strong) = (p.border_accent, p.bg_2, p.text_strong);
        div()
            .id(SharedString::from(format!("quality-{user}")))
            .h(px(24.))
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(8.))
            .border_1()
            .border_color(p.border)
            .bg(chip_bg(0.82))
            .text_color(p.text_2)
            .text_size(fs(10.))
            .css_weight(650.)
            .cursor_pointer()
            .hover(move |s| s.border_color(accent).bg(bg2).text_color(strong))
            .child(LEVELS[ix].1)
            .child(caret(alpha(p.text_2, 0.75)))
            .tooltip(tip("Yayını hangi kalitede alacağını seç"))
            .on_click(cx.listener(move |this, _, _, cx| {
                let user = target.clone();
                this.calls.update(cx, |c, cx| c.request_quality(&user, next, cx));
            }))
            .into_any_element()
    }

    // ------------------------------------------------------------ kişi menüsü (.voice-user-menu)

    pub(super) fn voice_menu_view(&self, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (user, x, y) = self.call_ui.menu.clone()?;
        let people = self.voice_room_people(cx);
        let part = people.iter().find(|x| x.user_id == user)?.clone();
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
        let slider = |id: &'static str, label: &'static str, value: u32, p: &Palette, on: Rc<dyn Fn(&mut Self, u32, &mut Context<Self>)>, cx: &mut Context<Self>| {
            let sliders = self.call_ui.sliders.clone();
            let (down, moving) = (on.clone(), on);
            let (s1, s2) = (sliders.clone(), sliders.clone());
            let pick = |b: Bounds<Pixels>, x: Pixels| (((f32::from(x) - f32::from(b.origin.x) - 8.) / (f32::from(b.size.width) - 16.)).clamp(0., 1.) * 100.).round() as u32;
            div()
                .px(px(5.))
                .py(px(7.))
                .flex()
                .flex_col()
                .gap(px(7.))
                .text_size(fs(10.))
                .text_color(p.text_2)
                .child(div().flex().justify_between().child(label).child(div().text_size(fs(9.)).css_weight(700.).text_color(p.text_accent).child(format!("{value}%"))))
                .child(
                    div()
                        .id(id)
                        .m(px(2.))
                        .relative()
                        .cursor_pointer()
                        .child(range_track(value as f32 / 100., p.accent, 206., p))
                        .child(canvas(move |b, _, _| { sliders.borrow_mut().insert(id.to_string(), b); }, |_, _, _, _| {}).absolute().inset_0())
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &gpui_kit::MouseDownEvent, _, cx| {
                            if let Some(b) = s1.borrow().get(id).copied() {
                                down(this, pick(b, ev.position.x), cx);
                            }
                        }))
                        .on_mouse_move(cx.listener(move |this, ev: &gpui_kit::MouseMoveEvent, _, cx| {
                            if ev.pressed_button == Some(MouseButton::Left) {
                                if let Some(b) = s2.borrow().get(id).copied() {
                                    moving(this, pick(b, ev.position.x), cx);
                                }
                            }
                        })),
                )
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
                            .child(user_initials(&part.display_name)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(div().text_size(fs(12.)).css_weight(700.).text_color(p.text).child(part.display_name.clone()))
                            .child(div().text_size(fs(9.)).text_color(p.text_3).child(if is_me { "Kendi ses ayarların" } else { "Kişisel ses ayarları" })),
                    ),
            );
        let close = |this: &mut Self, cx: &mut Context<Self>| {
            this.call_ui.menu = None;
            cx.notify();
        };
        if !is_me && live {
            let target = user.clone();
            menu = menu
                .child(item("vm-watch", "screen", if watched { "Yayını kapat".into() } else { "Yayını izle".into() }, p).on_click(cx.listener(move |this, _, _, cx| {
                    let t = target.clone();
                    this.calls.update(cx, |c, cx| c.watch(Some(&t), cx));
                    close(this, cx);
                })))
                .child(super::super::chat::view::menu_hr(p));
        }
        if is_me {
            let s = settings.clone();
            menu = menu
                .child(slider("vm-in", "Mikrofon seviyem", s.input_volume.min(100), p, Rc::new(|this, v, cx| this.set_media_volume(true, v, cx)), cx))
                .child(slider("vm-out", "Hoparlör seviyem", s.output_volume.min(100), p, Rc::new(|this, v, cx| this.set_media_volume(false, v, cx)), cx))
                .child(super::super::chat::view::menu_hr(p))
                .child(item("vm-mute", if muted { "mic" } else { "micOff" }, if muted { "Mikrofonumu aç".into() } else { "Mikrofonumu sustur".into() }, p).on_click(cx.listener(move |this, _, _, cx| {
                    this.calls.update(cx, |c, cx| c.toggle_mute(cx));
                    close(this, cx);
                })))
                .child(item("vm-deafen", "headphones", if deafened { "Sağırlaştırmayı kapat".into() } else { "Kendimi sağırlaştır".into() }, p).on_click(cx.listener(move |this, _, _, cx| {
                    this.calls.update(cx, |c, cx| c.toggle_deafen(cx));
                    close(this, cx);
                })));
        } else {
            let (t1, t2, t3) = (user.clone(), user.clone(), user.clone());
            menu = menu
                .child(slider("vm-user", "Kullanıcı ses seviyesi", volume, p, Rc::new(move |this, v, cx| this.calls.update(cx, |c, cx| c.set_user_volume(&t1, v, cx))), cx))
                .child(super::super::chat::view::menu_hr(p))
                .child(item("vm-local", if locally { "volume" } else { "volumeOff" }, if locally { "Susturmayı kaldır".into() } else { "Kendim için sustur".into() }, p).on_click(cx.listener(move |this, _, _, cx| {
                    let t = t2.clone();
                    this.calls.update(cx, |c, cx| c.toggle_local_mute(&t, cx));
                    close(this, cx);
                })))
                .child(item("vm-send", "headphones", if blocked { "Sesimi yeniden gönder".into() } else { "Sesimi bu kişiye gönderme".into() }, p).on_click(cx.listener(move |this, _, _, cx| {
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
        let speaking = calls.speaking.clone();
        let peer_states = calls.peer_states.clone();
        let (connected_at, started_at) = (calls.connected_at, calls.started_at);
        let me = self.auth.user.id.clone();
        let expanded = self.call_ui.expanded;
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
        let ringing = connected_at.is_none();
        let headline = if call.kind == "channel" { "Sesli oda".to_string() } else { roster.first().map(|r| r.0.display_name.clone()).unwrap_or_else(|| "Sesli arama".into()) };
        let since = |at: Option<std::time::Instant>| at.map(|t| t.elapsed().as_secs()).unwrap_or(0);
        let subtitle: AnyElement = if ringing {
            div()
                .text_color(p.text_accent)
                .child(format!("Çalıyor… {}", mmss(since(started_at))))
                .with_animation("ringing-fade", gpui_kit::Animation::new(Duration::from_millis(1600)).repeat(), |el, t| el.opacity(0.55 + 0.45 * (1. - (t * std::f32::consts::TAU).cos()) / 2.))
                .into_any_element()
        } else {
            div().child(format!("{} · {} kişi", mmss(since(connected_at)), call.participants.len())).into_any_element()
        };
        // Bağlantı notu (CallOverlay `linkNote`).
        let link_note: Option<(String, bool)> = if ringing {
            None
        } else {
            let pending: Vec<&CallParticipant> = others.iter().filter(|x| peer_states.get(&x.user_id).map(String::as_str) != Some("connected")).collect();
            if pending.is_empty() {
                None
            } else if let Some(broken) = pending.iter().find(|x| matches!(peer_states.get(&x.user_id).map(String::as_str), Some("failed" | "disconnected"))) {
                Some((format!("{} ile ses bağlantısı kurulamadı — Ayarlar > Ses ve Görüntü'den TURN sunucusu ekle.", broken.display_name), true))
            } else {
                Some((format!("Ses bağlantısı kuruluyor… ({})", pending.iter().map(|x| peer_states.get(&x.user_id).cloned().unwrap_or_else(|| "yeni".into())).collect::<Vec<_>>().join(", ")), false))
            }
        };
        let (muted, deafened, camera, screen) = (calls.muted, calls.deafened, calls.camera_on, calls.screen_on);
        let error = calls.error.as_ref().map(|e| e.0.clone());

        let control = |id: &'static str, name: &'static str, tip_text: &'static str, state: u8, p: &Palette| {
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
            .child(control("dock-mute", if muted { "micOff" } else { "mic" }, if muted { "Mikrofonu aç" } else { "Sustur" }, if muted { 2 } else { 0 }, p).on_click(cx.listener(|this, _, _, cx| this.calls.update(cx, |c, cx| c.toggle_mute(cx)))))
            .child(control("dock-deafen", if deafened { "headphonesOff" } else { "headphones" }, if deafened { "Sesi aç" } else { "Sesi kapat" }, if deafened { 2 } else { 0 }, p).on_click(cx.listener(|this, _, _, cx| this.calls.update(cx, |c, cx| c.toggle_deafen(cx)))))
            .child(control("dock-camera", if camera { "camera" } else { "cameraOff" }, if camera { "Kamerayı kapat" } else { "Kamerayı aç" }, if camera { 1 } else { 0 }, p).on_click(cx.listener(|this, _, _, cx| this.calls.update(cx, |c, cx| c.toggle_camera(cx)))))
            .child(control("dock-screen", if screen { "screen" } else { "screenOff" }, if screen { "Paylaşımı durdur" } else { "Ekran paylaş" }, if screen { 1 } else { 0 }, p).on_click(cx.listener(|this, _, window, cx| this.toggle_screen(window, cx))))
            .child(control("dock-invite", "personAdd", "Kişi ekle", if invite_open { 1 } else { 0 }, p).on_click(cx.listener(|this, _, _, cx| {
                this.call_ui.invite = !this.call_ui.invite;
                cx.notify();
            })))
            .child(control("dock-hangup", "hangup", "Aramadan ayrıl", 2, p).on_click(cx.listener(|this, _, window, cx| this.calls.update(cx, |c, cx| c.leave(window, cx)))));

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
                    ),
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
        if expanded {
            dock = dock.child(self.call_stage_tiles(&roster, ringing, p, cx));
        } else {
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
                        .child(user_initials(&part.display_name))
                        .tooltip(tip(format!("{}{} — profili aç", part.display_name, if *pending { " · bekleniyor" } else { "" })))
                        .on_click(cx.listener(move |this, _, _, cx| this.open_profile_by_id(&uid, cx))),
                );
            }
            if roster.is_empty() {
                chips = chips.child(div().text_size(fs(11.3)).text_color(p.text).child("Odada tek kişisin"));
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
                        .child(div().size(px(28.)).flex().items_center().justify_center().rounded(px(10.)).border_2().border_color(gpui_kit::transparent_black()).bg(gpui_kit::linear_gradient(145., gpui_kit::linear_color_stop(hue.bg, 0.), gpui_kit::linear_color_stop(hue.bg2, 1.))).text_color(hue.fg).text_size(fs(9.)).line_height(px(24.)).css_weight(800.).child(user_initials(&person.display_name)))
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
                list = list.child(div().text_size(fs(10.)).text_color(p.text_3).child("Eklenecek başka kimse yok."));
            }
            dock = dock.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .pt(px(8.))
                    .border_t_1()
                    .border_color(p.border_subtle)
                    .child(div().text_size(fs(9.)).css_weight(800.).tracking(0.18).text_color(p.text_accent).child("ARAMAYA EKLE"))
                    .child(list),
            );
        }
        if let Some(e) = error {
            dock = dock.child(div().text_size(fs(10.)).text_color(p.danger_text).child(e));
        }
        let el = if expanded {
            let vp = window.viewport_size();
            let w = (f32::from(vp.width) - 140.).min(760.);
            div().absolute().inset_0().flex().items_center().justify_center().child(dock.w(px(w)).p2_enter("call-dock-expanded", Entrance::Fade, Duration::ZERO)).into_any_element()
        } else {
            // `left: 78px; bottom: 12px` (pencere; kök başlık çubuğunun altında olduğundan aynı).
            div().absolute().left(px(78.)).bottom(px(12.)).w(px(274.)).child(dock.p2_enter("call-dock", Entrance::PopElastic, Duration::ZERO)).into_any_element()
        };
        Some(el)
    }

    /// `.call-stage` (büyütülmüş panel): görüntü ve avatar kutucukları.
    fn call_stage_tiles(&self, roster: &[(CallParticipant, bool)], ringing: bool, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let (speaking_set, videos, started_at) = {
            let calls = self.calls.read(cx);
            (calls.speaking.clone(), calls.videos.iter().map(|(k, v)| (k.clone(), v.0.clone())).collect::<std::collections::HashMap<_, _>>(), calls.started_at)
        };
        let me = self.auth.user.id.clone();
        let tile_base = |id: SharedString, speaking: bool, p: &Palette| {
            div()
                .id(id)
                .relative()
                .flex_1()
                .min_w(px(210.))
                .h(px(160.))
                .overflow_hidden()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(11.))
                .border_2()
                .border_color(if speaking { p.success } else { p.border_subtle })
                .bg(p.bg_1)
                .when(speaking, |s| s.shadow(vec![gpui_kit::BoxShadow { color: css_rgba(61, 220, 132, 0.3), offset: gpui_kit::point(px(0.), px(0.)), blur_radius: px(14.), spread_radius: px(0.), inset: false }]))
        };
        let footer = |label: String, muted: bool, deafened: bool, p: &Palette| {
            div()
                .absolute()
                .left(px(8.))
                .right(px(8.))
                .bottom(px(7.))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(6.))
                .child(div().px(px(7.)).py(px(2.)).rounded(px(7.)).bg(chip_bg(0.72)).text_color(p.text_strong).text_size(fs(10.)).css_weight(650.).child(label))
                .when(muted || deafened, |s| {
                    s.child(div().flex().items_center().gap(px(4.)).px(px(6.)).py(px(2.)).rounded(px(7.)).bg(chip_bg(0.72)).child(call_icon("micOff", 15., p.danger_text)).when(deafened, |s| s.child(call_icon("headphonesOff", 15., p.danger_text))))
                })
        };
        let mut tiles: Vec<AnyElement> = Vec::new();
        for (part, pending) in roster {
            let speaking = speaking_set.contains(&part.user_id);
            if let Some(image) = videos.get(&part.user_id).cloned() {
                let label = format!("{}{}", part.display_name, if part.screen { " · ekran" } else { "" });
                tiles.push(
                    tile_base(SharedString::from(format!("call-tile-{}", part.user_id)), speaking, p)
                        .child(img(ImageSource::Render(image)).size_full().object_fit(ObjectFit::Cover))
                        .child(footer(label, part.muted, part.deafened, p).child(self.quality_menu(&part.user_id, p, cx)))
                        .p2_enter(SharedString::from(format!("call-tile-in-{}", part.user_id)), Entrance::ScaleIn, Duration::ZERO)
                        .into_any_element(),
                );
                continue;
            }
            let hue = p.hues[avatar_hue(&part.user_id)];
            let uid = part.user_id.clone();
            tiles.push(
                tile_base(SharedString::from(format!("call-tile-{}", part.user_id)), speaking, p)
                    .cursor_pointer()
                    .child(
                        div()
                            .size(px(52.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(17.))
                            .bg(gpui_kit::linear_gradient(145., gpui_kit::linear_color_stop(hue.bg, 0.), gpui_kit::linear_color_stop(hue.bg2, 1.)))
                            .text_color(hue.fg)
                            .text_size(fs(16.))
                            .line_height(px(52.))
                            .css_weight(800.)
                            .tracking(-0.02)
                            .child(user_initials(&part.display_name)),
                    )
                    .child(footer(format!("{}{}", part.display_name, if *pending { " · çalıyor" } else { "" }), part.muted, part.deafened, p))
                    .tooltip(tip(format!("{} profilini aç", part.display_name)))
                    .on_click(cx.listener(move |this, _, _, cx| this.open_profile_by_id(&uid, cx)))
                    .p2_enter(SharedString::from(format!("call-tile-in-{}", part.user_id)), Entrance::ScaleIn, Duration::ZERO)
                    .into_any_element(),
            );
        }
        let _ = me;
        let mut stage = div().id("call-stage").flex().flex_wrap().gap(px(9.)).p(px(2.)).max_h(px(460.)).overflow_y_scroll().children(tiles);
        if roster.is_empty() {
            let text = if ringing { format!("Çalıyor… {}", mmss(started_at.map(|t| t.elapsed().as_secs()).unwrap_or(0))) } else { "Karşı tarafın katılması bekleniyor…".into() };
            stage = stage.child(div().w_full().py(px(26.)).flex().justify_center().text_size(fs(11.)).text_color(p.text_3).child(text));
        }
        stage.into_any_element()
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
        let button = |id: &'static str, label: &'static str, _p: &Palette| div().id(id).flex_1().h(px(38.)).flex().items_center().justify_center().rounded(px(11.)).border_1().text_size(fs(12.)).css_weight(700.).cursor_pointer().child(label);
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
                    .child(user_initials(&incoming.from_name)),
            )
            .child(div().mt(px(12.)).text_size(fs(17.)).css_weight(700.).text_color(p.text_strong).child(incoming.from_name.clone()))
            .child(div().text_size(fs(11.)).text_color(p.text_3).child(if incoming.call.kind == "channel" { "Kanal araması" } else { "Seni arıyor" }))
            .child(
                div()
                    .w_full()
                    .mt(px(18.))
                    .flex()
                    .gap(px(7.))
                    .child(button("incoming-decline", "Reddet", p).border_color(p.danger_border).bg(p.danger_soft).text_color(p.danger_text).hover(move |s| s.bg(danger).border_color(danger_border).text_color(white)).on_click(cx.listener(|this, _, window, cx| this.calls.update(cx, |c, cx| c.decline(window, cx)))))
                    .child(button("incoming-silence", "Sessize al", p).border_color(p.border).bg(p.bg_3).text_color(p.text_2).hover(move |s| s.bg(hover_bg).text_color(strong)).on_click(cx.listener(move |this, _, _, cx| {
                        this.call_ui.dismissed = Some(call_id.clone());
                        this.calls.update(cx, |c, cx| c.silence_incoming(cx));
                        cx.notify();
                    })))
                    .child({
                        let accept = button("incoming-accept", if joining { "Bağlanıyor…" } else { "Kabul et" }, p)
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
            if let PickerEvent::Picked(source, fps, quality, audio) = event {
                let (source, fps, quality, audio) = (source.clone(), *fps, quality.clone(), *audio);
                this.save_media(|m| m.share_audio = audio, cx);
                this.calls.update(cx, |c, cx| c.start_screen(source, fps, &quality, audio, cx));
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
        self.calls.read(cx).call.as_ref().and_then(|c| c.participants.iter().find(|p| &p.user_id == me).map(|p| p.display_name.clone())).unwrap_or_else(|| "Sen".into())
    }

    pub(super) fn toggle_pen(&mut self, target: &str, cx: &mut Context<Self>) {
        let was = self.call_pen.as_ref().map(|p| p.target.clone());
        if was.is_some() {
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
            let tip_text: &'static str = if color == own { "Kendi rengin" } else { "Renk" };
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
                    .tooltip(tip(format!("Kalınlık: {width:.0}px")))
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
                                this.call_ui.expanded = true;
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

/// `.quality-trigger > i` (7×4 aşağı üçgen).
fn caret(color: Hsla) -> gpui_kit::Svg {
    p2_ui::icons::Icon::raw(r#"<path d="M0 6H24L12 20Z"/>"#).stroke(0.).filled().el().size(px(7.)).text_color(color)
}
