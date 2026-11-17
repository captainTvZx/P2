//! "Sunucu yönetimi" paneli: kurulumun (instance) bütün yönetimi tek yerde — Genel, Kişiler (sunucu
//! yasağı, oturum kapatma, 2FA sıfırlama, sunucu yetkileri), Projeler (her proje: gir, sahipliği
//! devret, sil, geri yükle, kalıcı sil; eski geri dönüşüm burada) ve Ayarlar (sunucu adı, kayıt kuralı,
//! proje açma kuralı). Kurulum sahibi ve herhangi bir `server.*` yetkisi olan açar (`/server/...`).
//!
//! Görünüm proje yönetim panelinin kabuğunu (`admin.rs`: `.settings-modal`, `.settings-block`,
//! `.settings-button`, `.perm-line`) aynen kullanır. Uygulama kökünde (`app.rs`) bindirme olarak
//! çizilir: çalışma alanında da, projesi olmayan başlangıç ekranında da açılabilir.

use std::sync::Arc;

use chrono::{DateTime, Datelike, Local};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, Div, Entity, EventEmitter, FocusHandle, FontWeight, MouseButton, SharedString, Subscription, Window, div, px};
use p2_client_core::{Api, ApiError, ServerMe, ServerOverview, ServerProject, ServerSettings, ServerUser};
use p2_i18n::{t, tv};
use p2_ui::P2Text as _;
use p2_ui::P2Fx as _;
use p2_ui::controls::modal_foot;
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::{Palette, font_size, fs, radius, shadows};
use p2_ui::{Entrance, P2Motion as _};

use super::admin::{Btn, admin_avatar, confirm_modal, field_input, hint, modal_size, nav_button, perm_grid, perm_toggle, presence, settings_block, settings_button, settings_card, settings_field, settings_header, settings_layer, settings_nav, text, tr_upper, warn};
use crate::task;

/// Raydaki ve menüdeki kalkan simgesi (viewBox 0 0 24 24).
pub const SHIELD_ICON: &str = r#"<path d="M12 3.2l7 2.8v5.2c0 4.4-2.9 8.2-7 9.6-4.1-1.4-7-5.2-7-9.6V6z"/><path d="M9.2 12.1l2 2 3.7-4"/>"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Users,
    Projects,
    Settings,
}

impl Tab {
    fn key(self) -> &'static str {
        match self {
            Tab::Overview => "overview",
            Tab::Users => "users",
            Tab::Projects => "projects",
            Tab::Settings => "settings",
        }
    }
}

pub enum ServerAdminEvent {
    Close,
    /// Projeye girildi: `{ project, organization, organizationId, joined }` (çalışma alanı raya ekler).
    Entered(serde_json::Value),
    /// Sunucu (kuruluş) adı değişti.
    Renamed(String),
}

enum Confirm {
    Ban(ServerUser),
    Sessions(ServerUser),
    Reset2fa(ServerUser),
    Delete(ServerProject),
    Purge(ServerProject),
    Transfer(ServerProject, ServerUser),
}

pub struct ServerAdminPanel {
    api: Arc<Api>,
    me: String,
    info: ServerMe,
    tab: Tab,
    overview: Option<ServerOverview>,
    users: Vec<ServerUser>,
    projects: Vec<ServerProject>,
    settings: Option<ServerSettings>,
    selected_user: Option<String>,
    selected_project: Option<String>,
    /// Sahipliği devret: seçili projede kişi arama alanı açık mı.
    transferring: bool,
    busy: bool,
    confirm: Option<Confirm>,
    search: Entity<InputState>,
    project_search: Entity<InputState>,
    transfer_search: Entity<InputState>,
    ban_reason: Entity<InputState>,
    name: Entity<InputState>,
    focus: FocusHandle,
    /// Otomatik pilot adımları (`P2_AUTOPILOT_SERVER_ADMIN`, ters sırada).
    steps: Vec<String>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<ServerAdminEvent> for ServerAdminPanel {}

/// Tarih: Türkçede "13.11.2026", diğer dillerde "Nov 13, 2026".
fn day(value: Option<&str>) -> String {
    let Some(d) = value.and_then(|v| DateTime::parse_from_rfc3339(v).ok()).map(|d| d.with_timezone(&Local)) else { return "—".into() };
    if p2_i18n::language().id == "tr" { format!("{:02}.{:02}.{}", d.day(), d.month(), d.year()) } else { d.format("%b %-d, %Y").to_string() }
}

fn error_text(e: &ApiError) -> String {
    super::admin::error_text(e, &t("srv.failed"))
}

/// `.admin-stat` (proje panelindeki genel bakış kartı).
fn stat(label: String, value: String, sub: Option<String>, p: &Palette) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(3.))
        .px(px(13.))
        .py(px(12.))
        .border_1()
        .border_color(p.border)
        .rounded(radius::MD)
        .bg(p.glass.background())
        .shadow(shadows(&[p.inset_top]))
        .child(text(font_size::XXS).css_weight(800.).tracking(0.12).text_color(p.text_accent).child(tr_upper(&label)))
        .child(text(fs(20.)).css_weight(750.).tracking(-0.02).text_color(p.text_strong).child(value))
        .when_some(sub, |s, h| s.child(div().text_size(fs(10.)).line_height(gpui_kit::relative(1.4)).text_color(p.text_3).child(h)))
}

fn stats_grid(items: Vec<Div>, width: f32) -> Div {
    let cols = (((width + 8.) / 158.).floor() as usize).max(1).min(items.len().max(1));
    let mut col = div().flex().flex_col().gap(px(8.));
    let mut row: Vec<AnyElement> = Vec::new();
    let n = items.len();
    for (i, item) in items.into_iter().enumerate() {
        row.push(div().flex_1().min_w_0().flex().child(item.p2_enter(SharedString::from(format!("srv-stat-{i}")), Entrance::Rise, std::time::Duration::ZERO)).into_any_element());
        if row.len() == cols || i + 1 == n {
            while row.len() < cols {
                row.push(div().flex_1().into_any_element());
            }
            col = col.child(div().flex().gap(px(8.)).children(std::mem::take(&mut row)));
        }
    }
    col
}

