//! Çalışma alanının sohbet araçları: iş parçacığı / forum konusu paneli, mesaj iletme seçicisi
//! ve kanal tarayıcısı (ChannelContent.tsx thread paneli, ilet menüsü, ChannelBrowser.tsx).

use std::collections::HashSet;
use p2_ui::P2Fx as _; // backdrop_blur

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, Entity, FontWeight, SharedString, Subscription, Window, div, px};
use p2_client_core::realtime::RtEvent;
use p2_client_core::{Channel, ChannelType, ChatTarget, Message};
use p2_ui::controls::{BtnSize, BtnVariant, ModalSize, TabsStyle, button, kicker, lh, modal, modal_body, modal_layer, tab, tabs};
use p2_ui::form::{self, InputKind};
use p2_ui::tokens::{Palette, font_size, fs, radius};
use p2_ui::{Entrance, P2Motion as _};
use serde_json::json;

use super::WorkspaceScreen;
use super::super::admin::channel_icon;
use super::super::thread::{ThreadEvent, ThreadPane};
use crate::task;

pub(super) struct Browser {
    search: Entity<InputState>,
    /// 0 tümü, 1 katıldıkların, 2 ayrıldıkların
    filter: u8,
    _sub: Subscription,
}

fn kind_label(kind: &ChannelType) -> &'static str {
    match kind {
        ChannelType::Text => "Metin kanalı",
        ChannelType::Announcement => "Duyuru kanalı",
        ChannelType::Voice => "Sesli oda",
        ChannelType::Forum => "Forum",
    }
}

fn icon(kind: &ChannelType) -> &'static str {
    match kind {
        ChannelType::Announcement => "📣",
        ChannelType::Forum => "💬",
        ChannelType::Voice => "🔊",
        ChannelType::Text => "#",
    }
}

fn scrim(id: &'static str, p: &Palette) -> gpui_kit::Stateful<gpui_kit::Div> {
    div().id(id).absolute().inset_0().flex().items_center().justify_center().bg(p.overlay).backdrop_blur(p2_ui::fx::BLUR_SM)
}

impl WorkspaceScreen {
    // ------------------------------------------------------------ iş parçacığı

    pub(super) fn open_thread(&mut self, parent: Message, window: &mut Window, cx: &mut Context<Self>) {
        let Some(chat) = &self.chat else { return };
        let (target, forum, title) = {
            let c = chat.read(cx);
            (c.target.clone(), c.forum, c.label())
        };
        if !target.supports_threads() {
            return;
        }
        crate::trace(&format!("konu: açıldı «{}»", parent.content.lines().next().unwrap_or_default()));
        let (api, me) = (self.api.clone(), self.auth.user.clone());
        let pane = cx.new(|cx| ThreadPane::new(api, target, parent, forum, me, window, cx));
        // L ajanı: silme yetkisi sohbetle aynı.
        let can_delete_any = chat.read(cx).can_delete_any;
        pane.update(cx, |t, _| {
            t.title = title;
            t.can_delete_any = can_delete_any;
        });
        let sub = cx.subscribe_in(&pane, window, |this, _, event: &ThreadEvent, window, cx| match event {
            ThreadEvent::Close => this.close_thread(cx),
            ThreadEvent::Count { parent, replies } => {
                if let Some(chat) = &this.chat {
                    chat.update(cx, |c, cx| c.set_replies(parent, *replies, cx));
                }
            }
            // L ajanı: iş parçacığı menüsünden sohbetin iletişim kutuları.
            ThreadEvent::Delete(m) => {
                if let Some(chat) = &this.chat {
                    let m = m.clone();
                    chat.update(cx, |c, cx| c.ask_delete(m, cx));
                }
            }
            ThreadEvent::Forward(id) => this.start_forward(id.clone(), window, cx),
            ThreadEvent::History(id) => {
                if let Some(chat) = &this.chat {
                    let id = id.clone();
                    chat.update(cx, |c, cx| c.show_history(id, window, cx));
                }
            }
        });
        // L ajanı: forum konusu yan panel değil, akışın yerine (ChannelContent `.forum-topic-view`).
        if forum {
            let view = pane.clone();
            chat.update(cx, |c, cx| c.set_forum_topic(Some(view), cx));
        }
        self.thread = Some((pane, sub));
        cx.notify();
    }

    pub(super) fn close_thread(&mut self, cx: &mut Context<Self>) {
        if self.thread.take().is_some() {
            // L ajanı: forum konusu kapandı → konu listesi.
            if let Some(chat) = &self.chat {
                chat.update(cx, |c, cx| c.set_forum_topic(None, cx));
            }
            cx.notify();
        }
    }

