//! Çalışma alanı — Workspace.tsx çekirdeği: proje rayı, kanal listesi (kategoriler,
//! okunmamışlar), gruplar, kişiler ve birebir sohbetler (çevrimiçi durum, okunmamış DM),
//! sohbet bölmesi, profil paneli, kullanıcı menüsü (durum seçimi, ayarlar, çıkış),
//! gerçek zamanlı bağlantı.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use futures::StreamExt as _;
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, Entity, Hsla, StyleRefinement, Subscription, WeakEntity, Window, div, point, px, relative};
use p2_client_core::realtime::{self, Realtime, RtEvent};
use p2_client_core::{Api, ApiError, Auth, Channel, ChannelCategory, ChannelType, ChatGroup, ChatTarget, Conversation, Member, Organization, Project, Scope};
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::Palette;
use serde_json::json;

use super::board::BoardPage;
use super::documents::DocumentsPage;
use super::library::LibraryPage;
use super::chat::{ChatEvent, ChatPane, ChatSubject};
use super::group_creator::{GroupCreator, GroupCreatorEvent};
use super::settings::{Section, SettingsEvent, SettingsModal};
use crate::app::ClientApp;
use crate::call::CallController;
use crate::task;

/// SettingsModal.tsx `presenceOptions`: (durum, çeviri anahtarı).
pub const PRESENCE_OPTIONS: &[(&str, &str)] = &[("ONLINE", "presence.online"), ("AWAY", "presence.away"), ("BUSY", "presence.busy"), ("INVISIBLE", "presence.invisible")];

/// Durum etiketi (dil duyarlı).
pub fn status_text(status: &str) -> String {
    p2_i18n::t(match status {
        "ONLINE" => "presence.online",
        "AWAY" => "presence.away",
        "BUSY" => "presence.busy",
        "INVISIBLE" => "presence.invisible",
        _ => "friends.status.offline",
    })
}

pub fn status_label(status: &str) -> &'static str {
    match status {
        "ONLINE" => "Çevrimiçi",
        "AWAY" => "Uzakta",
        "BUSY" => "Meşgul",
        "INVISIBLE" => "Görünmez",
        _ => "Çevrimdışı",
    }
}

#[allow(dead_code)]
fn role_label(role: &str) -> String {
    match role {
        "OWNER" => p2_i18n::t("ws.role.owner"),
        "ADMIN" => p2_i18n::t("ws.role.admin"),
        "" => String::new(),
        _ => p2_i18n::t("ws.role.member"),
    }
}

pub fn presence_color(status: &str, p: &Palette) -> Hsla {
    match status {
        "ONLINE" => p.success,
        "AWAY" => p.warning,
        "BUSY" => p.danger,
        _ => p.text_4,
    }
}

/// Çevrimiçi durum noktası (sabit: sürekli yanıp sönmez — Faz 0 kararı).
pub fn presence_dot(status: &str, p: &Palette, ring: Hsla) -> impl IntoElement {
    div().absolute().right(px(-1.)).bottom(px(-1.)).size(px(11.)).rounded_full().border_2().border_color(ring).bg(presence_color(status, p))
}

fn status_rank(status: &str) -> u8 {
    match status {
        "ONLINE" => 0,
        "BUSY" => 1,
        "AWAY" => 2,
        _ => 3,
    }
}

fn sort_members(members: &mut [Member]) {
    members.sort_by(|a, b| status_rank(&a.status).cmp(&status_rank(&b.status)).then_with(|| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase())));
}
pub struct WorkspaceScreen {
    app: WeakEntity<ClientApp>,
    api: Arc<Api>,
    auth: Auth,
    org: Organization,
    projects: Vec<Project>,
    project: Option<usize>,
    categories: Vec<ChannelCategory>,
    collapsed: HashSet<String>,
    members: Vec<Member>,
    /// Bütün birebir sohbetlerim: Ana sayfa (`project_id` yok) + proje sohbetleri; gizlenenler dahil.
    /// Ana sayfa listesi ve proje kenar çubuğu kendi kapsamını süzer (docs/arkadaslar.md).
    conversations: Vec<Conversation>,
    /// Seçili projenin sohbet grupları (Ana sayfa grupları `home.groups` içinde).
    groups: Vec<ChatGroup>,
    chat: Option<Entity<ChatPane>>,
    chat_sub: Option<Subscription>,
    /// Sohbetin sağındaki iş parçacığı / forum konusu paneli.
    thread: Option<(Entity<super::thread::ThreadPane>, Subscription)>,
    /// İletilecek mesaj (hedef kanal seçiliyor).
    forwarding: Option<(ChatTarget, String)>,
    browser: Option<chat_tools::Browser>,
    /// Ayrılınan kanallar (kenar çubuğunda gizli).
    hidden: HashSet<String>,
    /// Yıldızlılar: "CHANNEL:id" / "PERSON:id".
    starred: HashSet<String>,
    /// Kanal başına bildirim tercihi (varsayılan MENTIONS).
    notify_prefs: HashMap<String, String>,
    connected: bool,
    realtime: Realtime,
    /// Kendi durum tercihim (sunucudan `presence:self` ile gelir).
    preference: String,
    my_status: String,
    profile: Option<Member>,
    creator: Option<(Entity<GroupCreator>, Subscription)>,
    settings: Option<(Entity<SettingsModal>, Subscription)>,
    admin: Option<(Entity<super::admin::AdminPanel>, Subscription)>,
    /// Davet koduyla projeye katıl penceresi (rayın "＋" menüsü).
    join: Option<(Entity<super::join_project::JoinDialog>, Subscription)>,
    /// Keşfet (raydaki pusula / "＋" menüsü): herkese açık projeler ve haberleri (discover_host.rs).
    discover: Option<(Entity<super::discover::DiscoverView>, Subscription)>,
    /// Seçili projenin varsayılan davet kodu (proje menüsünde gösterilir; `member.invite` gerekir).
    project_invite: Option<String>,
    /// Proje listesi gelmeden ulaşan `p2://play` bağlantısı (liste gelince işlenir).
    pending_play: Option<String>,
    manage: Option<manage_ui::Manage>,
    project_menu: bool,
    project_menu_categories: bool,
    channel_menu: Option<manage_ui::ChannelMenu>,
    channel_perms: Option<(Entity<super::channel_perms::ChannelPermsPane>, Subscription)>,
    /// Etkin yetkilerim (menülerde hangi işlem açık).
    my_perms: p2_client_core::MyPermissions,
    manage_steps: Vec<String>,
    manage_delay: u64,
    manage_started: bool,
    /// Uygunluk durumu otomatik değiştirdi mi.
    auto_away: bool,
    group_members: Option<p2_client_core::ChatGroup>,
    /// D: gruptan çıkar / ayrıl onayı (people_ui.rs).
    people_confirm: Option<people_ui::PeopleConfirm>,
    group_steps: Vec<String>,
    loading: bool,
    user_menu: bool,
    autopilot_done: bool,
    /// Arama durumu ve medya motoru (CallProvider karşılığı).
    calls: Entity<CallController>,
    _calls_sub: Subscription,
    /// Ana alanda gösterilen sesli oda (sahne); `None` ise sohbet.
    stage: Option<String>,
    call_autopilot_done: bool,
    /// Ana alanda araç sayfası (pano); `None` ise sohbet/sahne.
    board: Option<Entity<BoardPage>>,
    documents: Option<Entity<DocumentsPage>>,
    library: Option<Entity<LibraryPage>>,
    /// Kütüphane motoru (kurulum/indirme kütüphane sayfası kapalıyken de sürer).
    patcher: Arc<p2_patcher::Patcher>,
    /// Oyun içi overlay (kütüphaneden başlatılan oyunlar).
    overlay: Arc<p2_overlay_client::OverlayManager>,
    activity: Vec<p2_client_core::ActivityItem>,
    activity_unread: u32,
    activity_open: bool,
    picker: Option<(Entity<screen_picker::ScreenPicker>, Subscription)>,
    /// Sahnede büyütülen yayın (kullanıcı kimliği).
    stage_focus: Option<String>,
    /// Yayın üstüne çizen kalem (call_ui.rs): hangi yayında, süren çizgi.
    call_pen: Option<crate::annotate::Pen>,
    /// I ajanı: arama arayüzü görünüm durumu (call_ui.rs `CallUi`).
    call_ui: call_ui::CallUi,
    /// Kütüphane otomatik güncelleme kuyruğu (auto_update.rs).
    auto: auto_update::AutoUpdate,
    /// Oyun başlatma kartı (launch_splash.rs).
    launch_card: Option<launch_splash::LaunchCard>,
    /// Kenar çubuğu araması (Workspace.tsx `channel-search`).
    side_search: Entity<gpui_kit::component::input::InputState>,
    _side_search_sub: Subscription,
    /// Ray/kişi sağ tık menüsü (workspace_nav.rs).
    nav_menu: Option<workspace_nav::NavMenu>,
    /// Proje başına okunmamış (ray rozeti).
    project_unread: HashMap<String, u32>,
    /// Proje başına etkin yetkilerim (`/projects/:p/permissions/me`): raydaki sağ tık menüsü seçili
    /// olmayan projelerde de yalnız kullanabileceğim satırları göstersin diye. Henüz gelmediyse yok.
    pub(super) project_perms: HashMap<String, p2_client_core::MyPermissions>,
    /// Kuruluşun bütün üyeleri (sunucu sırası; sohbet başlığı ve bahsetmeler).
    org_members: Vec<Member>,
    /// Raydaki sürükleme: kaynak proje ve hedef (workspace_nav.rs).
    dragging_project: Option<String>,
    drag_target: Option<String>,
    /// L ajanı: bölünmüş görünümün ikinci bölmesi ve ayrı sohbet pencereleri.
    split: Option<(Entity<ChatPane>, Subscription)>,
    popouts: Vec<(Entity<ChatPane>, gpui_kit::AnyWindowHandle, Subscription)>,
    /// Ana sayfa: arkadaşlar ve projelerden bağımsız mesajlar (friends_ui.rs).
    home: friends_ui::Home,
    /// "Yeni mesaj" penceresi (Ana sayfa › Direkt mesajlar › ＋; new_message.rs).
    new_message: Option<(Entity<super::new_message::NewMessageDialog>, Subscription)>,
    /// Grup üyeleri açılır listesindeki "Grup adı" alanı (group_members.rs).
    group_rename: Entity<gpui_kit::component::input::InputState>,
}

#[path = "auto_update.rs"]
mod auto_update;
#[path = "launch_splash.rs"]
mod launch_splash;
#[path = "activity_ui.rs"]
mod activity_ui;
#[path = "screen_picker.rs"]
mod screen_picker;
#[path = "call_ui.rs"]
mod call_ui;
#[path = "overlay_ui.rs"]
mod overlay_ui;
#[path = "chat_tools.rs"]
mod chat_tools;
// L ajanı: bölünmüş görünüm + ayrı sohbet pencereleri.
#[path = "chat_split.rs"]
mod chat_split;
#[path = "manage_ui.rs"]
mod manage_ui;
#[path = "discover_host.rs"]
mod discover_host;
#[path = "group_members.rs"]
mod group_members;
#[path = "workspace_nav.rs"]
mod workspace_nav;
// D ajanı: profil kartı (ray avatarı) ve profil paneli.
// Proje toplulukları: proje olayları, projeden ayrılma.
#[path = "community_ui.rs"]
mod community_ui;
#[path = "people_ui.rs"]
mod people_ui;
// Arkadaşlar + Ana sayfa (ray düğmesi, DM listesi, arkadaşlar sayfası).
#[path = "friends_ui.rs"]
mod friends_ui;