/// Küçük rozet (`.team-tag` görünümü): sahip, yönetici, yasaklı, 2FA.
fn badge(label: String, kind: Btn, p: &Palette) -> Div {
    let (bg, fg, border) = match kind {
        Btn::Danger => (p.danger_soft, p.danger_text, p.danger_border),
        Btn::Primary => (p.accent_soft, p.text_accent, p.border_accent),
        Btn::Plain => (p.bg_3, p.text_2, p.border),
    };
    div().flex_none().px(px(7.)).py(px(2.)).border_1().border_color(border).rounded(radius::PILL).bg(bg).text_color(fg).text_size(fs(9.)).line_height(fs(11.)).css_weight(750.).tracking(0.04).child(tr_upper(&label))
}

/// Seçenek kartı (kayıt kuralı / proje açma kuralı): seçiliyse vurgu kenarlığı ve zemin.
fn choice(id: SharedString, title: String, body: String, selected: bool, disabled: bool, p: &Palette) -> gpui_kit::Stateful<Div> {
    let hover = p.bg_hover;
    div()
        .id(id)
        .flex_1()
        .min_w_0()
        .flex()
        .gap(px(10.))
        .px(px(12.))
        .py(px(11.))
        .border_1()
        .rounded(radius::MD)
        .map(|s| if selected { s.border_color(p.border_accent).bg(p.accent_soft) } else { s.border_color(p.border).bg(p.bg_3) })
        .when(disabled, |s| s.opacity(0.6))
        .when(!disabled && !selected, |s| s.cursor_pointer().hover(move |s| s.bg(hover)))
        .child(
            div()
                .mt(px(2.))
                .size(px(14.))
                .flex_none()
                .rounded_full()
                .border_1()
                .map(|s| if selected { s.border_color(p.accent).bg(p.accent) } else { s.border_color(p.border_strong) })
                .when(selected, |s| s.flex().items_center().justify_center().child(div().size(px(6.)).rounded_full().bg(p.text_on_accent))),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(3.))
                .child(text(fs(12.)).css_weight(700.).text_color(if selected { p.text_strong } else { p.text }).child(title))
                .child(div().text_size(fs(10.)).line_height(gpui_kit::relative(1.45)).text_color(p.text_3).child(body)),
        )
}

