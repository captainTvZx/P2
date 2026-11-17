//! Ana sayfa: arkadaşlar ve projelerden bağımsız mesajlar (docs/arkadaslar.md).
//!
//! - Rayın en üstünde Ana sayfa düğmesi + okunmamış DM baloncukları (`rail_home`).
//! - Ana sayfa kenar çubuğu: "Arkadaşlar" girişi, arama, DM listesi (birebir + gruplar) (`home_sidebar`).
//! - Arkadaşlar sayfası: Çevrimiçi / Tümü / Bekleyen / Engellenen sekmeleri + "Arkadaş ekle" (`friends_page`).
//! - Kişi menüsü ve profil kartı eylemleri (`friend_menu_items`, `friend_actions`).
//!
//! Karar: burada yalnız **Ana sayfa sohbetleri** (hesaba bağlı, `projectId` yok) ve **Ana sayfa
//! grupları** yaşar. Bir projenin kişi listesinden açılan sohbet o projenin sohbetidir ve projenin
//! kenar çubuğunda durur (workspace_nav.rs); aynı kişiyle ikisi ayrı sohbetlerdir. Yıldızlar da
//! kapsamlıdır: buradaki "Yıldızlı" bölümü yalnız Ana sayfada yıldızlananları gösterir.
//! Metinler `p2-i18n` (`friends.*`, `dm.*`).

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, Entity, MouseButton, MouseDownEvent, SharedString, Subscription, Window, div, linear_color_stop, linear_gradient, point, px};
use p2_client_core::{Api, ApiError, ChatGroup, Conversation, Friends, Member, Relation, Scope};
use p2_i18n::{t, tv};
use p2_ui::components::avatar_hue;
use p2_ui::controls::{self, BadgeTone, BtnSize, BtnVariant, TabsStyle, ToastTone};
use p2_ui::icons::{Icon, icon, svg_image};
use p2_ui::tokens::{Palette, fs};
use p2_ui::{P2Motion as _, P2Text as _};
use serde_json::Value;

use super::super::new_message::{Candidate, NewMessageDialog, NewMessageEvent};
use super::people_ui::tip_owned;
use super::workspace_nav::{self, NavMenu};
use super::{ChatSubject, WorkspaceScreen};
use crate::task;

const ICON_CHECK: &str = r#"<path d="M5 12.5l4.5 4.5L19 7.5"/>"#;
const ICON_USER_PLUS: &str = r#"<circle cx="10" cy="8" r="3.6"/><path d="M3.5 19.5c.8-3.4 3.3-5.2 6.5-5.2s5.700 1.800 6.500 5.200M18.5 6.500v5M16 9h5"/>"#;
const ICON_BLOCK: &str = r#"<circle cx="12" cy="12" r="8"/><path d="M6.4 6.400l11.200 11.200"/>"#;
const ICON_HOME_CHAT: &str = r#"<path d="M4.500 6.500a2.500 2.500 0 0 1 2.500-2.500h10a2.500 2.500 0 0 1 2.500 2.500v7a2.500 2.500 0 0 1-2.500 2.500h-5.200L7.500 19.800V16H7a2.500 2.500 0 0 1-2.500-2.500z"/><path d="M8.500 8.700h7M8.500 11.700h4.500"/>"#;

/// Arkadaşlar sayfasının sekmeleri.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Tab {
    Online,
    All,
    Pending,
    Blocked,
    Add,
}

/// Ana sayfa durumu (çalışma alanının `home` alanı).
pub(super) struct Home {
    /// Ana sayfa açık mı (proje yerine arkadaşlar/mesajlar gösterilir).
    pub open: bool,
    pub tab: Tab,
    pub tab_epoch: usize,
    pub data: Friends,
    /// Ana sayfa grupları (projeye bağlı olmayanlar); her zaman tutulur (ray rozeti, gelen mesajlar).
    pub groups: Vec<ChatGroup>,
    pub search: Entity<InputState>,
    pub add: Entity<InputState>,
    /// "Arkadaş ekle" formunun altındaki ileti (metin, başarılı mı).
    pub add_note: Option<(String, bool)>,
    pub busy: bool,
    _subs: Vec<Subscription>,
}

impl Home {
    pub fn new(window: &mut Window, cx: &mut Context<WorkspaceScreen>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx));
        let add = cx.new(|cx| InputState::new(window, cx));
        let subs = vec![
            cx.subscribe(&search, |_: &mut WorkspaceScreen, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe_in(&add, window, |this: &mut WorkspaceScreen, _, event: &InputEvent, window, cx| match event {
                InputEvent::PressEnter { .. } => this.friend_add_by_name(window, cx),
                _ => cx.notify(),
            }),
        ];
        Self { open: false, tab: Tab::Online, tab_epoch: 0, data: Friends::default(), groups: Vec::new(), search, add, add_note: None, busy: false, _subs: subs }
    }

    /// Gerçek zamanlı durum: arkadaşın çevrimiçi bilgisi.
    pub fn set_status(&mut self, user_id: &str, status: &str) {
        for f in self.data.friends.iter_mut().filter(|f| f.id == user_id) {
            f.status = status.to_string();
        }
    }
}

fn status_text(status: &str) -> String {
    t(match status {
        "ONLINE" => "friends.status.online",
        "AWAY" => "friends.status.away",
        "BUSY" => "friends.status.busy",
        _ => "friends.status.offline",
    })
}

/// Hata kodunun arayüz metni (bilinmeyen kodda sunucu iletisi).
fn error_text(e: &ApiError) -> String {
    match p2_client_core::friend_error_key(&e.code) {
        Some(key) => t(key),
        None if e.message.is_empty() => t("friends.err.generic"),
        None => e.message.clone(),
    }
}

fn upper(s: String) -> String {
    s.chars()
        .flat_map(|c| match c {
            'i' if p2_i18n::language().id == "tr" => vec!['İ'],
            'ı' => vec!['I'],
            c => c.to_uppercase().collect(),
        })
        .collect()
}

fn lower(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'İ' => vec!['i'],
            'I' if p2_i18n::language().id == "tr" => vec!['ı'],
            c => c.to_lowercase().collect(),
        })
        .collect()
}

/// Kişi avatarı (degrade zemin + fotoğraf/baş harfler); `status` verilirse durum halkası.
fn avatar(member_id: &str, name: &str, size: f32, radius: f32, status: Option<(&str, gpui_kit::Hsla)>, p: &Palette) -> gpui_kit::Div {
    let hue = p.hues[avatar_hue(member_id)];
    div()
        .relative()
        .flex_none()
        .size(px(size))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius))
        .bg(linear_gradient(145., linear_color_stop(hue.bg, 0.), linear_color_stop(hue.bg2, 1.)))
        .text_color(hue.fg)
        .text_size(fs((size * 0.34).max(9.)))
        .line_height(px(size))
        .css_weight(800.)
        .tracking(-0.02)
        .child(workspace_nav::face(member_id, name, radius))
        .when_some(status, |s, (status, ring)| s.child(workspace_nav::presence_mark(status, ring, p).absolute().right(px(-3.)).bottom(px(-3.))))
}

/// Son mesajın tek satırlık önizlemesi (dosya işaretleri gizlenir).
fn preview(content: &str, mine: bool) -> String {
    let line = content.lines().find(|l| !l.trim().is_empty()).unwrap_or_default();
    let text = if line.starts_with("[[p2-file|") { t("friends.file") } else { line.to_string() };
    if mine { format!("{}{}", t("friends.you"), text) } else { text }
}

/// Satır sonundaki yuvarlak simge düğmesi.
fn round_action(id: SharedString, glyph: Icon, tip_text: String, danger: bool, p: &Palette) -> gpui_kit::Stateful<gpui_kit::Div> {
    let (hover_bg, hover_fg, border) = if danger { (p.danger_soft, p.danger_text, p.danger_border) } else { (p.accent_soft_2, p.text_strong, p.border_accent) };
    let group: SharedString = format!("ra-{id}").into();
    div()
        .id(id)
        .group(group.clone())
        .flex_none()
        .size(px(34.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_color(p.border_subtle)
        .bg(p.bg_3)
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg).border_color(border))
        .child(glyph.el().size(px(16.)).text_color(p.text_2).group_hover(group, move |s| s.text_color(hover_fg)))
        .tooltip(tip_owned(tip_text))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
}

/// Sağ tık / "diğer" menüsü satırı (workspace_nav `entry` görünümü).
fn menu_entry(id: &'static str, label: String, danger: bool, p: &Palette) -> gpui_kit::Stateful<gpui_kit::Div> {
    let (hover_bg, hover_fg) = if danger { (p.danger_soft, p.danger_text) } else { (p.accent_soft, p.text_strong) };
    div()
        .id(id)
        .w_full()
        .h(px(32.))
        .px(px(10.))
        .flex()
        .items_center()
        .rounded(px(6.))
        .text_color(if danger { p.danger_text } else { p.text })
        .text_size(fs(12.))
        .css_weight(600.)
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg).text_color(hover_fg))
        .child(label)
}

impl WorkspaceScreen {
    // ------------------------------------------------------------ veri

