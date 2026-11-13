//! Kanal izinleri (ChannelPermissions.tsx): rol → kullanıcı grubu → kişi kuralları; en dar
//! kural son sözü söyler. Her yetki satırında üç seçenek: ✕ engelle · ／ varsayılan · ✓ izin ver.
//! Görünüm settings.css `.channel-perm-modal` (yönetim paneliyle aynı kabuk, `admin.rs`).

use std::sync::Arc;

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, EventEmitter, FocusHandle, FontWeight, MouseButton, SharedString, Window, div, px, rgb};
use p2_client_core::{Api, ChannelPermissions, Override, PermTarget};
use p2_ui::P2Text as _;
use p2_ui::controls::lh;
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::{Palette, fs, radius};
use p2_ui::{Entrance, P2Motion as _};

use super::admin::{Btn, block_title, hint, modal_size, perm_grid, perm_line, settings_button, settings_card, settings_header, settings_layer, settings_nav, text, warn};
use crate::task;

pub enum PermsEvent {
    Close,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Role,
    Group,
    User,
}

impl Tab {
    fn key(self) -> &'static str {
        match self {
            Tab::Role => "ROLE",
            Tab::Group => "GROUP",
            Tab::User => "USER",
        }
    }
}

pub struct ChannelPermsPane {
    api: Arc<Api>,
    org: String,
    project: String,
    channel: String,
    pub channel_name: String,
    data: Option<ChannelPermissions>,
    tab: Tab,
    target: Option<String>,
    busy: bool,
    error: Option<String>,
    focus: FocusHandle,
}

impl EventEmitter<PermsEvent> for ChannelPermsPane {}

fn label(tab: Tab, t: &PermTarget) -> (String, String) {
    match tab {
        Tab::Role => (t.name.clone(), if t.everyone { "Kuruluştaki herkes".into() } else { format!("@{}", t.handle) }),
        Tab::Group => (t.name.clone(), format!("@{} · {} kişi", t.handle, t.member_count)),
        Tab::User => (t.display_name.clone(), if t.owner { "Kuruluş sahibi".into() } else { format!("@{}", t.username) }),
    }
}

/// Başlıkta gösterilen ipucu: sekme değişince seçilen ilk hedef için Electron `@handle` kullanır.
fn header_hint(tab: Tab, t: &PermTarget, picked: bool) -> String {
    match (tab, picked) {
        (Tab::Group, false) => format!("@{}", t.handle),
        (Tab::User, false) => format!("@{}", t.username),
        _ => label(tab, t).1,
    }
}

impl ChannelPermsPane {
    pub fn new(api: Arc<Api>, org: String, project: String, channel: String, channel_name: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let mut pane = Self { api, org, project, channel, channel_name, data: None, tab: Tab::Role, target: None, busy: false, error: None, focus };
        pane.load(window, cx);
        pane
    }