impl ServerAdminPanel {
    pub fn new(api: Arc<Api>, me: String, tab: Option<Tab>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = |placeholder: String, window: &mut Window, cx: &mut Context<Self>| cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let search = input(t("srv.users.search"), window, cx);
        let project_search = input(t("srv.projects.search"), window, cx);
        let transfer_search = input(t("srv.projects.transferSearch"), window, cx);
        let ban_reason = input(t("srv.optional"), window, cx);
        let name = input(String::new(), window, cx);
        let subs = [&search, &project_search, &transfer_search, &ban_reason, &name].into_iter().map(|e| cx.subscribe(e, |_, _, _: &InputEvent, cx| cx.notify())).collect();
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let steps = std::env::var("P2_AUTOPILOT_SERVER_ADMIN").map(|s| s.split(';').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).rev().collect()).unwrap_or_default();
        let mut panel = Self {
            api,
            me,
            info: ServerMe::default(),
            tab: tab.unwrap_or(Tab::Overview),
            overview: None,
            users: Vec::new(),
            projects: Vec::new(),
            settings: None,
            selected_user: None,
            selected_project: None,
            transferring: false,
            busy: false,
            confirm: None,
            search,
            project_search,
            transfer_search,
            ban_reason,
            name,
            focus,
            steps,
            _subs: subs,
        };
        crate::trace("sunucu yönetimi: açıldı");
        let api = panel.api.clone();
        task::run(window, cx, move || api.server_me(), |this, r, window, cx| match r {
            Ok(info) => {
                crate::trace(&format!("sunucu yönetimi: yetkiler {}", info.server_permissions.join(",")));
                this.info = info;
                if !this.visible().contains(&this.tab) {
                    this.tab = Tab::Overview;
                }
                this.load(window, cx);
            }
            Err(e) => this.fail(e, cx),
        });
        panel.busy = true;
        panel
    }

    fn can(&self, permission: &str) -> bool {
        self.info.server_permissions.iter().any(|p| p == permission)
    }

    fn visible(&self) -> Vec<Tab> {
        let mut tabs = vec![Tab::Overview, Tab::Users];
        if self.can("server.view_all") {
            tabs.push(Tab::Projects);
        }
        tabs.push(Tab::Settings);
        tabs
    }

    fn fail(&mut self, e: ApiError, cx: &mut Context<Self>) {
        crate::trace(&format!("sunucu yönetimi: hata {} {}", e.code, e.message));
        // Bildirimler sağ üstte (toasts.rs).
        crate::toasts::error(error_text(&e));
        self.busy = false;
        cx.notify();
    }

    pub fn open(&mut self, tab: Tab, window: &mut Window, cx: &mut Context<Self>) {
        self.tab = tab;
        self.transferring = false;
        crate::trace(&format!("sunucu yönetimi: bölüm {}", tab.key()));
        self.load(window, cx);
        cx.notify();
    }

    fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let api = self.api.clone();
        self.busy = true;
        match self.tab {
            Tab::Overview => task::run(window, cx, move || api.server_overview(), |this, r, window, cx| match r {
                Ok(o) => {
                    this.overview = Some(o);
                    this.loaded(window, cx);
                }
                Err(e) => this.fail(e, cx),
            }),
            Tab::Users => task::run(window, cx, move || api.server_users(""), |this, r, window, cx| match r {
                Ok(list) => {
                    for u in &list {
                        crate::avatars::remember(&u.id, u.avatar_url.as_deref());
                    }
                    crate::trace(&format!("sunucu yönetimi: {} kişi", list.len()));
                    this.users = list;
                    this.loaded(window, cx);
                }
                Err(e) => this.fail(e, cx),
            }),
            Tab::Projects => task::run(window, cx, move || (api.server_projects(), api.server_users("")), |this, (r, users), window, cx| match r {
                Ok(list) => {
                    crate::trace(&format!("sunucu yönetimi: {} proje", list.len()));
                    this.projects = list;
                    if let Ok(users) = users {
                        this.users = users;
                    }
                    this.loaded(window, cx);
                }
                Err(e) => this.fail(e, cx),
            }),
            Tab::Settings => task::run(window, cx, move || api.server_settings(), |this, r, window, cx| match r {
                Ok(s) => {
                    let name = s.name.clone().unwrap_or_default();
                    this.name.update(cx, |st, cx| st.set_value(name, window, cx));
                    this.settings = Some(s);
                    this.loaded(window, cx);
                }
                Err(e) => this.fail(e, cx),
            }),
        }
    }

    fn loaded(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.busy = false;
        cx.notify();
        self.next_step(window, cx);
    }

    /// Eylem: bitince bölümü yeniden yükler, bildirim metnini gösterir.
    fn act(&mut self, trace: String, notice: String, work: impl FnOnce(&Api) -> Result<serde_json::Value, ApiError> + Send + 'static, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.confirm = None;
        cx.notify();
        let api = self.api.clone();
        task::run(window, cx, move || work(&api), move |this, r, window, cx| match r {
            Ok(value) => {
                crate::trace(&format!("sunucu yönetimi: {trace}"));
                crate::toasts::ok(notice);
                if let Some(name) = value.get("name").and_then(|n| n.as_str()) {
                    cx.emit(ServerAdminEvent::Renamed(name.to_string()));
                }
                if value.get("project").is_some() && value.get("organizationId").is_some() {
                    cx.emit(ServerAdminEvent::Entered(value.clone()));
                }
                this.load(window, cx);
            }
            Err(e) => this.fail(e, cx),
        });
    }

    fn toggle_permission(&mut self, user: &ServerUser, permission: &str, window: &mut Window, cx: &mut Context<Self>) {
        let mut next = user.permissions.clone();
        let on = if let Some(i) = next.iter().position(|p| p == permission) {
            next.remove(i);
            false
        } else {
            next.push(permission.to_string());
            true
        };
        let (id, name) = (user.id.clone(), user.display_name.clone());
        let trace = format!("{name} yetki {permission} {}", if on { "verildi" } else { "alındı" });
        let notice = tv(if on { "srv.users.granted" } else { "srv.users.revoked" }, &[("name", &name)]);
        self.act(trace, notice, move |api| api.server_set_permissions(&id, &next), window, cx);
    }

    fn set_admin(&mut self, user: &ServerUser, on: bool, window: &mut Window, cx: &mut Context<Self>) {
        let mut next: Vec<String> = user.permissions.iter().filter(|p| *p != "server.admin").cloned().collect();
        if on {
            next.push("server.admin".into());
        }
        let (id, name) = (user.id.clone(), user.display_name.clone());
        let notice = tv(if on { "srv.users.promoted" } else { "srv.users.demoted" }, &[("name", &name)]);
        self.act(format!("{name} sunucu yöneticisi {}", if on { "yapıldı" } else { "olmaktan çıktı" }), notice, move |api| api.server_set_permissions(&id, &next), window, cx);
    }

    fn save_settings(&mut self, patch: serde_json::Value, window: &mut Window, cx: &mut Context<Self>) {
        let trace = format!("ayar {patch}");
        self.act(trace, t("srv.saved"), move |api| api.server_update_settings(patch).map(|s| serde_json::json!({ "name": s.name })), window, cx);
    }

    fn run_confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(c) = self.confirm.take() else { return };
        match c {
            Confirm::Ban(u) => {
                let reason = self.ban_reason.read(cx).value().to_string();
                self.ban_reason.update(cx, |s, cx| s.set_value("", window, cx));
                let (id, name) = (u.id.clone(), u.display_name.clone());
                self.act(format!("{name} sunucudan yasaklandı"), tv("srv.users.bannedNotice", &[("name", &name)]), move |api| api.server_ban(&id, &reason), window, cx);
            }
            Confirm::Sessions(u) => {
                let (id, name) = (u.id.clone(), u.display_name.clone());
                self.act(format!("{name} oturumları kapatıldı"), tv("srv.users.sessionsClosed", &[("name", &name)]), move |api| api.server_revoke_sessions(&id), window, cx);
            }
            Confirm::Reset2fa(u) => {
                let (id, name) = (u.id.clone(), u.display_name.clone());
                self.act(format!("{name} 2FA sıfırlandı"), tv("srv.users.twoFactorReset", &[("name", &name)]), move |api| api.server_reset_two_factor(&id), window, cx);
            }
            Confirm::Delete(pr) => {
                let (id, name) = (pr.id.clone(), pr.name.clone());
                self.act(format!("{name} silindi"), tv("srv.projects.deleted", &[("name", &name)]), move |api| api.server_delete_project(&id), window, cx);
            }
            Confirm::Purge(pr) => {
                let (id, name) = (pr.id.clone(), pr.name.clone());
                self.act(format!("{name} kalıcı silindi"), tv("srv.projects.purged", &[("name", &name)]), move |api| api.server_purge_project(&id), window, cx);
            }
            Confirm::Transfer(pr, u) => {
                self.transferring = false;
                let (pid, uid, name, who) = (pr.id.clone(), u.id.clone(), pr.name.clone(), u.display_name.clone());
                self.act(format!("{name} sahipliği {who} kişisine devredildi"), tv("srv.projects.transferred", &[("name", &name), ("user", &who)]), move |api| api.server_transfer_project(&pid, &uid), window, cx);
            }
        }
    }

    // ------------------------------------------------------------ otomatik pilot

    fn next_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.steps.is_empty() {
            return;
        }
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(700)).await;
            let _ = this.update_in(cx, |this, window, cx| this.step(window, cx));
        })
        .detach();
    }

    fn step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(step) = self.steps.pop() else { return };
        crate::trace(&format!("sunucu yönetimi: adım {step}"));
        let parts: Vec<&str> = step.splitn(3, ':').collect();
        let user = |this: &Self, name: &str| this.users.iter().find(|u| u.username == name).cloned();
        let project = |this: &Self, name: &str| this.projects.iter().find(|p| p.name == name).cloned();
        match parts.as_slice() {
            ["wait", ms] => {
                let ms = ms.parse::<u64>().unwrap_or(1000);
                cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(std::time::Duration::from_millis(ms)).await;
                    let _ = this.update_in(cx, |this, window, cx| this.step(window, cx));
                })
                .detach();
                return;
            }
            ["overview"] => return self.open(Tab::Overview, window, cx),
            ["users"] => return self.open(Tab::Users, window, cx),
            ["projects"] => return self.open(Tab::Projects, window, cx),
            ["settings"] => return self.open(Tab::Settings, window, cx),
            // Kişiyi seç (arama kutusuna adını yazar, ayrıntısı açılır).
            ["user", name] => {
                self.selected_user = user(self, name).map(|u| u.id);
                let q = name.to_string();
                self.search.update(cx, |s, cx| s.set_value(q, window, cx));
            }
            ["project", name] => {
                self.selected_project = project(self, name).map(|p| p.id);
                let q = name.to_string();
                self.project_search.update(cx, |s, cx| s.set_value(q, window, cx));
            }
            ["ban", name, reason] => {
                if let Some(u) = user(self, name) {
                    let reason = reason.to_string();
                    self.ban_reason.update(cx, |s, cx| s.set_value(reason, window, cx));
                    self.confirm = Some(Confirm::Ban(u));
                    return self.run_confirm(window, cx);
                }
            }
            ["unban", name] => {
                if let Some(u) = user(self, name) {
                    let id = u.id.clone();
                    return self.act(format!("{name} yasağı kaldırıldı"), tv("srv.users.unbannedNotice", &[("name", &u.display_name)]), move |api| api.server_unban(&id), window, cx);
                }
            }
            ["grant", name, perm] | ["revoke", name, perm] => {
                if let Some(u) = user(self, name) {
                    let has = u.permissions.iter().any(|p| p == perm);
                    if has == (parts[0] == "revoke") {
                        return self.toggle_permission(&u, perm, window, cx);
                    }
                }
            }
            ["creation", v] => return self.save_settings(serde_json::json!({ "projectCreation": v }), window, cx),
            ["registration", v] => return self.save_settings(serde_json::json!({ "registrationPolicy": v }), window, cx),
            ["confirm-ban", name] => {
                // Yalnız görünüm: yasak onay penceresi açık kalır.
                if let Some(u) = user(self, name) {
                    self.confirm = Some(Confirm::Ban(u));
                }
            }
            _ => {}
        }
        cx.notify();
        self.next_step(window, cx);
    }

    // ------------------------------------------------------------ görünümler

    fn overview_view(&self, width: f32, p: &Palette) -> AnyElement {
        let Some(o) = &self.overview else { return hint(t("srv.loading"), p).into_any_element() };
        let name = o.name.clone().unwrap_or_else(|| "P2".into());
        div()
            .flex()
            .flex_col()
            .child(settings_block(name, false, p).child(stats_grid(
                vec![
                    stat(t("srv.ov.users"), o.users.to_string(), Some(tv("srv.ov.online", &[("n", &o.online.to_string())])), p),
                    stat(t("srv.ov.projects"), o.projects.to_string(), Some(tv("srv.ov.inTrash", &[("n", &o.deleted_projects.to_string())])), p),
                    stat(t("srv.ov.sessions"), o.sessions.to_string(), None, p),
                    stat(t("srv.ov.banned"), o.banned.to_string(), None, p),
                ],
                width,
            )))
            .child(settings_block(t("srv.ov.system"), true, p).child(stats_grid(
                vec![
                    stat(t("srv.ov.version"), format!("v{}", o.version), None, p),
                    stat(t("srv.ov.channels"), o.channels.to_string(), None, p),
                    stat(t("srv.ov.registration"), t(if o.registration_policy == "invite" { "srv.set.regInvite" } else { "srv.set.regOpen" }), None, p),
                    stat(t("srv.ov.creation"), t(if o.project_creation == "everyone" { "srv.set.createEveryone" } else { "srv.set.createPermission" }), None, p),
                ],
                width,
            )))
            .into_any_element()
    }

    fn user_row(&self, i: usize, u: &ServerUser, width: f32, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let open = self.selected_user.as_deref() == Some(u.id.as_str());
        let hover = p.bg_hover;
        let id = u.id.clone();
        let seen = if u.online { t("srv.users.online") } else { tv("srv.users.lastSeen", &[("date", &day(u.last_seen_at.as_deref()))]) };
        let server_admin = u.admin || u.permissions.iter().any(|p| p == "server.admin");
        let head = div()
            .id(SharedString::from(format!("srv-user-{i}")))
            .flex()
            .items_center()
            .gap(px(11.))
            .px(px(10.))
            .py(px(8.))
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .child(div().relative().flex_none().child(admin_avatar(&u.id, &u.display_name, true, p)).child(presence(if u.online { &u.status } else { "OFFLINE" }, p)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(text(fs(12.)).font_weight(FontWeight::BOLD).text_color(p.text).truncate().child(u.display_name.clone()))
                            .when(u.admin, |s| s.child(badge(t("srv.users.owner"), Btn::Primary, p)))
                            .when(!u.admin && server_admin, |s| s.child(badge(t("srv.users.admin"), Btn::Primary, p)))
                            .when(u.ban.is_some(), |s| s.child(badge(t("srv.users.banned"), Btn::Danger, p)))
                            .when(u.two_factor_enabled, |s| s.child(badge("2FA".into(), Btn::Plain, p))),
                    )
                    .child(text(fs(10.)).text_color(p.text_3).truncate().child(format!("@{} · {}", u.username, u.email))),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .items_end()
                    .gap(px(2.))
                    .child(text(fs(10.)).css_weight(650.).text_color(if u.online { p.success } else { p.text_3 }).child(seen))
                    .child(text(fs(9.5)).text_color(p.text_4).child(tv("srv.users.projectCount", &[("n", &u.projects.len().to_string())]))),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.selected_user = if this.selected_user.as_deref() == Some(id.as_str()) { None } else { Some(id.clone()) };
                cx.notify();
            }));
        let mut card = div().flex().flex_col().border_1().border_color(if open { p.border_accent } else { p.border }).rounded(radius::MD).bg(p.bg_3).overflow_hidden().child(head);
        if open {
            card = card.child(self.user_detail(u, width, p, window, cx));
        }
        card.into_any_element()
    }

    fn user_detail(&self, u: &ServerUser, width: f32, p: &Palette, _window: &mut Window, cx: &mut Context<Self>) -> Div {
        let me = u.id == self.me;
        let protected = u.admin || me;
        let busy = self.busy;
        let mut detail = div().flex().flex_col().gap(px(10.)).px(px(12.)).pt(px(4.)).pb(px(12.)).border_t_1().border_color(p.border_subtle);
        if let Some(b) = &u.ban {
            let mut line = tv("srv.users.bannedSince", &[("date", &day(b.created_at.as_deref()))]);
            if let Some(by) = b.banned_by.as_ref().filter(|b| !b.is_empty()) {
                line.push_str(&format!(" · {by}"));
            }
            if let Some(r) = b.reason.as_ref().filter(|r| !r.is_empty()) {
                line.push_str(&format!(" · {}: {r}", t("srv.users.reason")));
            }
            detail = detail.child(warn(line, p).pt(px(8.)));
        }
        // Projeleri
        let mut chips = div().flex().flex_wrap().gap(px(5.));
        for pr in &u.projects {
            let role = if pr.primary_owner { t("srv.users.roleOwner") } else if pr.role == "OWNER" { t("srv.users.roleAdmin") } else { t("srv.users.roleMember") };
            chips = chips.child(div().px(px(8.)).py(px(3.)).border_1().border_color(p.border).rounded(radius::PILL).bg(p.bg_2).text_size(fs(10.)).line_height(fs(12.)).text_color(p.text_2).child(format!("{} · {role}", pr.name)));
        }
        if u.projects.is_empty() {
            chips = chips.child(hint(t("srv.users.noProjects"), p));
        }
        detail = detail.child(div().pt(px(8.)).flex().flex_col().gap(px(6.)).child(text(font_size::XXS).css_weight(800.).tracking(0.12).text_color(p.text_accent).child(tr_upper(&t("srv.users.projects")))).child(chips));
        // Sunucu yetkileri
        let can_grant = self.can("server.admin") && !protected;
        let mut toggles = Vec::new();
        for info in self.info.catalog.iter().filter(|c| c.id != "server.admin") {
            let on = u.admin || u.permissions.iter().any(|p| *p == info.id) || u.permissions.iter().any(|p| p == "server.admin");
            let explicit = u.permissions.iter().any(|p| *p == info.id);
            let disabled = !can_grant || busy || (on && !explicit);
            let (user, perm) = (u.clone(), info.id.clone());
            toggles.push(
                perm_toggle(SharedString::from(format!("srv-perm-{}", info.id)), &t(&format!("srv.perm.{}", info.id)), &t(&format!("srv.perm.{}.hint", info.id)), on, disabled, p)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if !disabled {
                            this.toggle_permission(&user, &perm, window, cx);
                        }
                    }))
                    .into_any_element(),
            );
        }
        detail = detail.child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(text(font_size::XXS).css_weight(800.).tracking(0.12).text_color(p.text_accent).child(tr_upper(&t("srv.users.permissions"))))
                .when(protected, |s| s.child(hint(t(if me { "srv.users.selfHint" } else { "srv.users.ownerHint" }), p)))
                .when(!protected && !self.can("server.admin"), |s| s.child(hint(t("srv.users.needAdmin"), p)))
                .child(perm_grid(toggles, width - 24., 240.)),
        );
        // Eylemler
        let mut actions = div().flex().flex_wrap().gap(px(6.));
        let uid = u.clone();
        if self.can("server.manage_users") {
            let it = u.clone();
            actions = actions.child(settings_button("srv-act-sessions", t("srv.users.closeSessions"), Btn::Plain, busy || u.sessions == 0, p).on_click(cx.listener(move |this, _, _, cx| {
                if !this.busy && it.sessions > 0 {
                    this.confirm = Some(Confirm::Sessions(it.clone()));
                    cx.notify();
                }
            })));
            if u.two_factor_enabled {
                let it = u.clone();
                actions = actions.child(settings_button("srv-act-2fa", t("srv.users.reset2fa"), Btn::Plain, busy, p).on_click(cx.listener(move |this, _, _, cx| {
                    if !this.busy {
                        this.confirm = Some(Confirm::Reset2fa(it.clone()));
                        cx.notify();
                    }
                })));
            }
        }
        if self.can("server.admin") && !protected {
            let is_admin = u.permissions.iter().any(|p| p == "server.admin");
            let it = u.clone();
            actions = actions.child(settings_button("srv-act-admin", t(if is_admin { "srv.users.demote" } else { "srv.users.promote" }), if is_admin { Btn::Plain } else { Btn::Primary }, busy, p).on_click(cx.listener(move |this, _, window, cx| this.set_admin(&it, !is_admin, window, cx))));
        }
        if self.can("server.ban") && !protected {
            if u.ban.is_some() {
                let (id, name) = (uid.id.clone(), uid.display_name.clone());
                actions = actions.child(settings_button("srv-act-unban", t("srv.users.unban"), Btn::Plain, busy, p).on_click(cx.listener(move |this, _, window, cx| {
                    let (id, name) = (id.clone(), name.clone());
                    this.act(format!("{name} yasağı kaldırıldı"), tv("srv.users.unbannedNotice", &[("name", &name)]), move |api| api.server_unban(&id), window, cx);
                })));
            } else {
                let it = u.clone();
                actions = actions.child(settings_button("srv-act-ban", t("srv.users.ban"), Btn::Danger, busy, p).on_click(cx.listener(move |this, _, _, cx| {
                    if !this.busy {
                        this.confirm = Some(Confirm::Ban(it.clone()));
                        cx.notify();
                    }
                })));
            }
        }
        detail.child(actions)
    }

    fn users_view(&self, width: f32, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let q = self.search.read(cx).value().trim().to_lowercase();
        let list: Vec<ServerUser> = self.users.iter().filter(|u| q.is_empty() || u.username.to_lowercase().contains(&q) || u.display_name.to_lowercase().contains(&q) || u.email.to_lowercase().contains(&q)).cloned().collect();
        let mut rows = div().flex().flex_col().gap(px(5.));
        for (i, u) in list.iter().enumerate() {
            rows = rows.child(self.user_row(i, u, width, p, window, cx));
        }
        if list.is_empty() {
            rows = rows.child(hint(t(if self.users.is_empty() { "srv.loading" } else { "srv.users.none" }), p));
        }
        settings_block(tv("srv.users.title", &[("n", &self.users.len().to_string())]), true, p)
            .child(hint(t("srv.users.hint"), p))
            .child(field_input(&self.search, false, p, window, cx))
            .child(rows)
            .into_any_element()
    }

    fn project_row(&self, i: usize, pr: &ServerProject, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let open = self.selected_project.as_deref() == Some(pr.id.as_str());
        let deleted = pr.deleted_at.is_some();
        let hover = p.bg_hover;
        let id = pr.id.clone();
        let owner = pr.owner.as_ref().map(|o| o.display_name.clone()).unwrap_or_else(|| "—".into());
        let mut subs = vec![tv("srv.projects.owner", &[("name", &owner)]), tv("srv.projects.members", &[("n", &pr.members.to_string())])];
        if deleted {
            subs.push(tv("srv.projects.purgeAt", &[("date", &day(pr.purge_at.as_deref()))]));
        } else {
            subs.push(tv("srv.projects.created", &[("date", &day(pr.created_at.as_deref()))]));
        }
        let public = pr.visibility.eq_ignore_ascii_case("public");
        let head = div()
            .id(SharedString::from(format!("srv-project-{i}")))
            .flex()
            .items_center()
            .gap(px(11.))
            .px(px(10.))
            .py(px(9.))
            .cursor_pointer()
            .hover(move |s| s.bg(hover))
            .when(deleted, |s| s.opacity(0.7))
            .child(
                div()
                    .size(px(30.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(9.))
                    .border_1()
                    .border_color(p.border_subtle)
                    .bg(p.accent_soft)
                    .text_color(p.text_accent)
                    .text_size(fs(10.))
                    .css_weight(800.)
                    .child(p2_ui::components::user_initials(&pr.name)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(text(fs(12.)).font_weight(FontWeight::BOLD).text_color(p.text).truncate().child(pr.name.clone()))
                            .child(badge(t(if public { "srv.projects.public" } else { "srv.projects.private" }), if public { Btn::Primary } else { Btn::Plain }, p))
                            .when(pr.member, |s| s.child(badge(t("srv.projects.member"), Btn::Plain, p)))
                            .when(deleted, |s| s.child(badge(t("srv.projects.inTrash"), Btn::Danger, p))),
                    )
                    .child(text(fs(10.)).text_color(p.text_3).truncate().child(subs.join(" · "))),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.selected_project = if this.selected_project.as_deref() == Some(id.as_str()) { None } else { Some(id.clone()) };
                this.transferring = false;
                cx.notify();
            }));
        let mut card = div().flex().flex_col().border_1().border_color(if open { p.border_accent } else { p.border }).rounded(radius::MD).bg(p.bg_3).overflow_hidden().child(head);
        if open {
            card = card.child(self.project_detail(pr, p, window, cx));
        }
        card.into_any_element()
    }

    fn project_detail(&self, pr: &ServerProject, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let manage = self.can("server.manage_projects");
        let busy = self.busy || !manage;
        let deleted = pr.deleted_at.is_some();
        let mut actions = div().flex().flex_wrap().gap(px(6.));
        if !deleted {
            let id = pr.id.clone();
            let name = pr.name.clone();
            actions = actions.child(settings_button("srv-pr-enter", t(if pr.member { "srv.projects.open" } else { "srv.projects.enter" }), Btn::Primary, busy, p).on_click(cx.listener(move |this, _, window, cx| {
                if this.can("server.manage_projects") {
                    let id = id.clone();
                    this.act(format!("{name} projesine girildi"), tv("srv.projects.entered", &[("name", &name)]), move |api| api.server_enter_project(&id), window, cx);
                }
            })));
            actions = actions.child(settings_button("srv-pr-transfer", t("srv.projects.transfer"), Btn::Plain, busy, p).on_click(cx.listener(move |this, _, _, cx| {
                if this.can("server.manage_projects") {
                    this.transferring = !this.transferring;
                    cx.notify();
                }
            })));
            let it = pr.clone();
            actions = actions.child(settings_button("srv-pr-delete", t("srv.projects.delete"), Btn::Danger, busy, p).on_click(cx.listener(move |this, _, _, cx| {
                if this.can("server.manage_projects") {
                    this.confirm = Some(Confirm::Delete(it.clone()));
                    cx.notify();
                }
            })));
        } else {
            let (id, name) = (pr.id.clone(), pr.name.clone());
            actions = actions.child(settings_button("srv-pr-restore", t("srv.projects.restore"), Btn::Primary, busy, p).on_click(cx.listener(move |this, _, window, cx| {
                if this.can("server.manage_projects") {
                    let id = id.clone();
                    this.act(format!("{name} geri yüklendi"), tv("srv.projects.restored", &[("name", &name)]), move |api| api.server_restore_project(&id), window, cx);
                }
            })));
            let it = pr.clone();
            actions = actions.child(settings_button("srv-pr-purge", t("srv.projects.purge"), Btn::Danger, busy, p).on_click(cx.listener(move |this, _, _, cx| {
                if this.can("server.manage_projects") {
                    this.confirm = Some(Confirm::Purge(it.clone()));
                    cx.notify();
                }
            })));
        }
        let mut detail = div().flex().flex_col().gap(px(10.)).px(px(12.)).pt(px(10.)).pb(px(12.)).border_t_1().border_color(p.border_subtle);
        if let Some(d) = pr.description.as_ref().filter(|d| !d.is_empty()) {
            detail = detail.child(hint(d.clone(), p));
        }
        if !manage {
            detail = detail.child(hint(t("srv.projects.needManage"), p));
        }
        detail = detail.child(actions);
        if self.transferring && !deleted {
            let q = self.transfer_search.read(cx).value().trim().to_lowercase();
            let owner_id = pr.owner.as_ref().map(|o| o.id.clone()).unwrap_or_default();
            let mut list = div().flex().flex_col().gap(px(3.));
            let candidates: Vec<&ServerUser> = self.users.iter().filter(|u| u.id != owner_id && u.ban.is_none() && (q.is_empty() || u.username.to_lowercase().contains(&q) || u.display_name.to_lowercase().contains(&q))).take(6).collect();
            for (i, u) in candidates.iter().enumerate() {
                let (pr2, u2) = (pr.clone(), (*u).clone());
                let hover = p.bg_hover;
                list = list.child(
                    div()
                        .id(SharedString::from(format!("srv-tr-{i}")))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .px(px(8.))
                        .py(px(5.))
                        .rounded(radius::SM)
                        .cursor_pointer()
                        .hover(move |s| s.bg(hover))
                        .child(admin_avatar(&u.id, &u.display_name, true, p))
                        .child(text(fs(11.5)).css_weight(650.).text_color(p.text).child(u.display_name.clone()))
                        .child(text(fs(10.)).text_color(p.text_3).child(format!("@{}", u.username)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.confirm = Some(Confirm::Transfer(pr2.clone(), u2.clone()));
                            cx.notify();
                        })),
                );
            }
            if candidates.is_empty() {
                list = list.child(hint(t("srv.users.none"), p));
            }
            detail = detail.child(settings_field(&t("srv.projects.newOwner"), field_input(&self.transfer_search, false, p, window, cx), p)).child(list);
        }
        detail
    }

    fn projects_view(&self, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let q = self.project_search.read(cx).value().trim().to_lowercase();
        let matches = |pr: &&ServerProject| q.is_empty() || pr.name.to_lowercase().contains(&q) || pr.slug.contains(&q) || pr.owner.as_ref().is_some_and(|o| o.display_name.to_lowercase().contains(&q) || o.username.to_lowercase().contains(&q));
        let live: Vec<ServerProject> = self.projects.iter().filter(|p| p.deleted_at.is_none()).filter(matches).cloned().collect();
        let trash: Vec<ServerProject> = self.projects.iter().filter(|p| p.deleted_at.is_some()).filter(matches).cloned().collect();
        let mut live_rows = div().flex().flex_col().gap(px(5.));
        for (i, pr) in live.iter().enumerate() {
            live_rows = live_rows.child(self.project_row(i, pr, p, window, cx));
        }
        if live.is_empty() {
            live_rows = live_rows.child(hint(t("srv.projects.none"), p));
        }
        let mut trash_rows = div().flex().flex_col().gap(px(5.));
        for (i, pr) in trash.iter().enumerate() {
            trash_rows = trash_rows.child(self.project_row(1000 + i, pr, p, window, cx));
        }
        if trash.is_empty() {
            trash_rows = trash_rows.child(hint(t("srv.projects.trashEmpty"), p));
        }
        div()
            .flex()
            .flex_col()
            .child(
                settings_block(tv("srv.projects.title", &[("n", &self.projects.iter().filter(|p| p.deleted_at.is_none()).count().to_string())]), false, p)
                    .child(hint(t("srv.projects.hint"), p))
                    .child(field_input(&self.project_search, false, p, window, cx))
                    .child(live_rows),
            )
            .child(settings_block(tv("srv.projects.trash", &[("n", &self.projects.iter().filter(|p| p.deleted_at.is_some()).count().to_string())]), true, p).child(hint(t("srv.projects.trashHint"), p)).child(trash_rows))
            .into_any_element()
    }

    fn settings_view(&self, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(s) = &self.settings else { return hint(t("srv.loading"), p).into_any_element() };
        let editable = self.can("server.settings");
        let disabled = !editable || self.busy;
        let name_val = self.name.read(cx).value().trim().to_string();
        let name_changed = Some(name_val.as_str()) != s.name.as_deref() && name_val.chars().count() >= 2;
        let reg = |id: &'static str, value: &'static str, title: &str, body: &str, cx: &mut Context<Self>| {
            choice(id.into(), t(title), t(body), s.registration_policy == value, disabled, p).on_click(cx.listener(move |this, _, window, cx| {
                if this.can("server.settings") && !this.busy && this.settings.as_ref().is_some_and(|s| s.registration_policy != value) {
                    this.save_settings(serde_json::json!({ "registrationPolicy": value }), window, cx);
                }
            }))
        };
        let creation = |id: &'static str, value: &'static str, title: &str, body: &str, cx: &mut Context<Self>| {
            choice(id.into(), t(title), t(body), s.project_creation == value, disabled, p).on_click(cx.listener(move |this, _, window, cx| {
                if this.can("server.settings") && !this.busy && this.settings.as_ref().is_some_and(|s| s.project_creation != value) {
                    this.save_settings(serde_json::json!({ "projectCreation": value }), window, cx);
                }
            }))
        };
        div()
            .flex()
            .flex_col()
            .when(!editable, |d| d.child(hint(t("srv.set.readOnly"), p).mb(px(10.))))
            .child(
                settings_block(t("srv.set.name"), false, p).child(hint(t("srv.set.nameHint"), p)).child(
                    div()
                        .flex()
                        .items_end()
                        .gap(px(8.))
                        .child(div().flex_1().child(settings_field(&t("srv.set.nameLabel"), field_input(&self.name, !editable, p, window, cx), p)))
                        .child(settings_button("srv-set-name", t("srv.save"), Btn::Primary, disabled || !name_changed, p).h(px(34.)).on_click(cx.listener(move |this, _, window, cx| {
                            let name = this.name.read(cx).value().trim().to_string();
                            if this.can("server.settings") && !this.busy && name.chars().count() >= 2 {
                                this.save_settings(serde_json::json!({ "name": name }), window, cx);
                            }
                        }))),
                ),
            )
            .child(
                settings_block(t("srv.set.registration"), false, p)
                    .child(hint(t("srv.set.registrationHint"), p))
                    .child(div().flex().gap(px(8.)).child(reg("srv-reg-open", "open", "srv.set.regOpen", "srv.set.regOpenHint", cx)).child(reg("srv-reg-invite", "invite", "srv.set.regInvite", "srv.set.regInviteHint", cx))),
            )
            .child(
                settings_block(t("srv.set.creation"), true, p)
                    .child(hint(t("srv.set.creationHint"), p))
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(creation("srv-create-permission", "permission", "srv.set.createPermission", "srv.set.createPermissionHint", cx))
                            .child(creation("srv-create-everyone", "everyone", "srv.set.createEveryone", "srv.set.createEveryoneHint", cx)),
                    ),
            )
            .into_any_element()
    }

    fn confirm_dialog(&self, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (title, body, label) = match self.confirm.as_ref()? {
            Confirm::Ban(u) => (t("srv.confirm.ban"), tv("srv.confirm.banBody", &[("name", &u.display_name)]), t("srv.users.ban")),
            Confirm::Sessions(u) => (t("srv.users.closeSessions"), tv("srv.confirm.sessionsBody", &[("name", &u.display_name)]), t("srv.users.closeSessions")),
            Confirm::Reset2fa(u) => (t("srv.users.reset2fa"), tv("srv.confirm.reset2faBody", &[("name", &u.display_name)]), t("srv.users.reset2fa")),
            Confirm::Delete(pr) => (t("srv.confirm.delete"), tv("srv.confirm.deleteBody", &[("name", &pr.name)]), t("srv.projects.delete")),
            Confirm::Purge(pr) => (t("srv.confirm.purge"), tv("srv.confirm.purgeBody", &[("name", &pr.name), ("slug", &pr.slug)]), t("srv.projects.purge")),
            Confirm::Transfer(pr, u) => (t("srv.projects.transfer"), tv("srv.confirm.transferBody", &[("name", &pr.name), ("user", &u.display_name)]), t("srv.projects.transfer")),
        };
        let ban = matches!(self.confirm, Some(Confirm::Ban(_)));
        let danger = !matches!(self.confirm, Some(Confirm::Transfer(..)));
        let (card, close, cancel, ok) = confirm_modal(title, body, &label, danger, window, p);
        let card = card.when(ban, |s| s.child(div().px(px(24.)).pb(px(6.)).child(settings_field(&t("srv.users.reasonOptional"), field_input(&self.ban_reason, false, p, window, cx), p))));
        let dismiss = |this: &mut Self, _: &gpui_kit::ClickEvent, _: &mut Window, cx: &mut Context<Self>| {
            this.confirm = None;
            cx.notify();
        };
        let card = card
            .child(modal_foot(p).child(cancel.on_click(cx.listener(dismiss))).child(ok.on_click(cx.listener(|this, _, window, cx| this.run_confirm(window, cx)))))
            .child(div().absolute().top(px(18.)).right(px(16.)).child(close.on_click(cx.listener(dismiss))))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .p2_enter("srv-confirm-card", Entrance::PopElastic, std::time::Duration::ZERO);
        Some(
            div()
                .id("srv-confirm")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .px(px(24.))
                .pb(px(24.))
                .bg(p.overlay)
                .backdrop_blur(p2_ui::fx::BLUR_SM)
                .occlude()
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    this.confirm = None;
                    cx.notify();
                }))
                .child(card)
                .into_any_element(),
        )
    }
}