impl WorkspaceScreen {
    pub fn new(app: WeakEntity<ClientApp>, api: Arc<Api>, auth: Auth, org: Organization, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (rt, mut events) = realtime::connect(api.clone(), org.id.clone());
        let calls = cx.new(|_| CallController::new(auth.user.id.clone(), org.id.clone(), rt.sender()));
        let calls_sub = cx.observe(&calls, |this: &mut Self, _, cx| {
            this.sync_overlay_call(cx);
            // Bireysel / grup araması başlayınca arama görünümü açılır (call_ui.rs).
            this.call_ui_track(cx);
            cx.notify()
        });
        // Kütüphane motoru: olaylar arka plan iş parçacıklarından kanala, oradan arayüze.
        let (ptx, mut prx) = futures::channel::mpsc::unbounded::<p2_patcher::Event>();
        let patcher = p2_patcher::Patcher::new(api.clone(), p2_client_core::store::profile_dir().join("library"), crate::CLIENT_VERSION, Arc::new(move |e| {
            let _ = ptx.unbounded_send(e);
        }));
        if patcher.state().root.is_empty() {
            if let Ok(root) = std::env::var("P2_LIBRARY_ROOT") {
                patcher.set_root(&root);
            }
        }
        let overlay = Self::create_overlay(&api, &auth, &org.id, !cx.p2().dark, &patcher, window, cx);
        cx.spawn_in(window, async move |this, cx| {
            while let Some(event) = prx.next().await {
                if this.update_in(cx, |this, _, cx| this.on_patch_event(event, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        cx.spawn_in(window, async move |this, cx| {
            while let Some(event) = events.next().await {
                if this.update_in(cx, |this, window, cx| this.on_event(event, window, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();

        let side_search = cx.new(|cx| gpui_kit::component::input::InputState::new(window, cx));
        let side_search_sub = cx.subscribe(&side_search, |_: &mut Self, _, _: &gpui_kit::component::input::InputEvent, cx| cx.notify());
        let home = friends_ui::Home::new(window, cx);
        let group_rename = cx.new(|cx| gpui_kit::component::input::InputState::new(window, cx));
        let mut ws = Self {
            app,
            api,
            auth,
            org,
            projects: Vec::new(),
            project: None,
            categories: Vec::new(),
            collapsed: HashSet::new(),
            members: Vec::new(),
            conversations: Vec::new(),
            groups: Vec::new(),
            chat: None,
            chat_sub: None,
            thread: None,
            forwarding: None,
            browser: None,
            hidden: HashSet::new(),
            starred: HashSet::new(),
            notify_prefs: HashMap::new(),
            connected: false,
            realtime: rt,
            preference: "ONLINE".into(),
            my_status: "ONLINE".into(),
            profile: None,
            creator: None,
            settings: None,
            admin: None,
            join: None,
            discover: None,
            project_invite: None,
            pending_play: None,
            manage: None,
            project_menu: false,
            project_menu_categories: false,
            channel_menu: None,
            channel_perms: None,
            my_perms: Default::default(),
            manage_steps: std::env::var("P2_AUTOPILOT_MANAGE").map(|s| s.split(';').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).rev().collect()).unwrap_or_default(),
            manage_delay: 800,
            manage_started: false,
            auto_away: false,
            group_members: None,
            people_confirm: None,
            group_steps: Vec::new(),
            loading: true,
            user_menu: std::env::var_os("P2_AUTOPILOT_USER_MENU").is_some(),
            autopilot_done: false,
            calls,
            _calls_sub: calls_sub,
            stage: None,
            call_autopilot_done: false,
            board: None,
            documents: None,
            library: None,
            patcher,
            overlay,
            activity: Vec::new(),
            activity_unread: 0,
            activity_open: std::env::var_os("P2_AUTOPILOT_ACTIVITY").is_some(),
            picker: None,
            stage_focus: None,
            call_pen: None,
            call_ui: Default::default(),
            auto: Default::default(),
            launch_card: None,
            side_search,
            _side_search_sub: side_search_sub,
            nav_menu: None,
            project_unread: HashMap::new(),
            project_perms: HashMap::new(),
            org_members: Vec::new(),
            dragging_project: None,
            drag_target: None,
            split: None,
            popouts: Vec::new(),
            home,
            new_message: None,
            group_rename,
        };
        ws.load_projects(window, cx);
        {
            let (api, org) = (ws.api.clone(), ws.org.id.clone());
            task::run(window, cx, move || api.members(&org, None), |this, r, _, cx| {
                if let Ok(members) = r {
                    this.org_members = members;
                    if let Some(chat) = this.chat.clone() {
                        let m = this.org_members.clone();
                        chat.update(cx, |c, cx| {
                            c.set_members(m);
                            cx.notify();
                        });
                    }
                }
            });
        }
        ws.load_conversations(window, cx);
        ws.load_home_groups(window, cx);
        ws.load_friends(window, cx);
        ws.load_activity(window, cx);
        ws.load_preferences(window, cx);
        ws.start_auto_update(window, cx);
        // ClientApp şu an güncelleniyor (çalışma alanını kuruyor): tercihler bir sonraki turda okunur.
        cx.spawn(async move |this, cx| {
            let _ = this.update(cx, |this, cx| this.apply_prefs(cx));
        })
        .detach();
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(std::time::Duration::from_secs(if std::env::var_os("P2_FAKE_CLOCK").is_some() { 2 } else { 60 })).await;
            if this.update(cx, |this, cx| this.check_availability(cx)).is_err() {
                break;
            }
        })
        .detach();
        ws
    }

    /// Seçili projedeki etkin yetkilerim (`/projects/:p/permissions/me`): menüler, kanal işlemleri,
    /// yönetim paneli. Ayrıca davet yetkisi varsa projenin varsayılan davet kodu.
    fn load_project_perms(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.project_id() else { return };
        let api = self.api.clone();
        task::run(window, cx, move || (project.clone(), api.project_permissions(&project)), |this, (project, r), window, cx| {
            if this.project_id().as_deref() != Some(project.as_str()) {
                return;
            }
            if let Ok(p) = r {
                crate::trace(&format!("yetkiler: {} yetki{}", p.permissions.len(), if p.owner { " (sahip)" } else { "" }));
                this.project_perms.insert(project.clone(), p.clone());
                this.my_perms = p;
                // L ajanı: açık sohbetin silme yetkileri.
                if let Some(chat) = this.chat.clone() {
                    let perms = this.my_perms.clone();
                    chat.update(cx, |c, cx| {
                        c.apply_perms(&perms);
                        cx.notify();
                    });
                }
                if this.my_perms.can("member.invite") {
                    this.load_project_invite(window, cx);
                }
                cx.notify();
            }
        });
    }

    /// Raydaki projelerin yetkilerini arka planda alır (`only`: tek proje; yoksa hepsi). Sağ tık menüsü
    /// yalnız kullanabileceğim yönetim satırlarını gösterir; yetki gelene dek yalnız "Projeden ayrıl".
    pub(super) fn load_rail_perms(&mut self, only: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let ids: Vec<String> = match only {
            Some(id) => vec![id],
            None => self.projects.iter().map(|p| p.id.clone()).collect(),
        };
        if ids.is_empty() {
            return;
        }
        let api = self.api.clone();
        task::run(window, cx, move || ids.into_iter().filter_map(|id| api.project_permissions(&id).ok().map(|p| (id, p))).collect::<Vec<_>>(), |this, list, _, cx| {
            crate::trace(&format!("ray: {} projenin yetkileri alındı", list.len()));
            this.project_perms.extend(list);
            cx.notify();
        });
    }

    /// Projede yönetim paneline girebilir miyim: panelin en az bir bölümünü açan yetki (admin.rs
    /// `PROJECT_SECTIONS`: admin.panel, member.roles, member.invite, member.kick, organization.manage).
    pub(super) fn can_admin(perms: &p2_client_core::MyPermissions) -> bool {
        ["admin.panel", "member.invite", "member.kick", "member.roles", "organization.manage"].iter().any(|perm| perms.can(perm))
    }

    /// Raydaki proje menüsünde görünen satırlar (yalnız kullanabildiklerim; "Projeden ayrıl" hep durur).
    pub(super) fn project_menu_items(&self, index: usize) -> Vec<&'static str> {
        let perms = self.projects.get(index).and_then(|p| self.project_perms.get(&p.id));
        let mut items = Vec::new();
        if let Some(perms) = perms {
            if Self::can_admin(perms) {
                items.push("admin");
            }
            if perms.can("organization.manage") {
                // "Proje ayarları…" penceresi seçili projede açılır.
                if self.project == Some(index) {
                    items.push("settings");
                }
                items.push("delete");
            }
        }
        items.push("leave");
        items
    }

    /// Kenar çubuğu başlığındaki "•••" menüsünde görünen satırlar (iz / test).
    pub(super) fn header_menu_items(&self) -> Vec<&'static str> {
        let mut items = Vec::new();
        if self.my_perms.can("channel.manage") {
            items.push("category");
            items.push("channel");
        }
        items.push("browse");
        if self.my_perms.can("member.invite") {
            items.push("invite");
        }
        if Self::can_admin(&self.my_perms) {
            items.push("admin");
        }
        items.push("leave");
        items
    }

    /// Proje menüsündeki davet kodu: süresiz ve sınırsız varsayılan kod (yoksa ilk etkin kod).
    pub(super) fn load_project_invite(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.project_id() else { return };
        let api = self.api.clone();
        task::run(window, cx, move || (project.clone(), api.project_invites(&project, false)), |this, (project, r), _, cx| {
            if this.project_id().as_deref() != Some(project.as_str()) {
                return;
            }
            if let Ok(list) = r {
                this.project_invite = list.iter().find(|i| i.expires_at.is_none() && i.max_uses.is_none()).or(list.first()).map(|i| i.code.clone());
                cx.notify();
            }
        });
    }

    /// Projeyi raydan düşür (ayrıldım, çıkarıldım, silindi): seçiliyse ilk projeye geç.
    pub(super) fn remove_project(&mut self, project_id: &str, note: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.projects.iter().position(|p| p.id == project_id) else {
            if let Some(n) = note {
                crate::toasts::info(n);
            }
            return;
        };
        let was_selected = self.project == Some(ix);
        self.projects.remove(ix);
        self.project_unread.remove(project_id);
        if was_selected {
            self.project = None;
            self.admin = None;
            self.board = None;
            self.documents = None;
            self.library = None;
            self.categories.clear();
            self.members.clear();
            self.groups.clear();
            self.project_invite = None;
            self.close_chat(cx);
            crate::titlebar::set_project_title("");
            if !self.projects.is_empty() {
                self.select_project(0, window, cx);
            }
        }
        crate::trace(&format!("ray: proje düştü {project_id}"));
        if self.projects.is_empty() {
            // Son proje de gitti (silindi, ayrıldım, çıkarıldım): başlangıç (katıl / oluştur) ekranı.
            self.back_to_start(note, window, cx);
            return;
        }
        if !was_selected {
            if let Some(sel) = self.project.filter(|s| *s > ix) {
                self.project = Some(sel - 1);
            }
        }
        if let Some(n) = note {
            crate::toasts::info(n);
        }
        cx.notify();
    }

    /// Davetle katılınan ya da yeni açılan projeyi raya ekle ve seç.
    pub(super) fn adopt_project(&mut self, project: Project, window: &mut Window, cx: &mut Context<Self>) {
        let ix = match self.projects.iter().position(|p| p.id == project.id) {
            Some(ix) => ix,
            None => {
                self.projects.push(project);
                self.projects.len() - 1
            }
        };
        self.board = None;
        self.documents = None;
        self.library = None;
        self.select_project(ix, window, cx);
        cx.notify();
    }

    /// Rayın "＋" menüsü → "Davet koduyla katıl".
    pub(super) fn open_join(&mut self, code: Option<(String, bool)>, window: &mut Window, cx: &mut Context<Self>) {
        self.nav_menu = None;
        let dialog = cx.new(|cx| super::join_project::JoinDialog::new(self.api.clone(), window, cx));
        if let Some((code, go)) = code {
            dialog.update(cx, |d, cx| d.autopilot(&code, go, window, cx));
        }
        let sub = cx.subscribe_in(&dialog, window, |this, _, event: &super::join_project::JoinEvent, window, cx| {
            match event {
                super::join_project::JoinEvent::Close => {}
                super::join_project::JoinEvent::Joined(joined) => {
                    let name = joined.project.name.clone();
                    if joined.organization_id != this.org.id {
                        // Başka kurulum kabındaki proje: çalışma alanı o kuruluşla yeniden kurulur.
                        let (app, org) = (this.app.clone(), joined.organization.clone());
                        let _ = app.update(cx, |app, cx| app.enter_workspace(org, window, cx));
                        return;
                    }
                    this.adopt_project(joined.project.clone(), window, cx);
                    Self::joined_toast(&name, joined.joined);
                }
            }
            this.join = None;
            cx.notify();
        });
        crate::trace("katıl: pencere açıldı");
        self.join = Some((dialog, sub));
        cx.notify();
    }

    /// "X projesine katıldın" (başarı) / "zaten üyesin" (bilgi): sağ üstteki bildirim.
    pub(super) fn joined_toast(name: &str, joined: bool) {
        if joined {
            crate::toasts::ok(p2_i18n::tv("disc.note.joined", &[("name", name)]));
        } else {
            crate::toasts::info(p2_i18n::tv("disc.note.already", &[("name", name)]));
        }
    }

    fn fail(&mut self, e: ApiError, window: &mut Window, cx: &mut Context<Self>) {
        if e.code == "SESSION_EXPIRED" {
            let app = self.app.clone();
            let _ = app.update(cx, |app, cx| app.logout(window, cx));
        } else {
            crate::toasts::error(if e.message.is_empty() { p2_i18n::t("toast.failed") } else { e.message });
            cx.notify();
        }
    }

    fn project_id(&self) -> Option<String> {
        self.project.map(|ix| self.projects[ix].id.clone())
    }

    fn current_scope(&self, cx: &gpui_kit::App) -> Option<Scope> {
        self.chat.as_ref().map(|c| c.read(cx).target.scope())
    }

    // ------------------------------------------------------------ yükleme

    /// Ray rozetleri (C ajanı): `GET /organizations/:org/projects/unread` (Workspace.tsx `setProjectUnread`).
    pub(super) fn refresh_project_unread(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (api, org) = (self.api.clone(), self.org.id.clone());
        task::run(window, cx, move || api.projects_unread(&org), |this, r, _, cx| {
            if let Ok(map) = r {
                // İz (testler): proje adı=okunmamış, ada göre sıralı.
                let mut parts: Vec<String> = this.projects.iter().filter_map(|p| map.get(&p.id).map(|n| format!("{}={n}", p.name))).collect();
                parts.sort();
                crate::trace(&format!("ray: proje okunmamışları {} proje [{}]", map.len(), parts.join(", ")));
                this.project_unread = map;
                this.trace_badges();
                cx.notify();
            }
        });
    }

    /// Seçili projeden çıkılırken (başka proje / Ana sayfa) ray rozetinin kaynağını günceller: sunucudan
    /// alınmış eski toplam yerine eldeki kanal listesindeki okunmamışlar yazılır, ardından sunucudan da
    /// tazelenir. Eskiden bu yapılmıyordu: proje açılıp her şey okunsa bile başka projeye geçince rozet
    /// bellekteki eski sayıdan çiziliyor ve hiç kaybolmuyordu (p2_client_core::unread).
    pub(super) fn leave_project_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pid) = self.project_id() else { return };
        // Kanal listesi henüz gelmediyse eldeki sunucu toplamı korunur.
        if !self.categories.is_empty() {
            let open = match self.current_scope(cx) {
                Some(Scope::Channel(id)) => Some(id),
                _ => None,
            };
            let n = p2_client_core::unread::stash_channel_unread(&mut self.project_unread, &pid, &self.categories, &self.hidden, open.as_deref());
            crate::trace(&format!("ray: {} projesinden çıkıldı, kanal okunmamışı {n}", self.projects.iter().find(|p| p.id == pid).map(|p| p.name.as_str()).unwrap_or_default()));
        }
        self.refresh_project_unread(window, cx);
    }

    /// Raydaki proje rozeti: görünen kanallar + o projenin sohbetleri. Seçili projede kanal sayısı eldeki
    /// listeden (okudukça düşer), diğerlerinde sunucu toplamından gelir.
    pub(super) fn project_badge(&self, ix: usize) -> u32 {
        let Some(project) = self.projects.get(ix) else { return 0 };
        // Seçili proje: kullanıcı zaten içinde; rozet gösterilmez (okunmamışlar kenar çubuğunda).
        if self.project == Some(ix) {
            return 0;
        }
        let blocked: std::collections::HashSet<String> = self.home.data.blocked.iter().map(|b| b.user.id.clone()).collect();
        self.project_unread.get(&project.id).copied().unwrap_or(0) + p2_client_core::unread::conversation_unread(&self.conversations, &project.id, &blocked, None)
    }

    /// İz (testler): raydaki proje rozetleri, `ray: rozetler [Ad=n, …]` (ada göre sıralı; seçili proje 0).
    pub(super) fn trace_badges(&self) {
        let mut parts: Vec<String> = (0..self.projects.len()).map(|i| format!("{}={}", self.projects[i].name, self.project_badge(i))).collect();
        parts.sort();
        crate::trace(&format!("ray: rozetler [{}]", parts.join(", ")));
    }

    fn load_projects(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.refresh_project_unread(window, cx);
        let (api, org) = (self.api.clone(), self.org.id.clone());
        task::run(window, cx, move || api.projects(&org), |this, result, window, cx| {
            this.loading = false;
            match result {
                Ok(projects) => {
                    this.projects = projects;
                    this.apply_project_order();
                    crate::trace(&format!("ray: {} proje", this.projects.len()));
                    this.load_rail_perms(None, window, cx);
                    // Okunmamış toplamı proje listesinden önce gelmiş olabilir: rozet izi şimdi tam.
                    this.trace_badges();
                    if this.projects.is_empty() {
                        // Başlangıç ekranından "Arkadaşlar ve mesajlar" ile gelindiyse projesiz Ana sayfa açılır.
                        if super::onboarding::HOME_ONLY.load(std::sync::atomic::Ordering::SeqCst) {
                            this.enter_home(window, cx);
                            // Başlangıç ekranındaki "Projelere göz at": Keşfet rayın yanında açılır.
                            if super::onboarding::OPEN_DISCOVER.swap(false, std::sync::atomic::Ordering::SeqCst) {
                                this.open_discover(window, cx);
                            }
                            cx.notify();
                            return;
                        }
                        return this.back_to_start(None, window, cx);
                    }
                    // Liste gelmeden Ana sayfaya geçildiyse proje seçimi onu ezmez.
                    if !this.home.open {
                        // Otomatik pilot: `P2_AUTOPILOT_PROJECT=<ad>` o projeyi seçer (yoksa ilk proje).
                        let wanted = std::env::var("P2_AUTOPILOT_PROJECT").ok().and_then(|name| this.projects.iter().position(|p| p.name == name));
                        this.select_project(wanted.unwrap_or(0), window, cx);
                    }
                    if let Some(link) = this.pending_play.take() {
                        this.play_from_link(&link, window, cx);
                    }
                    // Otomatik pilot: `P2_AUTOPILOT_JOIN=kod[|join]` katılma penceresi; `P2_AUTOPILOT_RAIL_ADD=1`
                    // rayın "＋" menüsü (yalnız görünüm).
                    if let Ok(spec) = std::env::var("P2_AUTOPILOT_JOIN").ok().filter(|_| !super::join_project::AUTOPILOT_JOIN_USED.swap(true, std::sync::atomic::Ordering::SeqCst)).ok_or(()) {
                        let (code, go) = spec.split_once('|').map(|(c, g)| (c.to_string(), g == "join")).unwrap_or((spec.clone(), false));
                        this.open_join(Some((code, go)), window, cx);
                    }
                    if super::onboarding::OPEN_DISCOVER.swap(false, std::sync::atomic::Ordering::SeqCst) || std::env::var_os("P2_AUTOPILOT_DISCOVER").is_some() {
                        this.open_discover(window, cx);
                    }
                    if std::env::var_os("P2_AUTOPILOT_RAIL_ADD").is_some() {
                        // Düğme rayın altında: menü pencerenin altına yaslanır (görünüm kancası).
                        this.nav_menu = Some(workspace_nav::NavMenu::Add { at: point(px(66.), px(10000.)) });
                    }
                    cx.notify();
                }
                Err(e) => this.fail(e, window, cx),
            }
        });
    }

    fn load_conversations(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (api, org) = (self.api.clone(), self.org.id.clone());
        task::run(window, cx, move || api.conversations(&org), |this, result, _, cx| {
            if let Ok(mut conversations) = result {
                // Açık DM okunmuş sayılır.
                if let Some(Scope::Direct(open)) = this.current_scope(cx) {
                    conversations.iter_mut().filter(|c| c.id == open).for_each(|c| c.unread = 0);
                }
                this.conversations = conversations;
                for c in &this.conversations {
                    crate::avatars::remember(&c.user.id, c.user.avatar_url.as_deref());
                }
                crate::trace(&format!(
                    "sohbetler: {} ana sayfa, {} proje, {} gizli",
                    this.conversations.iter().filter(|c| c.is_home() && !c.hidden).count(),
                    this.conversations.iter().filter(|c| !c.is_home() && !c.hidden).count(),
                    this.conversations.iter().filter(|c| c.hidden).count()
                ));
                this.sync_chat_head(cx);
                cx.notify();
            }
        });
    }

    fn load_groups(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.project_id() else { return };
        let (api, org) = (self.api.clone(), self.org.id.clone());
        task::run(window, cx, move || (project.clone(), api.groups(&org, &project)), |this, (project, result), window, cx| {
            if this.project_id().as_deref() != Some(project.as_str()) {
                return;
            }
            if let Ok(groups) = result {
                this.groups = groups;
                static OPENED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
                if let Ok(name) = std::env::var("P2_AUTOPILOT_OPEN_GROUP") {
                    if let Some(g) = this.groups.iter().find(|g| g.name == name).cloned() {
                        if !OPENED.swap(true, std::sync::atomic::Ordering::SeqCst) {
                            this.select_group(g, window, cx);
                        }
                    }
                }
                cx.notify();
            }
        });
    }

    fn select_project(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.project == Some(ix) {
            return;
        }
        // Çıkılan projenin ray rozeti eldeki (okunmuş) kanal listesinden güncellenir.
        self.leave_project_view(window, cx);
        self.home.open = false;
        self.project = Some(ix);
        // Başlık çubuğu sayfa adı = seçili proje (Workspace.tsx data-project-title).
        crate::titlebar::set_project_title(&self.projects[ix].name);
        // Kök (başlık çubuğu) yeniden çizilsin; kurulum sırasında kök güncelleniyor olabilir → ertele.
        let app = self.app.clone();
        cx.defer(move |cx| {
            let _ = app.update(cx, |_, cx| cx.notify());
        });
        self.categories.clear();
        self.members.clear();
        self.groups.clear();
        self.project_invite = None;
        self.close_chat(cx);
        cx.notify();
        let project = self.projects[ix].clone();
        let (api, org) = (self.api.clone(), self.org.id.clone());
        let pid = project.id.clone();
        self.load_groups(window, cx);
        self.load_project_perms(window, cx);
        task::run(
            window,
            cx,
            // Kişi listesi projeye özel (`/projects/:p/members`).
            move || (api.channels(&org, &pid), api.project_members(&pid).map(|list| list.into_iter().map(Member::from).collect::<Vec<Member>>())),
            move |this, (channels, members), window, cx| {
                if this.project != Some(ix) {
                    return; // bu arada başka projeye geçildi
                }
                if let Ok(mut members) = members {
                    sort_members(&mut members);
                    this.members = members;
                    for m in &this.members {
                        crate::avatars::remember(&m.id, m.avatar_url.as_deref());
                    }
                }
                match channels {
                    Ok(categories) => {
                        this.categories = categories;
                        this.autopilot_browser(window, cx);
                        this.autopilot_menus(cx);
                        if !this.manage_steps.is_empty() && !this.manage_started {
                            this.manage_started = true;
                            this.manage_next(window, cx);
                        }
                        if std::env::var_os("P2_AUTOPILOT_ADMIN").is_some() && std::env::var_os("P2_AUTOPILOT_INSTANCE").is_none() && this.admin.is_none() {
                            static ADMIN_OPENED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
                            if !ADMIN_OPENED.swap(true, std::sync::atomic::Ordering::SeqCst) {
                                this.open_admin(window, cx);
                            }
                        }
                        // Sunucu yönetimi (`P2_AUTOPILOT_INSTANCE` ya da adımlı `P2_AUTOPILOT_SERVER_ADMIN`).
                        if (std::env::var_os("P2_AUTOPILOT_INSTANCE").is_some() || std::env::var_os("P2_AUTOPILOT_SERVER_ADMIN").is_some()) && this.auth.user.server_panel() {
                            static INSTANCE_OPENED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
                            if !INSTANCE_OPENED.swap(true, std::sync::atomic::Ordering::SeqCst) {
                                this.open_instance_admin(window, cx);
                            }
                        }
                        let all: Vec<&Channel> = this.categories.iter().flat_map(|c| c.channels.iter()).collect();
                        let autopilot = std::env::var("P2_AUTOPILOT_CHANNEL").ok();
                        let target = autopilot
                            .as_deref()
                            .and_then(|name| all.iter().find(|c| c.name == name))
                            .or_else(|| project.default_channel_id.as_deref().and_then(|id| all.iter().find(|c| c.id == id && c.kind != ChannelType::Voice)))
                            .or_else(|| all.iter().find(|c| c.kind != ChannelType::Voice))
                            .map(|c| (*c).clone());
                        if let Some(channel) = target.filter(|_| std::env::var_os("P2_AUTOPILOT_OPEN_GROUP").is_none()) {
                            this.select_channel(channel, window, cx);
                        }
                    }
                    Err(e) => this.fail(e, window, cx),
                }
                this.autopilot(window, cx);
                this.autopilot_call(window, cx);
                match std::env::var("P2_AUTOPILOT_PAGE").as_deref() {
                    Ok("board") => this.open_board(window, cx),
                    Ok("documents") => this.open_documents(window, cx),
                    Ok("library") => this.open_library(window, cx),
                    _ => {}
                }
                cx.notify();
            },
        );
    }

    /// Eski parite aracı (Electron ↔ Rust; kaldırıldı) için yalnız GÖRÜNÜM açan kancalar; veri değiştirmez:
    /// `P2_AUTOPILOT_PROJECT_MENU=1` proje menüsü, `P2_AUTOPILOT_CHANNEL_MENU=<kanal>[@x,y]` kanal
    /// sağ tık menüsü (x,y: pencere içi mantıksal px; Electron'daki tıklama noktası).
    fn autopilot_menus(&mut self, cx: &mut Context<Self>) {
        if std::env::var_os("P2_AUTOPILOT_PROJECT_MENU").is_some() {
            self.project_menu = true;
        }
        if let Ok(spec) = std::env::var("P2_AUTOPILOT_CHANNEL_MENU") {
            let (name, at) = spec.split_once('@').unwrap_or((spec.as_str(), "200,300"));
            let (x, y) = at.split_once(',').and_then(|(x, y)| Some((x.trim().parse::<f32>().ok()?, y.trim().parse::<f32>().ok()?))).unwrap_or((200., 300.));
            if let Some(channel) = self.categories.iter().flat_map(|c| c.channels.iter()).find(|c| c.name == name).cloned() {
                self.channel_menu = Some(manage_ui::ChannelMenu::channel(channel, point(px(x), px(y))));
                crate::trace(&format!("parite: kanal menüsü #{name}"));
            }
        }
        cx.notify();
    }

    /// Otomatik pilot (masaüstüne girdi göndermeden uçtan uca test): ortam değişkenleriyle
    /// DM aç, grup kur, profil göster, ayarları aç, profil adını değiştir, durum seç.
    fn autopilot(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.autopilot_done {
            return;
        }
        self.autopilot_done = true;
        if let Ok(username) = std::env::var("P2_AUTOPILOT_DM") {
            if let Some(member) = self.members.iter().find(|m| m.username == username).cloned() {
                self.open_direct(member, window, cx);
            }
        }
        if let Ok(spec) = std::env::var("P2_AUTOPILOT_GROUP") {
            // "Ad|kullanıcı1,kullanıcı2"
            let (name, users) = spec.split_once('|').unwrap_or(("", spec.as_str()));
            let users: Vec<String> = users.split(',').map(str::to_string).collect();
            self.open_creator(window, cx);
            if let Some((creator, _)) = &self.creator {
                let name = name.to_string();
                creator.update(cx, |c, cx| c.autopilot(&name, &users.iter().map(String::as_str).collect::<Vec<_>>(), window, cx));
            }
        }
        if let Ok(username) = std::env::var("P2_AUTOPILOT_PROFILE") {
            self.open_profile(&username, true, cx);
        }
        // D: kişiler görünüm kancaları (people_ui.rs).
        self.people_autopilot(window, cx);
        if let Ok(section) = std::env::var("P2_AUTOPILOT_SETTINGS") {
            let section = match section.as_str() {
                "appearance" => Section::Appearance,
                "language" => Section::Language,
                "shortcuts" => Section::Shortcuts,
                "media" => Section::Media,
                "library" => Section::Library,
                "availability" => Section::Availability,
                "app" => Section::App,
                "notifications" => Section::Notifications,
                _ => Section::Account,
            };
            self.open_settings(section, window, cx);
        }
        if let Ok(theme) = std::env::var("P2_AUTOPILOT_THEME") {
            self.open_settings(Section::Appearance, window, cx);
            if let Some((settings, _)) = &self.settings {
                settings.update(cx, |s, cx| s.set_theme(theme != "light", window, cx));
            }
        }
        if let Ok(name) = std::env::var("P2_AUTOPILOT_RENAME") {
            self.open_settings(Section::Account, window, cx);
            if let Some((settings, _)) = &self.settings {
                settings.update(cx, |s, cx| {
                    s.set_display_name(&name, window, cx);
                    s.save_profile(window, cx);
                });
            }
        }
    }

    // ------------------------------------------------------------ sohbet açma

    fn close_chat(&mut self, cx: &mut Context<Self>) {
        self.chat = None;
        self.chat_sub = None;
        self.thread = None;
        cx.notify();
    }

    fn open_chat(&mut self, target: ChatTarget, subject: ChatSubject, window: &mut Window, cx: &mut Context<Self>) {
        let (api, me) = (self.api.clone(), self.auth.user.clone());
        let chat = cx.new(|cx| ChatPane::new(api, target, subject, me, window, cx));
        // Electron: sohbetteki üye listesi (bahsetme, başlıktaki üye yığını) kuruluşun tamamı.
        let members = if self.org_members.is_empty() { self.members.clone() } else { self.org_members.clone() };
        // L ajanı: silme yetkileri.
        let perms = self.my_perms.clone();
        chat.update(cx, |c, _| {
            c.set_members(members);
            c.apply_perms(&perms);
        });
        self.thread = None;
        self.chat_sub = Some(cx.subscribe_in(&chat, window, |this, chat, event: &ChatEvent, window, cx| match event {
            ChatEvent::OpenProfile(user_id) => this.open_profile_by_id(user_id, cx),
            ChatEvent::OpenThread(message) => this.open_thread(message.clone(), window, cx),
            ChatEvent::Forward(id) => this.start_forward(id.clone(), window, cx),
            ChatEvent::ManageGroup(group) => this.open_group_members(group.clone(), window, cx),
            ChatEvent::LeaveGroup(group_id) => this.ask_leave_group(group_id.clone(), cx),
            ChatEvent::Typing(typing) => {
                let payload = match &chat.read(cx).target {
                    ChatTarget::Channel { org, channel, .. } => json!({ "organizationId": org, "channelId": channel, "typing": typing }),
                    ChatTarget::Direct { conversation, .. } => json!({ "conversationId": conversation, "typing": typing }),
                    ChatTarget::Group { .. } => return,
                };
                this.realtime.emit("typing", payload);
            }
            ChatEvent::StartCall => {
                let pane = chat.read(cx);
                match (&pane.target, &pane.subject) {
                    (target, ChatSubject::Direct(person)) => {
                        let id = person.id.clone();
                        // Arama sohbetin kapsamında başlar: proje sohbetinden o projede, Ana sayfadan hesaba bağlı.
                        let project = match target {
                            ChatTarget::Direct { conversation, .. } => this.conversations.iter().find(|c| &c.id == conversation).and_then(|c| c.project_id.clone()),
                            _ => None,
                        };
                        this.start_direct_call(&id, project, window, cx);
                    }
                    (ChatTarget::Group { group, .. }, _) => {
                        let id = group.clone();
                        this.calls.update(cx, |c, cx| c.join_group(&id, window, cx));
                    }
                    // Kanal başlığındaki arama düğmesi (Electron onCallChannel): kanalda arama.
                    (_, ChatSubject::Channel(channel)) => {
                        let id = channel.id.clone();
                        this.calls.update(cx, |c, cx| c.join_channel(&id, window, cx));
                    }
                    _ => {}
                }
            }
            ChatEvent::ToggleStar => match &chat.read(cx).subject {
                ChatSubject::Channel(channel) => {
                    let id = channel.id.clone();
                    this.toggle_star("CHANNEL", id, window, cx);
                }
                // Kişi yıldızı bulunduğum kapsama yazılır (Ana sayfa ya da bu proje).
                ChatSubject::Direct(person) => {
                    let id = person.id.clone();
                    this.toggle_star("PERSON", id, window, cx);
                }
                _ => {}
            },
            ChatEvent::HideConversation => {
                if let ChatTarget::Direct { conversation, .. } = &chat.read(cx).target {
                    let id = conversation.clone();
                    this.hide_conversation(id, window, cx);
                }
            }
            // C ajanı: başlık menüleri (bildirim, izinler, ayrıl, ayarlar).
            ChatEvent::SetNotify(level) => {
                if let ChatSubject::Channel(channel) = &chat.read(cx).subject {
                    let id = channel.id.clone();
                    this.set_channel_notify(id, level, window, cx);
                }
            }
            ChatEvent::ChannelPermissions => {
                if let ChatSubject::Channel(channel) = &chat.read(cx).subject {
                    let c = channel.clone();
                    this.open_channel_perms(c, window, cx);
                }
            }
            ChatEvent::LeaveChannel => {
                if let ChatSubject::Channel(channel) = &chat.read(cx).subject {
                    let id = channel.id.clone();
                    this.set_hidden(id, true, window, cx);
                    this.close_chat(cx);
                }
            }
            ChatEvent::ChannelUpdated(updated) => {
                for c in this.categories.iter_mut().flat_map(|c| c.channels.iter_mut()) {
                    if c.id == updated.id {
                        c.name = updated.name.clone();
                        c.topic = updated.topic.clone();
                    }
                }
                cx.notify();
            }
            // I ajanı: sesli oda "Sohbeti göster/gizle".
            ChatEvent::ToggleVoiceChat => this.toggle_voice_chat(cx),
            // L ajanı: bölünmüş görünüm / ayrı pencere.
            ChatEvent::OpenSplit => this.open_split(window, cx),
            ChatEvent::OpenWindow => this.open_popout(window, cx),
            ChatEvent::CloseSplit => {}
        }));
        self.chat = Some(chat);
        self.stage = None;
        self.call_ui_chat_opened(cx);
        self.sync_chat_head(cx);
        self.autopilot_split(window, cx);
        cx.notify();
    }

    /// Kanal başlığının yıldız/bildirim durumunu (birebir sohbette kişinin yıldızını) sohbete yazar.
    pub(super) fn sync_chat_head(&self, cx: &mut Context<Self>) {
        let Some(chat) = self.chat.clone() else { return };
        let head = match &chat.read(cx).subject {
            ChatSubject::Channel(channel) => (self.starred.contains(&format!("CHANNEL:{}", channel.id)), self.notify_prefs.get(&channel.id).cloned().unwrap_or_else(|| "MENTIONS".into())),
            ChatSubject::Direct(person) => (self.person_starred(&person.id), "MENTIONS".to_string()),
            _ => return,
        };
        chat.update(cx, |c, cx| {
            if c.head.starred != head.0 || c.head.notify != head.1 {
                c.head.starred = head.0;
                c.head.notify = head.1;
                cx.notify();
            }
        });
    }

    fn select_channel(&mut self, channel: Channel, window: &mut Window, cx: &mut Context<Self>) {
        // I ajanı: sesli oda da metin kanalı gibi açılır (Electron: başlık + VoiceRoomStage;
        // katılmak "Sesli odaya katıl" ya da başlıktaki arama düğmesiyle).
        if self.current_scope(cx) == Some(Scope::Channel(channel.id.clone())) {
            self.stage = None;
            cx.notify();
            return;
        }
        for c in self.categories.iter_mut().flat_map(|c| c.channels.iter_mut()) {
            if c.id == channel.id {
                c.unread = 0;
            }
        }
        let Some(project) = self.project_id() else { return };
        let target = ChatTarget::Channel { org: self.org.id.clone(), project, channel: channel.id.clone() };
        self.open_chat(target, ChatSubject::Channel(channel), window, cx);
    }

    /// Bulunduğum kapsam: Ana sayfadaysam `None`, bir projedeysem o proje. Kişiye tıklamak, "Mesaj
    /// gönder", "Sesli ara" ve kişi yıldızı bu kapsamda çalışır.
    pub(super) fn dm_scope(&self) -> Option<String> {
        if self.home.open { None } else { self.project_id() }
    }

    /// Kişiye tıklayınca birebir sohbet: proje içindeysem o projenin sohbeti (proje kenar çubuğunda
    /// yaşar), Ana sayfadaysam hesaba bağlı Ana sayfa sohbeti. İkisi ayrı sohbetlerdir.
    fn open_direct(&mut self, person: Member, window: &mut Window, cx: &mut Context<Self>) {
        let scope = self.dm_scope();
        self.open_direct_in(person, scope, window, cx);
    }

    /// Ana sayfa sohbeti (ray baloncukları, arkadaşlar sayfası, "Yeni mesaj"): gerekirse Ana sayfaya geçer.
    pub(super) fn open_home_direct(&mut self, person: Member, window: &mut Window, cx: &mut Context<Self>) {
        self.open_direct_in(person, None, window, cx);
    }

    /// Sohbet yoksa sunucu açar (POST /direct; `project` verilirse proje sohbeti).
    fn open_direct_in(&mut self, person: Member, project: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        if person.id == self.auth.user.id {
            self.profile = Some(self.me_as_member());
            cx.notify();
            return;
        }
        if let Some(Scope::Direct(open)) = self.current_scope(cx) {
            if self.conversations.iter().any(|c| c.id == open && c.user.id == person.id && c.project_id == project) {
                self.stage = None;
                cx.notify();
                return;
            }
        }
        let (api, org) = (self.api.clone(), self.org.id.clone());
        let user_id = person.id.clone();
        let wanted = project.clone();
        task::run(window, cx, move || api.open_conversation(&org, &user_id, wanted.as_deref()), move |this, result, window, cx| match result {
            // Engel ya da "arkadaş değil + ortak proje yok": sohbet açılmaz, neden söylenir.
            Err(e) if Self::dm_refusal_text(&e.code).is_some() => {
                crate::trace(&format!("dm: @{} açılamadı ({})", person.username, e.code));
                crate::toasts::warn(Self::dm_refusal_text(&e.code).unwrap_or_default())
            }
            Ok(conversation) => {
                match &project {
                    // Ana sayfa sohbeti Ana sayfada açılır.
                    None => this.enter_home(window, cx),
                    // Proje sohbeti: bu arada başka yere geçildiyse açılmaz.
                    Some(p) if this.home.open || this.project_id().as_deref() != Some(p.as_str()) => return,
                    Some(_) => {}
                }
                crate::trace(&format!("dm: @{} açıldı ({})", person.username, if project.is_some() { "proje" } else { "ana sayfa" }));
                match this.conversations.iter_mut().find(|c| c.id == conversation.id) {
                    Some(c) => {
                        c.unread = 0;
                        c.hidden = false;
                    }
                    None => this.conversations.insert(0, Conversation { unread: 0, hidden: false, ..conversation.clone() }),
                }
                // Liste satırındaki durum daha güncel olabilir (gerçek zamanlı).
                let subject = Member { status: person.status.clone(), role: person.role.clone(), title: person.title.clone(), ..conversation.user.clone() };
                let target = ChatTarget::Direct { org: this.org.id.clone(), conversation: conversation.id };
                this.open_chat(target, ChatSubject::Direct(subject), window, cx);
            }
            Err(e) => this.fail(e, window, cx),
        });
    }

    /// Sohbeti kendi listemden kaldırır (mesajlar silinmez). Yeni mesaj gelince ya da sohbeti yeniden
    /// açınca listeye geri döner.
    pub(super) fn hide_conversation(&mut self, conversation_id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.nav_menu = None;
        let Some(c) = self.conversations.iter_mut().find(|c| c.id == conversation_id) else { return };
        c.hidden = true;
        c.unread = 0;
        crate::trace(&format!("dm: @{} listeden kaldırıldı", c.user.username));
        if self.current_scope(cx) == Some(Scope::Direct(conversation_id.clone())) {
            self.close_chat(cx);
        }
        cx.notify();
        let (api, org) = (self.api.clone(), self.org.id.clone());
        let id = conversation_id.clone();
        task::run(window, cx, move || api.hide_conversation(&org, &id, true), move |this, result, window, cx| {
            if let Err(e) = result {
                if let Some(c) = this.conversations.iter_mut().find(|c| c.id == conversation_id) {
                    c.hidden = false;
                }
                this.fail(e, window, cx);
            }
        });
    }

    /// Bireysel arama: `project` verilirse o projenin sohbeti kapsamında (arama kaydı oraya düşer).
    pub(super) fn start_direct_call(&mut self, user_id: &str, project: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        crate::trace(&format!("arama: bireysel arama isteği ({})", if project.is_some() { "proje" } else { "ana sayfa" }));
        self.calls.update(cx, |c, cx| c.start_direct(user_id, project.as_deref(), window, cx));
        cx.notify();
    }

    /// Kimliği verilen grup (seçili projenin grupları + Ana sayfa grupları).
    pub(super) fn group_by_id(&self, group_id: &str) -> Option<&ChatGroup> {
        self.groups.iter().chain(self.home.groups.iter()).find(|g| g.id == group_id)
    }

    fn select_group(&mut self, group: ChatGroup, window: &mut Window, cx: &mut Context<Self>) {
        if self.current_scope(cx) == Some(Scope::Group(group.id.clone())) {
            self.stage = None;
            cx.notify();
            return;
        }
        for g in self.groups.iter_mut().chain(self.home.groups.iter_mut()).filter(|g| g.id == group.id) {
            g.unread = 0;
        }
        let target = ChatTarget::Group { org: self.org.id.clone(), group: group.id.clone() };
        self.open_chat(target, ChatSubject::Group(group), window, cx);
    }

    fn leave_group(&mut self, group_id: String, window: &mut Window, cx: &mut Context<Self>) {
        let (api, org) = (self.api.clone(), self.org.id.clone());
        let id = group_id.clone();
        task::run(window, cx, move || api.leave_group(&org, &id), move |this, result, _, cx| match result {
            Ok(()) => {
                crate::trace("grup: ayrıldın");
                this.remove_group(&group_id, cx);
            }
            Err(e) => crate::toasts::error(if e.message.is_empty() { p2_i18n::t("toast.failed") } else { e.message }),
        });
    }

    fn remove_group(&mut self, group_id: &str, cx: &mut Context<Self>) {
        self.groups.retain(|g| g.id != group_id);
        self.home.groups.retain(|g| g.id != group_id);
        if self.current_scope(cx) == Some(Scope::Group(group_id.to_string())) {
            self.close_chat(cx);
        }
        cx.notify();
    }

    fn open_creator(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.project_id() else { return };
        let me = self.auth.user.id.clone();
        let candidates: Vec<Member> = self.members.iter().filter(|m| m.id != me).cloned().collect();
        let creator = cx.new(|cx| GroupCreator::new(self.api.clone(), self.org.id.clone(), project, candidates, window, cx));
        let sub = cx.subscribe_in(&creator, window, |this, _, event: &GroupCreatorEvent, window, cx| {
            this.creator = None;
            if let GroupCreatorEvent::Created(group) = event {
                crate::trace(&format!("grup: oluşturuldu «{}» ({} üye)", group.name, group.members.len()));
                if !this.groups.iter().any(|g| g.id == group.id) {
                    this.groups.insert(0, group.clone());
                }
                this.select_group(group.clone(), window, cx);
            }
            cx.notify();
        });
        self.creator = Some((creator, sub));
        cx.notify();
    }

    // ------------------------------------------------------------ profil, ayarlar, durum

    fn me_as_member(&self) -> Member {
        let u = &self.auth.user;
        Member { id: u.id.clone(), username: u.username.clone(), display_name: u.display_name.clone(), title: u.title.clone(), avatar_url: u.avatar_url.clone(), status: self.my_status.clone(), role: if self.my_perms.owner { "OWNER".into() } else { "MEMBER".into() }, email: Some(u.email.clone()) }
    }

    fn find_member(&self, user_id: &str) -> Option<Member> {
        if user_id == self.auth.user.id {
            return Some(self.me_as_member());
        }
        self.members
            .iter()
            .find(|m| m.id == user_id)
            .cloned()
            .or_else(|| self.conversations.iter().find(|c| c.user.id == user_id).map(|c| c.user.clone()))
            .or_else(|| self.home.data.friends.iter().find(|f| f.id == user_id).map(|f| f.member()))
            .or_else(|| self.groups.iter().chain(self.home.groups.iter()).flat_map(|g| g.members.iter()).find(|m| m.id == user_id).cloned())
    }

    fn open_profile_by_id(&mut self, user_id: &str, cx: &mut Context<Self>) {
        if let Some(member) = self.find_member(user_id) {
            crate::trace(&format!("profil: @{}", member.username));
            self.profile = Some(member);
            cx.notify();
        }
    }

    fn open_profile(&mut self, username: &str, _by_name: bool, cx: &mut Context<Self>) {
        let id = self.members.iter().find(|m| m.username == username).map(|m| m.id.clone()).or_else(|| (self.auth.user.username == username).then(|| self.auth.user.id.clone()));
        if let Some(id) = id {
            self.open_profile_by_id(&id, cx);
        }
    }

    /// Seçili projenin yönetim paneli: önce o projedeki etkin yetkilerim alınır (yetkisiz sekmeler hiç görünmez).
    pub(super) fn open_admin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.user_menu = false;
        self.project_menu = false;
        let Some(project) = self.project.map(|ix| self.projects[ix].clone()) else { return };
        let (api, pid) = (self.api.clone(), project.id.clone());
        task::run(window, cx, move || api.project_permissions(&pid), move |this, r, window, cx| match r {
            Ok(perms) => this.mount_admin(super::admin::AdminScope::Project(project.clone()), perms, window, cx),
            Err(e) => this.fail(e, window, cx),
        });
    }

    /// Sunucu yönetimi (`server_admin.rs`): kurulum sahibi ya da herhangi bir `server.*` yetkisi olan.
    /// Panel uygulama kökünde açılır (başlangıç ekranında da çalışsın diye).
    pub(super) fn open_instance_admin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.user_menu = false;
        self.nav_menu = None;
        let app = self.app.clone();
        window.defer(cx, move |window, cx| {
            let _ = app.update(cx, |app, cx| app.open_server_admin(None, window, cx));
        });
    }

    /// Hiç proje kalmadı: uygulama başlangıç ekranına döner (bildirim varsa gösterilir).
    fn back_to_start(&mut self, note: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        crate::trace("ray: proje kalmadı, başlangıç ekranına dönülüyor");
        let app = self.app.clone();
        window.defer(cx, move |window, cx| {
            let _ = app.update(cx, |app, cx| {
                app.show_onboarding(window, cx);
                if let Some(n) = note {
                    crate::toasts::info(n);
                }
            });
        });
    }

    /// Sunucu yönetimi: kurulum adı değişti.
    pub(crate) fn instance_renamed(&mut self, name: String, cx: &mut Context<Self>) {
        self.org.name = name;
        cx.notify();
    }

    /// Başlık çubuğu: Keşfet açıkken sayfa adı "Keşfet".
    pub(crate) fn discover_open(&self) -> bool {
        self.discover.is_some()
    }

    /// Başlık çubuğu kökte çizilir: sayfa adı değişince (Keşfet açıldı / kapandı) kök yeniden çizilsin.
    pub(super) fn refresh_titlebar(&self, cx: &mut Context<Self>) {
        let app = self.app.clone();
        cx.defer(move |cx| {
            let _ = app.update(cx, |_, cx| cx.notify());
        });
    }

    pub(crate) fn org_id(&self) -> &str {
        &self.org.id
    }

    /// Sunucu yönetiminden girilen proje raya eklenir ve seçilir.
    pub(crate) fn enter_project(&mut self, project: Project, window: &mut Window, cx: &mut Context<Self>) {
        self.adopt_project(project, window, cx);
    }

    /// `server:permissions`: sunucu yetkilerim (proje oluşturma, kalkan) değişti.
    pub(crate) fn set_server_permissions(&mut self, permissions: Vec<String>, cx: &mut Context<Self>) {
        self.auth.user.server_permissions = permissions;
        cx.notify();
    }

    fn mount_admin(&mut self, scope: super::admin::AdminScope, perms: p2_client_core::MyPermissions, window: &mut Window, cx: &mut Context<Self>) {
        let (api, org, me) = (self.api.clone(), self.org.clone(), self.auth.user.id.clone());
        let panel = cx.new(|cx| super::admin::AdminPanel::new(api, org, scope, me, perms, window, cx));
        let sub = cx.subscribe_in(&panel, window, |this, _, event: &super::admin::AdminEvent, window, cx| match event {
            super::admin::AdminEvent::Close => {
                this.admin = None;
                crate::trace("yönetim: kapandı");
                cx.notify();
            }
            super::admin::AdminEvent::Renamed(name) => {
                this.org.name = name.clone();
                cx.notify();
            }
            // F: kalıcı silme sonucu (`p2:toast`).
            super::admin::AdminEvent::Toast(text) => crate::toasts::ok(text),
            super::admin::AdminEvent::ProjectUpdated(saved) => {
                if let Some(ix) = this.projects.iter().position(|p| p.id == saved.id) {
                    this.projects[ix] = saved.clone();
                    if this.project == Some(ix) {
                        crate::titlebar::set_project_title(&saved.name);
                    }
                }
                // Ad değişmiş olabilir; davet kodu ve yetkiler aynı kalır.
                let _ = window;
                cx.notify();
            }
        });
        self.admin = Some((panel, sub));
        cx.notify();
    }

    fn open_settings(&mut self, section: Section, window: &mut Window, cx: &mut Context<Self>) {
        self.user_menu = false;
        if let Some((settings, _)) = &self.settings {
            settings.update(cx, |s, cx| s.open(section, cx));
            return;
        }
        let modal = cx.new(|cx| SettingsModal::new(self.api.clone(), self.app.clone(), self.calls.clone(), self.auth.user.clone(), self.preference.clone(), section, window, cx));
        let sub = cx.subscribe_in(&modal, window, |this, _, event: &SettingsEvent, window, cx| match event {
            SettingsEvent::Close => this.close_settings(window, cx),
            SettingsEvent::ProfileSaved(user) => this.profile_saved(user.clone(), cx),
            SettingsEvent::Presence(status) => this.choose_presence(status, cx),
            SettingsEvent::PrefsChanged => this.apply_prefs(cx),
            SettingsEvent::LibraryRoot(root) => {
                this.patcher.set_root(root);
                crate::trace(&format!("kütüphane: konum {root}"));
            }
        });
        let root = self.patcher.state().root.clone();
        modal.update(cx, |m, _| m.library_root = root);
        // Pencere kurulurken (abonelikten önce) değişen tercihler de uygulansın.
        cx.spawn(async move |this, cx| {
            let _ = this.update(cx, |this, cx| this.apply_prefs(cx));
        })
        .detach();
        crate::trace(&format!("ayarlar: açıldı ({section:?})"));
        self.settings = Some((modal, sub));
        if section == Section::Media && std::env::var_os("P2_AUTOPILOT_MIC_TEST").is_some() {
            self.calls.update(cx, |c, cx| c.set_mic_test(true, false, window, cx));
        }
        cx.notify();
    }

    /// Ayarlar kapanınca açık mikrofon testi de durur.
    fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.settings = None;
        if self.calls.read(cx).mic_test {
            self.calls.update(cx, |c, cx| c.set_mic_test(false, false, window, cx));
        }
        cx.notify();
    }