    pub(super) fn load_friends(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let api = self.api.clone();
        task::run(window, cx, move || api.friends(), |this, result, window, cx| {
            if let Ok(data) = result {
                for f in data.friends.iter().chain(data.incoming.iter().map(|r| &r.user)).chain(data.outgoing.iter().map(|r| &r.user)).chain(data.blocked.iter().map(|r| &r.user)) {
                    crate::avatars::remember(&f.id, f.avatar_url.as_deref());
                }
                crate::trace(&format!("arkadaşlar: {} arkadaş, {} gelen, {} giden, {} engelli", data.friends.len(), data.incoming.len(), data.outgoing.len(), data.blocked.len()));
                this.home.data = data;
                this.home_autopilot(window, cx);
                cx.notify();
            }
        });
    }

    /// Ana sayfa grupları: projeye bağlı olmayan gruplarım (proje grupları projenin kenar çubuğunda).
    pub(super) fn load_home_groups(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (api, org) = (self.api.clone(), self.org.id.clone());
        task::run(window, cx, move || api.home_groups(&org), |this, result, _, cx| {
            if let Ok(mut groups) = result {
                // Açık grup okunmuş sayılır.
                if let Some(Scope::Group(open)) = this.current_scope(cx) {
                    groups.iter_mut().filter(|g| g.id == open).for_each(|g| g.unread = 0);
                }
                for m in groups.iter().flat_map(|g| g.members.iter()) {
                    crate::avatars::remember(&m.id, m.avatar_url.as_deref());
                }
                crate::trace(&format!("ana sayfa: {} grup", groups.len()));
                this.home.groups = groups;
                cx.notify();
            }
        });
    }

    /// "Yeni mesaj" penceresindeki kişiler: arkadaşlarım + (şimdilik bilinen) proje paylaştıklarım.
    fn new_message_candidates(&self, extra: &[Member]) -> Vec<Candidate> {
        let me = self.auth.user.id.as_str();
        let data = &self.home.data;
        let mut out: Vec<Candidate> = data.friends.iter().map(|f| Candidate { member: f.member(), friend: true }).collect();
        let others = self.conversations.iter().filter(|c| c.is_home() && !c.blocked_by_me).map(|c| &c.user).chain(self.members.iter()).chain(extra.iter());
        for m in others {
            if m.id != me && !data.is_blocked(&m.id) && !out.iter().any(|c| c.member.id == m.id) {
                out.push(Candidate { member: m.clone(), friend: false });
            }
        }
        out
    }

    /// Ana sayfa grubuna eklenebilecek kişiler (arkadaşlar + bilinen proje arkadaşları).
    pub(super) fn home_group_pool(&self) -> Vec<Member> {
        self.new_message_candidates(&self.org_members).into_iter().map(|c| c.member).collect()
    }

    /// Ana sayfa › Direkt mesajlar › ＋: "Yeni mesaj" penceresi. Tek kişi → Ana sayfa sohbeti;
    /// birden çok kişi → Ana sayfa grubu (projeye bağlı değil).
    pub(super) fn open_new_message(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.nav_menu = None;
        self.close_group_members(cx);
        let candidates = self.new_message_candidates(&[]);
        let dialog = cx.new(|cx| NewMessageDialog::new(self.api.clone(), self.org.id.clone(), None, candidates, window, cx));
        let sub = cx.subscribe_in(&dialog, window, |this, _, event: &NewMessageEvent, window, cx| {
            this.new_message = None;
            match event {
                NewMessageEvent::Direct(member) => {
                    crate::trace(&format!("yeni mesaj: @{} ile sohbet", member.username));
                    this.open_home_direct(member.clone(), window, cx);
                }
                NewMessageEvent::Group(group) => {
                    crate::trace(&format!("yeni mesaj: grup «{}» kuruldu ({} üye)", group.name, group.members.len()));
                    if !this.home.groups.iter().any(|g| g.id == group.id) {
                        this.home.groups.insert(0, group.clone());
                    }
                    this.enter_home(window, cx);
                    this.select_group(group.clone(), window, cx);
                }
                NewMessageEvent::Cancelled => {}
            }
            cx.notify();
        });
        crate::trace("yeni mesaj: pencere açıldı");
        self.new_message = Some((dialog, sub));
        cx.notify();
        // Proje paylaştığım kişiler arka planda gelir (bütün projelerimin üyeleri).
        let api = self.api.clone();
        let projects: Vec<String> = self.projects.iter().map(|p| p.id.clone()).collect();
        task::run(
            window,
            cx,
            move || projects.iter().filter_map(|p| api.project_members(p).ok()).flatten().map(Member::from).collect::<Vec<Member>>(),
            |this, members, _, cx| {
                for m in &members {
                    crate::avatars::remember(&m.id, m.avatar_url.as_deref());
                }
                let candidates = this.new_message_candidates(&members);
                if let Some((dialog, _)) = &this.new_message {
                    crate::trace(&format!("yeni mesaj: {} kişi listelendi", candidates.len()));
                    dialog.update(cx, |d, cx| {
                        d.set_candidates(candidates);
                        cx.notify();
                    });
                }
            },
        );
    }

    /// Projede yeni sohbet / grup başlatabilir miyim (`direct.send` ya da `group.create`): kenar
    /// çubuğundaki "＋" buna göre görünür.
    pub(super) fn can_start_project_chat(&self) -> bool {
        self.project.is_some() && (self.my_perms.can("direct.send") || self.my_perms.can("group.create"))
    }

    /// Proje kenar çubuğu › SOHBETLER / GRUPLAR › ＋: aynı "Yeni mesaj" penceresi, projeye bağlı.
    /// Listede yalnız bu projenin üyeleri vardır. Tek kişi → bu projenin sohbeti açılır / kurulur;
    /// birkaç kişi → proje grubu (`ChatGroup.projectId`) kurulur, kenar çubuğunun GRUPLAR bölümünde durur.
    pub(super) fn open_project_message(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.project.map(|ix| self.projects[ix].clone()) else { return };
        let (can_direct, can_group) = (self.my_perms.can("direct.send"), self.my_perms.can("group.create"));
        if !can_direct && !can_group {
            crate::toasts::warn(t("dm.new.project.forbidden"));
            return;
        }
        self.nav_menu = None;
        self.project_menu = false;
        self.close_group_members(cx);
        let me = self.auth.user.id.clone();
        let data = &self.home.data;
        let candidates: Vec<Candidate> = self.members.iter().filter(|m| m.id != me && !data.is_blocked(&m.id)).map(|m| Candidate { member: m.clone(), friend: false }).collect();
        let count = candidates.len();
        let scope = super::super::new_message::ProjectScope { id: project.id.clone(), name: project.name.clone(), can_direct, can_group };
        let dialog = cx.new(|cx| NewMessageDialog::new(self.api.clone(), self.org.id.clone(), Some(scope), candidates, window, cx));
        let pid = project.id.clone();
        let sub = cx.subscribe_in(&dialog, window, move |this, _, event: &NewMessageEvent, window, cx| {
            this.new_message = None;
            match event {
                NewMessageEvent::Direct(member) => {
                    crate::trace(&format!("yeni mesaj: @{} ile proje sohbeti", member.username));
                    this.open_direct_in(member.clone(), Some(pid.clone()), window, cx);
                }
                NewMessageEvent::Group(group) => {
                    crate::trace(&format!("yeni mesaj: proje grubu «{}» kuruldu ({} üye)", group.name, group.members.len()));
                    // Bu arada başka projeye geçildiyse liste o projenindir; grup oraya eklenmez.
                    if this.project_id().as_deref() == Some(pid.as_str()) {
                        if !this.groups.iter().any(|g| g.id == group.id) {
                            this.groups.insert(0, group.clone());
                        }
                        this.collapsed.remove("@groups");
                        this.select_group(group.clone(), window, cx);
                    }
                    crate::toasts::ok(tv("dm.new.project.created", &[("name", &group.name)]));
                }
                NewMessageEvent::Cancelled => {}
            }
            cx.notify();
        });
        crate::trace(&format!("yeni mesaj: proje penceresi açıldı ({} · {count} üye{})", project.name, if can_group { "" } else { ", grup yetkisi yok" }));
        self.new_message = Some((dialog, sub));
        cx.notify();
    }