    /// Yanıt olayları (parent_id dolu): açık konuya ya da sohbet satırının sayacına.
    /// Olay işlendiyse true.
    pub(super) fn route_reply(&mut self, event: &RtEvent, open: bool, cx: &mut Context<Self>) -> bool {
        let thread = self.thread.as_ref().map(|t| t.0.clone());
        let in_thread = |parent: &str, cx: &Context<Self>| thread.as_ref().is_some_and(|t| t.read(cx).parent.id == parent);
        match event {
            RtEvent::MessageCreated { parent_id: Some(parent), message, .. } => {
                crate::trace(&format!("olay: yanıt geldi «{}»", message.content));
                if in_thread(parent, cx) {
                    let m = message.clone();
                    thread.unwrap().update(cx, |t, cx| t.on_created(m, cx));
                } else if open {
                    if let Some(chat) = &self.chat {
                        chat.update(cx, |c, cx| c.bump_replies(parent, cx));
                    }
                }
                true
            }
            RtEvent::MessageUpdated { parent_id: Some(parent), message, .. } => {
                if in_thread(parent, cx) {
                    let m = message.clone();
                    thread.unwrap().update(cx, |t, cx| t.on_updated(m, cx));
                }
                true
            }
            RtEvent::MessageDeleted { parent_id: Some(parent), message_id, .. } => {
                if in_thread(parent, cx) {
                    thread.unwrap().update(cx, |t, cx| t.on_deleted(message_id, cx));
                }
                true
            }
            _ => false,
        }
    }

    // ------------------------------------------------------------ iletme

    pub(super) fn start_forward(&mut self, message_id: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(chat) = &self.chat else { return };
        let target = chat.read(cx).target.clone();
        if !matches!(target, ChatTarget::Channel { .. }) {
            return;
        }
        self.forwarding = Some((target, message_id));
        cx.notify();
        if let Ok(name) = std::env::var("P2_AUTOPILOT_FORWARD") {
            if let Some(to) = self.categories.iter().flat_map(|c| c.channels.iter()).find(|c| c.name == name).cloned() {
                self.do_forward(to, window, cx);
            }
        }
    }

    fn do_forward(&mut self, to: Channel, window: &mut Window, cx: &mut Context<Self>) {
        let Some((target, message)) = self.forwarding.take() else { return };
        let api = self.api.clone();
        let (to_id, to_name) = (to.id.clone(), to.name.clone());
        task::run(window, cx, move || api.forward_message(&target, &message, &to_id), move |this, r, window, cx| match r {
            Ok(_) => {
                crate::trace(&format!("sohbet: mesaj #{to_name} kanalına iletildi"));
                this.note(format!("Mesaj #{to_name} kanalına iletildi."), cx);
            }
            Err(e) => this.fail(e, window, cx),
        });
        cx.notify();
    }