    fn profile_saved(&mut self, user: p2_client_core::User, cx: &mut Context<Self>) {
        self.auth.user = user.clone();
        if let Some(m) = self.members.iter_mut().find(|m| m.id == user.id) {
            m.display_name = user.display_name.clone();
            m.title = user.title.clone();
        }
        if self.profile.as_ref().is_some_and(|m| m.id == user.id) {
            self.profile = Some(self.me_as_member());
        }
        let _ = self.app.update(cx, |app, _| {
            if let Some(auth) = &mut app.auth {
                auth.user = user;
            }
        });
        cx.notify();
    }

    fn choose_presence(&mut self, status: &str, cx: &mut Context<Self>) {
        crate::trace(&format!("durum: tercih {status}"));
        self.preference = status.to_string();
        self.realtime.emit("presence:prefer", json!({ "status": status }));
        cx.notify();
    }

    // ------------------------------------------------------------ gerçek zamanlı

    fn on_event(&mut self, event: RtEvent, window: &mut Window, cx: &mut Context<Self>) {
        // Arama olayları arama denetçisine gider (sinyaller izlenmez: çok sık).
        match &event {
            RtEvent::CallIncoming { .. }
            | RtEvent::CallState { .. }
            | RtEvent::CallSignal { .. }
            | RtEvent::CallEnded { .. }
            | RtEvent::CallDeclined { .. }
            | RtEvent::CallForceMute { .. }
            | RtEvent::VoicePresence { .. } => {
                if let RtEvent::CallState { call } = &event {
                    crate::trace(&format!("olay: arama durumu {} ({} kişi)", call.kind, call.participants.len()));
                }
                match &event {
                    RtEvent::CallIncoming { call, from_name, .. } => {
                        self.overlay_incoming(call, from_name);
                        crate::desktop_notify(&p2_i18n::tv("ws.call.incoming", &[("name", from_name.as_str())]), &p2_i18n::t("ws.call.incoming.body"));
                    }
                    RtEvent::CallEnded { call_id, .. } => self.overlay_call_over(call_id),
                    RtEvent::CallState { call } if call.has(&self.auth.user.id) => self.overlay_call_over(&call.id),
                    _ => {}
                }
                let accept = matches!(event, RtEvent::CallIncoming { .. }) && std::env::var_os("P2_AUTOPILOT_ACCEPT").is_some();
                self.calls.update(cx, |c, cx| {
                    c.on_rt(&event, window, cx);
                    if accept {
                        c.accept(window, cx);
                    }
                });
                return;
            }
            RtEvent::Connected => self.calls.update(cx, |c, cx| c.on_rt(&event, window, cx)),
            RtEvent::Board { event: name, payload } => {
                if let Some(board) = &self.board {
                    board.update(cx, |b, cx| b.on_event(name, payload, window, cx));
                }
                return;
            }
            // Bireysel arama başlatılamadı (yetki yok / engel): sessiz kalmaz, neden söylenir.
            RtEvent::Other { event: name, payload } if name == "call:refused" => {
                crate::trace(&format!("arama: reddedildi ({})", payload["code"].as_str().unwrap_or_default()));
                crate::toasts::push(p2_ui::controls::ToastTone::Error, None, &p2_i18n::t("call.refused"));
                self.calls.update(cx, |c, cx| c.start_refused(window, cx));
                return;
            }
            RtEvent::Other { event: name, payload } if name == "activity:new" => {
                if let Ok(item) = serde_json::from_value::<p2_client_core::ActivityItem>(payload["activity"].clone()) {
                    self.overlay_activity(&item); // J: oyun içi "görev atadı" kartı
                    self.on_activity(item, cx);
                }
                return;
            }
            // J: oyun içi "oyuna girdi/çıktı" kartları (overlayNotify.ts play / play-stop).
            RtEvent::Other { event: name, payload } if name == "presence:activity" => self.overlay_presence_activity(payload),
            RtEvent::Other { event: name, payload } if name.starts_with("release:") || name == "update:published" => {
                self.on_release_event(name, payload, cx);
                return;
            }
            RtEvent::Other { event: name, payload } if name == "server:permissions" => {
                let perms: Vec<String> = serde_json::from_value(payload["serverPermissions"].clone()).unwrap_or_default();
                crate::trace(&format!("sunucu yetkileri: {}", perms.join(",")));
                self.auth.user.server_permissions = perms.clone();
                let app = self.app.clone();
                window.defer(cx, move |_, cx| {
                    let _ = app.update(cx, |app, cx| app.server_permissions_changed(perms, cx));
                });
                cx.notify();
                return;
            }
            RtEvent::Other { event: name, payload } if name == "server:banned" => {
                let reason = payload["reason"].as_str().map(str::to_string);
                crate::trace("sunucu: yasaklandım, oturum kapanıyor");
                let app = self.app.clone();
                window.defer(cx, move |window, cx| {
                    let _ = app.update(cx, |app, cx| app.server_banned(reason, window, cx));
                });
                return;
            }
            RtEvent::Other { event: name, payload } if name.starts_with("project:") || name == "permissions:changed" => {
                self.on_project_event(name, payload, window, cx);
                return;
            }
            RtEvent::Other { event: name, payload } if name.starts_with("friend:") => {
                self.on_friend_event(name, payload, window, cx);
                return;
            }
            RtEvent::Other { event: name, payload } if name.starts_with("document:") => {
                if let Some(docs) = &self.documents {
                    docs.update(cx, |d, cx| d.on_event(name, payload, window, cx));
                }
                return;
            }
            _ => {}
        }
        let where_ = |scope: &Scope| match scope {
            Scope::Channel(_) => "kanal",
            Scope::Direct(_) => "dm",
            Scope::Group(_) => "grup",
        };
        crate::trace(&match &event {
            RtEvent::MessageCreated { message, scope, .. } => format!("olay: mesaj geldi «{}» ({}) [{}]", message.content, message.author.username, where_(scope)),
            RtEvent::MessageUpdated { message, scope, .. } => format!("olay: mesaj düzenlendi «{}» [{}]", message.content, where_(scope)),
            RtEvent::MessageDeleted { message_id, scope, .. } => format!("olay: mesaj silindi {message_id} [{}]", where_(scope)),
            RtEvent::Reactions { rows, scope, .. } => format!("olay: tepkiler {rows:?} [{}]", where_(scope)),
            RtEvent::Presence { user_id, status } => format!("olay: durum {user_id} {status}"),
            RtEvent::PresenceSelf { preference, status } => format!("olay: kendi durumum {preference}/{status}"),
            RtEvent::Typing { display_name, typing, scope, .. } => format!("olay: yazıyor {display_name} {typing} [{}]", where_(scope)),
            RtEvent::ConversationOpened { hidden: Some(h), .. } => format!("olay: dm {}", if *h { "listeden kaldırıldı" } else { "listeye döndü" }),
            RtEvent::ConversationOpened { .. } => "olay: yeni dm".into(),
            RtEvent::GroupChanged { group } => format!("olay: grup «{}» ({} üye)", group.name, group.members.len()),
            RtEvent::GroupLeft { group_id } => format!("olay: gruptan çıkıldı {group_id}"),
            RtEvent::Connected => "olay: gerçek zamanlı bağlandı".into(),
            RtEvent::Disconnected => "olay: gerçek zamanlı koptu".into(),
            RtEvent::Other { event, .. } => format!("olay: {event}"),
            _ => "olay: arama".into(),
        });
        // L ajanı: bölünmüş bölme ve ayrı pencereler de olayı alır.
        self.forward_side(&event, window, cx);
        let current = self.current_scope(cx);
        let open = |scope: &Scope| current.as_ref() == Some(scope);
        let me = self.auth.user.id.clone();
        if let RtEvent::MessageCreated { scope, parent_id: Some(_), .. } | RtEvent::MessageUpdated { scope, parent_id: Some(_), .. } | RtEvent::MessageDeleted { scope, parent_id: Some(_), .. } = &event {
            let is_open = open(scope);
            self.route_reply(&event, is_open, cx);
            return;
        }
        match event {
            RtEvent::Connected => {
                self.connected = true;
                if let Ok(status) = std::env::var("P2_AUTOPILOT_PRESENCE") {
                    self.choose_presence(&status, cx);
                }
                self.autopilot_call(window, cx);
                cx.notify();
            }
            RtEvent::Disconnected => {
                self.connected = false;
                cx.notify();
            }
            RtEvent::MessageCreated { scope, parent_id: None, message } => {
                // Yeni mesaj, listeden kaldırılmış sohbeti geri getirir (sunucu da bayrağı indirir).
                if let Scope::Direct(id) = &scope {
                    if let Some(c) = self.conversations.iter_mut().find(|c| &c.id == id) {
                        if c.hidden {
                            c.hidden = false;
                            crate::trace(&format!("dm: @{} listeye döndü", c.user.username));
                        }
                        // İlk mesaj / arama kaydı taslak sohbeti listeye alır.
                        c.last_message = Some(p2_client_core::LastMessage { id: message.id.clone(), content: message.content.clone(), created_at: message.created_at.clone(), author_id: message.author.id.clone() });
                        c.updated_at = message.created_at.clone();
                    }
                }
                if message.author.id != me {
                    let mention = format!("@{}", self.auth.user.username);
                    let personal = match &scope {
                        Scope::Channel(ch) => match self.notify_prefs.get(ch).map(String::as_str).unwrap_or("MENTIONS") {
                            "ALL" => true,
                            "MUTED" => false,
                            _ => message.content.contains(&mention) || message.content.contains("@everyone"),
                        },
                        _ => true,
                    };
                    if personal {
                        let who = if message.author.display_name.is_empty() { message.author.username.clone() } else { message.author.display_name.clone() };
                        let (_, files) = (&message.content, ());
                        let _ = files;
                        crate::desktop_notify(&who, &message.content.lines().filter(|l| !l.starts_with("[[p2-file|")).collect::<Vec<_>>().join(" "));
                        // Workspace.tsx 'message:created': etiketlendiğim (sessize alınmamış) ve bakmadığım kanalda ses.
                        if matches!(scope, Scope::Channel(_)) && !open(&scope) {
                            self.message_sound(window, cx);
                        }
                    }
                }
                if open(&scope) {
                    if let Some(chat) = &self.chat {
                        chat.update(cx, |chat, cx| chat.on_created(message, window, cx));
                    }
                } else if message.author.id != me {
                    self.overlay_message(&scope, &message);
                    self.count_unread(&scope, window, cx);
                    // Başka projenin kanalı: ray rozeti sunucudan yenilenir (olayda proje kimliği yok).
                    if let Scope::Channel(ch) = &scope {
                        if !self.categories.iter().any(|c| c.channels.iter().any(|x| &x.id == ch)) {
                            self.refresh_project_unread(window, cx);
                        }
                    }
                }
            }
            RtEvent::MessageUpdated { scope, parent_id: None, message } if open(&scope) => {
                if let Some(chat) = &self.chat {
                    chat.update(cx, |chat, cx| chat.on_updated(message, cx));
                }
            }
            RtEvent::MessageDeleted { scope, message_id, parent_id: None } if open(&scope) => {
                if let Some(chat) = &self.chat {
                    chat.update(cx, |chat, cx| chat.on_deleted(&message_id, cx));
                }
            }
            RtEvent::Reactions { scope, message_id, rows } if open(&scope) => {
                if let Some(chat) = &self.chat {
                    chat.update(cx, |chat, cx| chat.on_reactions(&message_id, &rows, cx));
                }
            }
            RtEvent::Typing { scope, user_id, display_name, typing } if open(&scope) => {
                if let Some(chat) = &self.chat {
                    chat.update(cx, |chat, cx| chat.on_typing(user_id, display_name, typing, window, cx));
                }
            }
            RtEvent::Presence { user_id, status } => {
                if let Some(m) = self.members.iter_mut().find(|m| m.id == user_id) {
                    m.status = status.clone();
                    sort_members(&mut self.members);
                }
                for c in self.conversations.iter_mut().filter(|c| c.user.id == user_id) {
                    c.user.status = status.clone();
                }
                if let Some(p) = self.profile.as_mut().filter(|p| p.id == user_id) {
                    p.status = status.clone();
                }
                self.home.set_status(&user_id, &status);
                if let Some((dialog, _)) = &self.new_message {
                    dialog.update(cx, |d, cx| {
                        d.set_status(&user_id, &status);
                        cx.notify();
                    });
                }
                if user_id == me {
                    self.my_status = status.clone();
                }
                if let Some(chat) = &self.chat {
                    chat.update(cx, |chat, cx| chat.set_peer_status(&user_id, &status, cx));
                }
                cx.notify();
            }
            RtEvent::PresenceSelf { preference, status } => {
                self.preference = preference.clone();
                self.my_status = status;
                if let Some((settings, _)) = &self.settings {
                    settings.update(cx, |s, cx| s.set_preference(&preference, cx));
                }
                cx.notify();
            }
            RtEvent::ConversationOpened { conversation_id, hidden, .. } => {
                // Başka oturumumda listeden kaldırdıysam burada da açık sohbet kapanır.
                if hidden == Some(true) && self.current_scope(cx) == Some(Scope::Direct(conversation_id)) {
                    self.close_chat(cx);
                }
                self.load_conversations(window, cx)
            }
            RtEvent::GroupChanged { group } => {
                // Ana sayfa grupları her zaman tutulur (ray rozeti); proje grupları yalnız seçili projede.
                let list = if group.home {
                    Some(&mut self.home.groups)
                } else if group.project_id.is_some() && group.project_id == self.project_id() {
                    Some(&mut self.groups)
                } else {
                    None
                };
                if let Some(list) = list {
                    match list.iter_mut().find(|g| g.id == group.id) {
                        Some(g) => *g = ChatGroup { unread: g.unread, last_message: g.last_message.clone().or(group.last_message.clone()), ..group.clone() },
                        None => list.insert(0, group.clone()),
                    }
                    if self.group_members.as_ref().is_some_and(|g| g.id == group.id) {
                        self.group_members = Some(group.clone());
                    }
                    if let Some(chat) = &self.chat {
                        chat.update(cx, |chat, cx| chat.set_group(group, cx));
                    }
                    cx.notify();
                }
            }
            RtEvent::GroupLeft { group_id } => self.remove_group(&group_id, cx),
            _ => {}
        }
    }

