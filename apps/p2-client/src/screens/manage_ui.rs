//! Proje ve kanal yönetimi (Workspace.tsx): proje menüsü (••• `.invite-menu`: kategori/kanal oluştur,
//! kanallara göz at, kişiyi davet et, yönetim paneli, davet kodu), kanal/kategori sağ tık menüsü
//! (`.channel-context-menu`: yıldız, başlangıç kanalı, sil), oluştur/sil pencereleri (`Modal`),
//! proje oluşturucu (`ProjectCreator`), proje ayarları (`ProjectSettingsDialog`: ikon, kapak, kütüphane).

use std::sync::Mutex;
use p2_ui::P2Fx as _; // backdrop_blur
use std::time::{Duration, Instant};

use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, Context, Div, Entity, FontWeight, MouseButton, Pixels, Point, SharedString, Stateful, Subscription, Window, div, px};
use p2_client_core::{Channel, ChannelCategory};
use p2_ui::P2Text as _;
use p2_ui::controls::{BtnSize, BtnVariant, ModalSize, button, kicker, lh, modal, modal_body, modal_foot};
use p2_ui::form::InputKind;
use p2_ui::tokens::{FONT_MONO, Palette, font_size, fs, radius, shadows};
use p2_ui::{Entrance, P2Motion as _};
use serde_json::json;

use super::WorkspaceScreen;
use super::super::admin::channel_icon;
use super::super::channel_perms::{ChannelPermsPane, PermsEvent, Tab};
use crate::task;

#[derive(Clone)]
pub(super) enum Dialog {
    NewProject,
    NewCategory,
    NewChannel(ChannelCategory),
    DeleteChannel(Channel),
    DeleteCategory(ChannelCategory),
    DeleteProject,
    Settings,
    /// Projeden ayrıl (proje topluluğu üyeliği biter).
    LeaveProject,
}

pub(super) struct Manage {
    dialog: Dialog,
    name: Entity<InputState>,
    desc: Entity<TextareaState>,
    kind: &'static str,
    busy: bool,
    error: Option<String>,
    icon: Option<String>,
    cover: Option<String>,
    /// Görsel yükleniyor: "icon" | "cover".
    uploading: Option<bool>,
    library: bool,
    /// Yeni proje: herkese açık mı (Keşfet'te listelenir) ve Keşfet kategorisi.
    public: bool,
    category: Option<&'static str>,
    _subs: Vec<Subscription>,
}

/// Workspace.tsx `channelTypeOptions` (değer, etiket anahtarı, açıklama anahtarı).
const KINDS: [(&str, &str, &str); 4] = [
    ("TEXT", "mng.kind.text", "mng.kind.text.hint"),
    ("ANNOUNCEMENT", "mng.kind.announcement", "mng.kind.announcement.hint"),
    ("VOICE", "mng.kind.voice", "mng.kind.voice.hint"),
    ("FORUM", "mng.kind.forum", "mng.kind.forum.hint"),
];

/// Sağ tık menüsü: kanal satırı ya da (boş) kategori.
pub(super) struct ChannelMenu {
    pub channel: Option<Channel>,
    pub category: Option<ChannelCategory>,
    pub at: Point<Pixels>,
}

impl ChannelMenu {
    pub fn channel(channel: Channel, at: Point<Pixels>) -> Self {
        Self { channel: Some(channel), category: None, at }
    }
    pub fn category(category: ChannelCategory, at: Point<Pixels>) -> Self {
        Self { channel: None, category: Some(category), at }
    }
}

/// "Davet kodu kopyalandı ✓" (1,8 sn) — Workspace.tsx `copied`.
static COPIED: Mutex<Option<Instant>> = Mutex::new(None);

fn copied() -> bool {
    COPIED.lock().unwrap().is_some_and(|t| t.elapsed() < Duration::from_millis(1800))
}

/// Çalışma alanının penceredeki üst kenarı (başlık çubuğu): Electron koordinatları pencereye göre.
const TOP: f32 = crate::titlebar::HEIGHT;

impl WorkspaceScreen {
    pub(super) fn open_dialog(&mut self, dialog: Dialog, window: &mut Window, cx: &mut Context<Self>) {
        self.project_menu = false;
        self.project_menu_categories = false;
        self.channel_menu = None;
        // Proje açmak sunucu yetkisi (`project.create`); sunucu da 403 PROJECT_CREATE_FORBIDDEN döner.
        if matches!(dialog, Dialog::NewProject) && !self.auth.user.can_server("project.create") {
            crate::trace("yönet: proje oluşturma yetkisi yok");
            crate::toasts::warn(p2_i18n::t("rail.createForbidden"));
            return;
        }
        let name = cx.new(|cx| {
            InputState::new(window, cx).placeholder(match &dialog {
                Dialog::NewProject => p2_i18n::t("mng.ph.project"),
                Dialog::NewCategory => p2_i18n::t("mng.ph.category"),
                _ => p2_i18n::t("mng.ph.channel"),
            })
        });
        let desc = cx.new(|cx| TextareaState::new(window, cx).auto_grow(3, 3).placeholder(p2_i18n::t("mng.ph.desc")));
        if matches!(dialog, Dialog::NewProject | Dialog::NewCategory | Dialog::NewChannel(_)) {
            name.update(cx, |s, cx| s.focus(window, cx));
        }
        // Sınırlar: kanal 40, kategori 32, proje 60 karakter (Electron `maxLength`).
        let max = match &dialog {
            Dialog::NewProject => 60,
            Dialog::NewCategory => 32,
            _ => 40,
        };
        let subs = vec![cx.subscribe_in(&name, window, move |_, state, ev: &InputEvent, window, cx| {
            if matches!(ev, InputEvent::Change) {
                let v = state.read(cx).value().to_string();
                if v.chars().count() > max {
                    let cut: String = v.chars().take(max).collect();
                    state.update(cx, |s, cx| s.set_value(cut, window, cx));
                }
            }
            cx.notify();
        })];
        let project = self.project.map(|i| self.projects[i].clone());
        self.manage = Some(Manage {
            dialog,
            name,
            desc,
            kind: "TEXT",
            busy: false,
            error: None,
            icon: project.as_ref().and_then(|p| p.icon_url.clone()),
            cover: project.as_ref().and_then(|p| p.cover_url.clone()),
            uploading: None,
            library: project.as_ref().is_some_and(|p| p.library_enabled),
            public: false,
            category: None,
            _subs: subs,
        });
        cx.notify();
    }

    /// Açık pencerede hata göster (otomatik pilotu ilerletmeden).
    pub(super) fn manage_error(&mut self, message: String, cx: &mut Context<Self>) {
        if let Some(m) = self.manage.as_mut() {
            m.busy = false;
            m.error = Some(message.clone());
        } else {
            crate::toasts::error(message);
        }
        cx.notify();
    }

    fn manage_fail(&mut self, message: String, window: &mut Window, cx: &mut Context<Self>) {
        crate::trace(&format!("yönet: hata {message}"));
        if let Some(m) = self.manage.as_mut() {
            m.busy = false;
            m.uploading = None;
            m.error = Some(message);
        }
        cx.notify();
        self.manage_next(window, cx);
    }

