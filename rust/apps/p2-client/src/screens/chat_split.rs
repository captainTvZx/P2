//! Bölünmüş görünüm ve ayrı sohbet pencereleri (L ajanı) — Workspace.tsx `splitTarget` +
//! `.split-stage` (iki eşit sütun, ikincinin solunda 1px `border-subtle`) ve popout.ts /
//! electron/main.cjs `openPopoutWindow` (980×760, en az 620×480, çerçevesiz; yalnız seçilen sohbet,
//! başlık çubuğunda sohbet adı). Ayrı bölmeler çalışma alanının gerçek zamanlı olaylarını alır.
//! Bilinen fark: bölünmüş bölme ve ayrı pencerede iş parçacığı paneli açılmaz (Electron'da bölmenin
//! içinde açılır); iş parçacığı ana bölmede açılır.

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, AnyWindowHandle, Bounds, Context, Entity, StyleRefinement, Subscription, Window, WindowBounds, WindowOptions, div, px, relative, size};
use p2_client_core::Scope;
use p2_client_core::realtime::RtEvent;
use p2_ui::P2Text as _;
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::Palette;
use serde_json::json;

use super::WorkspaceScreen;
use super::super::chat::{ChatEvent, ChatPane, ChatSubject};

/// Ayrı pencere kökü: 38px başlık çubuğu + sohbet + yazma kutusu + sohbetin açılır öğeleri.
pub struct PopoutView {
    pub chat: Entity<ChatPane>,
    title: String,
}

impl Render for PopoutView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        let mut style = StyleRefinement::default();
        style.flex_grow = Some(1.);
        style.min_size.height = Some(px(0.).into());
        style.size.width = Some(relative(1.).into());
        let chat = self.chat.clone();
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(p.bg_0)
            .text_color(p.text)
            .font_family(p2_ui::tokens::FONT)
            .on_key_down(cx.listener(|this, e: &gpui_kit::KeyDownEvent, window, cx| {
                if e.keystroke.key == "escape" {
                    this.chat.update(cx, |c, cx| c.escape(window, cx));
                }
            }))
            .child(popout_titlebar(&self.title, &p, window))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(chat.clone().cached(style))
                    .child(chat.read(cx).composer_bar(&p, chat.downgrade(), window, cx))
                    .children(chat.read(cx).floating(&p, chat.downgrade(), window, cx)),
            )
    }
}

/// Titlebar.tsx görünümü: ☰ (menüsüz), sayfa adı, pencere düğmeleri (Windows isabet testi).
fn popout_titlebar(title: &str, p: &Palette, window: &Window) -> AnyElement {
    use gpui_kit::WindowControlArea;
    let control = |id: &'static str, area: WindowControlArea, icon: &'static str, close: bool| {
        let (hover_bg, hover_fg) = if close { (p.danger, gpui_kit::white()) } else { (p.bg_hover, p.text_strong) };
        div()
            .id(id)
            .w(px(46.))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .group(id)
            .window_control_area(area)
            .hover(move |s| s.bg(hover_bg))
            .child(p2_ui::glyphs::ui(icon, 10.).text_color(p.text_3).group_hover(id, move |s| s.text_color(hover_fg)))
    };
    let line = |top: f32| div().absolute().left(px(10.)).top(px(top)).w(px(13.)).h(px(1.)).bg(p.text_3);
    div()
        .id("popout-titlebar")
        .relative()
        .flex_none()
        .h(px(38.))
        .w_full()
        .flex()
        .items_center()
        .justify_between()
        .pl(px(12.))
        .bg(gpui_kit::linear_gradient(180., gpui_kit::linear_color_stop(p.bg_2, 0.), gpui_kit::linear_color_stop(p.bg_1, 1.)))
        .border_b_1()
        .border_color(p.border_subtle)
        .shadow(p2_ui::tokens::shadows(&[p.inset_top]))
        .child(div().id("popout-drag").absolute().inset_0().window_control_area(WindowControlArea::Drag))
        .child(
            div()
                .h_full()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(div().relative().w(px(34.)).h(px(30.)).child(line(10.)).child(line(14.)).child(line(18.)))
                .child(div().text_color(p.text_2).text_size(p2_ui::tokens::font_size::SM).css_weight(650.).tracking(0.025).child(title.to_string())),
        )
        .child(
            div()
                .h_full()
                .flex()
                .child(control("pop-min", WindowControlArea::Min, "win-min", false))
                .child(control("pop-max", WindowControlArea::Max, if window.is_maximized() { "win-restore" } else { "win-max" }, false))
                .child(control("pop-close", WindowControlArea::Close, "win-close", true)),
        )
        .into_any_element()
}