    /// Açık olmayan sohbete mesaj: rozet artar.
    fn count_unread(&mut self, scope: &Scope, window: &mut Window, cx: &mut Context<Self>) {
        match scope {
            Scope::Channel(id) => {
                if let Some(c) = self.categories.iter_mut().flat_map(|c| c.channels.iter_mut()).find(|c| &c.id == id) {
                    c.unread += 1;
                }
            }
            Scope::Direct(id) => match self.conversations.iter_mut().find(|c| &c.id == id) {
                Some(c) => {
                    c.unread += 1;
                    crate::trace(&format!("okunmamış: @{} {} ({})", c.user.username, c.unread, if c.is_home() { "ana sayfa" } else { "proje" }));
                    // notifications.ts playMessageSound: bireysel mesajda kısa "ding".
                    self.message_sound(window, cx);
                }
                // Bilinmeyen sohbet: karşı taraf ilk kez yazdı.
                None => self.load_conversations(window, cx),
            },
            Scope::Group(id) => {
                if let Some(g) = self.groups.iter_mut().chain(self.home.groups.iter_mut()).find(|g| &g.id == id) {
                    g.unread += 1;
                    crate::trace(&format!("okunmamış: grup «{}» {}", g.name, g.unread));
                    // Workspace.tsx 'group:message': açık olmayan gruba gelen mesajda ses.
                    self.message_sound(window, cx);
                }
            }
        }
        cx.notify();
    }