    fn fail(&mut self, message: String, cx: &mut Context<Self>) {
        self.error = Some(message);
        self.busy = false;
        cx.notify();
    }

    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (api, org, project, channel) = (self.api.clone(), self.org.clone(), self.project.clone(), self.channel.clone());
        task::run(window, cx, move || api.channel_permissions(&org, &project, &channel), |this, r, _, cx| match r {
            Ok(data) => {
                crate::trace(&format!("kanal izinleri: {} kural, düzenleme {}", data.overrides.len(), if data.can_edit { "açık" } else { "kapalı" }));
                if this.target.is_none() {
                    this.target = data.roles.first().map(|r| r.id.clone());
                }
                this.data = Some(data);
                this.busy = false;
                cx.notify();
            }
            Err(e) => this.fail(if e.message.is_empty() { "Kanal izinleri yüklenemedi.".into() } else { e.message }, cx),
        });
    }

    fn targets(&self) -> Vec<PermTarget> {
        let Some(d) = &self.data else { return Vec::new() };
        match self.tab {
            Tab::Role => d.roles.clone(),
            Tab::Group => d.groups.clone(),
            Tab::User => d.members.clone(),
        }
    }

    fn current(&self) -> Option<Override> {
        let (d, t) = (self.data.as_ref()?, self.target.as_ref()?);
        d.overrides.iter().find(|o| o.target_type == self.tab.key() && &o.target_id == t).cloned()
    }

    pub fn select(&mut self, tab: Tab, target: Option<String>, cx: &mut Context<Self>) {
        self.tab = tab;
        self.target = target.or_else(|| self.targets().first().map(|t| t.id.clone()));
        cx.notify();
    }

    /// Bir yetkinin durumunu değiştir: "allow" | "deny" | "inherit".
    pub fn set(&mut self, permission: &str, next: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || !self.data.as_ref().is_some_and(|d| d.can_edit) {
            return;
        }
        let Some(target) = self.target.clone() else { return };
        let cur = self.current().unwrap_or_default();
        let mut allow: Vec<String> = cur.allow.into_iter().filter(|p| p != permission).collect();
        let mut deny: Vec<String> = cur.deny.into_iter().filter(|p| p != permission).collect();
        match next {
            "allow" => allow.push(permission.to_string()),
            "deny" => deny.push(permission.to_string()),
            _ => {}
        }
        self.busy = true;
        self.error = None;
        cx.notify();
        let (api, org, project, channel, tt) = (self.api.clone(), self.org.clone(), self.project.clone(), self.channel.clone(), self.tab.key());
        let note = format!("kanal izinleri: {tt} {permission} → {next}");
        task::run(window, cx, move || api.set_channel_permission(&org, &project, &channel, tt, &target, &allow, &deny), move |this, r, window, cx| match r {
            Ok(_) => {
                crate::trace(&note);
                this.load(window, cx);
            }
            Err(e) => this.fail(if e.message.is_empty() { "İzin kaydedilemedi.".into() } else { e.message }, cx),
        });
    }

    fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.target.clone() else { return };
        if self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        let (api, org, project, channel, tt) = (self.api.clone(), self.org.clone(), self.project.clone(), self.channel.clone(), self.tab.key());
        task::run(window, cx, move || api.clear_channel_permission(&org, &project, &channel, tt, &target), |this, r, window, cx| match r {
            Ok(_) => {
                crate::trace("kanal izinleri: kural sıfırlandı");
                this.load(window, cx);
            }
            Err(e) => this.fail(if e.message.is_empty() { "İzin sıfırlanamadı.".into() } else { e.message }, cx),
        });
    }

    pub fn target_by_name(&self, tab: Tab, name: &str) -> Option<String> {
        let d = self.data.as_ref()?;
        let list = match tab {
            Tab::Role => &d.roles,
            Tab::Group => &d.groups,
            Tab::User => &d.members,
        };
        list.iter().find(|t| t.name == name || t.username == name || t.display_name == name).map(|t| t.id.clone())
    }
}