impl WorkspaceScreen {
    /// Açık sohbetin bir kopyası (aynı hedef): üyeler ve yetkiler ana bölmeden.
    fn side_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<Entity<ChatPane>> {
        let chat = self.chat.clone()?;
        let (target, subject) = {
            let c = chat.read(cx);
            (c.target.clone(), c.subject.clone())
        };
        let (api, me) = (self.api.clone(), self.auth.user.clone());
        let members = if self.org_members.is_empty() { self.members.clone() } else { self.org_members.clone() };
        let perms = self.my_perms.clone();
        let pane = cx.new(|cx| ChatPane::new(api, target, subject, me, window, cx));
        pane.update(cx, |c, _| {
            c.set_members(members);
            c.apply_perms(&perms);
        });
        Some(pane)
    }

    fn side_events(&mut self, pane: &Entity<ChatPane>, window: &mut Window, cx: &mut Context<Self>) -> Subscription {
        cx.subscribe_in(pane, window, |this, chat, event: &ChatEvent, window, cx| match event {
            ChatEvent::OpenProfile(user_id) => this.open_profile_by_id(user_id, cx),
            ChatEvent::Forward(id) => this.start_forward(id.clone(), window, cx),
            ChatEvent::CloseSplit => this.close_split(cx),
            ChatEvent::OpenWindow => {
                let c = chat.clone();
                this.popout_pane(c, window, cx);
            }
            ChatEvent::Typing(typing) => {
                let payload = match &chat.read(cx).target {
                    p2_client_core::ChatTarget::Channel { org, channel, .. } => json!({ "organizationId": org, "channelId": channel, "typing": typing }),
                    p2_client_core::ChatTarget::Direct { conversation, .. } => json!({ "conversationId": conversation, "typing": typing }),
                    p2_client_core::ChatTarget::Group { .. } => return,
                };
                this.realtime.emit("typing", payload);
            }
            ChatEvent::OpenThread(_) => crate::trace("bölme: iş parçacığı ana bölmede açılır"),
            _ => {}
        })
    }