    /// notifications.ts `playMessageSound` (C ajanı): Ayarlar > Bildirimler "ses" açıksa kısa "ding".
    /// Ses motoru `P2_NO_AUDIO_DEVICE` altında aygıt açmaz (p2-media audio.rs).
    pub(super) fn message_sound(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !p2_client_core::store::load_settings().app.notification_sound {
            return;
        }
        crate::trace("bildirim: mesaj sesi");
        self.calls.update(cx, |calls, cx| calls.play_cue(p2_media::Cue::Message, window, cx));
    }

    // ------------------------------------------------------------ araç sayfaları

    /// Seçili projenin görev panosu (Workspace.tsx `setBoardOpen(true)`).
    fn open_board(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.project else { return };
        let project = self.projects[ix].clone();
        // Pano yönetimi projeye göre: projenin sahibi ya da yönetim yetkisi olan.
        let manage = self.my_perms.owner || self.my_perms.can("admin.panel");
        let (api, org, me, members) = (self.api.clone(), self.org.id.clone(), self.auth.user.id.clone(), self.members.clone());
        let app = self.app.clone();
        self.library = None;
        self.documents = None;
        self.board = Some(cx.new(|cx| BoardPage::new(app, api, org, project.id.clone(), project.name.clone(), me, members, manage, window, cx)));
        self.profile = None;
        crate::trace("sayfa: pano");
        cx.notify();
    }