/// PermissionRow.tsx `.perm-switch`: p 2, kenarlık, r 8, bg-1; 30×24 düğmeler (✕ ／ ✓).
fn perm_switch(id: &str, state: &str, disabled: bool, p: &Palette, cx: &mut Context<ChannelPermsPane>) -> AnyElement {
    let mut sw = div().flex().flex_none().p(px(2.)).border_1().border_color(p.border).rounded(radius::SM).bg(p.bg_1);
    for (value, glyph, title) in [("deny", "✕", "Engelle"), ("inherit", "／", "Varsayılan"), ("allow", "✓", "İzin ver")] {
        let on = state == value;
        let (bg, fg) = match (on, value) {
            (true, "allow") => (Some(p.success_soft), p.success_text),
            (true, "deny") => (Some(p.danger_soft), p.danger_text),
            (true, _) => (Some(p.bg_hover), p.text_strong),
            _ => (None, p.text_3),
        };
        let (hover, strong) = (p.bg_hover, p.text_strong);
        let perm = id.to_string();
        sw = sw.child(
            div()
                .id(SharedString::from(format!("ps-{id}-{value}")))
                .w(px(30.))
                .h(px(24.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(radius::XS)
                .text_size(fs(11.))
                .line_height(px(11.))
                .font_weight(FontWeight::EXTRA_BOLD)
                .text_color(fg)
                .when_some(bg, |s, b| s.bg(b))
                .when(!disabled, |s| s.cursor_pointer().hover(move |s| s.bg(hover).text_color(strong)))
                .tooltip(move |window, cx| p2_ui::controls::tooltip(title, window, cx))
                .child(glyph)
                .on_click(cx.listener(move |this, _, window, cx| {
                    if !on && !disabled {
                        this.set(&perm, value, window, cx);
                    }
                })),
        );
    }
    sw.into_any_element()
}

impl Render for ChannelPermsPane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        let (w, h) = modal_size(1000., 720., window);
        let can_edit = self.data.as_ref().is_some_and(|d| d.can_edit);

        // `.perm-tabs.channel-perm-tabs` düğmeleri `.settings-modal > nav button` kuralını da alır
        // (daha özgül): genişlik %100, p 8 10, kenarlık yok, r 8 → yalnız ilk sekme görünür, diğerleri
        // gezinme alanının dışına taşar (Electron'daki gibi). Etkin: bg-selected + 2px vurgu, yazı accent.
        let tab_btn = |id: &'static str, text_: &'static str, tab: Tab| {
            let on = self.tab == tab;
            let hover = p.bg_hover;
            div()
                .id(id)
                .relative()
                .w(px(185.))
                .flex_none()
                .h(px(26.))
                .px(px(10.))
                .flex()
                .items_center()
                .overflow_hidden()
                .rounded(radius::SM)
                .text_size(fs(10.5))
                .line_height(lh(fs(10.5)))
                .css_weight(650.)
                .cursor_pointer()
                .map(|s| if on { s.bg(p.bg_selected).text_color(p.text_accent).child(div().absolute().left_0().top_0().bottom_0().w(px(2.)).bg(p.accent_hover)) } else { s.text_color(p.text_3).hover(move |s| s.bg(hover).left(px(2.))) })
                .child(text_)
                .on_click(cx.listener(move |this, _, _, cx| this.select(tab, None, cx)))
        };
        let marked = |id: &str| self.data.as_ref().is_some_and(|d| d.overrides.iter().any(|o| o.target_type == self.tab.key() && o.target_id == id));
        let mut targets = div().id("perm-targets").flex_1().min_h_0().overflow_y_scroll().px(px(8.)).pb(px(10.)).flex().flex_col().gap(px(3.));
        let list = self.targets();
        for t in &list {
            let (name, sub) = label(self.tab, t);
            let id = t.id.clone();
            let tab = self.tab;
            let glyph = if tab == Tab::User {
                let (size, r) = (26., 9.);
                p2_ui::controls::avatar_box(size, p2_ui::components::avatar_hue(&t.id), false, &p).rounded(px(r)).text_size(fs(9.)).line_height(lh(fs(9.))).tracking(0.).child(p2_ui::components::user_initials(&name)).into_any_element()
            } else {
                let color = t.color.as_deref().and_then(|c| u32::from_str_radix(c.trim_start_matches('#'), 16).ok()).map(|v| gpui_kit::Hsla::from(rgb(v)));
                div()
                    .size(px(26.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(radius::SM)
                    .text_size(fs(11.))
                    .line_height(lh(fs(11.)))
                    .font_weight(FontWeight::EXTRA_BOLD)
                    .map(|s| match color {
                        Some(c) => s.bg(c.opacity(0x22 as f32 / 255.)).text_color(c),
                        None => s.bg(p.bg_selected).text_color(p.text_strong),
                    })
                    .child("@")
                    .into_any_element()
            };
            let active = self.target.as_deref() == Some(t.id.as_str());
            let hover = p.bg_hover;
            targets = targets.child(
                div()
                    .id(SharedString::from(format!("pt-{}", t.id)))
                    .relative()
                    .flex_none()
                    // `.settings-modal > nav button` kazanır: p 8 10, ızgara gap 2, kenarlık yok, r 8;
                    // b 12px/600 text-2, small 9,5px text-4; etkin bg-selected + 2px vurgu çizgisi.
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .px(px(10.))
                    .py(px(8.))
                    .overflow_hidden()
                    .rounded(radius::SM)
                    .cursor_pointer()
                    .group("perm-target")
                    .map(|s| if active { s.bg(p.bg_selected).child(div().absolute().left_0().top_0().bottom_0().w(px(2.)).bg(p.accent_hover)) } else { s.hover(move |s| s.bg(hover).left(px(2.))) })
                    .child(glyph)
                    .child({
                        let strong = p.text_strong;
                        div().flex_1().min_w_0().flex().flex_col().gap(px(2.)).child(text(fs(12.)).css_weight(600.).text_color(if active { p.text_strong } else { p.text_2 }).group_hover("perm-target", move |s| s.text_color(strong)).truncate().child(name)).child(text(fs(9.5)).text_color(if active { p.text_accent } else { p.text_4 }).truncate().child(sub))
                    })
                    .when(marked(&t.id), |s| s.child(div().size(px(7.)).flex_none().rounded_full().bg(p.accent)))
                    .on_click(cx.listener(move |this, _, _, cx| this.select(tab, Some(id.clone()), cx))),
            );
        }
        if list.is_empty() {
            targets = targets.child(hint("Liste boş.", &p));
        }
        let nav = settings_nav(format!("#{}", self.channel_name), "Kanal izinleri", &p)
            .child(div().flex().overflow_hidden().gap(px(6.)).px(px(12.)).pb(px(8.)).child(tab_btn("pt-role", "Roller", Tab::Role)).child(tab_btn("pt-group", "Gruplar", Tab::Group)).child(tab_btn("pt-user", "Kişiler", Tab::User)))
            .child(targets);

        let current = self.current();
        let target = list.iter().find(|t| Some(&t.id) == self.target.as_ref());
        let (title, sub) = target.map(|t| (label(self.tab, t).0, header_hint(self.tab, t, true))).unwrap_or_else(|| ("Kanal izinleri".into(), String::new()));
        let content_w = w - 2. - 230. - 40.;
        let mut body = div().flex().flex_col();
        if let Some(e) = &self.error {
            body = body.child(warn(e.clone(), &p));
        }
        match &self.data {
            None => {
                if self.error.is_none() {
                    body = body.child(hint("Yükleniyor…", &p));
                }
            }
            Some(d) => {
                let b = |t: &str| div().font_weight(FontWeight::BOLD).child(t.to_string());
                let nb = "\u{a0}";
                body = body
                    .child(
                        div()
                            .text_size(fs(10.))
                            .line_height(gpui_kit::relative(1.5))
                            .text_color(p.text_3)
                            .flex()
                            .flex_wrap()
                            .child(format!("Her satırda üç seçenek var:{nb}"))
                            .child(b("✕ engelle"))
                            .child(format!("{nb}·{nb}"))
                            .child(b("／ varsayılan"))
                            .child(format!("{nb}·{nb}"))
                            .child(b("✓ izin ver"))
                            .child(".\u{a0}Kurallar Herkes rolü → diğer roller → gruplar → kişi sırasıyla işlenir; kişiye özel kural her zaman kazanır.")
                            .when(!d.can_edit, |s| s.child(" Bu kanalın izinlerini değiştirme yetkin yok.")),
                    )
                    .child(hint("Kuruluş sahibinin yetkileri kanal bazında da kısıtlanamaz.", &p));
                let mut groups: Vec<&str> = Vec::new();
                for item in &d.catalog {
                    if !groups.contains(&item.group.as_str()) {
                        groups.push(&item.group);
                    }
                }
                let reset = current.is_some() && d.can_edit;
                let n = groups.len();
                let disabled = !d.can_edit || self.busy || self.target.is_none();
                for (gi, g) in groups.into_iter().enumerate() {
                    let mut items = Vec::new();
                    for item in d.catalog.iter().filter(|i| i.group == g) {
                        let state = match &current {
                            Some(o) if o.allow.contains(&item.id) => "allow",
                            Some(o) if o.deny.contains(&item.id) => "deny",
                            _ => "inherit",
                        };
                        let hint_text = match state {
                            "allow" => "Bu kanalda izin verildi",
                            "deny" => "Bu kanalda engellendi",
                            _ => "Kuruluş genelindeki yetki geçerli",
                        };
                        items.push(perm_line(SharedString::from(format!("pl-{}", item.id)), &item.label, hint_text, state, disabled, &p).child(perm_switch(&item.id, state, disabled, &p, cx)).into_any_element());
                    }
                    let last = gi + 1 == n && !reset;
                    body = body.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(9.))
                            .when(!last, |s| s.pb(px(18.)).mb(px(16.)).border_b_1().border_color(p.border_subtle))
                            .child(block_title(g, &p))
                            .child(perm_grid(items, content_w, 330.)),
                    );
                }
                if reset {
                    body = body.child(div().flex().child(settings_button("perm-clear", "Bu hedefin kanal kurallarını sıfırla", Btn::Plain, self.busy, &p).on_click(cx.listener(|this, _, window, cx| this.clear(window, cx)))));
                }
            }
        }
        let title_el = div()
            .flex()
            .items_baseline()
            .gap(px(9.))
            .child(text(fs(14.)).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(title))
            .child(text(fs(11.)).css_weight(500.).text_color(p.text_3).child(sub))
            .into_any_element();
        let card = settings_card("perm-card", w, h, &p)
            .track_focus(&self.focus)
            .child(nav)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(settings_header(title_el, div().id("perm-close").on_click(cx.listener(|_, _, _, cx| cx.emit(PermsEvent::Close))), &p))
                    .child(div().id("perm-body").flex_1().min_h_0().overflow_y_scroll().pt(px(16.)).px(px(20.)).pb(px(30.)).child(body).p2_enter("perm-body", Entrance::Rise, std::time::Duration::ZERO)),
            )
            .p2_enter("perms", Entrance::PopElastic, std::time::Duration::ZERO);
        let _ = can_edit;
        settings_layer("perm-scrim", card, &p).on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| cx.emit(PermsEvent::Close)))
    }
}