    /// Ana sayfaya geçer: seçili proje bırakılır (ray düğmesiyle geri dönülür), sohbet kapanır.
    pub(super) fn enter_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.board = None;
        self.documents = None;
        self.library = None;
        if self.home.open {
            cx.notify();
            return;
        }
        // Çıkılan projenin ray rozeti eldeki (okunmuş) kanal listesinden güncellenir.
        self.leave_project_view(window, cx);
        self.home.open = true;
        self.project = None;
        self.categories.clear();
        self.members.clear();
        self.groups.clear();
        self.project_invite = None;
        self.project_menu = false;
        self.channel_menu = None;
        self.stage = None;
        self.profile = None;
        self.close_chat(cx);
        crate::titlebar::set_project_title(&t("friends.home"));
        let app = self.app.clone();
        cx.defer(move |cx| {
            let _ = app.update(cx, |_, cx| cx.notify());
        });
        crate::trace("ana sayfa: açıldı");
        self.load_conversations(window, cx);
        self.load_friends(window, cx);
        self.load_home_groups(window, cx);
        self.sync_chat_head(cx);
        cx.notify();
    }

    /// Ana sayfa › Arkadaşlar (açık DM kapanır).
    pub(super) fn open_friends(&mut self, tab: Option<Tab>, window: &mut Window, cx: &mut Context<Self>) {
        self.enter_home(window, cx);
        if self.chat.is_some() {
            self.close_chat(cx);
        }
        if let Some(tab) = tab {
            self.home.tab = tab;
            self.home.tab_epoch += 1;
        }
        crate::trace(&format!("ana sayfa: arkadaşlar {:?}", self.home.tab));
        cx.notify();
    }

    /// `friend:*` olayları: bildirim + listeler yenilenir.
    pub(super) fn on_friend_event(&mut self, name: &str, payload: &Value, window: &mut Window, cx: &mut Context<Self>) {
        let me = self.auth.user.id.as_str();
        let who = |v: &Value| v["displayName"].as_str().unwrap_or_default().to_string();
        match name {
            "friend:request" if payload["to"]["id"].as_str() == Some(me) => {
                crate::toasts::push(ToastTone::Info, None, &tv("friends.note.request", &[("name", &who(&payload["from"]))]));
            }
            "friend:accepted" if payload["requester"]["id"].as_str() == Some(me) => {
                crate::toasts::push(ToastTone::Ok, None, &tv("friends.note.accepted", &[("name", &who(&payload["addressee"]))]));
            }
            _ => {}
        }
        crate::trace(&format!("arkadaşlar: olay {name}"));
        self.load_friends(window, cx);
        self.load_conversations(window, cx);
    }

    // ------------------------------------------------------------ eylemler

    fn friend_run(&mut self, job: impl FnOnce(&Api) -> Result<Value, ApiError> + Send + 'static, done: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let api = self.api.clone();
        task::run(window, cx, move || job(&api), move |this, result, window, cx| {
            match result {
                Ok(_) => {
                    if let Some(text) = done {
                        crate::trace(&format!("arkadaşlar: {text}"));
                        crate::toasts::push(ToastTone::Ok, None, &text);
                    }
                }
                Err(e) => {
                    crate::trace(&format!("arkadaşlar: hata {}", e.code));
                    crate::toasts::push(ToastTone::Error, None, &error_text(&e));
                }
            }
            this.load_friends(window, cx);
            this.load_conversations(window, cx);
        });
    }

    /// "Arkadaş ekle" formu: kullanıcı adıyla istek.
    pub(super) fn friend_add_by_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.home.add.read(cx).value().trim().trim_start_matches('@').to_string();
        if name.is_empty() || self.home.busy {
            return;
        }
        self.home.busy = true;
        self.home.add_note = None;
        cx.notify();
        let api = self.api.clone();
        task::run(window, cx, move || api.friend_request_by_name(&name), |this, result, window, cx| {
            this.home.busy = false;
            match result {
                Ok(v) => {
                    let who = v["user"]["displayName"].as_str().unwrap_or_default().to_string();
                    let key = if v["status"] == "ACCEPTED" { "friends.add.accepted" } else { "friends.add.sent" };
                    crate::trace(&format!("arkadaşlar: istek {} → {who}", v["status"].as_str().unwrap_or_default()));
                    // Başarı bildirimi sağ üstte; alanın altındaki satır yalnız doğrulama hatası içindir.
                    crate::toasts::ok(tv(key, &[("name", &who)]));
                    this.home.add.update(cx, |input, cx| input.set_value("", window, cx));
                }
                Err(e) => {
                    crate::trace(&format!("arkadaşlar: istek hatası {}", e.code));
                    this.home.add_note = Some((error_text(&e), false));
                }
            }
            this.load_friends(window, cx);
            cx.notify();
        });
    }

    pub(super) fn friend_add(&mut self, user_id: String, name: String, window: &mut Window, cx: &mut Context<Self>) {
        self.friend_run(move |api| api.friend_request(&user_id), Some(tv("friends.add.sent", &[("name", &name)])), window, cx);
    }

    pub(super) fn friend_accept(&mut self, request_id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.friend_run(move |api| api.accept_friend(&request_id), None, window, cx);
    }

    pub(super) fn friend_decline(&mut self, request_id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.friend_run(move |api| api.decline_friend(&request_id), None, window, cx);
    }

    pub(super) fn friend_cancel(&mut self, request_id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.friend_run(move |api| api.cancel_friend_request(&request_id), None, window, cx);
    }

    pub(super) fn friend_remove(&mut self, user_id: String, name: String, window: &mut Window, cx: &mut Context<Self>) {
        self.friend_run(move |api| api.remove_friend(&user_id), Some(tv("friends.note.removed", &[("name", &name)])), window, cx);
    }

    pub(super) fn friend_block(&mut self, user_id: String, name: String, window: &mut Window, cx: &mut Context<Self>) {
        self.friend_run(move |api| api.block_user(&user_id), Some(tv("friends.note.blocked", &[("name", &name)])), window, cx);
    }

    pub(super) fn friend_unblock(&mut self, user_id: String, name: String, window: &mut Window, cx: &mut Context<Self>) {
        self.friend_run(move |api| api.unblock_user(&user_id), Some(tv("friends.note.unblocked", &[("name", &name)])), window, cx);
    }

    /// DM açılamadığında (engel / arkadaş değil + ortak proje yok) gösterilecek metin.
    pub(super) fn dm_refusal_text(code: &str) -> Option<String> {
        matches!(code, "USER_BLOCKED" | "NO_SHARED_PROJECT").then(|| t(p2_client_core::friend_error_key(code).unwrap_or("friends.err.generic")))
    }

    // ------------------------------------------------------------ ray

    /// Okunmamış Ana sayfa sohbetleri, en yeni önce (engellediklerim ve listeden kaldırdıklarım hariç;
    /// proje sohbetlerinin okunmamışı o projenin ray rozetindedir).
    fn unread_dms(&self) -> Vec<&Conversation> {
        let mut list: Vec<&Conversation> = self.conversations.iter().filter(|c| c.is_home() && !c.hidden && c.unread > 0 && !c.blocked_by_me && !self.home.data.is_blocked(&c.user.id)).collect();
        list.sort_by(|a, b| dm_stamp(b).cmp(dm_stamp(a)));
        list
    }

    /// Rayın en üstü: Ana sayfa düğmesi, okunmamış DM baloncukları ve ayırıcı.
    pub(super) fn rail_home(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let tools_open = self.board.is_some() || self.documents.is_some() || self.library.is_some();
        let active = self.home.open && !tools_open;
        let unread = self.unread_dms();
        let pending = self.home.data.incoming.len() as u32;
        let open_group = match self.current_scope(cx) {
            Some(Scope::Group(id)) => Some(id),
            _ => None,
        };
        let group_unread: u32 = self.home.groups.iter().filter(|g| Some(&g.id) != open_group.as_ref()).map(|g| g.unread).sum();
        let total: u32 = unread.iter().map(|c| c.unread).sum::<u32>() + group_unread + pending;
        let (border_strong, text_2, strong) = (p.border_strong, p.text_2, p.text_strong);
        let rail_badge = |n: u32| {
            div().absolute().right(px(9.)).top(px(1.)).p(px(2.)).rounded(px(8.5)).bg(p.bg_1).child(
                div()
                    .min_w(px(13.))
                    .h(px(13.))
                    .px(px(4.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.5))
                    .bg(p.danger)
                    .text_color(gpui_kit::white())
                    .text_size(fs(9.))
                    .css_weight(750.)
                    .line_height(fs(9.))
                    .tracking(0.01)
                    .child(if n > 99 { "99+".to_string() } else { n.to_string() }),
            )
        };
        let glow = p.accent_glow_strong;
        let home = div()
            .id("rail-home")
            .group("rail-home")
            .relative()
            .flex_none()
            .w(px(72.))
            .h(px(52.))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .child(if active {
                div().absolute().left_0().top(px(8.)).w(px(3.)).h(px(36.)).rounded_r(px(3.)).bg(p.accent_hover).shadow(vec![gpui_kit::BoxShadow { color: p.accent_soft_2, offset: point(px(0.), px(0.)), blur_radius: px(10.), spread_radius: px(0.), inset: false }])
            } else {
                div().absolute().left_0().top(px(16.1)).w(px(3.)).h(px(19.8)).rounded_r(px(3.)).bg(text_2).invisible().group_hover("rail-home", |s| s.visible())
            })
            .child(
                div()
                    .relative()
                    .size(px(42.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(if active { 11. } else { 14. }))
                    .border_1()
                    .map(|s| {
                        if active {
                            s.border_color(p.border_accent).bg(p.accent_grad.background()).shadow(vec![gpui_kit::BoxShadow { color: glow.color, offset: point(px(glow.x), px(glow.y)), blur_radius: px(glow.blur), spread_radius: px(glow.spread), inset: glow.inset }])
                        } else {
                            let (soft, sm) = (p.accent_soft_2, p.shadow_sm());
                            s.border_color(p.border_subtle).bg(p.bg_3).group_hover("rail-home", move |s| s.rounded(px(11.)).border_color(border_strong).bg(soft).top(px(-1.)).shadow(sm))
                        }
                    })
                    .child(Icon::raw(ICON_HOME_CHAT).stroke(1.7).el().size(px(21.)).text_color(if active { p.text_on_accent } else { p.text_2 }).when(!active, |s| s.group_hover("rail-home", move |s| s.text_color(strong)))),
            )
            .when(total > 0, |s| s.child(rail_badge(total)))
            .tooltip(tip_owned(t("friends.rail.home")))
            .on_click(cx.listener(|this, _, window, cx| this.open_friends(None, window, cx)));

        let mut column = div().flex_none().flex().flex_col().items_center().child(home);
        let open_dm = self.open_dm_user(cx);
        for conversation in unread.into_iter().take(4) {
            let user = conversation.user.clone();
            let group: SharedString = format!("rail-dm-{}", user.id).into();
            let person = user.clone();
            let selected = open_dm.as_deref() == Some(user.id.as_str());
            column = column.child(
                div()
                    .id(SharedString::from(format!("rail-dm-{}", user.id)))
                    .group(group.clone())
                    .relative()
                    .flex_none()
                    .w(px(72.))
                    .h(px(48.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .child(div().absolute().left_0().top(px(19.)).w(px(3.)).h(px(10.)).rounded_r(px(3.)).bg(p.text_strong))
                    .child(
                        avatar(&user.id, &user.display_name, 40., 13., Some((&user.status, p.bg_1)), p)
                            .border_1()
                            .border_color(if selected { p.border_accent } else { p.border_subtle })
                            .group_hover(group, move |s| s.border_color(border_strong).top(px(-1.))),
                    )
                    .child(rail_badge(conversation.unread))
                    .tooltip(tip_owned(format!("{} · @{}", user.display_name, user.username)))
                    .on_click(cx.listener(move |this, _, window, cx| this.open_home_direct(person.clone(), window, cx)))
                    .p2_enter(SharedString::from(format!("rail-dm-in-{}", user.id)), p2_ui::Entrance::ScaleIn, std::time::Duration::ZERO),
            );
        }
        column.child(div().flex_none().w(px(32.)).h(px(1.)).mt(px(5.)).mb(px(7.)).bg(p.border_subtle)).into_any_element()
    }

    /// Açık birebir sohbetin karşı tarafı.
    fn open_dm_user(&self, cx: &gpui_kit::App) -> Option<String> {
        self.chat.as_ref().and_then(|c| match &c.read(cx).subject {
            ChatSubject::Direct(m) => Some(m.id.clone()),
            _ => None,
        })
    }

    // ------------------------------------------------------------ kenar çubuğu

    pub(super) fn home_sidebar(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let me = self.auth.user.id.clone();
        let query = lower(self.home.search.read(cx).value().trim());
        let search_empty = self.home.search.read(cx).value().is_empty();
        let current = self.current_scope(cx);
        let open_dm = self.open_dm_user(cx);
        let friends_selected = self.chat.is_none();
        let pending = self.home.data.incoming.len();

        // Ana sayfa listesi: yalnız Ana sayfa sohbetleri (proje sohbetleri projenin kenar çubuğunda) ve
        // Ana sayfa grupları; listeden kaldırılanlar ve engellediklerim görünmez.
        enum Item<'a> {
            Dm(&'a Conversation),
            Group(&'a ChatGroup),
            /// Yıldızlı ama (görünür) sohbeti olmayan kişi.
            Person(Member),
        }
        let matches = |name: &str, username: &str| query.is_empty() || lower(name).contains(&query) || lower(username).contains(&query);
        let mut starred: Vec<(String, Item)> = Vec::new();
        let mut items: Vec<(String, Item)> = Vec::new();
        for c in self.conversations.iter().filter(|c| c.is_home() && !c.hidden) {
            if c.blocked_by_me || self.home.data.is_blocked(&c.user.id) || !matches(&c.user.display_name, &c.user.username) {
                continue;
            }
            let entry = (dm_stamp(c).to_string(), Item::Dm(c));
            if self.person_starred(&c.user.id) { starred.push(entry) } else { items.push(entry) }
        }
        for g in &self.home.groups {
            if !query.is_empty() && !lower(&g.name).contains(&query) {
                continue;
            }
            items.push((g.last_message.as_ref().map(|m| m.created_at.clone()).unwrap_or_default(), Item::Group(g)));
        }
        // Yıldızlı kişiler (Ana sayfa kapsamı): sohbeti listede olmayanlar da "Yıldızlı" bölümünde durur.
        for f in &self.home.data.friends {
            if self.person_starred(&f.id) && matches(&f.display_name, &f.username) && !starred.iter().any(|(_, i)| matches!(i, Item::Dm(c) if c.user.id == f.id)) {
                starred.push((String::new(), Item::Person(f.member())));
            }
        }
        starred.sort_by(|a, b| b.0.cmp(&a.0));
        items.sort_by(|a, b| b.0.cmp(&a.0));

        let (hover_bg, strong) = (p.bg_hover, p.text_strong);
        let mut list = div().id("home-dm-scroll").flex_1().min_h_0().overflow_y_scroll().px(px(8.)).pt(px(2.)).pb(px(14.)).flex().flex_col();

        // "Arkadaşlar" girişi.
        list = list.child(
            div()
                .id("home-friends-row")
                .group("home-friends-row")
                .relative()
                .w_full()
                .h(px(38.))
                .px(px(10.))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(10.))
                .rounded(px(9.))
                .cursor_pointer()
                .when(friends_selected, |s| s.bg(p.bg_selected))
                .hover(move |s| s.bg(hover_bg))
                .child(icon("people").stroke(1.8).el().size(px(18.)).text_color(if friends_selected { p.text_accent } else { p.text_3 }))
                .child(div().flex_1().min_w_0().truncate().text_size(fs(13.)).css_weight(650.).text_color(if friends_selected { p.text_strong } else { p.text_2 }).group_hover("home-friends-row", move |s| s.text_color(strong)).child(t("friends.title")))
                .when(pending > 0, |s| s.child(controls::badge(pending.to_string(), BadgeTone::Danger, p)))
                .on_click(cx.listener(|this, _, window, cx| this.open_friends(None, window, cx))),
        );

        let section_label = |text: String| div().text_size(fs(9.5)).css_weight(800.).tracking(0.14).text_color(p.text_3).child(text);
        let render = |item: &Item, star: bool, this: &Self, cx: &mut Context<Self>| -> AnyElement {
            match item {
                Item::Dm(c) => {
                    let selected = open_dm.as_deref() == Some(c.user.id.as_str());
                    this.home_person_row(&c.user, Some(c), selected, star, &me, p, cx)
                }
                Item::Person(m) => this.home_person_row(m, None, open_dm.as_deref() == Some(m.id.as_str()), star, &me, p, cx),
                Item::Group(g) => this.home_group_row(g, current == Some(Scope::Group(g.id.clone())), &me, p, cx),
            }
        };

        // "Yıldızlı" bölümü (proje kenar çubuğundaki gibi en üstte).
        if !starred.is_empty() {
            list = list.child(
                div().flex_none().mt(px(14.)).mb(px(4.)).pl(px(10.)).pr(px(4.)).h(px(24.)).flex().items_center().gap(px(6.)).child(icon("star").filled().el().size(px(11.)).text_color(p.warning)).child(section_label(t("dm.starred"))),
            );
            for (_, item) in &starred {
                list = list.child(render(item, true, self, cx));
            }
        }

        // Bölüm başlığı + "yeni mesaj".
        let (soft, border) = (p.accent_soft, p.border);
        list = list.child(
            div()
                .flex_none()
                .mt(px(14.))
                .mb(px(4.))
                .pl(px(10.))
                .pr(px(4.))
                .h(px(24.))
                .flex()
                .items_center()
                .justify_between()
                .child(section_label(t("friends.dms")))
                .child(
                    div()
                        .id("home-new-message")
                        .group("home-new-message")
                        .size(px(24.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(gpui_kit::transparent_black())
                        .cursor_pointer()
                        .hover(move |s| s.bg(soft).border_color(border))
                        .child(icon("plus").stroke(2.).el().size(px(14.)).text_color(p.text_3).group_hover("home-new-message", move |s| s.text_color(strong)))
                        .tooltip(tip_owned(t("dm.new.title")))
                        .on_click(cx.listener(|this, _, window, cx| this.open_new_message(window, cx))),
                ),
        );

        let empty = items.is_empty();
        for (_, item) in &items {
            list = list.child(render(item, false, self, cx));
        }
        if empty {
            list = list.child(div().mx(px(10.)).mt(px(6.)).text_size(fs(10.5)).line_height(gpui_kit::relative(1.5)).text_color(p.text_4).child(t(if query.is_empty() { "friends.empty.dms" } else { "friends.empty.search" })));
        }

        // Başlık: proje kenar çubuğuyla aynı kapak (hizalar ve görünüm tutarlı kalsın).
        let side_w = crate::resize::value(&crate::resize::SIDEBAR);
        let cover_w = (side_w - 1.).ceil();
        let header = div()
            .relative()
            .flex_none()
            .h(px(150.))
            .pt(px(12.))
            .pr(px(12.))
            .pl(px(16.))
            .flex()
            .items_start()
            .overflow_hidden()
            .border_b_1()
            .border_color(p.border_subtle)
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top(px(49.))
                    .w(px(cover_w))
                    .h(px(100.))
                    .overflow_hidden()
                    .child(svg_image(workspace_nav::cover_svg(p, workspace_nav::COVER_BASE_W)).object_fit(gpui_kit::ObjectFit::Fill).absolute().left_0().top_0().w(px(cover_w)).h(px(100.)))
                    .child(svg_image(workspace_nav::cover_ellipse_svg(p)).absolute().left(px(cover_w - 56. - 90.)).top(px(91. - 60.)).w(px(180.)).h(px(120.))),
            )
            .child(
                div()
                    .relative()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(div().truncate().text_size(fs(15.)).css_weight(700.).text_color(p.text_strong).child(t("friends.home")))
                    .child(div().mt(px(4.)).text_size(fs(9.5)).css_weight(800.).tracking(0.18).text_color(p.text_accent).child(t("friends.homeKicker"))),
            );
        let search = div().flex_none().h(px(44.)).px(px(8.)).flex().items_center().child(
            div()
                .relative()
                .w_full()
                .h(px(32.))
                .flex()
                .items_center()
                .rounded(px(8.))
                .border_1()
                .border_color(p.border)
                .bg(p.bg_3)
                .pl(px(29.))
                .pr(px(9.))
                .child(div().absolute().left(px(9.)).top(px(6.5)).text_size(fs(16.)).text_color(p.text_4).child("⌕"))
                .when(search_empty, |s| s.child(div().absolute().left(px(29.)).top_0().bottom_0().flex().items_center().text_size(fs(13.)).text_color(p.text_4).child(t("friends.search"))))
                .child(Input::new(&self.home.search).appearance(false).w_full().h(px(30.)).px_0().py_0().text_size(fs(13.))),
        );
        div()
            .relative()
            .w(px(side_w))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .bg(p.bg_2)
            .border_r_1()
            .border_color(p.border_subtle)
            .child(header)
            .child(search)
            .child(list)
            .child(crate::resize::handle("home-sidebar-resize", crate::resize::SIDEBAR, crate::resize::Edge::Right, "Kenar çubuğu genişliği", p))
            .p2_enter("home-sidebar", p2_ui::Entrance::SlideLeft, std::time::Duration::ZERO)
            .into_any_element()
    }

    /// Ana sayfa listesindeki kişi satırı: sohbet (`conversation`) ya da yıldızlı ama sohbeti olmayan kişi.
    /// Sağ tık: kişi menüsü (+ sohbet satırında "Listeden kaldır").
    #[allow(clippy::too_many_arguments)]
    fn home_person_row(&self, user: &Member, conversation: Option<&Conversation>, selected: bool, star: bool, me: &str, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let (hover_bg, strong) = (p.bg_hover, p.text_strong);
        let unread = if selected { 0 } else { conversation.map_or(0, |c| c.unread) };
        let key = format!("home-dm-{}-{}", if star { "s" } else { "d" }, user.id);
        let group: SharedString = key.clone().into();
        let (person, menu_person) = (user.clone(), user.clone());
        let dm = conversation.map(|c| c.id.clone());
        let sub = match conversation.and_then(|c| c.last_message.as_ref()) {
            Some(m) => preview(&m.content, m.author_id == me),
            None => format!("@{}", user.username),
        };
        let name_color = if selected || unread > 0 { p.text_strong } else { p.text_2 };
        div()
            .id(SharedString::from(key))
            .group(group.clone())
            .relative()
            .w_full()
            .h(px(46.))
            .px(px(8.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(10.))
            .rounded(px(9.))
            .cursor_pointer()
            .when(selected, |s| s.bg(p.bg_selected))
            .hover(move |s| s.bg(hover_bg))
            .child(avatar(&user.id, &user.display_name, 32., 11., Some((&user.status, p.bg_2)), p))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(div().truncate().text_size(fs(12.5)).line_height(fs(14.)).css_weight(if unread > 0 { 700. } else { 600. }).text_color(name_color).group_hover(group, move |s| s.text_color(strong)).child(user.display_name.clone()))
                    .child(div().truncate().text_size(fs(10.5)).line_height(fs(12.)).css_weight(500.).text_color(if unread > 0 { p.text_2 } else { p.text_4 }).child(sub)),
            )
            .when(unread > 0, |s| s.child(workspace_nav::unread_badge(unread, p)))
            .on_click(cx.listener(move |this, _, window, cx| this.open_home_direct(person.clone(), window, cx)))
            .on_mouse_down(MouseButton::Right, cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                this.nav_menu = Some(NavMenu::Person { person: menu_person.clone(), dm: dm.clone(), at: ev.position });
                cx.notify();
            }))
            .into_any_element()
    }

    /// Ana sayfa grubu satırı: üye avatarları (üst üste iki avatar), grup simgesi ve son mesaj.
    /// Sağ tık: sohbeti aç / üyeler ve ad / gruptan ayrıl.
    fn home_group_row(&self, g: &ChatGroup, selected: bool, me: &str, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let (hover_bg, strong) = (p.bg_hover, p.text_strong);
        let unread = if selected { 0 } else { g.unread };
        let group: SharedString = format!("home-group-{}", g.id).into();
        let (chat_group, menu_group) = (g.clone(), g.clone());
        let hue = p.hues[8];
        let sub = match &g.last_message {
            Some(m) => preview(&m.content, m.author_id == me),
            None => tv("friends.members", &[("n", &g.members.len().to_string())]),
        };
        // Üye avatarları: benden başka ilk iki üye; tek kişi kaldıysa grup simgeli kutu.
        let others: Vec<&Member> = g.members.iter().filter(|m| m.id != me).take(2).collect();
        let ring = if selected { p.bg_selected } else { p.bg_2 };
        let faces = if others.len() >= 2 {
            div()
                .relative()
                .flex_none()
                .size(px(32.))
                .child(avatar(&others[0].id, &others[0].display_name, 21., 8., None, p).absolute().left_0().top_0())
                .child(div().absolute().right(px(-1.)).bottom(px(-1.)).p(px(1.5)).rounded(px(9.5)).bg(ring).child(avatar(&others[1].id, &others[1].display_name, 21., 8., None, p)))
        } else {
            div()
                .flex_none()
                .size(px(32.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(11.))
                .bg(linear_gradient(145., linear_color_stop(hue.bg, 0.), linear_color_stop(hue.bg2, 1.)))
                .child(icon("people").stroke(1.8).el().size(px(16.)).text_color(hue.fg))
        };
        div()
            .id(SharedString::from(format!("home-group-{}", g.id)))
            .group(group.clone())
            .relative()
            .w_full()
            .h(px(46.))
            .px(px(8.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(10.))
            .rounded(px(9.))
            .cursor_pointer()
            .when(selected, |s| s.bg(p.bg_selected))
            .hover(move |s| s.bg(hover_bg))
            .child(faces)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(div().truncate().text_size(fs(12.5)).line_height(fs(14.)).css_weight(if unread > 0 { 700. } else { 600. }).text_color(if selected || unread > 0 { p.text_strong } else { p.text_2 }).group_hover(group, move |s| s.text_color(strong)).child(g.name.clone()))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .min_w_0()
                            .child(icon("people").stroke(1.9).el().flex_none().size(px(11.)).text_color(p.text_4))
                            .child(div().flex_1().min_w_0().truncate().text_size(fs(10.5)).line_height(fs(12.)).css_weight(500.).text_color(if unread > 0 { p.text_2 } else { p.text_4 }).child(sub)),
                    ),
            )
            .when(unread > 0, |s| s.child(workspace_nav::unread_badge(unread, p)))
            .tooltip(tip_owned(g.members.iter().map(|m| m.display_name.clone()).collect::<Vec<_>>().join(", ")))
            .on_click(cx.listener(move |this, _, window, cx| this.select_group(chat_group.clone(), window, cx)))
            .on_mouse_down(MouseButton::Right, cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                this.nav_menu = Some(NavMenu::Group { group: menu_group.clone(), at: ev.position });
                cx.notify();
            }))
            .into_any_element()
    }

    // ------------------------------------------------------------ arkadaşlar sayfası

    pub(super) fn friends_page(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let home = &self.home;
        let data = &home.data;
        let tab = home.tab;
        let tab_button = |id: &'static str, key: &str, which: Tab, count: usize| {
            controls::tab(id, t(key), tab == which, TabsStyle::Plain, home.tab_epoch, p).when(count > 0, |s| s.child(controls::badge(count.to_string(), BadgeTone::Danger, p)))
        };
        let switch = |which: Tab| {
            cx.listener(move |this: &mut Self, _: &gpui_kit::ClickEvent, _, cx| {
                this.home.tab = which;
                this.home.tab_epoch += 1;
                this.home.add_note = None;
                crate::trace(&format!("ana sayfa: sekme {which:?}"));
                cx.notify();
            })
        };
        let add_active = tab == Tab::Add;
        let bar = div()
            .flex_none()
            .h(px(54.))
            .px(px(20.))
            .flex()
            .items_center()
            .gap(px(14.))
            .border_b_1()
            .border_color(p.border_subtle)
            .bg(p.bg_1)
            .child(icon("people").stroke(1.8).el().size(px(19.)).text_color(p.text_accent))
            .child(div().flex_none().text_size(fs(15.)).css_weight(700.).text_color(p.text_strong).child(t("friends.title")))
            .child(div().flex_none().w(px(1.)).h(px(20.)).bg(p.border))
            .child(
                controls::tabs(TabsStyle::Plain, p)
                    .child(tab_button("friends-tab-online", "friends.tab.online", Tab::Online, 0).on_click(switch(Tab::Online)))
                    .child(tab_button("friends-tab-all", "friends.tab.all", Tab::All, 0).on_click(switch(Tab::All)))
                    .child(tab_button("friends-tab-pending", "friends.tab.pending", Tab::Pending, data.incoming.len()).on_click(switch(Tab::Pending)))
                    .child(tab_button("friends-tab-blocked", "friends.tab.blocked", Tab::Blocked, 0).on_click(switch(Tab::Blocked))),
            )
            .child(div().flex_1())
            .child(
                controls::button("friends-add", if add_active { BtnVariant::Secondary } else { BtnVariant::Primary }, BtnSize::Sm, false, false, p)
                    .child(Icon::raw(ICON_USER_PLUS).stroke(1.9).el().size(px(15.)).text_color(if add_active { p.text } else { p.text_on_accent }))
                    .child(t("friends.add"))
                    .on_click(switch(Tab::Add)),
            );

        let section = |label: String| div().flex_none().h(px(34.)).px(px(12.)).flex().items_center().child(controls::kicker(upper(label), p));
        let mut body = div().w_full().max_w(px(880.)).flex().flex_col();
        let mut rows = 0usize;
        match tab {
            Tab::Online | Tab::All => {
                let list: Vec<&p2_client_core::Friend> = data.friends.iter().filter(|f| tab == Tab::All || !matches!(f.status.as_str(), "" | "OFFLINE" | "INVISIBLE")).collect();
                if !list.is_empty() {
                    body = body.child(section(tv(if tab == Tab::All { "friends.count.all" } else { "friends.count.online" }, &[("n", &list.len().to_string())])));
                }
                for f in list {
                    body = body.child(self.friend_row(rows, &f.member(), RowKind::Friend, p, cx));
                    rows += 1;
                }
            }
            Tab::Pending => {
                if !data.incoming.is_empty() {
                    body = body.child(section(tv("friends.count.incoming", &[("n", &data.incoming.len().to_string())])));
                }
                for r in &data.incoming {
                    body = body.child(self.friend_row(rows, &r.user.member(), RowKind::Incoming(r.id.clone()), p, cx));
                    rows += 1;
                }
                if !data.outgoing.is_empty() {
                    body = body.child(section(tv("friends.count.outgoing", &[("n", &data.outgoing.len().to_string())])).mt(px(if data.incoming.is_empty() { 0. } else { 10. })));
                }
                for r in &data.outgoing {
                    body = body.child(self.friend_row(rows, &r.user.member(), RowKind::Outgoing(r.id.clone()), p, cx));
                    rows += 1;
                }
            }
            Tab::Blocked => {
                if !data.blocked.is_empty() {
                    body = body.child(section(tv("friends.count.blocked", &[("n", &data.blocked.len().to_string())])));
                }
                for r in &data.blocked {
                    body = body.child(self.friend_row(rows, &r.user.member(), RowKind::Blocked, p, cx));
                    rows += 1;
                }
            }
            Tab::Add => {
                body = body.child(self.add_friend_card(p, cx));
                if !data.outgoing.is_empty() {
                    body = body.child(section(tv("friends.count.outgoing", &[("n", &data.outgoing.len().to_string())])).mt(px(18.)));
                }
                for r in &data.outgoing {
                    body = body.child(self.friend_row(rows, &r.user.member(), RowKind::Outgoing(r.id.clone()), p, cx));
                    rows += 1;
                }
                rows += 1;
            }
        }
        if rows == 0 {
            let (title, detail) = match tab {
                Tab::Online => ("friends.empty.online", "friends.empty.online.hint"),
                Tab::All => ("friends.empty.all", "friends.empty.all.hint"),
                Tab::Pending => ("friends.empty.pending", "friends.empty.pending.hint"),
                _ => ("friends.empty.blocked", "friends.empty.blocked.hint"),
            };
            body = body.child(
                div()
                    .mt(px(56.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(
                        div()
                            .size(px(64.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(20.))
                            .border_1()
                            .border_color(p.border)
                            .bg(p.accent_soft)
                            .child(if tab == Tab::Blocked { Icon::raw(ICON_BLOCK) } else { icon("people") }.stroke(1.6).el().size(px(28.)).text_color(p.text_accent)),
                    )
                    .child(controls::empty(t(title), Some(t(detail).into()), p).py(px(16.))),
            );
        }
        div()
            .id("friends-page")
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(p.bg_0)
            .child(bar)
            .child(
                div()
                    .id("friends-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(20.))
                    .pt(px(12.))
                    .pb(px(24.))
                    .flex()
                    .flex_col()
                    .child(body.p2_enter(("friends-body", home.tab_epoch), p2_ui::Entrance::Rise, std::time::Duration::ZERO)),
            )
            .into_any_element()
    }

    fn add_friend_card(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let home = &self.home;
        let empty = home.add.read(cx).value().is_empty();
        let ready = !empty && !home.busy;
        div()
            .mt(px(8.))
            .p(px(20.))
            .flex()
            .flex_col()
            .rounded(px(14.))
            .border_1()
            .border_color(p.border)
            .bg(p.glass.background())
            .shadow(p.shadow_sm())
            .child(controls::kicker(upper(t("friends.add.kicker")), p))
            .child(div().mt(px(8.)).text_size(fs(18.)).css_weight(750.).tracking(-0.01).text_color(p.text_strong).child(t("friends.add.title")))
            .child(div().mt(px(6.)).text_size(fs(12.)).line_height(gpui_kit::relative(1.5)).text_color(p.text_3).child(t("friends.add.hint")))
            .child(
                div()
                    .mt(px(16.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .child(
                        div()
                            .relative()
                            .flex_1()
                            .min_w_0()
                            .h(px(40.))
                            .flex()
                            .items_center()
                            .rounded(px(10.))
                            .border_1()
                            .border_color(if home.add_note.as_ref().is_some_and(|n| !n.1) { p.danger_border } else { p.border })
                            .bg(p.bg_3)
                            .pl(px(32.))
                            .pr(px(10.))
                            .child(div().absolute().left(px(13.)).top_0().bottom_0().flex().items_center().text_size(fs(14.)).css_weight(700.).text_color(p.text_accent).child("@"))
                            .when(empty, |s| s.child(div().absolute().left(px(32.)).top_0().bottom_0().flex().items_center().text_size(fs(13.)).text_color(p.text_4).child(t("friends.add.placeholder"))))
                            .child(Input::new(&home.add).appearance(false).w_full().h(px(36.)).px_0().py_0().text_size(fs(13.))),
                    )
                    .child(
                        controls::button("friends-add-send", BtnVariant::Primary, BtnSize::Lg, false, false, p)
                            .when(!ready, |s| s.opacity(0.55))
                            .child(t("friends.add.send"))
                            .on_click(cx.listener(|this, _, window, cx| this.friend_add_by_name(window, cx))),
                    ),
            )
            .when_some(home.add_note.clone(), |s, (text, ok)| {
                s.child(
                    div()
                        .mt(px(12.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .text_size(fs(12.))
                        .css_weight(600.)
                        .text_color(if ok { p.success_text } else { p.danger_text })
                        .child(if ok { Icon::raw(ICON_CHECK) } else { icon("close") }.stroke(2.2).el().size(px(14.)).text_color(if ok { p.success_text } else { p.danger_text }))
                        .child(text),
                )
            })
            .into_any_element()
    }

    fn friend_row(&self, ix: usize, member: &Member, kind: RowKind, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let hover_bg = p.bg_hover;
        let group: SharedString = format!("friend-row-{ix}").into();
        let id = member.id.clone();
        let name = member.display_name.clone();
        let (status, sub) = match &kind {
            RowKind::Friend => (Some(member.status.as_str()), status_text(&member.status)),
            RowKind::Incoming(_) => (None, t("friends.req.incoming")),
            RowKind::Outgoing(_) => (None, t("friends.req.outgoing")),
            RowKind::Blocked => (None, t("friends.blockedRow")),
        };
        let sub_color = match (&kind, member.status.as_str()) {
            (RowKind::Friend, "ONLINE") => p.success_text,
            (RowKind::Friend, "AWAY") => p.warning_text,
            (RowKind::Friend, "BUSY") => p.danger_text,
            _ => p.text_3,
        };
        let mut actions = div().flex_none().flex().items_center().gap(px(8.));
        match &kind {
            RowKind::Friend => {
                let (a, b, c) = (member.clone(), member.id.clone(), member.clone());
                actions = actions
                    .child(round_action(format!("fr-msg-{ix}").into(), icon("chat"), t("friends.act.message"), false, p).on_click(cx.listener(move |this, _, window, cx| this.open_direct(a.clone(), window, cx))))
                    .child(round_action(format!("fr-call-{ix}").into(), icon("call"), t("friends.act.call"), false, p).on_click(cx.listener(move |this, _, window, cx| {
                        let id = b.clone();
                        this.start_direct_call(&id, None, window, cx);
                    })))
                    .child(round_action(format!("fr-remove-{ix}").into(), icon("close"), t("friends.act.remove"), true, p).on_click(cx.listener({
                        let (id, name) = (id.clone(), name.clone());
                        move |this, _, window, cx| this.friend_remove(id.clone(), name.clone(), window, cx)
                    })))
                    .child(round_action(format!("fr-more-{ix}").into(), icon("more"), t("friends.act.more"), false, p).on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.nav_menu = Some(NavMenu::Person { person: c.clone(), dm: None, at: ev.position });
                        cx.notify();
                    })));
            }
            RowKind::Incoming(request) => {
                let (yes, no) = (request.clone(), request.clone());
                actions = actions
                    .child(
                        controls::button(SharedString::from(format!("fr-accept-{ix}")), BtnVariant::Primary, BtnSize::Sm, false, false, p)
                            .child(Icon::raw(ICON_CHECK).stroke(2.4).el().size(px(13.)).text_color(p.text_on_accent))
                            .child(t("friends.act.accept"))
                            .on_click(cx.listener(move |this, _, window, cx| this.friend_accept(yes.clone(), window, cx))),
                    )
                    .child(round_action(format!("fr-decline-{ix}").into(), icon("close"), t("friends.act.decline"), true, p).on_click(cx.listener(move |this, _, window, cx| this.friend_decline(no.clone(), window, cx))));
            }
            RowKind::Outgoing(request) => {
                let request = request.clone();
                actions = actions.child(
                    controls::button(SharedString::from(format!("fr-cancel-{ix}")), BtnVariant::Secondary, BtnSize::Sm, false, false, p)
                        .child(t("friends.act.cancel"))
                        .on_click(cx.listener(move |this, _, window, cx| this.friend_cancel(request.clone(), window, cx))),
                );
            }
            RowKind::Blocked => {
                let (id, name) = (id.clone(), name.clone());
                actions = actions.child(
                    controls::button(SharedString::from(format!("fr-unblock-{ix}")), BtnVariant::Secondary, BtnSize::Sm, false, false, p)
                        .child(t("friends.act.unblock"))
                        .on_click(cx.listener(move |this, _, window, cx| this.friend_unblock(id.clone(), name.clone(), window, cx))),
                );
            }
        }
        let (person, menu_person) = (member.clone(), member.clone());
        let is_friend = kind == RowKind::Friend;
        div()
            .flex_none()
            .w_full()
            .flex()
            .flex_col()
            .child(div().mx(px(12.)).h(px(1.)).bg(p.border_subtle))
            .child(
                div()
                    .id(("friend-row", ix))
                    .group(group.clone())
                    .w_full()
                    .h(px(60.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .rounded(px(10.))
                    .hover(move |s| s.bg(hover_bg))
                    .when(is_friend, |s| s.cursor_pointer().on_click(cx.listener(move |this, _, window, cx| this.open_direct(person.clone(), window, cx))))
                    .on_mouse_down(MouseButton::Right, cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                        this.nav_menu = Some(NavMenu::Person { person: menu_person.clone(), dm: None, at: ev.position });
                        cx.notify();
                    }))
                    .child(avatar(&member.id, &member.display_name, 38., 13., status.map(|s| (s, p.bg_0)), p).when(kind == RowKind::Blocked, |s| s.opacity(0.55)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(7.))
                                    .min_w_0()
                                    .child(div().truncate().text_size(fs(13.5)).line_height(fs(15.)).css_weight(700.).text_color(p.text_strong).child(member.display_name.clone()))
                                    .child(div().flex_none().text_size(fs(11.)).line_height(fs(15.)).css_weight(500.).text_color(p.text_4).child(format!("@{}", member.username))),
                            )
                            .child(div().truncate().text_size(fs(11.)).line_height(fs(12.5)).css_weight(600.).text_color(sub_color).child(sub)),
                    )
                    .child(actions),
            )
            .into_any_element()
    }

    // ------------------------------------------------------------ kişi menüsü ve profil kartı

    /// Kişi menüsüne (sağ tık / "diğer") arkadaşlık satırları: duruma göre ekle / iptal / kabul /
    /// çıkar ve engelle / engeli kaldır.
    pub(super) fn friend_menu_items(&self, person: &Member, p: &Palette, cx: &mut Context<Self>) -> Vec<AnyElement> {
        if person.id == self.auth.user.id {
            return Vec::new();
        }
        let (id, name) = (person.id.clone(), person.display_name.clone());
        let mut items: Vec<AnyElement> = Vec::new();
        let relation = self.home.data.relation(&person.id);
        match &relation {
            Relation::None => {
                let (id, name) = (id.clone(), name.clone());
                items.push(menu_entry("nm-friend-add", t("friends.add"), false, p).on_click(cx.listener(move |this, _, window, cx| {
                    this.nav_menu = None;
                    this.friend_add(id.clone(), name.clone(), window, cx);
                })).into_any_element());
            }
            Relation::Outgoing(request) => {
                let request = request.clone();
                items.push(menu_entry("nm-friend-cancel", t("friends.act.cancel"), false, p).on_click(cx.listener(move |this, _, window, cx| {
                    this.nav_menu = None;
                    this.friend_cancel(request.clone(), window, cx);
                })).into_any_element());
            }
            Relation::Incoming(request) => {
                let (yes, no) = (request.clone(), request.clone());
                items.push(menu_entry("nm-friend-accept", t("friends.act.acceptRequest"), false, p).on_click(cx.listener(move |this, _, window, cx| {
                    this.nav_menu = None;
                    this.friend_accept(yes.clone(), window, cx);
                })).into_any_element());
                items.push(menu_entry("nm-friend-decline", t("friends.act.declineRequest"), false, p).on_click(cx.listener(move |this, _, window, cx| {
                    this.nav_menu = None;
                    this.friend_decline(no.clone(), window, cx);
                })).into_any_element());
            }
            Relation::Friend => {
                let (id, name) = (id.clone(), name.clone());
                items.push(menu_entry("nm-friend-remove", t("friends.act.remove"), true, p).on_click(cx.listener(move |this, _, window, cx| {
                    this.nav_menu = None;
                    this.friend_remove(id.clone(), name.clone(), window, cx);
                })).into_any_element());
            }
            Relation::Blocked => {}
        }
        if relation == Relation::Blocked {
            items.push(menu_entry("nm-friend-unblock", t("friends.act.unblock"), false, p).on_click(cx.listener(move |this, _, window, cx| {
                this.nav_menu = None;
                this.friend_unblock(id.clone(), name.clone(), window, cx);
            })).into_any_element());
        } else {
            items.push(menu_entry("nm-friend-block", t("friends.act.block"), true, p).on_click(cx.listener(move |this, _, window, cx| {
                this.nav_menu = None;
                this.friend_block(id.clone(), name.clone(), window, cx);
            })).into_any_element());
        }
        items
    }

    /// Profil kartındaki arkadaşlık düğmeleri (kendi profilimde yok).
    pub(super) fn friend_actions(&self, member: &Member, show_message: bool, p: &Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        if member.id == self.auth.user.id {
            return None;
        }
        let (id, name) = (member.id.clone(), member.display_name.clone());
        let relation = self.home.data.relation(&member.id);
        let mut row = div().mt(px(8.)).mb(px(4.)).flex().flex_wrap().gap(px(6.));
        if show_message && relation != Relation::Blocked {
            let person = member.clone();
            row = row.child(
                controls::button("pf-message", BtnVariant::Primary, BtnSize::Sm, false, false, p)
                    .child(icon("chat").stroke(1.9).el().size(px(13.)).text_color(p.text_on_accent))
                    .child(t("friends.act.message"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.profile = None;
                        this.open_direct(person.clone(), window, cx);
                    })),
            );
        }
        match &relation {
            Relation::None => {
                let (id, name) = (id.clone(), name.clone());
                row = row.child(
                    controls::button("pf-friend-add", if show_message { BtnVariant::Secondary } else { BtnVariant::Primary }, BtnSize::Sm, false, false, p)
                        .child(Icon::raw(ICON_USER_PLUS).stroke(1.9).el().size(px(14.)).text_color(if show_message { p.text } else { p.text_on_accent }))
                        .child(t("friends.add"))
                        .on_click(cx.listener(move |this, _, window, cx| this.friend_add(id.clone(), name.clone(), window, cx))),
                );
            }
            Relation::Outgoing(request) => {
                let request = request.clone();
                row = row.child(controls::button("pf-friend-cancel", BtnVariant::Secondary, BtnSize::Sm, false, false, p).child(t("friends.act.cancel")).on_click(cx.listener(move |this, _, window, cx| this.friend_cancel(request.clone(), window, cx))));
            }
            Relation::Incoming(request) => {
                let (yes, no) = (request.clone(), request.clone());
                row = row
                    .child(controls::button("pf-friend-accept", BtnVariant::Primary, BtnSize::Sm, false, false, p).child(t("friends.act.acceptRequest")).on_click(cx.listener(move |this, _, window, cx| this.friend_accept(yes.clone(), window, cx))))
                    .child(controls::button("pf-friend-decline", BtnVariant::Secondary, BtnSize::Sm, false, false, p).child(t("friends.act.decline")).on_click(cx.listener(move |this, _, window, cx| this.friend_decline(no.clone(), window, cx))));
            }
            Relation::Friend => {
                let (id, name) = (id.clone(), name.clone());
                row = row.child(controls::button("pf-friend-remove", BtnVariant::Secondary, BtnSize::Sm, false, false, p).child(t("friends.act.remove")).on_click(cx.listener(move |this, _, window, cx| this.friend_remove(id.clone(), name.clone(), window, cx))));
            }
            Relation::Blocked => {}
        }
        row = if relation == Relation::Blocked {
            row.child(controls::button("pf-friend-unblock", BtnVariant::Secondary, BtnSize::Sm, false, false, p).child(t("friends.act.unblock")).on_click(cx.listener(move |this, _, window, cx| this.friend_unblock(id.clone(), name.clone(), window, cx))))
        } else {
            row.child(controls::button("pf-friend-block", BtnVariant::Danger, BtnSize::Sm, false, false, p).child(t("friends.act.block")).on_click(cx.listener(move |this, _, window, cx| this.friend_block(id.clone(), name.clone(), window, cx))))
        };
        Some(row.into_any_element())
    }

    // ------------------------------------------------------------ otomatik pilot

    /// Test/görüntü kancası (masaüstüne girdi göndermeden): `P2_AUTOPILOT_HOME` =
    /// `online|all|pending|blocked|add` (sekme), `dm:<kullanıcı>` (sohbet aç), `menu:<kullanıcı>` (kişi
    /// menüsü), `profile:<kullanıcı>` (profil kartı), `request:<kullanıcı>` (formdan istek gönder).
    /// Birden çok adım `;` ile ayrılır; her adım 1,2 sn (`P2_AUTOPILOT_HOME_DELAY` ms) arayla işlenir;
    /// bilinmeyen `x:y` adımı (ör. `wait:`) yalnız bekler.
    fn home_autopilot(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        static STARTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        let Ok(spec) = std::env::var("P2_AUTOPILOT_HOME") else { return };
        if STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        let steps: Vec<String> = spec.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        let delay = std::env::var("P2_AUTOPILOT_HOME_DELAY").ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(1200);
        cx.spawn_in(window, async move |this, cx| {
            for step in steps {
                cx.background_executor().timer(std::time::Duration::from_millis(delay)).await;
                if this.update_in(cx, |this, window, cx| this.home_autopilot_step(&step, window, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn home_autopilot_step(&mut self, step: &str, window: &mut Window, cx: &mut Context<Self>) {
        let person = |this: &Self, username: &str| -> Option<Member> {
            let d = &this.home.data;
            d.friends
                .iter()
                .chain(d.incoming.iter().map(|r| &r.user))
                .chain(d.outgoing.iter().map(|r| &r.user))
                .chain(d.blocked.iter().map(|r| &r.user))
                .find(|f| f.username == username)
                .map(|f| f.member())
                .or_else(|| this.conversations.iter().find(|c| c.user.username == username).map(|c| c.user.clone()))
                .or_else(|| this.members.iter().find(|m| m.username == username).cloned())
        };
        crate::trace(&format!("ana sayfa: otomatik pilot «{step}»"));
        // Önceki adımın açık bıraktığı menü kapanır (menü adımları yenisini açar).
        self.nav_menu = None;
        match step.split_once(':') {
            // "Yeni mesaj" penceresi: aç / onayla; arama: büyüt / kapat.
            None if step == "new" => self.open_new_message(window, cx),
            // Proje içinde: kenar çubuğundaki "＋" (aynı pencere, projeye bağlı).
            None if step == "pnew" => self.open_project_message(window, cx),
            None if step == "go" => {
                if let Some(dialog) = self.new_message.as_ref().map(|d| d.0.clone()) {
                    dialog.update(cx, |d, cx| d.autopilot(&[], "", true, window, cx));
                }
            }
            None if step == "expand" => self.call_ui_expand(true),
            None if step == "collapse" => self.call_ui_expand(false),
            None if step == "hangup" => self.calls.update(cx, |c, cx| c.leave(window, cx)),
            None if step == "members" => {
                let open = match self.current_scope(cx) {
                    Some(Scope::Group(id)) => self.group_by_id(&id).cloned(),
                    _ => None,
                };
                if let Some(g) = open {
                    self.open_group_members(g, window, cx);
                }
            }
            None => {
                let tab = match step {
                    "all" => Tab::All,
                    "pending" => Tab::Pending,
                    "blocked" => Tab::Blocked,
                    "add" => Tab::Add,
                    _ => Tab::Online,
                };
                self.open_friends(Some(tab), window, cx);
            }
            // Ana sayfa sohbeti (nerede olursam olayım Ana sayfaya geçer).
            Some(("dm", user)) => {
                if let Some(m) = person(self, user) {
                    self.open_home_direct(m, window, cx);
                }
            }
            // Bulunduğum kapsamın sohbeti: proje içindeysem o projenin sohbeti.
            Some(("pdm", user)) => {
                if let Some(m) = person(self, user) {
                    self.open_direct(m, window, cx);
                }
            }
            Some(("menu", user)) => {
                if let Some(m) = person(self, user) {
                    self.nav_menu = Some(NavMenu::Person { person: m, dm: None, at: point(px(560.), px(250.)) });
                }
            }
            // Sohbet satırının sağ tık menüsü ("Listeden kaldır" ile), bulunduğum kapsamda.
            Some(("dmmenu", user)) => {
                let scope = self.dm_scope();
                if let Some(c) = self.conversations.iter().find(|c| c.user.username == user && c.project_id == scope && !c.hidden) {
                    self.nav_menu = Some(NavMenu::Person { person: c.user.clone(), dm: Some(c.id.clone()), at: point(px(150.), px(330.)) });
                }
            }
            Some(("hide", user)) => {
                let scope = self.dm_scope();
                if let Some(id) = self.conversations.iter().find(|c| c.user.username == user && c.project_id == scope).map(|c| c.id.clone()) {
                    self.hide_conversation(id, window, cx);
                }
            }
            Some(("star", user)) => {
                if let Some(m) = person(self, user) {
                    self.toggle_star("PERSON", m.id, window, cx);
                }
            }
            Some(("call", user)) => {
                if let Some(m) = person(self, user) {
                    let scope = self.dm_scope();
                    self.start_direct_call(&m.id, scope, window, cx);
                }
            }
            // "Yeni mesaj" penceresinde seçim: `pick:kullanıcı1,kullanıcı2[|Grup adı]`.
            Some(("pick", spec)) => {
                let (users, name) = spec.split_once('|').unwrap_or((spec, ""));
                let users: Vec<&str> = users.split(',').map(str::trim).filter(|u| !u.is_empty()).collect();
                if let Some(dialog) = self.new_message.as_ref().map(|d| d.0.clone()) {
                    dialog.update(cx, |d, cx| d.autopilot(&users, name, false, window, cx));
                }
            }
            Some(("group", name)) => {
                if let Some(g) = self.home.groups.iter().find(|g| g.name == name).cloned() {
                    self.enter_home(window, cx);
                    self.select_group(g, window, cx);
                }
            }
            Some(("groupmenu", name)) => {
                if let Some(g) = self.home.groups.iter().find(|g| g.name == name).cloned() {
                    self.nav_menu = Some(NavMenu::Group { group: g, at: point(px(150.), px(330.)) });
                }
            }
            // Proje grupları (seçili projenin kenar çubuğu): aç / sağ tık menüsü.
            Some(("pgroup", name)) => match self.groups.iter().find(|g| g.name == name).cloned() {
                Some(g) => {
                    crate::trace(&format!("grup: proje grubu «{}» açılıyor ({} üye)", g.name, g.members.len()));
                    self.select_group(g, window, cx);
                }
                None => crate::trace(&format!("grup: proje grubu «{name}» yok")),
            },
            Some(("pgroupmenu", name)) => {
                if let Some(g) = self.groups.iter().find(|g| g.name == name).cloned() {
                    crate::trace(&format!("grup: proje grubu menüsü «{}»", g.name));
                    self.nav_menu = Some(NavMenu::Group { group: g, at: point(px(150.), px(420.)) });
                }
            }
            // Raydaki projeye sağ tık (ad ile): görünen satırlar iz olarak düşer.
            Some(("projmenu", name)) => {
                if let Some(ix) = self.projects.iter().position(|p| p.name == name) {
                    self.open_project_nav_menu(ix, point(px(60.), px(140. + 52. * ix as f32)), window, cx);
                }
            }
            // Proje menüsü (kenar çubuğu başlığındaki "•••").
            Some(("headmenu", _)) => {
                self.project_menu = true;
                crate::trace(&format!("proje menüsü: [{}]", self.header_menu_items().join(", ")));
            }
            // Ada göre projeye geç.
            Some(("goto", name)) => {
                if let Some(ix) = self.projects.iter().position(|p| p.name == name) {
                    self.board = None;
                    self.documents = None;
                    self.library = None;
                    self.select_project(ix, window, cx);
                }
            }
            // Keşfet'i aç / kapat (raydaki pusula).
            Some(("discover", _)) => self.toggle_discover(window, cx),
            Some(("rename", name)) => {
                let name = name.to_string();
                self.group_rename.update(cx, |input, cx| input.set_value(name, window, cx));
                self.rename_open_group(window, cx);
            }
            Some(("profile", user)) => {
                self.nav_menu = None;
                if let Some(m) = person(self, user) {
                    self.profile = Some(m);
                }
            }
            Some(("request", user)) => {
                self.open_friends(Some(Tab::Add), window, cx);
                let name = user.to_string();
                self.home.add.update(cx, |input, cx| input.set_value(name, window, cx));
                self.friend_add_by_name(window, cx);
            }
            Some(("accept", user)) => {
                if let Some(r) = self.home.data.incoming.iter().find(|r| r.user.username == user).map(|r| r.id.clone()) {
                    self.friend_accept(r, window, cx);
                }
            }
            Some(("block", user)) => {
                if let Some(m) = person(self, user) {
                    self.friend_block(m.id, m.display_name, window, cx);
                }
            }
            Some(("unblock", user)) => {
                if let Some(m) = person(self, user) {
                    self.friend_unblock(m.id, m.display_name, window, cx);
                }
            }
            Some(("remove", user)) => {
                if let Some(m) = person(self, user) {
                    self.friend_remove(m.id, m.display_name, window, cx);
                }
            }
            Some(("project", ix)) => {
                if let Some(ix) = ix.parse::<usize>().ok().filter(|i| *i < self.projects.len()) {
                    self.select_project(ix, window, cx);
                }
            }
            _ => {}
        }
        cx.notify();
    }
}

/// Arkadaşlar sayfası satır türü.
#[derive(Clone, PartialEq, Eq)]
enum RowKind {
    Friend,
    Incoming(String),
    Outgoing(String),
    Blocked,
}

/// Sohbetin son etkinlik damgası (ISO; son mesaj yoksa güncelleme zamanı).
fn dm_stamp(c: &Conversation) -> &str {
    c.last_message.as_ref().map(|m| m.created_at.as_str()).unwrap_or(c.updated_at.as_str())
}