    /// İlet penceresi (C ajanı: ChannelContent.tsx message-action-dialog + forward-channel-list):
    /// 400px cam kart, 66px başlık (MESAJI İLET / Bir kanal seç), 40px satırlar; Electron'daki gibi
    /// projenin bütün kanalları (açık kanal ve sesli odalar dahil).
    pub(super) fn forward_modal(&self, p: &Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        use p2_ui::P2Text as _;
        use p2_ui::tokens::fs;
        self.forwarding.as_ref()?;
        let mut list = div().id("fwd-list").max_h(px(400.)).overflow_y_scroll().p(px(8.)).flex().flex_col();
        for channel in self.categories.iter().flat_map(|c| c.channels.iter()) {
            let to = channel.clone();
            let (soft, strong) = (p.accent_soft, p.text_strong);
            let glyph: AnyElement = match channel.kind {
                ChannelType::Text => div().child("#").into_any_element(),
                ChannelType::Announcement => p2_ui::icons::icon("channel:announcement").stroke(1.8).el().size(px(15.)).text_color(p.text_accent).into_any_element(),
                ChannelType::Voice => p2_ui::icons::icon("channel:voice").stroke(1.8).el().size(px(15.)).text_color(p.text_accent).into_any_element(),
                ChannelType::Forum => p2_ui::icons::icon("channel:forum").stroke(1.8).el().size(px(15.)).text_color(p.text_accent).into_any_element(),
            };
            list = list.child(
                div()
                    .id(SharedString::from(format!("fwd-{}", channel.id)))
                    .w_full()
                    .flex_none()
                    .h(px(40.))
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .rounded(px(6.))
                    .text_color(p.text)
                    .cursor_pointer()
                    .hover(move |s| s.bg(soft).text_color(strong))
                    .child(div().w(px(27.)).child(glyph))
                    .child(div().flex_1().child(channel.name.clone()))
                    .on_click(cx.listener(move |this, _, window, cx| this.do_forward(to.clone(), window, cx))),
            );
        }
        let (soft, strong) = (p.accent_soft, p.text_strong);
        let card = div()
            .id("fwd-card")
            .w(px(400.))
            .max_h(px(520.))
            .overflow_hidden()
            .flex()
            .flex_col()
            .rounded(px(18.))
            .border_1()
            .border_color(p.border_strong)
            .bg(p.glass_strong.background())
            .shadow(p.shadow_lg())
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .h(px(66.))
                    .flex_none()
                    .px(px(17.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(p.border_subtle)
                    .child(div().flex().flex_col().child(div().text_size(fs(9.5)).css_weight(800.).tracking(p2_ui::tokens::KICKER_TRACKING_EM).text_color(p.text_accent).child("MESAJI İLET")).child(div().mt(px(3.)).text_size(fs(17.)).css_weight(700.).text_color(p.text_strong).child("Bir kanal seç")))
                    .child(
                        div()
                            .id("fwd-close")
                            .size(px(29.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .text_size(fs(19.))
                            .text_color(p.text_3)
                            .cursor_pointer()
                            .hover(move |s| s.bg(soft).text_color(strong))
                            .child("×")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.forwarding = None;
                                cx.notify();
                            })),
                    ),
            )
            .child(list)
            .p2_enter("fwd", Entrance::PopElastic, std::time::Duration::ZERO);
        Some(
            scrim("fwd-scrim", p)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.forwarding = None;
                    cx.notify();
                }))
                .child(card)
                .into_any_element(),
        )
    }

    // ------------------------------------------------------------ kanal tercihleri ve tarayıcı

    /// Ayrılınan (gizlenen) kanallar: kenar çubuğunda görünmez.
    pub(super) fn load_preferences(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (api, org) = (self.api.clone(), self.org.id.clone());
        task::run(window, cx, move || api.preferences(&org), |this, r, window, cx| {
            if let Ok(prefs) = r {
                this.starred = prefs.iter().filter(|p| p.starred).map(|p| format!("{}:{}", p.kind, p.target_id)).collect();
                this.notify_prefs = prefs.iter().filter(|p| p.kind == "CHANNEL" && !p.notify.is_empty()).map(|p| (p.target_id.clone(), p.notify.clone())).collect();
                this.hidden = prefs.into_iter().filter(|p| p.kind == "CHANNEL" && p.hidden).map(|p| p.target_id).collect::<HashSet<_>>();
                crate::trace(&format!("kanallar: {} kanaldan ayrılınmış", this.hidden.len()));
                this.sync_chat_head(cx);
                cx.notify();
            }
            this.autopilot_browser(window, cx);
        });
    }

    pub(super) fn autopilot_browser(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        static DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        let Ok(spec) = std::env::var("P2_AUTOPILOT_BROWSER") else { return };
        if self.categories.is_empty() || DONE.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        self.open_browser(window, cx);
        // "leave:adı" / "join:adı"
        if let Some((verb, name)) = spec.split_once(':') {
            if let Some(ch) = self.categories.iter().flat_map(|c| c.channels.iter()).find(|c| c.name == name).cloned() {
                self.set_hidden(ch.id, verb == "leave", window, cx);
            }
        }
    }

    pub(super) fn open_browser(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Kanal ara"));
        let sub = cx.subscribe(&search, |_, _, _: &InputEvent, cx| cx.notify());
        search.update(cx, |s, cx| s.focus(window, cx));
        self.browser = Some(Browser { search, filter: 0, _sub: sub });
        crate::trace("kanallar: tarayıcı açıldı");
        cx.notify();
    }

    /// Yıldızla / yıldızı kaldır (`kind`: CHANNEL | PERSON).
    pub(super) fn toggle_star(&mut self, kind: &'static str, target: String, window: &mut Window, cx: &mut Context<Self>) {
        let key = format!("{kind}:{target}");
        let on = !self.starred.contains(&key);
        if on {
            self.starred.insert(key.clone());
        } else {
            self.starred.remove(&key);
        }
        self.channel_menu = None;
        self.sync_chat_head(cx);
        cx.notify();
        let (api, org) = (self.api.clone(), self.org.id.clone());
        task::run(window, cx, move || api.set_preference(&org, kind, &target, json!({ "starred": on })), move |this, r, window, cx| match r {
            Ok(_) => crate::trace(&format!("tercih: {key} {}", if on { "yıldızlandı" } else { "yıldızı kaldırıldı" })),
            Err(e) => {
                this.starred.remove(&key);
                this.fail(e, window, cx);
            }
        });
    }

    /// Kanal bildirimi: ALL | MENTIONS | MUTED.
    pub(super) fn set_channel_notify(&mut self, channel: String, level: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        self.notify_prefs.insert(channel.clone(), level.to_string());
        self.channel_menu = None;
        self.sync_chat_head(cx);
        cx.notify();
        let (api, org) = (self.api.clone(), self.org.id.clone());
        task::run(window, cx, move || api.set_preference(&org, "CHANNEL", &channel, json!({ "notify": level })), move |this, r, window, cx| match r {
            Ok(_) => crate::trace(&format!("tercih: kanal bildirimi {level}")),
            Err(e) => this.fail(e, window, cx),
        });
    }

    pub(super) fn set_hidden(&mut self, channel: String, hidden: bool, window: &mut Window, cx: &mut Context<Self>) {
        let (api, org) = (self.api.clone(), self.org.id.clone());
        // İyimser: hemen uygula, hata olursa geri al.
        if hidden {
            self.hidden.insert(channel.clone());
        } else {
            self.hidden.remove(&channel);
        }
        cx.notify();
        let id = channel.clone();
        task::run(window, cx, move || api.set_preference(&org, "CHANNEL", &id, json!({ "hidden": hidden })), move |this, r, window, cx| match r {
            Ok(_) => crate::trace(&format!("kanallar: {} {}", channel, if hidden { "ayrılındı" } else { "katılındı" })),
            Err(e) => {
                if hidden {
                    this.hidden.remove(&channel);
                } else {
                    this.hidden.insert(channel.clone());
                }
                this.fail(e, window, cx);
            }
        });
    }

    /// ChannelBrowser.tsx: `Modal size="md" kicker="KANALLAR" title="Kanallara göz at"` + `.channel-browser-filters`
    /// (`.p2-input` + `.p2-tabs.segment`) + `.channel-browser-list` (satır: 26px ikon | ad/alt yazı | Ayrıl/Katıl).
    /// Ölçüler dom-styles (channel-browser) ile doğrulandı; Electron'da alt çubuk ve "Aç" düğmesi yok (adın kendisi açar).
    pub(super) fn browser_modal(&self, p: &Palette, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let browser = self.browser.as_ref()?;
        let query = browser.search.read(cx).value().trim().to_lowercase();
        let left = self.categories.iter().flat_map(|c| c.channels.iter()).filter(|c| self.hidden.contains(&c.id)).count();
        // max-height: min(52vh, 420px); padding 8 0
        let vh = f32::from(window.viewport_size().height);
        let mut list = div().id("browser-list").max_h(px((vh * 0.52).min(420.))).overflow_y_scroll().py(px(8.)).flex().flex_col();
        let mut shown = 0;
        let hover_bg = p.bg_hover;
        for category in &self.categories {
            for channel in &category.channels {
                let hidden = self.hidden.contains(&channel.id);
                if (browser.filter == 1 && hidden) || (browser.filter == 2 && !hidden) {
                    continue;
                }
                let topic = channel.topic.clone().unwrap_or_default();
                if !query.is_empty() && !channel.name.to_lowercase().contains(&query) && !topic.to_lowercase().contains(&query) {
                    continue;
                }
                shown += 1;
                let (id, open) = (channel.id.clone(), channel.clone());
                let sub = if topic.trim().is_empty() { format!("{} · {}", kind_label(&channel.kind), category.name) } else { topic.trim().to_string() };
                let kind = match channel.kind {
                    ChannelType::Announcement => "ANNOUNCEMENT",
                    ChannelType::Voice => "VOICE",
                    ChannelType::Forum => "FORUM",
                    ChannelType::Text => "TEXT",
                };
                let name = div()
                    .id(SharedString::from(format!("br-name-{}", channel.id)))
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(div().truncate().text_size(fs(12.)).line_height(lh(fs(12.))).font_weight(FontWeight::BOLD).text_color(p.text).child(channel.name.clone()))
                    .child(div().truncate().text_size(fs(10.)).line_height(lh(fs(10.))).text_color(p.text_3).child(sub))
                    .when(!hidden, |s| {
                        s.cursor_pointer().on_click(cx.listener(move |this, _, window, cx| {
                            this.browser = None;
                            this.select_channel(open.clone(), window, cx);
                        }))
                    });
                let toggle = button(SharedString::from(format!("br-tg-{}", channel.id)), if hidden { BtnVariant::Primary } else { BtnVariant::Secondary }, BtnSize::Sm, false, false, p)
                    .child(if hidden { "Katıl" } else { "Ayrıl" })
                    .on_click(cx.listener(move |this, _, window, cx| this.set_hidden(id.clone(), !hidden, window, cx)));
                list = list.child(
                    div()
                        .id(SharedString::from(format!("br-row-{}", channel.id)))
                        .flex()
                        .items_center()
                        .gap(px(9.))
                        .py(px(6.))
                        .px(px(8.))
                        .rounded(radius::SM)
                        .hover(move |s| s.bg(hover_bg))
                        .when(hidden, |s| s.opacity(0.62))
                        .child(div().w(px(26.)).flex_shrink_0().flex().justify_center().child(channel_icon(kind, 15., p.text_accent)))
                        .child(name)
                        .child(toggle)
                        .p2_enter(SharedString::from(format!("br-rise-{}", channel.id)), Entrance::Rise, std::time::Duration::ZERO),
                );
            }
        }
        if shown == 0 {
            list = list.child(
                div()
                    .py(px(22.))
                    .px(px(10.))
                    .text_center()
                    .text_size(fs(11.))
                    .text_color(p.text_3)
                    .child(if query.is_empty() { "Bu listede kanal yok." } else { "Aramanla eşleşen kanal yok." }),
            );
        }
        let filter = browser.filter;
        let seg_tab = |ix: u8, label: String| {
            tab(SharedString::from(format!("br-tab-{ix}")), label, filter == ix, TabsStyle::Segment, 0, p).on_click(cx.listener(move |this, _, _, cx| {
                if let Some(b) = this.browser.as_mut() {
                    b.filter = ix;
                }
                cx.notify();
            }))
        };
        let filters = div()
            .flex()
            .flex_col()
            .items_start()
            .gap(px(9.))
            .pt(px(4.))
            .pb(px(12.))
            .border_b_1()
            .border_color(p.border_subtle)
            .child(div().w_full().child(form::input(&browser.search, InputKind::P2, p, window, cx)))
            .child(
                tabs(TabsStyle::Segment, p)
                    .child(seg_tab(0, "Tümü".into()))
                    .child(seg_tab(1, "Katıldıkların".into()))
                    .child(seg_tab(2, if left > 0 { format!("Ayrıldıkların · {left}") } else { "Ayrıldıkların".into() })),
            );
        let close = button("br-close", BtnVariant::Ghost, BtnSize::Sm, true, false, p).text_size(font_size::MD).child("×").on_click(cx.listener(|this, _, _, cx| {
            this.browser = None;
            cx.notify();
        }));
        let head = div()
            .flex()
            .items_start()
            .gap(px(12.))
            .pt(px(18.))
            .pr(px(16.))
            .pb(px(12.))
            .pl(px(20.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    // span.p2-kicker satır içi: kök satır kutusu (13,6px normal = 16px) içinde taban çizgisine oturur (+5px).
                    .child(div().h(px(16.)).pt(px(5.)).child(kicker("KANALLAR", p).line_height(lh(font_size::XXS))))
                    .child(div().text_size(font_size::XL).font_weight(FontWeight::BOLD).text_color(p.text_strong).line_height(gpui_kit::relative(1.25)).child("Kanallara göz at")),
            )
            .child(close);
        let card = modal(ModalSize::Md, window, p)
            .id("browser-card")
            .occlude()
            .on_mouse_down(gpui_kit::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(head)
            .child(modal_body().flex().flex_col().child(filters).child(list))
            .p2_enter("browser", Entrance::PopElastic, std::time::Duration::ZERO);
        // Katman çalışma alanının içinde (başlık çubuğunun altında) — Electron'un 38px üst dolgusu burada zaten var.
        Some(
            div()
                .absolute()
                .inset_0()
                .child(
                    modal_layer("browser-scrim", false, p)
                        .size_full()
                        .pt(px(0.))
                        .on_mouse_down(gpui_kit::MouseButton::Left, cx.listener(|this, _, _, cx| {
                            this.browser = None;
                            cx.notify();
                        }))
                        .child(card)
                        .p2_enter("browser-layer", Entrance::Fade, std::time::Duration::ZERO),
                )
                .into_any_element(),
        )
    }
}