    /// Kütüphane (seçili proje öne alınır).
    /// `p2://play/<proje>` (main.cjs `playFromArgs`): kuruluysa başlat, değilse kütüphanede aç.
    pub fn play_from_link(&mut self, project: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.loading {
            self.pending_play = Some(project.to_string());
            return;
        }
        let Some(ix) = self.projects.iter().position(|p| p.id == project) else {
            crate::toasts::warn(p2_i18n::t("toast.linkProjectMissing"));
            return;
        };
        let name = self.projects[ix].name.clone();
        if self.patcher.state().projects.contains_key(project) {
            match self.patcher.launch(&self.org.id, project, &name) {
                Ok(()) => crate::trace(&format!("kütüphane: bağlantıdan başlatıldı {name}")),
                Err(e) => crate::toasts::error(e),
            }
        } else {
            self.project = Some(ix);
            crate::titlebar::set_project_title(&name);
            self.open_library(window, cx);
            crate::trace(&format!("kütüphane: bağlantı — {name} kurulu değil, kütüphane açıldı"));
        }
    }

    fn open_library(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.projects.is_empty() {
            return;
        }
        let ix = self.project.unwrap_or(0);
        let (api, patcher, org, me, projects) = (self.api.clone(), self.patcher.clone(), self.org.id.clone(), self.auth.user.id.clone(), self.projects.clone());
        self.board = None;
        self.documents = None;
        let page = cx.new(|cx| LibraryPage::new(api, patcher, org, projects, ix, me, window, cx));
        // Kütüphane istekleri (H): ayarlar, proje ayarları, pano, proje seçimi (LibraryPage.tsx props).
        cx.subscribe_in(&page, window, |this, _, e: &super::library::LibraryEvent, window, cx| {
            use super::library::LibraryEvent as L;
            let find = |this: &Self, id: &str| this.projects.iter().position(|p| p.id == id);
            match e {
                L::OpenSettings => this.open_settings(Section::Library, window, cx),
                L::OpenBoard => this.open_board(window, cx),
                L::SelectProject(id) => {
                    if let Some(ix) = find(this, id) {
                        this.select_project(ix, window, cx);
                    }
                }
                L::ProjectSettings(id) => {
                    if let Some(ix) = find(this, id) {
                        if this.project != Some(ix) {
                            this.select_project(ix, window, cx);
                        }
                        this.open_dialog(manage_ui::Dialog::Settings, window, cx);
                    }
                }
            }
        })
        .detach();
        // Otomatik pilot: P2_AUTOPILOT_UPLOAD = "klasör|exe|kategori|sürüm"
        if let Ok(spec) = std::env::var("P2_AUTOPILOT_UPLOAD") {
            let parts: Vec<String> = spec.split('|').map(str::to_string).collect();
            if parts.len() == 4 {
                page.update(cx, |p, cx| p.autopilot_upload(std::path::PathBuf::from(&parts[0]), &parts[1], &parts[2], &parts[3], window, cx));
            }
        }
        self.library = Some(page);
        self.profile = None;
        crate::trace("sayfa: kütüphane");
        cx.notify();
    }