    /// "Bölünmüş görünümde aç": açık sohbetin yanında ikinci bölme.
    pub(super) fn open_split(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pane) = self.side_pane(window, cx) else { return };
        pane.update(cx, |c, _| c.split_pane = true);
        let sub = self.side_events(&pane, window, cx);
        crate::trace(&format!("bölme: bölünmüş görünüm {}", pane.read(cx).label()));
        self.split = Some((pane, sub));
        cx.notify();
    }

    pub(super) fn close_split(&mut self, cx: &mut Context<Self>) {
        if self.split.take().is_some() {
            crate::trace("bölme: bölünmüş görünüm kapandı");
            cx.notify();
        }
    }

    /// "Yeni pencerede aç" (ana sohbetten).
    pub(super) fn open_popout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pane) = self.side_pane(window, cx) else { return };
        self.popout_pane(pane, window, cx);
    }

    /// Ayrı pencere: aynı hedef açıksa öne getirilir (main.cjs `popouts.get(query)`).
    fn popout_pane(&mut self, source: Entity<ChatPane>, window: &mut Window, cx: &mut Context<Self>) {
        let scope = source.read(cx).target.scope();
        self.popouts.retain(|(_, handle, _)| handle.update(cx, |_, _, _| ()).is_ok());
        if let Some((_, handle, _)) = self.popouts.iter().find(|(c, _, _)| c.read(cx).target.scope() == scope) {
            let _ = handle.update(cx, |_, window, _| window.activate_window());
            return;
        }
        let pane = if source.read(cx).split_pane || self.chat.as_ref() == Some(&source) {
            let Some(p) = self.side_pane(window, cx) else { return };
            p
        } else {
            source
        };
        let (target, subject) = {
            let c = pane.read(cx);
            (c.target.clone(), c.subject.clone())
        };
        let _ = target;
        pane.update(cx, |c, _| {
            c.popout = true;
            c.split_pane = false;
        });
        // popout.ts: `${kind==='channel'?'#':''}${name} · P2`; başlık çubuğunda `popoutTitle()`.
        let title = match &subject {
            ChatSubject::Channel(c) => format!("#{}", c.name),
            ChatSubject::Direct(m) => m.display_name.clone(),
            ChatSubject::Group(g) => g.name.clone(),
        };
        let bounds = Bounds::centered(None, size(px(980.), px(760.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(620.), px(480.))),
            ..gpui_kit::component::TitleBar::window_options()
        };
        let view_pane = pane.clone();
        let win_title = format!("{title} · P2");
        let opened = cx.open_window(options, move |window, cx| {
            window.set_window_title(&win_title);
            cx.new(|_| PopoutView { chat: view_pane, title })
        });
        match opened {
            Ok(handle) => {
                crate::trace(&format!("bölme: ayrı pencere {}", pane.read(cx).label()));
                let sub = self.side_events(&pane, window, cx);
                self.popouts.push((pane, handle.into(), sub));
            }
            Err(e) => crate::trace(&format!("bölme: pencere açılamadı {e}")),
        }
    }

    /// Ana alan + (varsa) bölünmüş bölme: iki eşit sütun.
    pub(super) fn with_split(&self, main: AnyElement, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some((split, _)) = &self.split else { return main };
        let mut style = StyleRefinement::default();
        style.flex_grow = Some(1.);
        style.min_size.height = Some(px(0.).into());
        style.size.width = Some(relative(1.).into());
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .child(div().flex_1().min_w_0().h_full().flex().child(main))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .border_l_1()
                    .border_color(p.border_subtle)
                    .child(split.clone().cached(style))
                    .child(split.read(cx).composer_bar(p, split.downgrade(), window, cx)),
            )
            .into_any_element()
    }

    /// Bölünmüş bölmenin açılır öğeleri (menüler, pencereler) kök düzeyde.
    pub(super) fn split_floating(&self, p: &Palette, window: &Window, cx: &gpui_kit::App) -> Vec<AnyElement> {
        self.split.as_ref().map(|(c, _)| c.read(cx).floating(p, c.downgrade(), window, cx)).unwrap_or_default()
    }

    /// Gerçek zamanlı olayı bölünmüş bölmeye ve ayrı pencerelere de ilet (kapsam eşleşirse).
    pub(super) fn forward_side(&mut self, event: &RtEvent, window: &mut Window, cx: &mut Context<Self>) {
        let mut panes: Vec<Entity<ChatPane>> = self.split.iter().map(|s| s.0.clone()).collect();
        self.popouts.retain(|(_, handle, _)| handle.update(cx, |_, _, _| ()).is_ok());
        panes.extend(self.popouts.iter().map(|p| p.0.clone()));
        for pane in panes {
            let scope: Scope = pane.read(cx).target.scope();
            match event.clone() {
                RtEvent::MessageCreated { scope: s, parent_id: None, message } if s == scope => pane.update(cx, |c, cx| c.on_created(message, window, cx)),
                RtEvent::MessageCreated { scope: s, parent_id: Some(parent), .. } if s == scope => pane.update(cx, |c, cx| c.bump_replies(&parent, cx)),
                RtEvent::MessageUpdated { scope: s, parent_id: None, message } if s == scope => pane.update(cx, |c, cx| c.on_updated(message, cx)),
                RtEvent::MessageDeleted { scope: s, message_id, parent_id: None } if s == scope => pane.update(cx, |c, cx| c.on_deleted(&message_id, cx)),
                RtEvent::Reactions { scope: s, message_id, rows } if s == scope => pane.update(cx, |c, cx| c.on_reactions(&message_id, &rows, cx)),
                RtEvent::Typing { scope: s, user_id, display_name, typing } if s == scope => pane.update(cx, |c, cx| c.on_typing(user_id, display_name, typing, window, cx)),
                RtEvent::Presence { user_id, status } => pane.update(cx, |c, cx| c.set_peer_status(&user_id, &status, cx)),
                _ => {}
            }
        }
    }

    /// Parite kancası: `P2_AUTOPILOT_SPLIT=1` açık sohbeti bölünmüş görünümde de açar (veri yazmaz).
    pub(super) fn autopilot_split(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        static DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if std::env::var_os("P2_AUTOPILOT_SPLIT").is_some() && self.chat.is_some() && !DONE.swap(true, std::sync::atomic::Ordering::SeqCst) {
            self.open_split(window, cx);
        }
    }
}

#[allow(dead_code)]
fn _handle_type(_: AnyWindowHandle) {}