impl Render for ServerAdminPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        let (w, h) = modal_size(1080., 760., window);
        let role = if self.info.owner { t("srv.roleOwner") } else if self.can("server.admin") { t("srv.roleAdmin") } else { t("srv.roleStaff") };
        let name = self.overview.as_ref().and_then(|o| o.name.clone()).or_else(|| self.settings.as_ref().and_then(|s| s.name.clone())).unwrap_or_else(|| "P2".into());
        let mut nav = settings_nav(t("srv.title"), format!("{name} · {role}"), &p);
        for tab in self.visible() {
            let (label, sub) = (t(&format!("srv.tab.{}", tab.key())), t(&format!("srv.tab.{}.hint", tab.key())));
            nav = nav.child(nav_button(SharedString::from(format!("srv-tab-{}", tab.key())), &label, Some(&sub), self.tab == tab, &p).on_click(cx.listener(move |this, _, window, cx| this.open(tab, window, cx))));
        }
        let content_w = w - 2. - 230. - 40.;
        let body = match self.tab {
            Tab::Overview => self.overview_view(content_w, &p),
            Tab::Users => self.users_view(content_w, &p, window, cx),
            Tab::Projects => self.projects_view(&p, window, cx),
            Tab::Settings => self.settings_view(&p, window, cx),
        };
        let title = t(&format!("srv.tab.{}", self.tab.key()));
        let scroll = div()
            .id(("srv-scroll", self.tab as usize))
            .flex_1()
            .min_h_0()
            .pt(px(16.))
            .px(px(20.))
            .pb(px(30.))
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .child(body)
            .p2_enter(("srv-body", self.tab as usize), Entrance::Rise, std::time::Duration::ZERO);
        let card = settings_card("srv-card", w, h, &p)
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
                    .child(settings_header(text(fs(14.)).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(title).into_any_element(), div().id("srv-close").on_click(cx.listener(|_, _, _, cx| cx.emit(ServerAdminEvent::Close))), &p))
                    .child(scroll),
            )
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .p2_enter("srv-admin", Entrance::PopElastic, std::time::Duration::ZERO);
        let confirm = self.confirm_dialog(&p, window, cx);
        settings_layer("srv-scrim", card, &p)
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                if this.confirm.is_none() {
                    cx.emit(ServerAdminEvent::Close)
                }
            }))
            .children(confirm)
    }
}