    fn on_patch_event(&mut self, event: p2_patcher::Event, cx: &mut Context<Self>) {
        use p2_patcher::Event as E;
        if let E::Job(j) = &event {
            if !j.active {
                crate::trace(&format!("kütüphane: iş {} indirilen={} yazılan={} {}", j.phase, j.bytes_done, j.bytes_to_write, j.error.clone().unwrap_or_default()));
            }
        }
        if let E::Upload(u) = &event {
            if matches!(u.phase.as_str(), "registering" | "ready") {
                crate::trace(&format!("kütüphane: yükleme {} ({}/{})", u.phase, u.done, u.total));
            }
        }
        if let E::Played { seconds, .. } = &event {
            crate::trace(&format!("kütüphane: oyun kapandı ({seconds} sn)"));
        }
        if let E::Job(j) = &event {
            self.auto_on_job(j, cx);
        }
        if let E::Running { project_id, running } = &event {
            self.launch_card_on_running(project_id, *running, cx);
        }
        let Some(page) = &self.library else { return };
        page.update(cx, |p, cx| match event {
            E::Job(j) => p.on_job(j, cx),
            E::Upload(u) => p.on_upload(u, cx),
            E::Running { project_id, running } => p.on_running(project_id, running, cx),
            E::Played { .. } | E::State | E::Crash { .. } => p.on_state(cx),
        });
    }