    /// Kanal listesini (açık sohbeti bozmadan) yeniden yükler.
    pub(super) fn reload_channels(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pid) = self.project_id() else { return };
        let (api, org) = (self.api.clone(), self.org.id.clone());
        task::run(window, cx, move || api.channels(&org, &pid), |this, r, _, cx| {
            if let Ok(categories) = r {
                this.categories = categories;
                cx.notify();
            }
        });
    }

    /// İşlem bitti: pencere kapanır, liste tazelenir. Electron bu işlemlerde bildirim göstermez
    /// (`note` yalnız iz satırı).
    fn manage_done(&mut self, note: String, window: &mut Window, cx: &mut Context<Self>) {
        crate::trace(&format!("yönet: {note}"));
        self.manage = None;
        self.reload_channels(window, cx);
        cx.notify();
        self.manage_next(window, cx);
    }

    pub(super) fn manage_submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(m) = self.manage.as_mut() else { return };
        if m.busy {
            return;
        }
        let name = m.name.read(cx).value().trim().to_string();
        let desc = m.desc.read(cx).value().trim().to_string();
        let (kind, icon, cover, library) = (m.kind, m.icon.clone(), m.cover.clone(), m.library);
        let (visibility, category) = (if m.public { p2_client_core::VISIBILITY_PUBLIC } else { p2_client_core::VISIBILITY_PRIVATE }, m.category.filter(|_| m.public));
        let dialog = m.dialog.clone();
        match &dialog {
            Dialog::NewProject if p2_client_core::project_slug(&name).len() < 2 => return,
            Dialog::NewCategory | Dialog::NewChannel(_) if name.is_empty() => return,
            _ => {}
        }
        m.busy = true;
        m.error = None;
        cx.notify();
        let (api, org) = (self.api.clone(), self.org.id.clone());
        let project = self.project_id().unwrap_or_default();
        match dialog {
            Dialog::NewProject => task::run(window, cx, move || api.open_project_with(&name, &desc, Some(&org), visibility, category), |this, r, window, cx| match r {
                Ok(p) => {
                    crate::trace(&format!("yönet: proje oluşturuldu {}{}", p.name, if p.visibility == p2_client_core::VISIBILITY_PUBLIC { " (herkese açık)" } else { "" }));
                    this.manage = None;
                    this.adopt_project(p, window, cx);
                    this.manage_next(window, cx);
                }
                Err(e) => this.manage_fail(if e.message.is_empty() { p2_i18n::t("mng.err.createProject") } else { e.message }, window, cx),
            }),
            Dialog::NewCategory => task::run(window, cx, move || api.create_category(&org, &project, &name), move |this, r, window, cx| match r {
                Ok(c) => this.manage_done(format!("{} kategorisi oluşturuldu.", c.name), window, cx),
                Err(e) => this.manage_fail(if e.message.is_empty() { p2_i18n::t("mng.err.createCategory") } else { e.message }, window, cx),
            }),
            Dialog::NewChannel(cat) => task::run(window, cx, move || api.create_channel(&org, &project, &cat.id, &name, kind).map(|c| (c, cat.id)), move |this, r, window, cx| match r {
                Ok((c, cat)) => {
                    // Electron: yeni kanal seçilir, kategorisi açılır.
                    this.collapsed.remove(&cat);
                    let ch = c.clone();
                    this.manage_done(format!("#{} kanalı oluşturuldu.", c.name), window, cx);
                    if ch.kind != p2_client_core::ChannelType::Voice && this.manage_steps.is_empty() {
                        this.select_channel(ch, window, cx);
                    }
                }
                Err(e) => this.manage_fail(if e.message.is_empty() { p2_i18n::t("mng.err.createChannel") } else { e.message }, window, cx),
            }),
            Dialog::DeleteChannel(ch) => task::run(window, cx, move || api.delete_channel(&org, &project, &ch.id).map(|_| ch.name), move |this, r, window, cx| match r {
                Ok(n) => this.manage_done(format!("#{n} silindi."), window, cx),
                Err(e) => this.manage_fail(if e.message.is_empty() { p2_i18n::t("mng.err.deleteChannel") } else { e.message }, window, cx),
            }),
            Dialog::DeleteCategory(cat) => task::run(window, cx, move || api.delete_category(&org, &project, &cat.id).map(|_| cat.name), move |this, r, window, cx| match r {
                Ok(n) => this.manage_done(format!("{n} kategorisi silindi."), window, cx),
                Err(e) => this.manage_fail(if e.message.is_empty() { p2_i18n::t("mng.err.deleteCategory") } else { e.message }, window, cx),
            }),
            Dialog::LeaveProject => {
                let current = self.project.map(|i| self.projects[i].clone());
                // Birincil sahip ayrılamaz: düğme yönetim paneline (Üyeler › Sahipliği devret) götürür.
                if current.as_ref().and_then(|p| p.owner_id.as_deref()) == Some(self.auth.user.id.as_str()) {
                    self.manage = None;
                    self.open_admin(window, cx);
                    return;
                }
                if let Some(p) = current {
                    self.leave_project(p, window, cx);
                }
            }
            Dialog::DeleteProject => task::run(window, cx, move || api.delete_project(&org, &project), move |this, r, window, cx| match r {
                Ok(_) => {
                    crate::trace("yönet: proje silindi");
                    this.manage = None;
                    if let Some(ix) = this.project.take() {
                        this.projects.remove(ix);
                    }
                    this.categories.clear();
                    this.close_chat(cx);
                    if !this.projects.is_empty() {
                        this.select_project(0, window, cx);
                    }
                    this.manage_next(window, cx);
                }
                Err(e) => this.manage_fail(if e.message.is_empty() { p2_i18n::t("mng.err.deleteProject") } else { e.message }, window, cx),
            }),
            Dialog::Settings => {
                let current = self.project.map(|i| self.projects[i].clone());
                let mut patch = serde_json::Map::new();
                if let Some(p) = &current {
                    if icon != p.icon_url {
                        patch.insert("iconUrl".into(), json!(icon));
                    }
                    if cover != p.cover_url {
                        patch.insert("coverUrl".into(), json!(cover));
                    }
                    if library != p.library_enabled && self.my_perms.can("release.publish") {
                        patch.insert("libraryEnabled".into(), json!(library));
                    }
                }
                task::run(
                    window,
                    cx,
                    move || if patch.is_empty() { Ok(None) } else { api.update_project_settings(&project, serde_json::Value::Object(patch)).map(Some) },
                    |this, r, window, cx| match r {
                        Ok(saved) => {
                            if let (Some(saved), Some(ix)) = (saved, this.project) {
                                this.projects[ix] = saved;
                            }
                            this.manage_done("Proje ayarları kaydedildi.".into(), window, cx);
                        }
                        Err(e) => this.manage_fail(if e.message.is_empty() { p2_i18n::t("mng.err.save") } else { e.message }, window, cx),
                    },
                );
            }
        }
    }

    /// Proje ayarları: ikon ya da kapak görseli seç ve yükle.
    fn pick_image(&mut self, cover: bool, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(gpui_kit::PathPromptOptions { files: true, directories: false, multiple: false, prompt: Some(if cover { p2_i18n::t("mng.pick.cover").into() } else { p2_i18n::t("mng.pick.icon").into() }) });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                if let Some(path) = paths.into_iter().next() {
                    let _ = this.update_in(cx, |this, window, cx| this.upload_image(cover, path, window, cx));
                }
            }
        })
        .detach();
    }

    pub(super) fn upload_image(&mut self, cover: bool, path: std::path::PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let api = self.api.clone();
        if let Some(m) = self.manage.as_mut() {
            m.uploading = Some(cover);
            m.error = None;
        }
        cx.notify();
        task::run(
            window,
            cx,
            move || {
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "gorsel.png".into());
                if !p2_client_core::mime_for(&name).starts_with("image/") {
                    return Err(p2_i18n::t("mng.err.imageOnly"));
                }
                let data = std::fs::read(&path).map_err(|e| e.to_string())?;
                // İkon 8 MB, afiş 15 MB (hareketli GIF dahil).
                if data.len() > if cover { 15 } else { 8 } * 1024 * 1024 {
                    return Err(p2_i18n::t(if cover { "mng.err.coverTooBig" } else { "mng.err.imageTooBig" }));
                }
                api.upload_file(&name, &data).map(|f| f.url).map_err(|e| if e.message.is_empty() { p2_i18n::t("mng.err.upload") } else { e.message })
            },
            move |this, r, window, cx| match r {
                Ok(url) => {
                    let path = url.find("/uploads/").map(|i| url[i..].to_string()).unwrap_or(url);
                    crate::trace(&format!("yönet: {} yüklendi {path}", if cover { "kapak" } else { "ikon" }));
                    if let Some(m) = this.manage.as_mut() {
                        m.uploading = None;
                        if cover {
                            m.cover = Some(path);
                        } else {
                            m.icon = Some(path);
                        }
                    }
                    cx.notify();
                }
                Err(e) => this.manage_fail(e, window, cx),
            },
        );
    }

    pub(super) fn open_channel_perms(&mut self, channel: Channel, window: &mut Window, cx: &mut Context<Self>) {
        self.channel_menu = None;
        let Some(project) = self.project_id() else { return };
        let (api, org) = (self.api.clone(), self.org.id.clone());
        let pane = cx.new(|cx| ChannelPermsPane::new(api, org, project, channel.id.clone(), channel.name.clone(), window, cx));
        let sub = cx.subscribe(&pane, |this, _, _: &PermsEvent, cx| {
            this.channel_perms = None;
            crate::trace("kanal izinleri: kapandı");
            cx.notify();
        });
        crate::trace(&format!("kanal izinleri: #{} açıldı", channel.name));
        self.channel_perms = Some((pane, sub));
        cx.notify();
    }

    /// Başlangıç kanalı yap (ya da `None`: başlangıç kanalı olmaktan çıkar).
    fn make_default(&mut self, channel: Option<Channel>, window: &mut Window, cx: &mut Context<Self>) {
        self.channel_menu = None;
        let Some(project) = self.project_id() else { return };
        let (api, org) = (self.api.clone(), self.org.id.clone());
        let id = channel.as_ref().map(|c| c.id.clone());
        let name = channel.map(|c| c.name);
        let _ = org;
        task::run(window, cx, move || api.update_project_settings(&project, json!({ "defaultChannelId": id })).map(|p| (p, name)), |this, r, window, cx| match r {
            Ok((p, name)) => {
                if let Some(ix) = this.project {
                    this.projects[ix] = p;
                }
                let note = match name {
                    Some(name) => format!("#{name} varsayılan kanal yapıldı."),
                    None => "başlangıç kanalı kaldırıldı.".into(),
                };
                this.manage_done(note, window, cx);
            }
            Err(e) => this.fail(e, window, cx),
        });
    }

    // ------------------------------------------------------------ otomatik pilot

    /// `P2_AUTOPILOT_MANAGE` adımları (`;`): `category:Ad`, `channel:Kategori:ad:TÜR`,
    /// `default:kanal`, `perms:kanal`, `perm:ROLE|USER:Ad:yetki:allow|deny|inherit`,
    /// `perms-close`, `delete-channel:ad`, `delete-category:Ad`, `project:Ad`,
    /// `library:on|off`, `icon:yol`, `delete-project`, `wait:ms`.
    /// Yalnız görünüm (veri yazmaz): `view:new-project`, `view:new-category`,
    /// `view:new-channel:Kategori[:ad]`, `view:delete-channel:kanal`, `view:delete-category:Ad`,
    /// `view:delete-project`, `view:settings`, `view:menu-categories`, `view:category-menu:Ad@x,y`.
    pub(super) fn manage_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.manage_steps.is_empty() {
            return;
        }
        let delay = self.manage_delay;
        self.manage_delay = 400;
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(delay)).await;
            let _ = this.update_in(cx, |this, window, cx| this.manage_step(window, cx));
        })
        .detach();
    }

    fn manage_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(step) = self.manage_steps.pop() else { return };
        crate::trace(&format!("yönet: adım {step}"));
        let parts: Vec<String> = step.split(':').map(str::to_string).collect();
        let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
        let channel = |this: &Self, name: &str| this.categories.iter().flat_map(|c| c.channels.iter()).find(|c| c.name == name).cloned();
        let category = |this: &Self, name: &str| this.categories.iter().find(|c| c.name == name).cloned();
        let set_name = |this: &mut Self, v: &str, window: &mut Window, cx: &mut Context<Self>| {
            if let Some(m) = this.manage.as_ref() {
                let v = v.to_string();
                m.name.update(cx, |s, cx| s.set_value(v, window, cx));
            }
        };
        match parts.as_slice() {
            ["view", rest @ ..] => {
                match rest {
                    ["new-project"] => self.open_dialog(Dialog::NewProject, window, cx),
                    // Görünüm: "herkese açık" seçili, kategori çipleri açık.
                    ["new-project-public"] => {
                        self.open_dialog(Dialog::NewProject, window, cx);
                        if let Some(m) = self.manage.as_mut() {
                            (m.public, m.category) = (true, Some("GAME"));
                            let name = "Project Aurora".to_string();
                            m.name.update(cx, |s, cx| s.set_value(name, window, cx));
                        }
                    }
                    ["new-category"] => self.open_dialog(Dialog::NewCategory, window, cx),
                    ["new-channel", cat, more @ ..] => {
                        if let Some(c) = category(self, cat).or_else(|| self.categories.first().cloned()) {
                            self.open_dialog(Dialog::NewChannel(c), window, cx);
                            if let Some(name) = more.first() {
                                set_name(self, name, window, cx);
                            }
                        }
                    }
                    ["delete-channel", name] => {
                        if let Some(c) = channel(self, name) {
                            self.open_dialog(Dialog::DeleteChannel(c), window, cx);
                        }
                    }
                    ["delete-category", name] => {
                        if let Some(c) = category(self, name) {
                            self.open_dialog(Dialog::DeleteCategory(c), window, cx);
                        }
                    }
                    ["delete-project"] => self.open_dialog(Dialog::DeleteProject, window, cx),
                    ["leave-project"] => self.open_dialog(Dialog::LeaveProject, window, cx),
                    ["settings"] => self.open_dialog(Dialog::Settings, window, cx),
                    ["menu-categories"] => {
                        self.project_menu = true;
                        self.project_menu_categories = true;
                    }
                    ["category-menu", spec] => {
                        let (name, at) = spec.split_once('@').unwrap_or((spec, "200,300"));
                        let (x, y) = at.split_once(',').and_then(|(x, y)| Some((x.parse::<f32>().ok()?, y.parse::<f32>().ok()?))).unwrap_or((200., 300.));
                        if let Some(c) = category(self, name) {
                            self.channel_menu = Some(ChannelMenu::category(c, gpui_kit::point(px(x), px(y))));
                        }
                    }
                    _ => {}
                }
                crate::trace(&format!("yönet: görünüm {}", rest.join(":")));
                cx.notify();
                self.manage_next(window, cx);
            }
            ["category", name] => {
                self.open_dialog(Dialog::NewCategory, window, cx);
                set_name(self, name, window, cx);
                self.manage_submit(window, cx);
            }
            ["channel", cat, name, kind] => match category(self, cat) {
                Some(c) => {
                    self.open_dialog(Dialog::NewChannel(c), window, cx);
                    set_name(self, name, window, cx);
                    if let Some(m) = self.manage.as_mut() {
                        m.kind = KINDS.iter().find(|k| k.0 == *kind).map(|k| k.0).unwrap_or("TEXT");
                    }
                    self.manage_submit(window, cx);
                }
                None => self.manage_next(window, cx),
            },
            ["default", name] => match channel(self, name) {
                Some(c) => self.make_default(Some(c), window, cx),
                None => self.manage_next(window, cx),
            },
            ["perms", name] => {
                if let Some(c) = channel(self, name) {
                    self.open_channel_perms(c, window, cx);
                }
                self.manage_delay = 1500;
                self.manage_next(window, cx);
            }
            ["perm", tab, target, perm, state] => {
                if let Some((pane, _)) = &self.channel_perms {
                    let tab = if *tab == "USER" { Tab::User } else if *tab == "GROUP" { Tab::Group } else { Tab::Role };
                    let pane = pane.clone();
                    let (perm, state) = (perm.to_string(), state.to_string());
                    let id = pane.read(cx).target_by_name(tab, target);
                    pane.update(cx, |p, cx| {
                        p.select(tab, id, cx);
                        p.set(&perm, &state, window, cx);
                    });
                }
                self.manage_delay = 1500;
                self.manage_next(window, cx);
            }
            ["perms-close"] => {
                self.channel_perms = None;
                crate::trace("kanal izinleri: kapandı");
                cx.notify();
                self.manage_next(window, cx);
            }
            ["delete-channel", name] => match channel(self, name) {
                Some(c) => {
                    self.open_dialog(Dialog::DeleteChannel(c), window, cx);
                    self.manage_submit(window, cx);
                }
                None => self.manage_next(window, cx),
            },
            ["delete-category", name] => match category(self, name) {
                Some(c) => {
                    self.open_dialog(Dialog::DeleteCategory(c), window, cx);
                    self.manage_submit(window, cx);
                }
                None => self.manage_next(window, cx),
            },
            ["project", name] => {
                self.open_dialog(Dialog::NewProject, window, cx);
                set_name(self, name, window, cx);
                self.manage_submit(window, cx);
            }
            ["library", state] => {
                self.open_dialog(Dialog::Settings, window, cx);
                if let Some(m) = self.manage.as_mut() {
                    m.library = *state == "on";
                }
                self.manage_submit(window, cx);
            }
            ["icon", ..] => {
                let path = step.splitn(2, ':').nth(1).unwrap_or_default().to_string();
                self.open_dialog(Dialog::Settings, window, cx);
                self.upload_image(false, path.into(), window, cx);
                self.manage_delay = 2000;
                self.manage_next(window, cx);
            }
            ["save"] => self.manage_submit(window, cx),
            ["delete-project"] => {
                self.open_dialog(Dialog::DeleteProject, window, cx);
                self.manage_submit(window, cx);
            }
            ["leave-project"] => {
                self.open_dialog(Dialog::LeaveProject, window, cx);
                self.manage_submit(window, cx);
            }
            ["star", name] => {
                if let Some(c) = channel(self, name) {
                    self.toggle_star("CHANNEL", c.id, window, cx);
                }
                self.manage_delay = 800;
                self.manage_next(window, cx);
            }
            ["star-person", name] => {
                if let Some(m) = self.members.iter().find(|m| m.username == *name).cloned() {
                    self.toggle_star("PERSON", m.id, window, cx);
                }
                self.manage_delay = 800;
                self.manage_next(window, cx);
            }
            ["notify", name, level] => {
                if let Some(c) = channel(self, name) {
                    let level: &'static str = match *level {
                        "ALL" => "ALL",
                        "MUTED" => "MUTED",
                        _ => "MENTIONS",
                    };
                    self.set_channel_notify(c.id, level, window, cx);
                }
                self.manage_delay = 800;
                self.manage_next(window, cx);
            }
            ["wait", ms] => {
                self.manage_delay = ms.parse().unwrap_or(1000);
                self.manage_next(window, cx);
            }
            _ => self.manage_next(window, cx),
        }
    }

    // ------------------------------------------------------------ çizim

    /// ui/Modal.tsx: `.p2-modal-layer` + `.p2-modal` (+ `.centered`) + başlık (kicker, b, small) + gövde + alt.
    fn p2_modal(&self, size: ModalSize, centered: bool, kicker_text: Option<String>, title: String, subtitle: Option<String>, body: AnyElement, footer: Vec<AnyElement>, window: &Window, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let close = button("mg-close", BtnVariant::Ghost, BtnSize::Sm, true, false, p).text_size(font_size::MD).child("×").on_click(cx.listener(|this, _, _, cx| {
            this.manage = None;
            cx.notify();
        }));
        let head_text = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .when(centered, |s| s.items_center())
            .when_some(kicker_text, |s, k| s.child(kicker(p2_ui_upper(&k), p).line_height(lh(font_size::XXS))))
            .child(div().text_size(font_size::XL).font_weight(FontWeight::BOLD).text_color(p.text_strong).line_height(gpui_kit::relative(1.25)).when(centered, |s| s.text_center()).child(title))
            .when_some(subtitle, |s, t| s.child(div().mt(px(3.)).text_size(font_size::SM).text_color(p.text_3).line_height(gpui_kit::relative(1.4)).when(centered, |s| s.text_center()).child(t)));
        let head = div()
            .flex()
            .items_start()
            .gap(px(12.))
            .pt(px(18.))
            .pr(px(16.))
            .pb(px(12.))
            .pl(px(20.))
            .child(head_text)
            .map(|s| if centered { s.child(div().absolute().top(px(10.)).right(px(10.)).child(close)) } else { s.child(close) });
        let card = modal(size, window, p)
            .id("mg-card")
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(head)
            .child(modal_body().when(centered, |s| s.flex().flex_col().items_center()).child(body))
            .child(modal_foot(p).when(centered, |s| s.justify_center()).children(footer));
        div()
            .id("mg-layer")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .px(px(24.))
            .pb(px(24.))
            .bg(p.overlay).backdrop_blur(p2_ui::fx::BLUR_SM)
            .occlude()
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.manage = None;
                cx.notify();
            }))
            .child(card.p2_enter("mg", Entrance::PopElastic, Duration::ZERO))
            .into_any_element()
    }

    /// `.p2-field`: üstte `600 fs-sm text-3` etiket, 6px boşluk.
    fn p2_field(label: &str, control: impl IntoElement, p: &Palette) -> Div {
        div().flex().flex_col().gap(px(6.)).child(div().text_size(font_size::SM).line_height(lh(font_size::SM)).css_weight(600.).text_color(p.text_3).child(label.to_string())).child(control)
    }

    /// `.p2-danger-mark` + `.p2-modal-body > p`.
    fn danger_body(text: &str, error: Option<String>, p: &Palette) -> AnyElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .child(
                div()
                    .mt(px(4.))
                    .mb(px(10.))
                    .size(px(44.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(radius::LG)
                    .bg(p.danger_soft)
                    .border_1()
                    .border_color(p.danger_border)
                    .text_color(p.danger_text)
                    .text_size(fs(20.))
                    .line_height(lh(fs(20.)))
                    .font_weight(FontWeight::EXTRA_BOLD)
                    .child("!")
                    .p2_enter("danger-mark", Entrance::PopElastic, Duration::ZERO),
            )
            .child(div().mb(px(6.)).text_center().text_size(font_size::MD).line_height(gpui_kit::relative(1.6)).text_color(p.text_2).child(text.to_string()))
            .when_some(error, |s, e| s.child(div().text_size(font_size::SM).text_color(p.danger_text).child(e)))
            .into_any_element()
    }

    pub(super) fn manage_modal(&self, p: &Palette, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let m = self.manage.as_ref()?;
        let project = self.project.map(|i| self.projects[i].clone());
        let pname = project.as_ref().map(|p| p.name.clone()).unwrap_or_default();
        let ghost = |cx: &mut Context<Self>| {
            button("mg-cancel", BtnVariant::Ghost, BtnSize::Md, false, false, p).child(p2_i18n::t("act.cancel")).on_click(cx.listener(|this, _, _, cx| {
                this.manage = None;
                cx.notify();
            }))
        };
        let go = |label: String, variant: BtnVariant, disabled: bool, cx: &mut Context<Self>| {
            button("mg-go", variant, BtnSize::Md, false, false, p).when(disabled, |s| s.opacity(0.5).cursor_default()).child(label).on_click(cx.listener(move |this, _, window, cx| {
                if !disabled {
                    this.manage_submit(window, cx)
                }
            }))
        };
        let name_val = m.name.read(cx).value().to_string();
        let name_empty = name_val.trim().is_empty();
        let busy = m.busy;
        let el = match &m.dialog {
            Dialog::NewProject => {
                let slug = p2_client_core::project_slug(&name_val);
                let body = div()
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .child(Self::p2_field(&p2_i18n::t("mng.field.projectName"), p2_ui::form::input(&m.name, InputKind::P2, p, window, cx), p))
                    .child(Self::p2_field(
                        &p2_i18n::t("mng.field.projectCode"),
                        div()
                            .px(px(12.))
                            .py(px(10.))
                            .border_1()
                            .border_color(p.border)
                            .rounded(radius::SM)
                            .bg(p.bg_1)
                            .font_family(FONT_MONO)
                            .text_size(fs(12.))
                            .line_height(lh(fs(12.)))
                            .flex()
                            .child(div().text_color(p.text_4).child(format!("{}/", self.org.slug)))
                            .child(div().text_color(p.text_2).child(if slug.is_empty() { p2_i18n::t("mng.slugPlaceholder") } else { slug.clone() })),
                        p,
                    ))
                    .child(Self::p2_field(&p2_i18n::t("mng.field.shortDesc"), textarea(&m.desc, p, window, cx), p))
                    // Görünürlük: özel (yalnız davet kodu) / herkese açık (Keşfet'te listelenir); açıksa kategori.
                    .child({
                        use crate::screens::discover_ui as dui;
                        let (public, category) = (m.public, m.category);
                        let tiles = div()
                            .flex()
                            .gap(px(10.))
                            .child(dui::choice_tile("mg-vis-private", dui::glyph::LOCK, p2_i18n::t("vis.private"), p2_i18n::t("vis.private.hint"), !public, p).on_click(cx.listener(|this, _, _, cx| {
                                if let Some(m) = this.manage.as_mut() {
                                    m.public = false;
                                }
                                cx.notify();
                            })))
                            .child(dui::choice_tile("mg-vis-public", dui::glyph::GLOBE, p2_i18n::t("vis.public"), p2_i18n::t("vis.public.hint"), public, p).on_click(cx.listener(|this, _, _, cx| {
                                if let Some(m) = this.manage.as_mut() {
                                    m.public = true;
                                }
                                cx.notify();
                            })));
                        let mut field = Self::p2_field(&p2_i18n::t("vis.label"), tiles, p);
                        if public {
                            let mut chips = div().mt(px(4.)).flex().flex_wrap().gap(px(7.));
                            for id in p2_client_core::DISCOVER_CATEGORIES {
                                chips = chips.child(dui::category_chip(SharedString::from(format!("mg-cat-{id}")), dui::category_glyph(id), dui::category_label(id), None, category == Some(*id), p).on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(m) = this.manage.as_mut() {
                                        m.category = if m.category == Some(*id) { None } else { Some(*id) };
                                    }
                                    cx.notify();
                                })));
                            }
                            field = field.child(chips);
                        }
                        field
                    })
                    .when_some(m.error.clone(), |s, e| s.child(div().text_size(font_size::SM).text_color(p.danger_text).child(e)));
                let label = if busy { p2_i18n::t("mng.newProject.busy") } else { p2_i18n::t("mng.newProject.go") };
                self.p2_modal(ModalSize::Md, false, Some(p2_i18n::t("mng.newProject.kicker")), p2_i18n::t("mng.newProject.title"), Some(p2_i18n::t("mng.newProject.sub")), body.into_any_element(), vec![ghost(cx).into_any_element(), go(label, BtnVariant::Primary, busy || slug.len() < 2, cx).into_any_element()], window, p, cx)
            }
            Dialog::NewCategory => {
                let body = div().flex().flex_col().gap(px(14.)).child(Self::p2_field(&p2_i18n::t("mng.field.categoryName"), p2_ui::form::input(&m.name, InputKind::P2, p, window, cx), p)).when_some(m.error.clone(), |s, e| s.child(div().text_size(font_size::SM).text_color(p.danger_text).child(e)));
                let label = if busy { p2_i18n::t("mng.creating") } else { p2_i18n::t("mng.newCategory.go") };
                self.p2_modal(ModalSize::Sm, false, Some(p2_i18n::t("mng.newCategory.kicker")), p2_i18n::t("mng.newCategory.title"), None, body.into_any_element(), vec![ghost(cx).into_any_element(), go(label, BtnVariant::Primary, busy || name_empty, cx).into_any_element()], window, p, cx)
            }
            Dialog::NewChannel(cat) => {
                let focused = gpui_kit::Focusable::focus_handle(m.name.read(cx), cx).is_focused(window);
                let name_input = div()
                    .h(px(40.))
                    .flex()
                    .items_center()
                    .border_1()
                    .rounded(radius::SM)
                    .bg(p.bg_3)
                    .map(|s| if focused { s.border_color(p.border_accent).shadow(shadows(&[p.ring])) } else { s.border_color(p.border) })
                    .child(div().w(px(32.)).flex_none().flex().justify_center().text_color(p.text_accent).text_size(fs(18.)).line_height(lh(fs(18.))).child("#"))
                    .child(div().flex_1().min_w_0().pr(px(10.)).child(Input::new(&m.name).appearance(false).bordered(false).focus_bordered(false).px_0().text_size(fs(14.)).text_color(p.text).h(px(38.))));
                let mut kinds = div().flex().flex_col().gap(px(5.)).child(div().mb(px(8.)).text_size(font_size::SM).line_height(lh(font_size::SM)).css_weight(600.).text_color(p.text_3).child(p2_i18n::t("mng.field.channelType")));
                for (value, label, hint) in KINDS {
                    let (label, hint) = (p2_i18n::t(label), p2_i18n::t(hint));
                    let on = m.kind == value;
                    let (hover, border, strong) = (p.bg_hover, p.border, p.text_strong);
                    kinds = kinds.child(
                        div()
                            .id(SharedString::from(format!("kind-{value}")))
                            .h(px(52.))
                            .flex()
                            .items_center()
                            .px(px(12.))
                            .border_1()
                            .rounded(radius::SM)
                            .cursor_pointer()
                            .map(|s| if on { s.border_color(p.border_accent).bg(p.accent_soft).text_color(p.text_strong) } else { s.border_color(p.border_subtle).bg(p.bg_3).text_color(p.text_2).hover(move |s| s.bg(hover).border_color(border).text_color(strong)) })
                            .child(div().w(px(30.)).flex_none().child(channel_icon(value, 16., if on { p.text_strong } else { p.text_2 })))
                            .child(div().flex_1().min_w_0().flex().flex_col().gap(px(3.)).child(div().text_size(font_size::MD).line_height(lh(font_size::MD)).font_weight(FontWeight::BOLD).child(label)).child(div().text_size(font_size::SM).line_height(lh(font_size::SM)).text_color(p.text_3).child(hint)))
                            .child(div().size(px(10.)).flex_none().rounded_full().flex().items_center().justify_center().map(|s| if on { s.bg(p.accent).child(div().size(px(4.)).rounded_full().bg(p2_ui::components::over(p.accent_soft, p.bg_3))) } else { s.border_1().border_color(p.navy_500) }))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(m) = this.manage.as_mut() {
                                    m.kind = value;
                                }
                                cx.notify();
                            })),
                    );
                }
                let body = div().flex().flex_col().gap(px(14.)).child(Self::p2_field(&p2_i18n::t("mng.field.channelName"), name_input, p)).child(kinds).when_some(m.error.clone(), |s, e| s.child(div().text_size(font_size::SM).text_color(p.danger_text).child(e)));
                let label = if busy { p2_i18n::t("mng.creating") } else { p2_i18n::t("mng.newChannel.go") };
                self.p2_modal(ModalSize::Sm, false, Some(cat.name.clone()), p2_i18n::t("mng.createChannel"), None, body.into_any_element(), vec![ghost(cx).into_any_element(), go(label, BtnVariant::Primary, busy || name_empty, cx).into_any_element()], window, p, cx)
            }
            Dialog::DeleteChannel(c) => self.p2_modal(
                ModalSize::Sm,
                true,
                None,
                p2_i18n::tv("mng.deleteChannel.title", &[("name", &c.name)]),
                None,
                Self::danger_body(&p2_i18n::t("mng.deleteChannel.body"), m.error.clone(), p),
                vec![ghost(cx).into_any_element(), go(if busy { p2_i18n::t("mng.deleting") } else { p2_i18n::t("mng.deleteChannel.go") }, BtnVariant::Danger, busy, cx).into_any_element()],
                window,
                p,
                cx,
            ),
            Dialog::DeleteCategory(c) => self.p2_modal(
                ModalSize::Sm,
                true,
                None,
                p2_i18n::tv("mng.deleteCategory.title", &[("name", &c.name)]),
                None,
                Self::danger_body(&p2_i18n::t("mng.deleteCategory.body"), m.error.clone(), p),
                vec![ghost(cx).into_any_element(), go(if busy { p2_i18n::t("mng.deleting") } else { p2_i18n::t("mng.deleteCategory.go") }, BtnVariant::Danger, busy, cx).into_any_element()],
                window,
                p,
                cx,
            ),
            Dialog::DeleteProject => self.p2_modal(
                ModalSize::Sm,
                true,
                Some(p2_i18n::t("mng.deleteProject.kicker")),
                p2_i18n::tv("mng.deleteProject.title", &[("name", &pname)]),
                None,
                Self::danger_body(&p2_i18n::t("mng.deleteProject.body"), m.error.clone(), p),
                vec![ghost(cx).into_any_element(), go(if busy { p2_i18n::t("mng.deleting") } else { p2_i18n::t("mng.deleteProject.go") }, BtnVariant::Danger, busy, cx).into_any_element()],
                window,
                p,
                cx,
            ),
            Dialog::Settings => self.project_settings_view(m, &pname, project.as_ref().map(|p| p.id.clone()).unwrap_or_default(), p, window, cx),
            Dialog::LeaveProject => {
                let owner = project.as_ref().and_then(|p| p.owner_id.as_deref()) == Some(self.auth.user.id.as_str());
                let text = if owner {
                    p2_i18n::t("mng.leave.ownerBody")
                } else {
                    p2_i18n::t("mng.leave.body")
                };
                let label = if owner { p2_i18n::t("mng.leave.transfer") } else if busy { p2_i18n::t("mng.leave.busy") } else { p2_i18n::t("mng.leave.go") };
                self.p2_modal(
                    ModalSize::Sm,
                    true,
                    Some(p2_i18n::t("mng.leave.kicker")),
                    p2_i18n::tv("mng.leave.title", &[("name", &pname)]),
                    None,
                    Self::danger_body(&text, m.error.clone(), p),
                    vec![ghost(cx).into_any_element(), go(label, if owner { BtnVariant::Primary } else { BtnVariant::Danger }, busy, cx).into_any_element()],
                    window,
                    p,
                    cx,
                )
            }
        };
        Some(el)
    }

    /// ProjectSettingsDialog.tsx (`.library-modal-backdrop` + `.library-upload-modal.library-project-modal`).
    fn project_settings_view(&self, m: &Manage, pname: &str, pid: String, p: &Palette, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let can_library = self.my_perms.can("release.publish");
        let busy_icon = m.uploading == Some(false);
        let busy_cover = m.uploading == Some(true);
        let saving = m.busy;
        // `.library-image-actions button`: h 30, p 0 12, kenarlık, r 8, bg-3, 650 11px.
        let small_btn = |id: &'static str, label: String, disabled: bool| {
            let (hover, strong) = (p.bg_hover, p.text_strong);
            div()
                .id(id)
                .h(px(30.))
                .px(px(12.))
                .flex()
                .items_center()
                .border_1()
                .border_color(p.border)
                .rounded(radius::SM)
                .bg(p.bg_3)
                .text_color(p.text)
                .text_size(fs(11.))
                .line_height(lh(fs(11.)))
                .css_weight(650.)
                .map(|s| if disabled { s.opacity(0.5) } else { s.cursor_pointer().hover(move |s| s.bg(hover).text_color(strong)) })
                .child(label)
        };
        let image = |key: String, url: &Option<String>, r: f32| -> Option<AnyElement> {
            url.as_ref()?;
            crate::avatars::remember(&key, url.as_deref());
            crate::avatars::image(&key).map(|img| gpui_kit::img(gpui_kit::ImageSource::Render(img)).size_full().rounded(px(r)).object_fit(gpui_kit::ObjectFit::Cover).into_any_element())
        };
        let icon_key = format!("project-icon:{pid}:{}", m.icon.clone().unwrap_or_default());
        let icon_face = image(icon_key, &m.icon, 20.).unwrap_or_else(|| p2_ui::components::initials_label(p2_ui::components::user_initials(pname)).into_any_element());
        let hue = p.hues[p2_ui::components::avatar_hue(&pid) % 10];
        let icon_box = div()
            .id("ps-icon")
            .size(px(72.))
            .flex_none()
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(20.))
            .bg(gpui_kit::linear_gradient(145., gpui_kit::linear_color_stop(hue.bg, 0.), gpui_kit::linear_color_stop(hue.bg2, 1.)))
            .text_color(hue.fg)
            .text_size(fs(22.))
            .line_height(px(72.))
            .font_weight(FontWeight::EXTRA_BOLD)
            .shadow(shadows(&p.shadow_md))
            .cursor_pointer()
            .child(icon_face)
            .on_click(cx.listener(move |this, _, window, cx| {
                if !busy_icon {
                    this.pick_image(false, window, cx)
                }
            }));
        let field = |label: &str, body: AnyElement| div().flex().flex_col().gap(px(5.)).child(div().text_size(fs(10.)).line_height(lh(fs(10.))).css_weight(650.).text_color(p.text_3).child(label.to_string())).child(body);
        let identity = div()
            .flex()
            .items_center()
            .gap(px(14.))
            .child(icon_box)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.))
                    .child(small_btn("ps-icon-pick", if busy_icon { p2_i18n::t("app.loading") } else if m.icon.is_some() { p2_i18n::t("mng.icon.change") } else { p2_i18n::t("mng.icon.upload") }, busy_icon).on_click(cx.listener(move |this, _, window, cx| {
                        if !busy_icon {
                            this.pick_image(false, window, cx)
                        }
                    })))
                    .when(m.icon.is_some(), |s| {
                        s.child(small_btn("ps-icon-x", p2_i18n::t("mng.remove"), false).on_click(cx.listener(|this, _, _, cx| {
                            if let Some(m) = this.manage.as_mut() {
                                m.icon = None;
                            }
                            cx.notify();
                        })))
                    })
                    .child(div().w_full().text_size(fs(10.5)).line_height(gpui_kit::relative(1.5)).text_color(p.text_3).child(p2_i18n::t("mng.icon.hint"))),
            );
        let modal_w = 600f32.min(f32::from(window.viewport_size().width) - 48.);
        let cover_w = modal_w - 42.;
        let cover_h = (cover_w * 9. / 16.).min(210.);
        let cover_key = format!("project-cover:{pid}:{}", m.cover.clone().unwrap_or_default());
        let soft = p.bg_hover;
        let cover = div()
            .id("ps-cover")
            .w_full()
            .h(px(cover_h))
            .overflow_hidden()
            .flex()
            .items_center()
            .justify_center()
            .border_1()
            .border_dashed()
            .border_color(p.border_accent)
            .rounded(radius::LG)
            .bg(p.bg_3)
            .text_color(p.text_3)
            .text_size(fs(12.))
            .cursor_pointer()
            .hover(move |s| s.bg(soft))
            .child(image(cover_key, &m.cover, 14.).unwrap_or_else(|| (if busy_cover { p2_i18n::t("app.loading") } else { p2_i18n::t("mng.cover.pick") }).into_any_element()))
            .on_click(cx.listener(move |this, _, window, cx| {
                if !busy_cover {
                    this.pick_image(true, window, cx)
                }
            }));
        let cover_actions = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .child(small_btn("ps-cover-pick", if m.cover.is_some() { p2_i18n::t("mng.cover.change") } else { p2_i18n::t("mng.cover.upload") }, busy_cover).on_click(cx.listener(move |this, _, window, cx| {
                if !busy_cover {
                    this.pick_image(true, window, cx)
                }
            })))
            .when(m.cover.is_some(), |s| {
                s.child(small_btn("ps-cover-x", p2_i18n::t("mng.remove"), false).on_click(cx.listener(|this, _, _, cx| {
                    if let Some(m) = this.manage.as_mut() {
                        m.cover = None;
                    }
                    cx.notify();
                })))
            });
        let library = m.library;
        let body = div()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(field(&p2_i18n::t("mng.field.icon"), identity.into_any_element()))
            .child(field(&p2_i18n::t("mng.field.cover"), div().flex().flex_col().gap(px(5.)).child(cover).child(cover_actions).into_any_element()))
            .when(can_library, |s| {
                s.child(
                    div()
                        .id("ps-library")
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(14.))
                        .px(px(12.))
                        .py(px(10.))
                        .border_1()
                        .border_color(p.border)
                        .rounded(radius::MD)
                        .bg(p.bg_2)
                        .cursor_pointer()
                        .child(div().min_w_0().flex().flex_col().gap(px(2.)).child(div().text_size(font_size::SM).line_height(lh(font_size::SM)).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(p2_i18n::t("mng.library.title"))).child(div().text_size(font_size::XS).line_height(gpui_kit::relative(1.4)).text_color(p.text_3).child(p2_i18n::t("mng.library.hint"))))
                        .child(p2_ui::form::toggle("ps-library-toggle", library, 0, p))
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Some(m) = this.manage.as_mut() {
                                m.library = !m.library;
                            }
                            cx.notify();
                        })),
                )
            })
            .when_some(m.error.clone(), |s, e| {
                s.child(div().flex().items_center().gap(px(10.)).px(px(12.)).py(px(9.)).border_1().border_color(p.danger_border).rounded(radius::MD).bg(p.danger_soft).text_color(p.danger_text).text_size(fs(12.)).line_height(gpui_kit::relative(1.5)).mx(px(28.)).mt(px(12.)).child(e))
            })
            .child({
                let (hover, strong) = (p.bg_hover, p.text_strong);
                let disabled = m.uploading.is_some() || saving;
                div()
                    .mt(px(6.))
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        div()
                            .id("ps-cancel")
                            .h(px(32.))
                            .px(px(14.))
                            .flex()
                            .items_center()
                            .border_1()
                            .border_color(p.border)
                            .rounded(radius::SM)
                            .bg(p.bg_3)
                            .text_color(p.text)
                            .text_size(fs(11.))
                            .css_weight(650.)
                            .cursor_pointer()
                            .hover(move |s| s.bg(hover).text_color(strong))
                            .child(p2_i18n::t("act.cancel"))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.manage = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .id("ps-save")
                            .h(px(32.))
                            .px(px(14.))
                            .flex()
                            .items_center()
                            .border_1()
                            .border_color(gpui_kit::transparent_black())
                            .rounded(radius::SM)
                            .bg(p.accent_grad.background())
                            .text_color(p.text_on_accent)
                            .text_size(fs(11.))
                            .css_weight(650.)
                            .shadow(shadows(&[p.accent_glow_strong, p.inset_top]))
                            .map(|s| if disabled { s.opacity(0.5) } else { s.cursor_pointer() })
                            .child(if saving { p2_i18n::t("mng.saving") } else { p2_i18n::t("mng.save") })
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if !disabled {
                                    this.manage_submit(window, cx)
                                }
                            })),
                    )
            });
        let (soft, strong) = (p.accent_soft, p.text_strong);
        let card = div()
            .id("ps-card")
            .relative()
            .w(px(modal_w))
            .max_h(px(f32::from(window.viewport_size().height) - 64.))
            .overflow_y_scroll()
            .p(px(20.))
            .border_1()
            .border_color(p.border_strong)
            .rounded(radius::XL)
            .bg(p.glass_strong.background())
            .shadow(p.shadow_lg())
            .text_color(p.text)
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .mb(px(16.))
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(12.))
                    .child(div().flex().flex_col().child(kicker(p2_i18n::t("mng.settings.kicker"), p).line_height(lh(font_size::XXS))).child(div().mt(px(3.)).text_size(fs(16.)).line_height(lh(fs(16.))).css_weight(750.).text_color(p.text_strong).child(pname.to_string())))
                    .child(
                        div()
                            .id("ps-close")
                            .size(px(30.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(radius::XS)
                            .text_color(p.text_3)
                            .text_size(fs(19.))
                            .line_height(px(19.))
                            .cursor_pointer()
                            .hover(move |s| s.bg(soft).text_color(strong))
                            .child("×")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.manage = None;
                                cx.notify();
                            })),
                    ),
            )
            .child(body);
        // `.library-modal-backdrop` p 24 (pencereye göre ortalı → çalışma alanında altta +başlık payı).
        div()
            .id("ps-layer")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .px(px(24.))
            .pb(px(TOP))
            .bg(p.overlay).backdrop_blur(p2_ui::fx::BLUR_SM)
            .occlude()
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                this.manage = None;
                cx.notify();
            }))
            .child(card.p2_enter("ps", Entrance::PopElastic, Duration::ZERO))
            .into_any_element()
    }

    /// Proje menüsü (kenar çubuğu başlığındaki •••) — `.channel-context-menu.invite-menu`.
    pub(super) fn project_menu_view(&self, p: &Palette, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.project_menu {
            return None;
        }
        let can_channels = self.my_perms.can("channel.manage");
        let (soft, strong) = (p.accent_soft, p.text_strong);
        // `.invite-menu .menu-title`: 8px 800 .14em büyük harf, accent; kenar 4 3 5.
        let title = |t: &str, first: bool| div().mt(px(if first { 0. } else { 4. })).mx(px(3.)).mb(px(5.)).text_size(fs(8.)).line_height(lh(fs(8.))).font_weight(FontWeight::EXTRA_BOLD).tracking(0.14).text_color(p.text_accent).child(t.to_string());
        // `.project-menu-action`: min-h 45, ızgara 26px 1fr, gap 8, p 6 7, r 6; simge kutusu 25×25 r7 bg-3.
        let action = |id: &'static str, glyph: &'static str, label: String, sub: String, enabled: bool| {
            div()
                .id(id)
                .w_full()
                .min_h(px(45.))
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(7.))
                .py(px(6.))
                .rounded(radius::XS)
                .text_color(p.text)
                .map(|s| if enabled { s.cursor_pointer().hover(move |s| s.bg(soft).text_color(strong)) } else { s.opacity(0.42) })
                .child(div().w(px(26.)).flex_none().child(div().size(px(25.)).flex().items_center().justify_center().rounded(px(7.)).bg(p.bg_3).text_color(p.text_accent).text_size(fs(13.)).line_height(lh(fs(13.))).css_weight(750.).when(matches!(glyph, "🛡" | "◎"), |s| s.font_family("Segoe UI Symbol")).child(glyph)))
                .child(div().flex_1().min_w_0().flex().flex_col().gap(px(2.)).child(div().text_size(fs(11.)).line_height(lh(fs(11.))).font_weight(FontWeight::BOLD).child(label)).child(div().text_size(fs(9.)).line_height(lh(fs(9.))).css_weight(600.).text_color(p.text_3).truncate().child(sub)))
        };
        let has_categories = !self.categories.is_empty();
        let mut menu = div()
            .id("project-menu")
            .w(px(236.))
            .p(px(9.))
            .flex()
            .flex_col()
            .rounded(radius::MD)
            .border_1()
            .border_color(p.border_strong)
            .bg(p.glass.background()).backdrop_blur(p2_ui::fx::BLUR)
            .shadow(p.shadow_lg())
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(title(&p2_i18n::t("mng.menu.title"), true))
            // Yetkim olmayan işlemler hiç görünmez (soluk satır yok): kategori / kanal "Kanalları yönet" ister.
            .when(can_channels, |menu| {
                menu.child(action("pm-category", "＋", p2_i18n::t("mng.menu.createCategory"), p2_i18n::t("mng.menu.createCategory.sub"), true).on_click(cx.listener(move |this, _, window, cx| {
                    this.open_dialog(Dialog::NewCategory, window, cx);
                })))
                .child(action("pm-channel", "#", p2_i18n::t("mng.createChannel"), if has_categories { p2_i18n::t("mng.menu.createChannel.sub") } else { p2_i18n::t("mng.menu.createChannel.none") }, has_categories).on_click(cx.listener(move |this, _, _, cx| {
                    if !this.categories.is_empty() {
                        this.project_menu_categories = !this.project_menu_categories;
                        cx.notify();
                    }
                })))
            });
        if self.project_menu_categories {
            let mut cats = div().mt(px(2.)).mb(px(5.)).ml(px(33.)).pl(px(5.)).border_l_1().border_color(p.border).flex().flex_col().gap(px(2.));
            for cat in &self.categories {
                let c = cat.clone();
                cats = cats.child(
                    div()
                        .id(SharedString::from(format!("pmc-{}", cat.id)))
                        .h(px(28.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .px(px(8.))
                        .rounded(radius::XS)
                        .text_color(p.text_3)
                        .text_size(fs(10.))
                        .line_height(lh(fs(10.)))
                        .css_weight(600.)
                        .cursor_pointer()
                        .hover(move |s| s.bg(soft).text_color(strong))
                        .child(p2_ui::glyphs::ui_stroke("chevron-right", 9., 2.).text_color(p.text_3))
                        .child(cat.name.clone())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.project_menu_categories = false;
                            this.open_dialog(Dialog::NewChannel(c.clone()), window, cx);
                        })),
                );
            }
            menu = menu.child(cats);
        }
        let hidden = self.hidden.len();
        let code = self.project_invite.clone().filter(|_| self.my_perms.can("member.invite"));
        let pname = self.project.map(|i| self.projects[i].name.clone()).unwrap_or_default();
        let is_copied = copied();
        let copy = move |this: &mut Self, code: &str, window: &mut Window, cx: &mut Context<Self>| {
            cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(code.to_string()));
            *COPIED.lock().unwrap() = Some(Instant::now());
            crate::trace("proje menüsü: davet kodu kopyalandı");
            cx.notify();
            cx.spawn_in(window, async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(1850)).await;
                let _ = this.update(cx, |_, cx| cx.notify());
            })
            .detach();
            let _ = this;
        };
        let code_a = code.clone();
        menu = menu
            .child(action("pm-browse", "◎", p2_i18n::t("mng.menu.browse"), if hidden > 0 { p2_i18n::tv("mng.menu.browse.hidden", &[("n", &hidden.to_string())]) } else { p2_i18n::t("mng.menu.browse.sub") }, true).on_click(cx.listener(|this, _, window, cx| {
                this.project_menu = false;
                this.open_browser(window, cx);
            })))
            // "Kişiyi davet et" yalnız "Davet kodu yönet" yetkisiyle görünür (kod yoksa soluk: önce kod oluşturulmalı).
            .when(self.my_perms.can("member.invite"), |menu| {
                menu.child(action("pm-invite", "＠", p2_i18n::t("mng.menu.invite"), if is_copied { p2_i18n::t("mng.menu.invite.copied") } else if code.is_some() { p2_i18n::t("mng.menu.invite.copy") } else { p2_i18n::t("mng.menu.invite.none") }, code.is_some()).on_click(cx.listener(move |this, _, window, cx| {
                    if let Some(code) = &code_a {
                        copy(this, code, window, cx);
                    }
                })))
            })
            .child(div().h(px(1.)).mx(px(3.)).my(px(7.)).bg(p.border_subtle))
            .child(title(&p2_i18n::t("mng.menu.project"), false))
            .child(div().mx(px(3.)).mb(px(2.)).text_size(fs(12.)).line_height(lh(fs(12.))).css_weight(650.).text_color(p.text_strong).truncate().child(pname.clone()));
        if Self::can_admin(&self.my_perms) {
            menu = menu.child(action("pm-admin", "🛡", p2_i18n::t("mng.menu.admin"), p2_i18n::t("mng.menu.admin.sub"), true).on_click(cx.listener(|this, _, window, cx| {
                this.project_menu = false;
                this.open_admin(window, cx);
            })));
        }
        let danger = p.danger_text;
        menu = menu.child(action("pm-leave", "⎋", p2_i18n::t("mng.leave.go"), p2_i18n::t("mng.menu.leave.sub"), true).text_color(danger).on_click(cx.listener(|this, _, window, cx| {
            this.project_menu = false;
            this.open_dialog(Dialog::LeaveProject, window, cx);
        })));
        let hint = |t: &str| div().mt(px(8.)).mx(px(3.)).mb(px(2.)).text_size(fs(9.5)).line_height(gpui_kit::relative(1.45)).text_color(p.text_3).child(t.to_string());
        match code.clone().filter(|_| self.my_perms.can("member.invite")) {
            Some(code) => {
                
                let c2 = code.clone();
                menu = menu
                    .child(title(&p2_i18n::t("mng.menu.code"), false))
                    .child(
                        div()
                            .id("pm-code")
                            // direct.css + organization.css `.invite-code` birlikte: 32px (menü düğmesi), kenarlık/zemin yok,
                            // dikey 27px kenar boşluğu; kod `700 12px` mono (ölçeklenmez), "Kopyala" .08em.
                            .w_full()
                            .mt(px(22.))
                            .mb(px(27.))
                            .h(px(32.))
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(8.))
                            .px(px(10.))
                            .rounded(radius::XS)
                            .text_color(p.text)
                            .cursor_pointer()
                            .hover(move |s| s.bg(soft).text_color(strong))
                            .child(div().font_family(FONT_MONO).text_size(px(12.)).line_height(lh(px(12.))).font_weight(FontWeight::BOLD).tracking(0.06).text_color(p.text_strong).child(code))
                            .child(div().flex_none().text_size(fs(9.)).line_height(lh(fs(9.))).font_weight(FontWeight::BOLD).tracking(0.08).text_color(p.text_accent).child(if is_copied { p2_i18n::t("mng.copied") } else { p2_i18n::t("act.copy") }))
                            .on_click(cx.listener(move |this, _, window, cx| copy(this, &c2, window, cx))),
                    )
                    .child(hint(&p2_i18n::t("mng.menu.code.hint")).mt(px(0.)));
            }
            None if self.my_perms.can("member.invite") => menu = menu.child(hint(&p2_i18n::t("mng.menu.code.none"))),
            // Davet yetkim yoksa davet bölümü hiç görünmez.
            None => {}
        }
        // Workspace.tsx: left = min(düğme.sağ, pencere−236) − 224, top = düğme.alt + 6 (pencere koordinatı).
        // Düğme: ray 72 + kenar çubuğu genişliği − 10 − 32 sağ kenar; başlık 38 + 12 + 32.
        let vw = f32::from(window.viewport_size().width);
        let right: f32 = 72. + crate::resize::value(&crate::resize::SIDEBAR) - 13.;
        let left = right.min(vw - 236.) - 224.;
        let top = 86. - TOP;
        Some(
            div()
                .id("project-menu-layer")
                .absolute()
                .inset_0()
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    this.project_menu = false;
                    this.project_menu_categories = false;
                    cx.notify();
                }))
                // p2_enter öğeyi `relative` yapar: konum dış sarmalayıcıda.
                .child(div().absolute().left(px(left)).top(px(top)).child(menu.p2_enter("project-menu", Entrance::ScaleIn, Duration::ZERO)))
                .into_any_element(),
        )
    }

    /// Kanal / kategori sağ tık menüsü — `.channel-context-menu` (168px, 32px satırlar).
    pub(super) fn channel_menu_view(&self, p: &Palette, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.channel_menu.as_ref()?;
        let can = self.my_perms.can("channel.manage");
        let (soft, strong) = (p.accent_soft, p.text_strong);
        let entry = |id: &'static str, label: String, enabled: bool| -> Stateful<Div> {
            div()
                .id(id)
                .w_full()
                .h(px(32.))
                .flex_none()
                .flex()
                .items_center()
                .px(px(10.))
                .rounded(radius::XS)
                .text_color(p.text)
                .text_size(fs(12.))
                .line_height(lh(fs(12.)))
                .css_weight(600.)
                .map(|s| if enabled { s.cursor_pointer().hover(move |s| s.bg(soft).text_color(strong)) } else { s.opacity(0.45) })
                .child(label)
        };
        let mut items: Vec<AnyElement> = Vec::new();
        if let Some(ch) = menu.channel.clone() {
            let starred = self.starred.contains(&format!("CHANNEL:{}", ch.id));
            let id = ch.id.clone();
            items.push(entry("cm-star", if starred { p2_i18n::t("mng.cm.unstar") } else { p2_i18n::t("mng.cm.star") }, true).on_click(cx.listener(move |this, _, window, cx| {
                this.channel_menu = None;
                this.toggle_star("CHANNEL", id.clone(), window, cx)
            })).into_any_element());
            if can {
                let is_default = self.project.and_then(|i| self.projects[i].default_channel_id.clone()).as_deref() == Some(ch.id.as_str());
                let c = ch.clone();
                items.push(entry("cm-default", if is_default { p2_i18n::t("mng.cm.undefault") } else { p2_i18n::t("mng.cm.default") }, true).on_click(cx.listener(move |this, _, window, cx| this.make_default(if is_default { None } else { Some(c.clone()) }, window, cx))).into_any_element());
            }
            if self.my_perms.can_in(&ch.id, "channel.manage") {
                let c = ch.clone();
                items.push(entry("cm-delete", p2_i18n::t("mng.cm.delete"), true).on_click(cx.listener(move |this, _, window, cx| this.open_dialog(Dialog::DeleteChannel(c.clone()), window, cx))).into_any_element());
            }
        } else if let Some(cat) = menu.category.clone() {
            if can {
                items.push(entry("cm-delete", p2_i18n::t("mng.cm.delete"), true).on_click(cx.listener(move |this, _, window, cx| this.open_dialog(Dialog::DeleteCategory(cat.clone()), window, cx))).into_any_element());
            }
        }
        // Yetkim olmayan satırlar görünmez; hiç satır kalmadıysa menü de açılmaz.
        if items.is_empty() {
            return None;
        }
        // ContextMenuViewportGuard: pencere içinde 8px boşlukla.
        let (w, h) = (168., 14. + 32. * items.len() as f32);
        let vp = window.viewport_size();
        let x = f32::from(menu.at.x).min(f32::from(vp.width) - 8. - w).max(8.);
        let y = f32::from(menu.at.y).min(f32::from(vp.height) - 8. - h).max(8.) - TOP;
        let card = div()
            .id("channel-menu")
            .w(px(w))
            .p(px(6.))
            .flex()
            .flex_col()
            .rounded(radius::MD)
            .border_1()
            .border_color(p.border_strong)
            .bg(p.glass.background()).backdrop_blur(p2_ui::fx::BLUR)
            .shadow(p.shadow_lg())
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .children(items)
            .p2_enter("channel-menu", Entrance::ScaleIn, Duration::ZERO);
        Some(
            div()
                .id("channel-menu-layer")
                .absolute()
                .inset_0()
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                    this.channel_menu = None;
                    cx.notify();
                }))
                .on_mouse_down(MouseButton::Right, cx.listener(|this, _, _, cx| {
                    this.channel_menu = None;
                    cx.notify();
                }))
                .child(div().absolute().left(px(x)).top(px(y)).child(card))
                .into_any_element(),
        )
    }
}

/// `.p2-textarea` (min-h 84, satır 1,45; rows=3).
fn textarea(state: &Entity<TextareaState>, p: &Palette, window: &Window, cx: &App) -> Div {
    let focused = gpui_kit::Focusable::focus_handle(state.read(cx), cx).is_focused(window);
    let bs = p.border_strong;
    div()
        .min_h(px(84.))
        .px(px(12.))
        .py(px(9.))
        .border_1()
        .rounded(radius::SM)
        .bg(p.bg_3)
        .text_size(font_size::MD)
        .line_height(gpui_kit::relative(1.45))
        .font_weight(FontWeight::MEDIUM)
        .text_color(p.text)
        .map(|s| if focused { s.border_color(p.border_accent).shadow(shadows(&[p.ring])) } else { s.border_color(p.border).hover(move |s| s.border_color(bs)) })
        .child(Textarea::new(state).appearance(false).text_size(font_size::MD).line_height(gpui_kit::relative(1.45)))
}

/// `text-transform: uppercase` (tr).
fn p2_ui_upper(s: &str) -> String {
    super::super::admin::tr_upper(s)
}