    /// Seçili projenin dokümanları.
    fn open_documents(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ix) = self.project else { return };
        let project = self.projects[ix].clone();
        let (api, org, me, members) = (self.api.clone(), self.org.id.clone(), self.auth.user.id.clone(), self.members.clone());
        self.board = None;
        self.library = None;
        self.documents = Some(cx.new(|cx| DocumentsPage::new(api, org, project.id.clone(), project.name.clone(), me, members, window, cx)));
        self.profile = None;
        crate::trace("sayfa: dokümanlar");
        cx.notify();
    }
}

impl Render for WorkspaceScreen {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        crate::count_render("çalışma alanı");
        let p = cx.p2().clone();
        let mut chat_style = StyleRefinement::default();
        chat_style.flex_grow = Some(1.);
        chat_style.min_size.height = Some(px(0.).into());
        chat_style.size.width = Some(relative(1.).into());

        // I ajanı: sesli oda (başlık + VoiceRoomStage [+ oda sohbeti]); değilse sohbet.
        let main: AnyElement = if let Some(voice) = self.voice_main(&chat_style, &p, window, cx) {
            voice
        } else { match &self.chat {
            // Sohbet önbellekli; yazma kutusu onun kardeşi (bkz. ChatPane::composer_bar).
            Some(chat) => div()
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .flex()
                        .flex_col()
                        .child(chat.clone().cached(chat_style))
                        .child(chat.read(cx).composer_bar(&p, chat.downgrade(), window, cx)),
                )
                .into_any_element(),
            // Ana sayfa: sohbet açık değilken arkadaşlar sayfası.
            None if self.home.open => self.friends_page(&p, cx),
            None => div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(p.text_3)
                .child(p2_i18n::t(if self.loading {
                    "ws.loading"
                } else if self.projects.is_empty() {
                    "ws.noProject"
                } else {
                    "ws.pickChat"
                }))
                .into_any_element(),
        } };
        let profile = self.profile.clone().map(|m| self.profile_panel(&m, &p, cx));
        let incoming = self.incoming_card(&p, cx);

        div()
            .size_full()
            .relative()
            .flex()
            .bg(p.bg_0)
            .key_context("Workspace")
            .on_key_down(cx.listener(|this, event: &gpui_kit::KeyDownEvent, window, cx| {
                let k = &event.keystroke;
                if k.key == "escape" {
                    // I ajanı: kişi menüsü, ekran paylaşımı penceresi, büyütülmüş arama paneli.
                    if this.call_ui_escape(cx) {
                        cx.notify();
                        return;
                    }
                    if this.discover_escape(cx) {
                        return;
                    }
                    if this.nav_menu.take().is_some() || this.people_confirm.take().is_some() || std::mem::take(&mut this.activity_open) || std::mem::take(&mut this.user_menu) || this.group_members.take().is_some() || this.manage.take().is_some() || this.channel_perms.take().is_some() || this.channel_menu.take().is_some() || std::mem::take(&mut this.project_menu) || this.admin.take().is_some() || this.join.take().is_some() || this.forwarding.take().is_some() || this.browser.take().is_some() {
                        cx.notify();
                        return;
                    }
                    if this.settings.is_some() {
                        this.close_settings(window, cx);
                    } else if this.creator.is_some() {
                        this.creator = None;
                    } else if this.new_message.is_some() {
                        this.new_message = None;
                    } else if this.profile.is_some() {
                        this.profile = None;
                    } else if let Some(chat) = this.chat.clone() {
                        // C ajanı: sohbetin menü/çekmece/pencereleri (odak yazma kutusunda değilken de).
                        chat.update(cx, |c, cx| c.escape(window, cx));
                        return;
                    } else {
                        return;
                    }
                    cx.notify();
                } else if k.key == "," && k.modifiers.control {
                    this.open_settings(Section::Account, window, cx);
                } else if this.calls.read(cx).ptt_key().as_deref() == Some(k.key.as_str()) {
                    this.calls.update(cx, |c, cx| c.push_to_talk(true, cx));
                }
            }))
            .on_key_up(cx.listener(|this, event: &gpui_kit::KeyUpEvent, _, cx| {
                if this.calls.read(cx).ptt_key().as_deref() == Some(event.keystroke.key.as_str()) {
                    this.calls.update(cx, |c, cx| c.push_to_talk(false, cx));
                }
            }))
            // Raya tıklamak (proje, pano, kütüphane…) açık Keşfet'i kapatır; pusula ve "＋" kendi tıklamasını tutar.
            .child(div().h_full().flex_none().on_mouse_down(gpui_kit::MouseButton::Left, cx.listener(|this, _, _, cx| {
                if this.discover.take().is_some() {
                    this.refresh_titlebar(cx);
                    cx.notify();
                }
            })).child(self.rail(&p, cx)))
            // Pano açıkken kenar çubuğu yok (Electron: ızgara 72px + ana alan).
            .when(self.board.is_none() && self.documents.is_none() && self.library.is_none(), |s| if self.home.open { s.child(self.home_sidebar(&p, cx)) } else { s.child(self.sidebar(&p, cx)) })
            .child(match (&self.board, &self.documents, &self.library) {
                // Sayfa kökleri size_full: sarmalayıcı kalan genişliği verir (yoksa ray kadar taşar).
                (Some(board), _, _) => div().flex_1().min_w_0().h_full().child(board.clone()).into_any_element(),
                (None, Some(docs), _) => div().flex_1().min_w_0().h_full().child(docs.clone()).into_any_element(),
                (None, None, Some(lib)) => div().flex_1().min_w_0().h_full().child(lib.clone()).into_any_element(),
                _ => self.with_split(main, &p, window, cx),
            })
            // I ajanı: arama paneli (`.call-dock`, z 40) ve sesli oda kişi menüsü.
            .children(self.call_dock(&p, window, cx))
            .children(profile)
            .when_some(self.discover.as_ref().map(|c| c.0.clone()), |s, c| s.child(c))
            .when_some(self.creator.as_ref().map(|c| c.0.clone()), |s, c| s.child(c))
            .when_some(self.new_message.as_ref().map(|c| c.0.clone()), |s, c| s.child(c))
            .when_some(self.settings.as_ref().map(|c| c.0.clone()), |s, c| s.child(c))
            .when_some(self.admin.as_ref().map(|c| c.0.clone()), |s, c| s.child(c))
            .when_some(self.join.as_ref().map(|c| c.0.clone()), |s, c| s.child(c))
            .children(incoming)
            .children(self.activity_panel(&p, cx))
            .when_some(self.picker.as_ref().map(|c| c.0.clone()), |s, c| s.child(c))
            // C ajanı: iş parçacığı paneli Electron'daki gibi sağda sabit çekmece (sohbeti daraltmaz),
            // sohbetin açılır öğeleri (menüler, arama çekmecesi, pencereler) kök düzeyde.
            // L ajanı: forum konusu sohbetin içinde çizilir (yan panel değil).
            .when_some(self.thread.as_ref().filter(|t| !t.0.read(cx).forum).map(|t| t.0.clone()), |s, t| s.child(div().absolute().top_0().right_0().bottom_0().child(t)))
            .children(self.chat.as_ref().map(|c| c.read(cx).floating(&p, c.downgrade(), window, cx)).unwrap_or_default())
            .children(self.split_floating(&p, window, cx))
            .children(self.forward_modal(&p, cx))
            .children(self.project_menu_view(&p, window, cx))
            .children(self.channel_menu_view(&p, window, cx))
            .children(self.nav_menu_view(&p, window, cx))
            .children(self.voice_menu_view(&p, window, cx))
            .children(self.call_dropdown_view(&p, window, cx))
            .children(self.manage_modal(&p, window, cx))
            .children(self.group_members_modal(&p, window, cx))
            .when_some(self.channel_perms.as_ref().map(|c| c.0.clone()), |s, c| s.child(c))
            .children(self.browser_modal(&p, window, cx))
            .children(self.user_menu_layer(&p, cx))
            .children(self.people_confirm_view(&p, window, cx))
    }
}
