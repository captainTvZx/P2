//! Kütüphane — LibraryPage.tsx + library.css birebir: solda uygulama listesi (arama, sürüklenebilir
//! genişlik), sağda kapak (hero), yapışkan eylem çubuğu (YÜKLE/ÇALIŞTIR/DURDUR bölünmüş düğme +
//! istatistikler + kalem/çark), indirme paneli, konum uyarısı, iki sütun: Aktivite/Notlar/Ekran
//! görüntüleri sekmeleri ve sağda Oynayan arkadaşlar, Başarımlar, İncelemeler, Görevler.
//! Sağ tık menüsü, Sürümler / Sürüm yönetimi / yükleyici / düzenleme / başarım / kurulumlar
//! pencereleri `library_dialogs.rs`'te; notlar ve ekran görüntüleri kendi dosyalarında.
//! Ağır iş `p2-patcher` motorunda.

#[path = "library_ui.rs"]
pub(crate) mod ui;
#[path = "library_editor.rs"]
pub(crate) mod editor;
#[path = "library_dialogs.rs"]
mod dialogs;
#[path = "library_notes.rs"]
mod notes;
#[path = "library_shots.rs"]
mod shots;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::prelude::*;
use gpui_kit::{AnimationExt as _, Focusable as _, AnyElement, Context, Entity, EventEmitter, FontWeight, ImageSource, MouseButton, MouseDownEvent, ObjectFit, PathPromptOptions, Pixels, RenderImage, ScrollHandle, SharedString, Subscription, Window, div, img, point, px, relative};
use p2_client_core::{Api, ApiError, HubData, HubPost, Project, ReleaseDiff, ReleaseInstall, ReleaseList, ReleaseView};
use p2_patcher::{JobState, LibraryState, Patcher, UploadState};
use p2_ui::P2Fx as _;
use p2_ui::P2Text as _;
use p2_ui::components::user_initials;
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::{Palette, css_rgba, font_size, fs, radius, shadows};
use p2_ui::{Entrance, P2Motion as _};

use crate::task;
use editor::NoteBox;
use ui::*;

/// Çalışma alanına giden istekler (Electron: `onOpenSettings`, `ProjectSettingsDialog`, `onOpenBoard`, `onSelectProject`).
pub enum LibraryEvent {
    OpenSettings,
    ProjectSettings(String),
    OpenBoard,
    SelectProject(String),
}

impl EventEmitter<LibraryEvent> for LibraryPage {}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Tab {
    Activity,
    Notes,
    Shots,
}

/// Onay penceresinin eylemi (ConfirmDialog).
#[derive(Clone)]
pub(super) enum Act {
    Verify(String),
    Uninstall(String),
    DeletePost(String),
    DeleteAchievement(String),
    Publish(String),
    Live(String),
    DeleteRelease(String),
    InstallVersion(ReleaseView, bool),
}

#[derive(Clone)]
pub(super) struct Confirm {
    pub title: String,
    pub body: String,
    pub label: String,
    pub danger: bool,
    pub act: Act,
}

/// Sağ tık menüsü (ekran konumu + hedef proje).
#[derive(Clone)]
pub(super) struct Menu {
    pub x: f32,
    pub y: f32,
    pub target: usize,
    pub epoch: usize,
    /// 0 = fare konumu, 1 = ÇALIŞTIR yanındaki ok, 2 = eylem çubuğu çarkı (`openMenuBelow`).
    pub anchor: u8,
}

/// Görev satırı (Görevler bölümü).
#[derive(Clone)]
pub(super) struct TaskRow {
    pub number: i64,
    pub title: String,
    pub status: String,
    pub priority: String,
    pub board: String,
}

/// Yükleyici (LibraryUploader.tsx): form → çalışıyor → hazır.
pub(super) struct Uploader {
    pub dir: Option<PathBuf>,
    pub executables: Vec<String>,
    pub exe: usize,
    pub file_count: u64,
    pub total_size: u64,
    pub channel: Entity<InputState>,
    pub version: Entity<InputState>,
    pub args: Entity<InputState>,
    pub notes: NoteBox,
    pub phase: &'static str,
    pub release: Option<String>,
    pub diff: Option<Option<ReleaseDiff>>,
    pub error: Option<String>,
    pub busy: bool,
    pub exe_open: bool,
    pub _subs: Vec<Subscription>,
}

/// Sürüm düzenleme (EditReleaseDialog).
pub(super) struct EditRelease {
    pub release: ReleaseView,
    pub channel: Entity<InputState>,
    pub version: Entity<InputState>,
    pub args: Entity<InputState>,
    pub notes: NoteBox,
    pub busy: bool,
    pub error: Option<String>,
}

/// Başarım ekleme (AchievementDialog).
pub(super) struct AchDialog {
    pub icon: String,
    pub custom: Entity<InputState>,
    pub title: Entity<InputState>,
    pub description: Entity<InputState>,
    pub busy: bool,
    pub error: Option<String>,
    pub _subs: Vec<Subscription>,
}

pub struct LibraryPage {
    pub(super) api: Arc<Api>,
    pub(super) patcher: Arc<Patcher>,
    pub(super) org: String,
    pub(super) projects: Vec<Project>,
    pub(super) project: usize,
    pub(super) me: String,
    pub(super) me_name: String,
    pub(super) me_avatar: Option<String>,
    pub(super) can_edit_project: bool,
    pub(super) state: LibraryState,
    pub(super) jobs: HashMap<String, JobState>,
    pub(super) running: Vec<String>,
    pub(super) releases: Option<ReleaseList>,
    pub(super) channel: Option<String>,
    pub(super) hub: Option<HubData>,
    pub(super) tasks: Option<(Vec<TaskRow>, usize)>,
    pub(super) tab: Tab,
    pub(super) tab_epoch: usize,
    search: Entity<InputState>,
    scroll: ScrollHandle,
    pub(super) post: NoteBox,
    pub(super) post_busy: bool,
    pub(super) editing_post: Option<(String, NoteBox)>,
    pub(super) review_open: Option<bool>,
    pub(super) review_rec: bool,
    pub(super) review_body: Entity<TextareaState>,
    pub(super) review_busy: bool,
    pub(super) menu: Option<Menu>,
    /// ÇALIŞTIR/YÜKLE yanındaki ok düğmesinin pencere sınırları (`openMenuBelow`: `box.left, box.bottom + 6`).
    pub(super) caret_bounds: std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<gpui_kit::Pixels>>>>,
    pub(super) confirm: Option<Confirm>,
    pub(super) versions_open: bool,
    pub(super) manage_open: bool,
    pub(super) uploader: Option<Uploader>,
    pub(super) editing: Option<EditRelease>,
    pub(super) achievement: Option<AchDialog>,
    pub(super) installs: Option<(ReleaseView, Option<Vec<ReleaseInstall>>)>,
    pub(super) diffs: HashMap<String, Option<Option<ReleaseDiff>>>,
    pub(super) expanded: HashSet<String>,
    pub(super) notes: Option<Entity<notes::NotesPanel>>,
    pub(super) shots: Option<Entity<shots::ShotsPanel>>,
    cover: Option<(String, Arc<RenderImage>)>,
    pub(super) upload: Option<UploadState>,
    pub(super) upload_cancel: Arc<AtomicBool>,
    _subs: Vec<Subscription>,
}

fn job_percent(j: &JobState) -> u32 {
    if j.bytes_total > 0 {
        ((j.bytes_done as f64 / j.bytes_total as f64) * 100.).round().min(100.) as u32
    } else if j.total > 0 {
        ((j.done as f64 / j.total as f64) * 100.).round() as u32
    } else {
        0
    }
}

impl LibraryPage {
    #[allow(clippy::too_many_arguments)]
    pub fn new(api: Arc<Api>, patcher: Arc<Patcher>, org: String, projects: Vec<Project>, project: usize, me: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = patcher.state();
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(p2_i18n::t("lib.search")));
        let review_body = cx.new(|cx| TextareaState::new(window, cx).auto_grow(3, 8).placeholder(p2_i18n::t("lib.review.placeholder")));
        let subs = vec![cx.subscribe(&search, |_, _, _: &InputEvent, cx| cx.notify())];
        let name = projects.get(project).map(|p| p.name.clone()).unwrap_or_default();
        let post = NoteBox::new(&p2_i18n::tv("lib.post.placeholder", &[("name", &name)]), true, window, cx);
        let mut page = Self {
            api,
            patcher,
            org,
            projects,
            project,
            me,
            me_name: String::new(),
            me_avatar: None,
            can_edit_project: false,
            state,
            jobs: HashMap::new(),
            running: Vec::new(),
            releases: None,
            channel: None,
            hub: None,
            tasks: None,
            tab: Tab::Activity,
            tab_epoch: 0,
            search,
            scroll: ScrollHandle::new(),
            post,
            post_busy: false,
            editing_post: None,
            review_open: None,
            review_rec: true,
            review_body,
            review_busy: false,
            menu: None,
            caret_bounds: Default::default(),
            confirm: None,
            versions_open: false,
            manage_open: false,
            uploader: None,
            editing: None,
            achievement: None,
            installs: None,
            diffs: HashMap::new(),
            expanded: HashSet::new(),
            notes: None,
            shots: None,
            cover: None,
            upload: None,
            upload_cancel: Arc::new(AtomicBool::new(false)),
            _subs: subs,
        };
        page.load_identity(window, cx);
        page.load(window, cx);
        page
    }

    pub(super) fn pid(&self) -> String {
        self.projects[self.project].id.clone()
    }

    pub(super) fn pname(&self) -> String {
        self.projects[self.project].name.clone()
    }

    pub(super) fn fail(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        let m = message.into();
        crate::trace(&format!("kütüphane: hata {m}"));
        // Bildirimler sağ üstte (toasts.rs); sayfanın içinde ayrı hata şeridi yok.
        crate::toasts::error(m);
        cx.notify();
    }

    pub(super) fn fail_api(&mut self, e: ApiError, fallback: &str, cx: &mut Context<Self>) {
        self.fail(if e.message.is_empty() { fallback.to_string() } else { e.message }, cx);
    }

    pub fn select_project(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        if ix >= self.projects.len() {
            return;
        }
        self.project = ix;
        self.releases = None;
        self.hub = None;
        self.tasks = None;
        self.notes = None;
        self.shots = None;
        self.cover = None;
        self.diffs.clear();
        self.editing_post = None;
        self.review_open = None;
        // Düzenleme yetkisi projeye göre değişir.
        self.load_identity(window, cx);
        let name = self.pname();
        self.post = NoteBox::new(&p2_i18n::tv("lib.post.placeholder", &[("name", &name)]), true, window, cx);
        crate::titlebar::set_project_title(&name);
        cx.emit(LibraryEvent::SelectProject(self.pid()));
        self.load(window, cx);
        if self.tab != Tab::Activity {
            self.ensure_tab(window, cx);
        }
    }

    /// Ben (ad/fotoğraf) ve seçili projedeki düzenleme yetkisi (`channel.manage`, proje başına).
    fn load_identity(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (api, project) = (self.api.clone(), self.pid());
        task::run(window, cx, move || (api.me(), api.project_permissions(&project)), |this, (me, perms), _, cx| {
            if let Ok(user) = me {
                this.me_name = user.display_name.clone();
                this.me_avatar = user.avatar_url.clone();
            }
            if let Ok(perms) = perms {
                this.can_edit_project = perms.can("channel.manage");
            }
            cx.notify();
        });
    }

    pub(super) fn load(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.pid());
        crate::titlebar::set_project_title(&self.pname());
        task::run(window, cx, move || api.releases(&org, &project), |this, releases, window, cx| {
            match releases {
                Ok(list) => {
                    crate::trace(&format!("kütüphane: {} sürüm, {} kategori", list.releases.len(), list.channels.len()));
                    let installed = this.state.projects.get(&this.pid()).map(|p| p.channel.clone());
                    let keep = this.channel.clone().filter(|c| list.channels.contains(c));
                    this.channel = keep.or_else(|| installed.filter(|c| list.channels.contains(c))).or_else(|| list.channels.first().cloned());
                    this.releases = Some(list);
                }
                Err(e) => this.fail_api(e, &p2_i18n::t("lib.err.releasesLoad"), cx),
            }
            this.autopilot(window, cx);
            cx.notify();
        });
        self.load_hub(window, cx);
        self.load_tasks(window, cx);
        self.load_cover(window, cx);
        self.autopilot_view(window, cx);
    }

    pub(super) fn load_hub(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.pid());
        task::run(window, cx, move || api.hub(&org, &project), |this, hub, _, cx| {
            this.hub = hub.ok();
            if let Some(h) = &this.hub {
                crate::trace(&format!("kütüphane: merkez {} gönderi, {} başarım, {} inceleme", h.posts.len(), h.achievements.len(), h.reviews.len()));
            }
            cx.notify();
        });
    }

    /// ProjectTasks: panolardaki açık işler (Görevler).
    fn load_tasks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.pid());
        task::run(
            window,
            cx,
            move || {
                let boards = api.boards(&org, &project).ok()?;
                let mut rows = Vec::new();
                for b in &boards {
                    if let Ok(content) = api.board(&org, &project, &b.id) {
                        rows.extend(content.tasks.into_iter().map(|t| TaskRow { number: t.number, title: t.title, status: t.status, priority: t.priority, board: b.name.clone() }));
                    }
                }
                Some((rows, boards.len()))
            },
            |this, rows, _, cx| {
                this.tasks = Some(rows.unwrap_or_default());
                cx.notify();
            },
        );
    }

    /// Kapak görseli (yalnız üstte; GPUI'de uzak resim yükleyici yok).
    fn load_cover(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(url) = self.projects[self.project].cover_url.clone().filter(|u| !u.trim().is_empty()) else { return };
        let path = url.find("/uploads/").map(|i| url[i..].to_string()).unwrap_or(url.clone());
        let api = self.api.clone();
        task::run(
            window,
            cx,
            move || {
                let bytes = api.get_bytes(&path).ok()?;
                let mut rgba = image::load_from_memory(&bytes).ok()?.to_rgba8();
                for px in rgba.pixels_mut() {
                    px.0.swap(0, 2);
                }
                Some(Arc::new(RenderImage::new(vec![image::Frame::new(rgba)])))
            },
            move |this, image, _, cx| {
                if let Some(image) = image {
                    this.cover = Some((url, image));
                    cx.notify();
                }
            },
        );
    }

    /// Otomatik pilot: P2_AUTOPILOT_LIBRARY = install | play | verify | uninstall.
    fn autopilot(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(action) = std::env::var("P2_AUTOPILOT_LIBRARY") else { return };
        static DONE: AtomicBool = AtomicBool::new(false);
        if DONE.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        match action.as_str() {
            "install" => self.install(None, false, window, cx),
            "play" => self.play(None, cx),
            "verify" => self.verify(None, window, cx),
            "uninstall" => self.uninstall(None, cx),
            _ => {}
        }
    }

    /// Parite aracı için yalnız GÖRÜNÜM açan kancalar (veri yazmaz):
    /// `P2_AUTOPILOT_LIBRARY_VIEW` = notes | screenshots | hub | menu | versions | manage | achievement | upload.
    fn autopilot_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Ok(view) = std::env::var("P2_AUTOPILOT_LIBRARY_VIEW") else { return };
        static DONE: AtomicBool = AtomicBool::new(false);
        if DONE.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        crate::trace(&format!("kütüphane: görünüm {view}"));
        match view.as_str() {
            "notes" => self.set_tab(Tab::Notes, window, cx),
            "screenshots" => self.set_tab(Tab::Shots, window, cx),
            "hub" => {
                // Proje merkezi: sütunlar (aktivite + sağ bölümler) görünür olsun.
                let scroll = self.scroll.clone();
                cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(Duration::from_millis(700)).await;
                    scroll.set_offset(point(px(0.), px(-430.)));
                    let _ = this.update(cx, |_, cx| cx.notify());
                })
                .detach();
            }
            "menu" => self.menu = Some(Menu { x: 0., y: 0., target: self.project, epoch: 1, anchor: 1 }),
            "versions" => self.versions_open = true,
            "manage" => self.manage_open = true,
            "achievement" => self.open_achievement(window, cx),
            "upload" => self.open_uploader(window, cx),
            _ => {}
        }
        cx.notify();
    }

    // ------------------------------------------------------------ motor olayları (çalışma alanından)

    pub fn on_job(&mut self, job: JobState, cx: &mut Context<Self>) {
        if !job.active && (job.phase == "done" || job.phase == "canceled") {
            self.jobs.remove(&job.project_id);
        } else {
            self.jobs.insert(job.project_id.clone(), job);
        }
        cx.notify();
    }

    pub fn on_upload(&mut self, up: UploadState, cx: &mut Context<Self>) {
        if let Some(u) = self.uploader.as_mut() {
            if let Some(r) = &up.release_id {
                u.release = Some(r.clone());
            }
        }
        self.upload = Some(up);
        cx.notify();
    }

    pub fn on_running(&mut self, project: String, running: bool, cx: &mut Context<Self>) {
        self.running.retain(|p| *p != project);
        if running {
            self.running.push(project);
        }
        cx.notify();
    }

    pub fn on_state(&mut self, cx: &mut Context<Self>) {
        self.state = self.patcher.state();
        cx.notify();
    }

    // ------------------------------------------------------------ eylemler

    pub(super) fn live(&self) -> Option<&ReleaseView> {
        let ch = self.active_channel()?;
        self.releases.as_ref()?.latest.get(&ch)?.as_ref()
    }

    /// Etkin kategori: seçili, yoksa kurulu, yoksa ilk.
    pub(super) fn active_channel(&self) -> Option<String> {
        let list = self.releases.as_ref()?;
        if let Some(c) = self.channel.as_ref().filter(|c| list.channels.contains(c)) {
            return Some(c.clone());
        }
        let installed = self.state.projects.get(&self.pid()).map(|p| p.channel.clone());
        installed.filter(|c| list.channels.contains(c)).or_else(|| list.channels.first().cloned())
    }

    pub(super) fn published(&self) -> Vec<ReleaseView> {
        let mut out: Vec<ReleaseView> = self.releases.as_ref().map(|l| l.releases.iter().filter(|r| r.status == "PUBLISHED").cloned().collect()).unwrap_or_default();
        out.sort_by_key(|r| std::cmp::Reverse(parse_ms(r.published_at.as_deref().unwrap_or(&r.created_at)).unwrap_or(0)));
        out
    }

    pub(super) fn can_publish(&self) -> bool {
        self.releases.as_ref().is_some_and(|l| l.permissions.publish)
    }

    pub(super) fn choose_root(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Ok(root) = std::env::var("P2_LIBRARY_ROOT") {
            self.patcher.set_root(&root);
            self.state = self.patcher.state();
            return cx.notify();
        }
        let rx = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: Some(p2_i18n::t("lib.rootPrompt").into()) });
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                if let Some(p) = paths.first() {
                    let root = p.join("P2ProjectLibrary").to_string_lossy().to_string();
                    let _ = this.update(cx, |this, cx| {
                        this.patcher.set_root(&root);
                        this.state = this.patcher.state();
                        cx.notify();
                    });
                }
            }
        })
        .detach();
    }

    /// `pinned`: elle seçilen eski/başka sürüm canlıya otomatik geçmez.
    pub(super) fn install(&mut self, release: Option<ReleaseView>, pinned: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.root.is_empty() {
            self.choose_root(window, cx);
            if self.state.root.is_empty() {
                return;
            }
        }
        let Some(target) = release.or_else(|| self.live().cloned()) else { return self.fail(p2_i18n::t("lib.err.noLiveRelease"), cx) };
        self.channel = Some(target.channel.clone());
        crate::trace(&format!("kütüphane: kurulum başladı {} {}", target.channel, target.version));
        self.patcher.install(self.org.clone(), self.pid(), self.pname(), target.id.clone(), target.channel.clone(), target.version.clone(), Some(pinned));
        cx.notify();
    }

    pub(super) fn play(&mut self, target: Option<usize>, cx: &mut Context<Self>) {
        let ix = target.unwrap_or(self.project);
        let (id, name) = (self.projects[ix].id.clone(), self.projects[ix].name.clone());
        match self.patcher.launch(&self.org, &id, &name) {
            Ok(()) => crate::trace("kütüphane: oyun başlatıldı"),
            Err(e) => self.fail(e, cx),
        }
    }

    pub(super) fn stop(&mut self, target: Option<usize>, cx: &mut Context<Self>) {
        let id = self.projects[target.unwrap_or(self.project)].id.clone();
        if let Err(e) = self.patcher.stop(&id) {
            self.fail(e, cx);
        }
    }

    pub(super) fn verify(&mut self, target: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let project = target.unwrap_or_else(|| self.pid());
        let name = self.projects.iter().find(|p| p.id == project).map(|p| p.name.clone()).unwrap_or_default();
        let (patcher, org) = (self.patcher.clone(), self.org.clone());
        crate::toasts::info(&p2_i18n::t("lib.verify.running"));
        cx.notify();
        task::run(window, cx, move || patcher.verify(&org, &project, &name), |this, result, _, cx| {
            let note = match result {
                Ok(0) => {
                    crate::toasts::ok(&p2_i18n::t("lib.verify.ok"));
                    p2_i18n::t("lib.verify.ok")
                }
                Ok(n) => {
                    let text = p2_i18n::tv("lib.verify.broken", &[("n", &n.to_string())]);
                    crate::toasts::warn(&text);
                    text
                }
                Err(e) => {
                    this.fail(e, cx);
                    String::new()
                }
            };
            crate::trace(&format!("kütüphane: doğrulama {note}"));
            cx.notify();
        });
    }

    pub(super) fn uninstall(&mut self, target: Option<String>, cx: &mut Context<Self>) {
        let project = target.unwrap_or_else(|| self.pid());
        match self.patcher.uninstall(&self.org, &project) {
            Ok(()) => {
                crate::trace("kütüphane: kaldırıldı");
                self.state = self.patcher.state();
            }
            Err(e) => self.fail(e, cx),
        }
        cx.notify();
    }

    pub(super) fn release_action(&mut self, release: String, live: bool, window: &mut Window, cx: &mut Context<Self>) {
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.pid());
        task::run(window, cx, move || if live { api.make_release_live(&org, &project, &release) } else { api.publish_release(&org, &project, &release) }, move |this, result, window, cx| match result {
            Ok(_) => {
                crate::trace(&format!("kütüphane: sürüm {}", if live { "canlıya alındı" } else { "yayınlandı" }));
                this.load(window, cx);
            }
            Err(e) => this.fail_api(e, &p2_i18n::t("lib.err.actionFailed"), cx),
        });
    }

    /// Otomatik pilot / testler: klasör ve alanları doldurup yükle.
    pub fn autopilot_upload(&mut self, dir: PathBuf, exe: &str, channel: &str, version: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.open_uploader(window, cx);
        if let Some(u) = self.uploader.as_mut() {
            u.dir = Some(dir);
            u.executables = vec![exe.to_string()];
            u.exe = 0;
            for (input, v) in [(&u.channel, channel), (&u.version, version)] {
                let v = v.to_string();
                input.update(cx, |i, cx| i.set_value(v, window, cx));
            }
        }
        self.start_upload(window, cx);
    }

    pub(super) fn set_tab(&mut self, tab: Tab, window: &mut Window, cx: &mut Context<Self>) {
        if self.tab != tab {
            self.tab = tab;
            self.tab_epoch += 1;
        }
        self.ensure_tab(window, cx);
        cx.notify();
    }

    fn ensure_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let manage = self.hub.as_ref().is_some_and(|h| h.permissions.manage);
        match self.tab {
            Tab::Notes if self.notes.is_none() => {
                let (api, org, project, me) = (self.api.clone(), self.org.clone(), self.pid(), self.me.clone());
                self.notes = Some(cx.new(|cx| notes::NotesPanel::new(api, org, project, me, manage, window, cx)));
            }
            Tab::Shots if self.shots.is_none() => {
                let (api, org, project, me) = (self.api.clone(), self.org.clone(), self.pid(), self.me.clone());
                self.shots = Some(cx.new(|cx| shots::ShotsPanel::new(api, org, project, me, manage, window, cx)));
            }
            _ => {}
        }
    }

    // ---- vitrin (hub) işlemleri ----

    fn submit_post(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.post.is_empty(cx) || self.post_busy {
            return;
        }
        let content = self.post.value(cx);
        self.post_busy = true;
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.pid());
        task::run(window, cx, move || api.create_hub_post(&org, &project, &content), |this, result, window, cx| {
            this.post_busy = false;
            match result {
                Ok(_) => {
                    this.post.clear(window, cx);
                    this.load_hub(window, cx);
                }
                Err(e) => this.fail_api(e, &p2_i18n::t("lib.err.postFailed"), cx),
            }
            cx.notify();
        });
    }

    fn save_post(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, nb)) = self.editing_post.as_ref() else { return };
        let content = nb.value(cx);
        if content.trim().is_empty() {
            return;
        }
        let (api, org, project, id) = (self.api.clone(), self.org.clone(), self.pid(), id.clone());
        task::run(window, cx, move || api.update_hub_post(&org, &project, &id, &content), |this, result, window, cx| match result {
            Ok(_) => {
                this.editing_post = None;
                this.load_hub(window, cx);
            }
            Err(e) => this.fail_api(e, &p2_i18n::t("lib.err.saveFailed"), cx),
        });
    }

    fn toggle_unlock(&mut self, id: String, unlocked: bool, window: &mut Window, cx: &mut Context<Self>) {
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.pid());
        task::run(window, cx, move || api.unlock_achievement(&org, &project, &id, unlocked), |this, _, window, cx| this.load_hub(window, cx));
    }

    fn save_review(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let body: String = self.review_body.read(cx).value().chars().take(2000).collect();
        let rec = self.review_rec;
        self.review_busy = true;
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.pid());
        task::run(window, cx, move || api.save_review(&org, &project, rec, &body), |this, result, window, cx| {
            this.review_busy = false;
            match result {
                Ok(_) => {
                    this.review_open = Some(false);
                    this.load_hub(window, cx);
                }
                Err(e) => this.fail_api(e, &p2_i18n::t("lib.err.saveFailed"), cx),
            }
        });
    }

    fn delete_review(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.pid());
        task::run(window, cx, move || api.delete_review(&org, &project), |this, _, window, cx| {
            this.review_open = None;
            this.load_hub(window, cx);
        });
    }

    /// Onay penceresinin eylemini çalıştırır.
    pub(super) fn run_confirm(&mut self, act: Act, window: &mut Window, cx: &mut Context<Self>) {
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.pid());
        match act {
            Act::Verify(id) => self.verify(Some(id), window, cx),
            Act::Uninstall(id) => self.uninstall(Some(id), cx),
            Act::DeletePost(id) => task::run(window, cx, move || api.delete_hub_post(&org, &project, &id), |this, _, window, cx| this.load_hub(window, cx)),
            Act::DeleteAchievement(id) => task::run(window, cx, move || api.delete_achievement(&org, &project, &id), |this, _, window, cx| this.load_hub(window, cx)),
            Act::Publish(id) => self.release_action(id, false, window, cx),
            Act::Live(id) => self.release_action(id, true, window, cx),
            Act::DeleteRelease(id) => task::run(window, cx, move || api.delete_release(&org, &project, &id), |this, result, window, cx| match result {
                Ok(()) => this.load(window, cx),
                Err(e) => this.fail_api(e, &p2_i18n::t("lib.err.deleteFailed"), cx),
            }),
            Act::InstallVersion(release, pinned) => {
                self.versions_open = false;
                self.install(Some(release), pinned, window, cx);
            }
        }
        cx.notify();
    }

    pub(super) fn ask(&mut self, title: impl Into<String>, body: impl Into<String>, label: &str, danger: bool, act: Act, cx: &mut Context<Self>) {
        self.menu = None;
        self.confirm = Some(Confirm { title: title.into(), body: body.into(), label: label.into(), danger, act });
        cx.notify();
    }

    fn open_menu_at(&mut self, target: usize, x: f32, y: f32, cx: &mut Context<Self>) {
        self.open_menu(target, x, y, 0, cx);
    }

    fn open_menu(&mut self, target: usize, x: f32, y: f32, anchor: u8, cx: &mut Context<Self>) {
        let epoch = self.menu.as_ref().map_or(0, |m| m.epoch + 1);
        self.menu = Some(Menu { x, y, target, epoch, anchor });
        cx.notify();
    }

    /// İçerik kaydırma miktarı (px, pozitif).
    pub(super) fn scrolled(&self) -> f32 {
        -f32::from(self.scroll.offset().y)
    }

    /// LibraryPage.tsx `p2.library.side` (sürüklenebilir sol panel, 190–440).
    pub(super) fn side_width(&self) -> f32 {
        crate::resize::value(&crate::resize::LIBRARY_SIDE)
    }

    // ------------------------------------------------------------ çizim: sol panel

    fn side_panel(&self, window: &Window, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let query = self.search.read(cx).value().trim().to_lowercase();
        let apps: Vec<(usize, &Project)> = self.projects.iter().enumerate().filter(|(_, p)| p.library_enabled).collect();
        let listed: Vec<(usize, &Project)> = apps.iter().copied().filter(|(_, p)| query.is_empty() || p.name.to_lowercase().contains(&query)).collect();
        let mut games = div().id("library-games").flex_1().min_h_0().overflow_y_scroll().pt(px(2.)).px(px(6.)).pb(px(12.)).flex().flex_col();
        if listed.is_empty() {
            let text = if apps.is_empty() { p2_i18n::t("lib.side.noApps") } else { p2_i18n::t("lib.side.noMatchApps") };
            games = games.child(div().my(px(10.)).mx(px(12.)).text_color(p.text_3).text_size(font_size::XS).line_height(relative(1.45)).child(text));
        }
        for (n, (ix, project)) in listed.iter().enumerate() {
            let ix = *ix;
            let active = ix == self.project;
            let installed = self.state.projects.get(&project.id);
            let running = self.running.contains(&project.id);
            let job = self.jobs.get(&project.id).filter(|j| j.active);
            let (hover_bg, strong) = (p.bg_hover, p.text_strong);
            let color = if active { p.text_strong } else if installed.is_some() { p.text_2 } else { p.text_3 };
            let name_color = if running { p.success_text } else { color };
            let state: Option<AnyElement> = if let Some(j) = job {
                let label = match j.phase.as_str() {
                    "downloading" | "preparing" => p2_i18n::tv("lib.pct", &[("n", &(if j.bytes_total > 0 { ((j.bytes_done as f64 / j.bytes_total as f64) * 100.).round() as u32 } else { 0 }).to_string())]),
                    "installing" => p2_i18n::t("lib.side.installing"),
                    _ => "…".into(),
                };
                Some(
                    div()
                        .min_w(px(36.))
                        .px(px(6.))
                        .py(px(2.))
                        .flex()
                        .justify_center()
                        .rounded(radius::PILL)
                        .bg(p.accent_soft)
                        .text_color(p.text_accent)
                        .text_size(fs(10.))
                        .css_weight(750.)
                        .child(label)
                        .with_animation(SharedString::from(format!("lib-pct-{ix}")), gpui_kit::Animation::new(Duration::from_millis(1600)).repeat(), |el, t| el.opacity(1. - 0.5 * (1. - (2. * t - 1.).abs())))
                        .into_any_element(),
                )
            } else if running {
                Some(state_dot(p).with_animation(SharedString::from(format!("lib-run-{ix}")), gpui_kit::Animation::new(Duration::from_millis(1400)).repeat(), |el, t| el.opacity(1. - 0.5 * (1. - (2. * t - 1.).abs()))).into_any_element())
            } else {
                installed.map(|_| state_dot(p).into_any_element())
            };
            let accent_bar = p.accent_hover;
            games = games.child(
                div()
                    .id(("lib-game", ix))
                    .relative()
                    .w_full()
                    .min_h(px(34.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(9.))
                    .rounded(radius::SM)
                    .text_color(color)
                    .cursor_pointer()
                    .when(active, |s| s.bg(p.bg_selected).shadow(shadows(&[p2_ui::tokens::Shadow { x: 2., y: 0., blur: 0., spread: 0., color: accent_bar, inset: true }])))
                    .when(!active, |s| s.hover(move |s| s.bg(hover_bg).text_color(strong).left(px(2.))))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if ix != this.project {
                            this.select_project(ix, window, cx);
                        }
                    }))
                    .on_mouse_down(MouseButton::Right, cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.open_menu_at(ix, f32::from(e.position.x), f32::from(e.position.y), cx);
                    }))
                    .child(project_icon(project, 22., 6., 8., false, p))
                    .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_size(fs(12.5)).font_weight(FontWeight::SEMIBOLD).text_color(name_color).child(project.name.clone()))
                    .children(state)
                    .p2_enter(("lib-game-in", ix), Entrance::Rise, Duration::from_millis((n as u64 * 28).min(360))),
            );
        }
        if listed.is_empty() {
            games = games.child(div().py(px(10.)).px(px(8.)).child(hint(if query.is_empty() { p2_i18n::t("lib.side.noProjects") } else { p2_i18n::t("lib.side.noMatchProjects") }, p)));
        }
        let (soft, strong) = (p.accent_soft, p.text_strong);
        let search_focused = self.search.read(cx).focus_handle(cx).is_focused(window);
        div()
            .relative()
            .w(px(self.side_width()))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(p.border_subtle)
            .bg(p.bg_1)
            .p2_enter("library-side", Entrance::SlideLeft, Duration::ZERO)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.))
                    .pt(px(12.))
                    .pr(px(10.))
                    .pb(px(8.))
                    .pl(px(14.))
                    .child(div().text_color(p.text_strong).text_size(fs(13.)).font_weight(FontWeight::BOLD).tracking(0.02).child(p2_i18n::t("lib.title")))
                    .child(
                        div()
                            .id("library-gear")
                            .size(px(28.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(radius::XS)
                            .text_color(p.text_3)
                            .cursor_pointer()
                            // .library-gear:hover { transform: rotate(30deg) } (transform var(--m-base) var(--ease-pop))
                            .hover(move |s| { use p2_ui::P2Fx as _; s.bg(soft).text_color(strong).rotate(30.) })
                            .map(|s| { use p2_ui::P2Transition as _; s.transition(p2_ui::motion::M_BASE, p2_ui::motion::EASE_POP) })
                            .tooltip(|w, cx| p2_ui::controls::tooltip(p2_i18n::t("lib.settingsTip"), w, cx))
                            .group("library-gear").child(ico(GEAR, 1.8, 16., p.text_3).group_hover("library-gear", move |s| s.text_color(strong)))
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(LibraryEvent::OpenSettings))),
                    ),
            )
            .child(
                div()
                    .h(px(30.))
                    .mx(px(10.))
                    .mb(px(6.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .border_1()
                    .border_color(if search_focused { p.border_accent } else { p.border })
                    .when(search_focused, |s| s.shadow(shadows(&[p.ring])))
                    .rounded(radius::SM)
                    .bg(p.bg_3)
                    .text_color(p.text_4)
                    .child(div().w(px(18.)).flex().justify_center().child(ico(SEARCH, 2., 14., p.text_4)))
                    .child(div().flex_1().min_w_0().text_size(fs(12.)).text_color(p.text).child(Input::new(&self.search).appearance(false).h(px(26.)).px(px(2.)).text_size(fs(12.)))),
            )
            .child(games)
            // `.library-resizer`: 6px, kenarın 3px dışına taşar; `:hover, :active` → `accent-soft-2`.
            .child({
                let tint = p.accent_soft_2;
                crate::resize::strip("library-resizer", crate::resize::LIBRARY_SIDE)
                    .right(px(-3.))
                    .w(px(6.))
                    .hover(move |s| s.bg(tint))
                    .when(crate::resize::dragging(crate::resize::LIBRARY_SIDE.key), move |s| s.bg(tint))
            })
            .into_any_element()
    }

    // ------------------------------------------------------------ çizim: kapak + eylem çubuğu

    fn content_width(&self, window: &Window) -> f32 {
        // Ray 72 + sol panel + kaydırma çubuğu oluğu 10 (Chromium `overflow-y: auto`).
        (f32::from(window.viewport_size().width) - 72. - self.side_width() - 10.).max(320.)
    }

    fn hero(&self, width: f32, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        const HERO_W: f32 = 800.;
        let project = &self.projects[self.project];
        let pid = project.id.clone();
        let h = 330.;
        let svg = |c: gpui_kit::Hsla| p2_ui::icons::svg_color(c);
        let (s2, s2a) = svg(p.accent_soft_2);
        let (b2, _) = svg(p.bg_2);
        let (b0, _) = svg(p.bg_0);
        // Hareketli GIF kapak oynar; durağan kapak tam boyutuyla çizilir.
        let cover = super::discover_images::animated_cover(project.cover_url.as_deref()).or_else(|| self.cover.as_ref().filter(|(u, _)| project.cover_url.as_deref() == Some(u.as_str())).map(|(_, i)| i.clone()));
        // `.no-cover`: radial(90% 80% at 20% 0%, accent-soft-2, transparent 70%) + linear(180, bg-2, bg-0);
        // `.library-hero-fade`: rgba(0,0,0,.08) 0 → saydam 28% → bg-0 %58 64% → bg-0 100%.
        // Yataydaki her şey genişlikle orantılı (elips merkezi/yarıçapı, dikey gradyanlar): SVG sabit
        // `HERO_W` genişlikte bir kez rasterlenip yatayda esnetilir (birebir). Pencere boyutlandırılırken
        // her karede yeni görsel üretilip rasterlenene dek boş kalmasın (titreme).
        let hero_w = width;
        let width = HERO_W;
        let (rx, ry) = (width * 0.9, h * 0.8);
        let base = if cover.is_some() {
            String::new()
        } else {
            format!(
                r##"<linearGradient id="l" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{b2}"/><stop offset="1" stop-color="{b0}"/></linearGradient><radialGradient id="r" gradientUnits="userSpaceOnUse" cx="{cx}" cy="0" r="{rx}" gradientTransform="translate({cx} 0) scale(1 {k}) translate(-{cx} 0)"><stop offset="0" stop-color="{s2}" stop-opacity="{s2a}"/><stop offset="0.7" stop-color="{s2}" stop-opacity="0"/></radialGradient>"##,
                cx = width * 0.2,
                k = ry / rx
            )
        };
        let fills = if cover.is_some() { String::new() } else { format!(r#"<rect width="{width}" height="{h}" fill="url(#l)"/><rect width="{width}" height="{h}" fill="url(#r)"/>"#) };
        let markup = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{h}" viewBox="0 0 {width} {h}"><defs>{base}<linearGradient id="f" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#000" stop-opacity="0.08"/><stop offset="0.28" stop-color="{b0}" stop-opacity="0"/><stop offset="0.64" stop-color="{b0}" stop-opacity="0.58"/><stop offset="1" stop-color="{b0}" stop-opacity="1"/></linearGradient></defs>{fills}<rect width="{width}" height="{h}" fill="url(#f)"/></svg>"##
        );
        let width = hero_w;
        let band = width * 0.3;
        let shine = div()
            .absolute()
            .top(px(-0.2 * h))
            .h(px(0.8 * h))
            .left(px(-band))
            .w(px(band))
            .flex()
            .child(div().w(px(band / 2.)).h_full().bg(gpui_kit::linear_gradient(90., gpui_kit::linear_color_stop(css_rgba(120, 180, 255, 0.), 0.), gpui_kit::linear_color_stop(css_rgba(120, 180, 255, 0.10), 1.))))
            .child(div().w(px(band / 2.)).h_full().bg(gpui_kit::linear_gradient(90., gpui_kit::linear_color_stop(css_rgba(120, 180, 255, 0.10), 0.), gpui_kit::linear_color_stop(css_rgba(120, 180, 255, 0.), 1.))))
            .with_animation(SharedString::from(format!("lib-shine-{pid}")), gpui_kit::Animation::new(Duration::from_millis(1700)), move |el, t| {
                // p2-shine: .3 sn gecikme + 1,4 sn ease-out; soldan sağa geçer.
                let raw = ((t * 1.7 - 0.3) / 1.4).clamp(0., 1.);
                let k = 1. - (1. - raw).powi(3);
                el.left(px(-band + (width + band) * k)).opacity(if raw <= 0. || raw >= 1. { 0. } else { 1. })
            });
        div()
            .id("library-hero")
            .relative()
            .w(px(width))
            .h(px(h))
            .flex_shrink_0()
            .overflow_hidden()
            .on_mouse_down(MouseButton::Right, cx.listener(|this, e: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.open_menu_at(this.project, f32::from(e.position.x), f32::from(e.position.y), cx);
            }))
            .when_some(cover, |s, image| s.child(img(ImageSource::Render(image)).absolute().top_0().left_0().w(px(width)).h(px(h)).object_fit(ObjectFit::Cover)))
            .child(p2_ui::icons::svg_image(markup).object_fit(ObjectFit::Fill).absolute().top_0().left_0().w(px(width)).h(px(h)))
            .when(project.cover_url.is_none(), |s| {
                s.child(div().absolute().inset_0().flex().items_center().justify_center().text_size(fs(110.)).css_weight(800.).tracking(0.04).text_color(css_rgba(127, 182, 255, 0.06)).child(user_initials(&project.name)))
            })
            .child(shine)
            .child(div().absolute().left(px(28.)).bottom(px(28.)).right(px(28.)).child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(16.))
                    .child(project_icon(project, 64., 16., 20., true, p).shadow(shadows(&{
                        let mut v = p.shadow_md.clone();
                        v.push(p2_ui::tokens::Shadow::new(0., 0., 0., 1., css_rgba(120, 180, 255, 0.2)));
                        v
                    })))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .min_w_0()
                            .child(div().text_color(p.text_strong).text_size(fs(34.)).line_height(relative(1.1)).css_weight(800.).tracking(-0.01).child(project.name.clone())),
                    )
                    .p2_enter(("lib-hero-body", self.project), Entrance::Rise, Duration::from_millis(100)),
            ))
            .into_any_element()
    }

    /// Bölünmüş büyük düğme (`.library-play.install/.play/.stop`).
    fn play_button(&self, label: &str, tone: &'static str, disabled: bool, cx: &mut Context<Self>, p: &Palette) -> AnyElement {
        let (fg, main_bg, caret_bg, glow) = match tone {
            "install" => (p.text_on_accent, None, (p.accent, p.accent_active), p.accent_glow),
            "stop" => (p.text_on_accent, Some((p.danger_light, p.danger, p.danger_mid)), (p.danger_mid, p.danger_deep), p2_ui::tokens::Shadow::new(0., 0., 24., 0., css_rgba(255, 92, 108, 0.25))),
            _ => (p.text_on_success, Some((p.success_light, p.success, p.success_mid)), (p.success_mid, p.success_deep), p2_ui::tokens::Shadow::new(0., 0., 24., 0., css_rgba(61, 220, 132, 0.25))),
        };
        let svg = |c: gpui_kit::Hsla| p2_ui::icons::svg_color(c);
        let main_layer = match main_bg {
            Some((a, b, c)) => {
                let ((a, aa), (b, ba), (c, ca)) = (svg(a), svg(b), svg(c));
                p2_ui::icons::svg_image(format!(r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="10" preserveAspectRatio="none" viewBox="0 0 100 10"><defs><linearGradient id="g"><stop offset="0" stop-color="{a}" stop-opacity="{aa}"/><stop offset="0.6" stop-color="{b}" stop-opacity="{ba}"/><stop offset="1" stop-color="{c}" stop-opacity="{ca}"/></linearGradient></defs><rect width="100" height="10" fill="url(#g)"/></svg>"##)).absolute().inset_0().size_full().rounded_l(radius::MD).into_any_element()
            }
            None => div().absolute().inset_0().rounded_l(radius::MD).bg(p.accent_grad.background()).into_any_element(),
        };
        let icon: AnyElement = match tone {
            "stop" => ico_fill(STOP, 20., fg).into_any_element(),
            "install" => ico(DOWNLOAD, 2.6, 20., fg).into_any_element(),
            _ => ico_fill(PLAY, 20., fg).into_any_element(),
        };
        let rest = shadows(&[p2_ui::tokens::Shadow::new(0., 10., 28., 0., css_rgba(0, 0, 0, 0.4)), p.inset_top]);
        let hot = shadows(&[p2_ui::tokens::Shadow::new(0., 14., 32., 0., css_rgba(0, 0, 0, 0.45)), glow, p.inset_top]);
        let tone_s = tone.to_string();
        div()
            .id(SharedString::from(format!("library-play-{tone}")))
            .relative()
            .h(px(46.))
            .flex()
            .flex_shrink_0()
            .rounded(radius::MD)
            .overflow_hidden()
            .shadow(rest)
            .hover(move |s| s.shadow(hot.clone()).top(px(-1.)))
            .child(
                div()
                    .id("library-play-main")
                    // GPUI overflow_hidden köşeyi kırpmaz: yuvarlaklık çocuk zeminlerine de verilir (.library-play r 11).
                    .relative()
                    .rounded_l(radius::MD)
                    .h_full()
                    .pl(px(20.))
                    .pr(px(26.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .text_color(fg)
                    .text_size(fs(15.))
                    .css_weight(800.)
                    .tracking(0.06)
                    .when(disabled, |s| s.opacity(0.75))
                    .when(!disabled, |s| s.cursor_pointer())
                    .child(main_layer)
                    .child(div().relative().flex().items_center().gap(px(10.)).child(icon).child(tr_upper(label)))
                    .when(!disabled, |s| {
                        s.on_click(cx.listener(move |this, _, window, cx| match tone_s.as_str() {
                            "install" => this.install(None, false, window, cx),
                            "stop" => this.stop(None, cx),
                            _ => this.play(None, cx),
                        }))
                    }),
            )
            .child(
                div()
                    .id("library-play-caret")
                    .w(px(34.))
                    .rounded_r(radius::MD)
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_l_1()
                    .border_color(css_rgba(0, 0, 0, 0.22))
                    .bg(gpui_kit::linear_gradient(90., gpui_kit::linear_color_stop(caret_bg.0, 0.), gpui_kit::linear_color_stop(caret_bg.1, 1.)))
                    .text_color(fg)
                    .cursor_pointer()
                    .child(ico(CARET, 2.6, 16., fg))
                    .child({
                        let cell = self.caret_bounds.clone();
                        gpui_kit::canvas(move |b, _, _| cell.set(Some(b)), |_, _, _, _| {}).absolute().size_full()
                    })
                    .on_mouse_down(MouseButton::Left, cx.listener(|this, _: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        // Düğmenin altına (box.left, box.bottom + 6).
                        this.open_menu(this.project, 0., 0., 1, cx);
                    })),
            )
            .into_any_element()
    }

    fn primary_area(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let pid = self.pid();
        let job = self.jobs.get(&pid);
        if job.is_some_and(|j| j.phase == "failed") {
            let err = job.and_then(|j| j.error.clone()).unwrap_or_default();
            return div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(secondary("lib-retry", p2_i18n::t("lib.retry"), Sec::Plain, false, p).on_click(cx.listener(|this, _, window, cx| this.install(None, false, window, cx))))
                .child(div().text_color(p.danger_text).text_size(fs(12.)).child(err))
                .into_any_element();
        }
        let Some(list) = &self.releases else { return hint("", p).into_any_element() };
        if list.channels.is_empty() {
            return hint(p2_i18n::t("lib.noReleases"), p).into_any_element();
        }
        if let Some(j) = job.filter(|j| j.active) {
            let label = phase_label(&j.phase);
            return self.play_button(&if label.is_empty() { p2_i18n::t("libui.phase.downloading") } else { label }, "install", true, cx, p);
        }
        let installed = self.state.projects.get(&pid);
        let live = self.live();
        if installed.is_none() {
            return match live {
                Some(_) => self.play_button(&p2_i18n::t("lib.play.install"), "install", false, cx, p),
                None => hint(p2_i18n::t("lib.noReleasesInChannel"), p).into_any_element(),
            };
        }
        if self.running.contains(&pid) {
            return self.play_button(&p2_i18n::t("lib.play.stop"), "stop", false, cx, p);
        }
        let updatable = installed.zip(live).is_some_and(|(i, l)| i.release_id != l.id && !i.pinned);
        let version = live.map(|l| l.version.clone()).unwrap_or_default();
        div()
            .flex()
            .items_center()
            .gap(px(10.))
            .child(self.play_button(&p2_i18n::t("lib.play.run"), "play", false, cx, p))
            .when(updatable, |s| s.child(secondary("lib-update", p2_i18n::tv("lib.updateTo", &[("version", &version)]), Sec::Plain, false, p).on_click(cx.listener(|this, _, window, cx| this.install(None, false, window, cx)))))
            .into_any_element()
    }

    fn actionbar(&self, key: &str, width: f32, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let pid = self.pid();
        let installed = self.state.projects.get(&pid).cloned();
        let live = self.live().cloned();
        let job = self.jobs.get(&pid).filter(|j| j.active).cloned();
        let hub_mine = self.hub.as_ref().map_or(0, |h| h.playtime.mine);
        let play_seconds = hub_mine.max(installed.as_ref().map_or(0, |i| i.play_seconds));
        let last = installed.as_ref().and_then(|i| i.last_played_at.map(|v| v as i64)).or_else(|| self.hub.as_ref().and_then(|h| h.playtime.last_played_at.as_deref()).and_then(parse_ms));
        let stat = |label: &str, value: AnyElement| {
            div()
                .flex()
                .flex_col()
                .flex_shrink_0()
                .gap(px(3.))
                .child(div().text_color(p.text_accent).text_size(font_size::XXS).css_weight(800.).tracking(0.12).whitespace_nowrap().child(tr_upper(label)))
                .child(div().flex().items_center().gap(px(4.)).min_h(px(20.)).text_color(p.text).text_size(fs(12.5)).css_weight(650.).whitespace_nowrap().child(value))
        };
        let text = |s: String| div().child(s).into_any_element();
        let bar = div()
            .id(SharedString::from(format!("library-actionbar-{key}")))
            .w(px(width))
            .min_h(px(76.))
            .flex()
            .items_center()
            .gap(px(22.))
            .py(px(12.))
            .px(px(28.))
            .border_t_1()
            .border_b_1()
            .border_color(p.border_subtle)
            .bg(gpui_kit::Hsla { a: p.bg_1.a * 0.88, ..p.bg_1 })
            // backdrop-filter: blur(18px) saturate(1.3) — yapışkan çubuğun altından kayan içerik bulanık görünür.
            .backdrop(18., 1.3);
        let tools = div()
            .flex()
            .items_center()
            .gap(px(4.))
            .flex_shrink_0()
            .when(self.can_edit_project, |s| {
                s.child(icon_btn(SharedString::from(format!("lib-edit-{key}")), PENCIL, 2., p).tooltip(|w, cx| p2_ui::controls::tooltip(p2_i18n::t("lib.projectSettingsTip"), w, cx)).on_click(cx.listener(|this, _, _, cx| cx.emit(LibraryEvent::ProjectSettings(this.pid())))))
            })
            .child(icon_btn(SharedString::from(format!("lib-gear-{key}")), GEAR, 1.8, p).tooltip(|w, cx| p2_ui::controls::tooltip(p2_i18n::t("lib.moreTip"), w, cx)).on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.open_menu(this.project, 0., 0., 2, cx);
            })));
        if let Some(j) = job {
            return bar.child(div().flex_1().min_w_0().child(self.download_panel(&j, p, cx))).child(tools).into_any_element();
        }
        let channel = self.active_channel();
        bar.child(div().flex().items_center().min_w_0().child(self.primary_area(p, cx)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_wrap()
                    .gap_x(px(26.))
                    .gap_y(px(8.))
                    .items_start()
                    .child(stat(&p2_i18n::t("lib.stat.playtime"), text(fmt_duration(play_seconds as f64))))
                    .child(stat(&p2_i18n::t("lib.stat.lastRun"), text(last.map(time_ago_ms).unwrap_or_else(|| "—".into()))))
                    .child(stat(&p2_i18n::t("lib.stat.size"), text(live.as_ref().map(|l| fmt_bytes(l.total_size)).unwrap_or_else(|| "—".into()))))
                    .child(stat(
                        &p2_i18n::t("lib.stat.installed"),
                        div()
                            .flex()
                            .items_center()
                            .child(installed.as_ref().map(|i| i.version.as_str().map(str::to_string).unwrap_or_else(|| i.version.to_string())).unwrap_or_else(|| "—".into()))
                            .when(installed.as_ref().is_some_and(|i| i.pinned), |s| s.child(div().text_color(p.text_3).text_size(fs(10.5)).css_weight(600.).child(p2_i18n::t("lib.stat.pinnedSuffix"))))
                            .into_any_element(),
                    ))
                    .child(stat(&p2_i18n::t("lib.stat.lastUpdate"), text(live.as_ref().and_then(|l| l.published_at.as_ref().map(|pa| l.live_at.clone().unwrap_or_else(|| pa.clone()))).map(|s| time_ago(&s)).unwrap_or_else(|| "—".into()))))
                    .child(stat(&p2_i18n::t("lib.stat.channel"), channel.map(|c| chip_badge(&c, p).into_any_element()).unwrap_or_else(|| text("—".into())))),
            )
            .child(tools)
            .into_any_element()
    }

    /// İndirme paneli: yüzde, akan çubuk, hız sayaçları, duraklat/devam/iptal.
    fn download_panel(&self, job: &JobState, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let downloading = job.phase == "downloading";
        let label = if job.paused { p2_i18n::t("libui.phase.paused") } else { phase_label(&job.phase) };
        let write_pct = (job.phase == "installing" && job.bytes_to_write > 0).then(|| ((job.bytes_written as f64 / job.bytes_to_write as f64) * 100.).round().min(100.) as u32);
        let shown = write_pct.unwrap_or_else(|| job_percent(job));
        let sub = if downloading { format!("{} / {}", fmt_bytes(job.bytes_done), fmt_bytes(job.bytes_total)) } else if job.phase == "installing" { p2_i18n::tv("lib.dl.written", &[("done", &fmt_bytes(job.bytes_written)), ("total", &fmt_bytes(job.bytes_to_write))]) } else { String::new() };
        let meter = |label: &str, value: String| div().flex().flex_col().gap(px(1.)).child(div().text_color(p.text_accent).text_size(font_size::XXS).css_weight(800.).tracking(0.12).child(tr_upper(label))).child(div().text_color(p.text).text_size(fs(12.5)).css_weight(650.).whitespace_nowrap().child(value));
        let paused = job.paused;
        let fill = if paused {
            div().h_full().rounded(radius::PILL).bg(gpui_kit::linear_gradient(90., gpui_kit::linear_color_stop(p.warning_deep, 0.), gpui_kit::linear_color_stop(p.warning, 1.)))
        } else {
            div().h_full().rounded(radius::PILL).bg(p.accent_bar.background()).shadow(shadows(&[p2_ui::tokens::Shadow::new(0., 0., 14., 0., css_rgba(79, 157, 255, 0.5))]))
        };
        let stripes = (!paused).then(|| {
            // repeating-linear-gradient(-55deg, beyaz .18 0 10px, saydam 10px 20px), 0,9 sn kayar.
            p2_ui::icons::svg_image(r##"<svg xmlns="http://www.w3.org/2000/svg" width="1400" height="12" viewBox="0 0 1400 12"><defs><pattern id="s" width="28" height="12" patternUnits="userSpaceOnUse"><path d="M-8 12 L0 0 L12.2 0 L4.2 12 Z M20 12 L28 0 L40.2 0 L32.2 12 Z" fill="#fff" fill-opacity="0.18"/></pattern></defs><rect width="1400" height="12" fill="url(#s)"/></svg>"##.into())
                .absolute()
                .top_0()
                .h(px(12.))
                .w(px(1400.))
                .with_animation("lib-stripes", gpui_kit::Animation::new(Duration::from_millis(900)).repeat(), |el, t| el.left(px(-28. + 28. * t)))
        });
        let shimmer = (!paused).then(|| {
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .w(relative(0.4))
                .flex()
                .child(div().w(relative(0.5)).h_full().bg(gpui_kit::linear_gradient(90., gpui_kit::linear_color_stop(css_rgba(255, 255, 255, 0.), 0.), gpui_kit::linear_color_stop(css_rgba(255, 255, 255, 0.14), 1.))))
                .child(div().w(relative(0.5)).h_full().bg(gpui_kit::linear_gradient(90., gpui_kit::linear_color_stop(css_rgba(255, 255, 255, 0.14), 0.), gpui_kit::linear_color_stop(css_rgba(255, 255, 255, 0.), 1.))))
                .with_animation("lib-shimmer", gpui_kit::Animation::new(Duration::from_millis(2200)).repeat(), |el, t| {
                    let k = (t / 0.6).min(1.);
                    let e = if k < 0.5 { 2. * k * k } else { 1. - (-2. * k + 2.).powi(2) / 2. };
                    el.left(relative(-0.4 + 1.5 * e))
                })
        });
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .min_w_0()
            .child(
                div()
                    .flex()
                    .items_end()
                    .justify_between()
                    .gap(px(14.))
                    .child(div().flex().flex_col().gap(px(2.)).child(div().text_color(p.text_strong).text_size(fs(13.)).css_weight(750.).tracking(0.02).child(label.to_string())).child(div().text_color(p.text_3).text_size(fs(11.)).child(sub)))
                    .child(div().text_color(if paused { p.warning_text } else { p.blue_300 }).text_size(fs(26.)).css_weight(800.).line_height(relative(1.)).tracking(-0.02).child(p2_i18n::tv("lib.pct", &[("n", &shown.to_string())]))),
            )
            .child(
                div()
                    .relative()
                    .h(px(12.))
                    .w_full()
                    .rounded(radius::PILL)
                    .overflow_hidden()
                    .bg(css_rgba(0, 0, 0, 0.35))
                    .shadow(shadows(&[p2_ui::tokens::Shadow { x: 0., y: 1., blur: 2., spread: 0., color: css_rgba(0, 0, 0, 0.5), inset: true }]))
                    .child(div().relative().h_full().w(relative(shown as f32 / 100.)).overflow_hidden().rounded(radius::PILL).child(fill.size_full()).children(stripes))
                    .children(shimmer),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(16.))
                    .flex_wrap()
                    .child(
                        div()
                            .flex()
                            .gap(px(22.))
                            .flex_wrap()
                            .child(meter(&p2_i18n::t("lib.dl.download"), if downloading && !paused { fmt_speed(job.speed) } else { "—".into() }))
                            .child(meter(&p2_i18n::t("lib.dl.diskWrite"), "—".into()))
                            .child(meter(&p2_i18n::t("lib.dl.remaining"), if downloading && !paused && job.eta > 0 { fmt_eta(job.eta) } else { "—".into() }))
                            .child(meter(&p2_i18n::t("lib.dl.chunks"), if job.total > 0 { format!("{}/{}", job.done, job.total) } else { "—".into() })),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .when(downloading, |s| {
                                s.child(
                                    secondary("job-pause", if paused { p2_i18n::t("lib.dl.resume") } else { p2_i18n::t("lib.dl.pause") }, if paused { Sec::Accent } else { Sec::Plain }, false, p)
                                        .min_h(px(32.))
                                        .px(px(12.))
                                        .text_size(fs(11.5))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.patcher.pause(&this.pid(), !paused);
                                            cx.notify();
                                        })),
                                )
                            })
                            .child(secondary("job-cancel", p2_i18n::t("lib.cancel"), Sec::Danger, false, p).min_h(px(32.)).px(px(12.)).text_size(fs(11.5)).on_click(cx.listener(|this, _, _, cx| {
                                this.patcher.cancel(&this.pid());
                                cx.notify();
                            }))),
                    ),
            )
            .p2_enter("lib-download", Entrance::Rise, Duration::ZERO)
            .into_any_element()
    }

    // ------------------------------------------------------------ çizim: sütunlar

    fn activity_panel(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let tab = self.tab;
        let tab_btn = |id: &'static str, label: String, t: Tab| {
            let active = tab == t;
            let (hover_bg, text) = (p.bg_hover, p.text);
            div()
                .id(id)
                .h(px(24.))
                .px(px(10.))
                .flex()
                .items_center()
                .rounded(radius::SM)
                .text_size(font_size::XS)
                .css_weight(650.)
                .cursor_pointer()
                .map(|s| if active { s.bg(p.bg_3).text_color(p.text_strong).shadow(shadows(&[p.inset_top])) } else { s.text_color(p.text_3).hover(move |s| s.bg(hover_bg).text_color(text)) })
                .child(label)
                .on_click(cx.listener(move |this, _, window, cx| this.set_tab(t, window, cx)))
        };
        let title = match tab {
            Tab::Notes => p2_i18n::t("lib.tab.notes"),
            Tab::Shots => p2_i18n::t("lib.tab.shots"),
            Tab::Activity => p2_i18n::t("lib.tab.activity"),
        };
        let head = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .mb(px(12.))
            .child(kicker_text(&title, p))
            .child(div().ml_auto().flex().gap(px(4.)).p(px(3.)).rounded(radius::MD).bg(p.bg_2).border_1().border_color(p.border_subtle).child(tab_btn("lib-tab-activity", p2_i18n::t("lib.tab.activity"), Tab::Activity)).child(tab_btn("lib-tab-notes", p2_i18n::t("lib.tab.notes"), Tab::Notes)).child(tab_btn("lib-tab-shots", p2_i18n::t("lib.tab.shots"), Tab::Shots)));
        let body: AnyElement = match tab {
            Tab::Notes => self.notes.clone().map(|n| n.into_any_element()).unwrap_or_else(|| div().into_any_element()),
            Tab::Shots => self.shots.clone().map(|n| n.into_any_element()).unwrap_or_else(|| div().into_any_element()),
            Tab::Activity => self.activity_feed(p, cx),
        };
        panel(p).child(head).child(body).p2_enter(("lib-col-main", self.project), Entrance::Rise, Duration::ZERO).into_any_element()
    }

    fn activity_feed(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let mut col = div().flex().flex_col();
        let Some(hub) = &self.hub else {
            return col.child(hint(p2_i18n::t("lib.activity.loading"), p)).into_any_element();
        };
        if hub.permissions.post && self.editing_post.is_none() {
            let editor = editor::note_box("lib-post", &self.post, self.api.clone(), |v: &mut Self| Some(&mut v.post), Some((&p2_i18n::t("lib.post.share"), !self.post.is_empty(cx), |v: &mut Self, w, cx| v.submit_post(w, cx))), self.post_busy, p, cx);
            col = col.child(div().flex().items_start().gap(px(10.)).mb(px(14.)).child(div().mt(px(4.)).child(lib_avatar(&self.me, &self.me_name, self.me_avatar.as_deref(), 30., 10., p))).child(div().flex_1().min_w_0().child(editor)));
        }
        if hub.posts.is_empty() {
            col = col.child(div().py(px(6.)).child(hint(if hub.permissions.post { p2_i18n::t("lib.activity.emptyPost") } else { p2_i18n::t("lib.activity.empty") }, p)));
        }
        for (i, post) in hub.posts.iter().enumerate() {
            col = col.child(self.post_row(i, post, hub.permissions.manage, p, cx));
        }
        col.into_any_element()
    }

    fn post_row(&self, i: usize, post: &HubPost, manage: bool, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let editing = self.editing_post.as_ref().filter(|(id, _)| *id == post.id);
        let can = post.author_id == self.me || manage;
        let group = SharedString::from(format!("lib-post-{}", post.id));
        let (soft, strong, dsoft, dtext) = (p.accent_soft, p.text_strong, p.danger_soft, p.danger_text);
        let tool = |key: &str, label: &str| {
            div().id(SharedString::from(format!("{key}-{}", post.id))).px(px(7.)).py(px(2.)).rounded(radius::XS).text_color(p.text_3).text_size(fs(11.)).font_weight(FontWeight::SEMIBOLD).cursor_pointer().child(label.to_string())
        };
        let (pe, pd, post_c) = (post.clone(), post.id.clone(), post.content.clone());
        let tools = (can && editing.is_none()).then(|| {
            div()
                .ml_auto()
                .flex()
                .gap(px(2.))
                .invisible()
                .group_hover(group.clone(), |s| s.visible())
                .child(tool("lib-post-edit", &p2_i18n::t("lib.edit")).hover(move |s| s.bg(soft).text_color(strong)).on_click(cx.listener(move |this, _, window, cx| {
                    let mut nb = NoteBox::new(&p2_i18n::t("lib.post.editPlaceholder"), true, window, cx);
                    nb.set(&pe.content, window, cx);
                    nb.text.update(cx, |s, cx| s.focus(window, cx));
                    this.editing_post = Some((pe.id.clone(), nb));
                    cx.notify();
                })))
                .child(tool("lib-post-del", &p2_i18n::t("lib.delete")).hover(move |s| s.bg(dsoft).text_color(dtext)).on_click(cx.listener(move |this, _, _, cx| this.ask(p2_i18n::t("lib.post.deleteTitle"), p2_i18n::t("lib.post.deleteBody"), &p2_i18n::t("lib.delete"), true, Act::DeletePost(pd.clone()), cx))))
        });
        let body: AnyElement = match editing {
            Some((_, nb)) => div()
                .flex()
                .flex_col()
                .items_end()
                .gap(px(6.))
                .child(editor::note_box(
                    "lib-post-editor",
                    nb,
                    self.api.clone(),
                    |v: &mut Self| v.editing_post.as_mut().map(|e| &mut e.1),
                    Some((&p2_i18n::t("lib.save"), !nb.is_empty(cx), |v: &mut Self, w, cx| v.save_post(w, cx))),
                    false,
                    p,
                    cx,
                ))
                .child(div().id("lib-post-cancel").py(px(4.)).text_color(p.text_3).text_size(fs(11.5)).cursor_pointer().hover(|s| s.underline()).child(p2_i18n::t("lib.cancelEdit")).on_click(cx.listener(|this, _, _, cx| {
                    this.editing_post = None;
                    cx.notify();
                })))
                .into_any_element(),
            // .library-post-content .rich-message p { margin: 0 0 6px } — son paragrafın alt boşluğu ızgara öğesinde (post-body) kalır.
            // GPUI metin yüksekliğini tam piksele yuvarlar (17,68 → 18): tek satırlık gönderide 6px boşluktan 0,32 düşülür ki
            // satır Electron gibi 72,68px olsun (yoksa her satırda 0,3px kayma birikiyordu).
            None => div().pb(px(5.68)).text_color(p.text).text_size(fs(13.)).line_height(px(17.68)).child(post_content(&post_c, p)).into_any_element(),
        };
        let edited = post.updated_at != post.created_at && !post.updated_at.is_empty();
        div()
            .id(SharedString::from(format!("lib-post-row-{}", post.id)))
            .group(group)
            .flex()
            .gap(px(12.))
            .py(px(14.))
            .when(i > 0, |s| s.border_t_1().border_color(p.border_subtle))
            .child(lib_avatar(&post.author.id, &post.author.display_name, post.author.avatar_url.as_deref(), 34., 11., p))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        // align-items: baseline — b (14px kutu) ve rozet (16px) üstte, small (11px) 2px aşağıda (dom-styles).
                        div()
                            .flex()
                            .items_start()
                            .gap(px(8.))
                            .flex_wrap()
                            .mb(px(4.))
                            .child(div().text_color(p.text_strong).text_size(fs(13.)).line_height(px(14.)).font_weight(FontWeight::BOLD).child(post.author.display_name.clone()))
                            .when(post.kind == "RELEASE", |s| s.child(badge(&p2_i18n::t("lib.post.releaseBadge"), Tone::Live, p)))
                            .child(div().mt(px(2.)).line_height(px(11.)).text_color(p.text_3).text_size(fs(11.)).child(format!("{}{}", time_ago(&post.created_at), if edited { p2_i18n::t("lib.post.editedSuffix") } else { String::new() })))
                            .children(tools),
                    )
                    .child(body),
            )
            .into_any_element()
    }

    fn side_column(&self, window: &Window, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let hub = self.hub.as_ref();
        let delay = |n: u64| Duration::from_millis(60 * n);
        // Oynayan arkadaşlar (etkinlik akışı Rust istemcisinde yok: boş durum).
        let players = panel(p).child(h3(&p2_i18n::t("lib.players.title"), p)).child(hint(p2_i18n::t("lib.players.empty"), p));
        // Başarımlar
        let mut ach = panel(p);
        let mut head = h3(&p2_i18n::t("lib.ach.title"), p);
        if let Some(h) = hub.filter(|h| !h.achievements.is_empty()) {
            head = head.child(count(format!("{}/{}", h.achievements.iter().filter(|a| a.unlocked_by_me).count(), h.achievements.len()), None, p));
        }
        if hub.is_some_and(|h| h.permissions.manage) {
            head = head.child(h3_action("lib-ach-add", &p2_i18n::t("lib.ach.add"), p).on_click(cx.listener(|this, _, window, cx| this.open_achievement(window, cx))));
        }
        ach = ach.child(head);
        if let Some(h) = hub {
            if h.achievements.is_empty() {
                ach = ach.child(hint(if h.permissions.manage { p2_i18n::t("lib.ach.emptyManage") } else { p2_i18n::t("lib.ach.empty") }, p));
            } else {
                let mut list = div().flex().flex_col().gap(px(6.));
                for a in &h.achievements {
                    let (id, unlocked, title) = (a.id.clone(), a.unlocked_by_me, a.title.clone());
                    let manage = h.permissions.manage;
                    let (hover_bg, bs) = (p.bg_hover, p.border_strong);
                    let tip = format!(
                        "{}{}\n{}\n{}",
                        a.title,
                        a.description.as_ref().filter(|d| !d.is_empty()).map(|d| format!(" — {d}")).unwrap_or_default(),
                        if a.unlocks.is_empty() { p2_i18n::t("lib.ach.nobody") } else { p2_i18n::tv("lib.ach.earnedBy", &[("names", &a.unlocks.iter().map(|u| u.user.display_name.clone()).collect::<Vec<_>>().join(", "))]) },
                        if unlocked { p2_i18n::t("lib.ach.clickRevoke") } else { p2_i18n::t("lib.ach.clickEarn") }
                    );
                    let icon_bg = if unlocked {
                        gpui_kit::linear_gradient(160., gpui_kit::linear_color_stop(css_rgba(255, 201, 77, 0.32), 0.), gpui_kit::linear_color_stop(css_rgba(255, 160, 0, 0.12), 1.))
                    } else {
                        gpui_kit::linear_gradient(160., gpui_kit::linear_color_stop(css_rgba(255, 255, 255, 0.1), 0.), gpui_kit::linear_color_stop(css_rgba(255, 255, 255, 0.03), 1.))
                    };
                    list = list.child(
                        div()
                            .id(SharedString::from(format!("lib-ach-{}", a.id)))
                            .group("lib-ach-row")
                            .flex()
                            .items_center()
                            .gap(px(11.))
                            .w_full()
                            .py(px(7.))
                            .px(px(9.))
                            .border_1()
                            .border_color(p.border_subtle)
                            .rounded(radius::MD)
                            .bg(p.bg_3)
                            .cursor_pointer()
                            .hover(move |s| s.bg(hover_bg).border_color(bs).relative().top(px(-1.)))
                            .tooltip(move |w, cx| p2_ui::controls::tooltip(tip.clone(), w, cx))
                            .on_click(cx.listener(move |this, _, window, cx| this.toggle_unlock(id.clone(), !unlocked, window, cx)))
                            .when(manage, |s| {
                                let (id, title) = (a.id.clone(), title.clone());
                                s.on_mouse_down(MouseButton::Right, cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    this.ask(p2_i18n::t("lib.ach.deleteTitle"), p2_i18n::tv("lib.ach.deleteBody", &[("title", &title)]), &p2_i18n::t("lib.delete"), true, Act::DeleteAchievement(id.clone()), cx)
                                }))
                            })
                            .child(
                                div()
                                    .size(px(40.))
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(12.))
                                    .bg(icon_bg)
                                    .when(unlocked, |s| s.shadow(shadows(&[p2_ui::tokens::Shadow::new(0., 0., 0., 1., css_rgba(255, 201, 77, 0.35)), p2_ui::tokens::Shadow::new(0., 6., 18., 0., css_rgba(255, 170, 0, 0.18))])))
                                    .text_size(fs(21.))
                                    .font_family(p2_ui::tokens::FONT_EMOJI)
                                    // filter: grayscale(1) opacity(.45); satır hover'ında scale(1.1) (.2s)
                                    .when(!unlocked, |s| { use p2_ui::P2Fx as _; s.grayscale(1.).opacity(0.45) })
                                    .group_hover("lib-ach-row", |s| { use p2_ui::P2Fx as _; s.scale(1.1) })
                                    .child(if a.icon.is_empty() { "🏆".to_string() } else { a.icon.clone() }),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .gap(px(1.))
                                    .child(div().overflow_hidden().whitespace_nowrap().text_ellipsis().text_color(if unlocked { p.text_strong } else { p.text_3 }).text_size(fs(12.5)).css_weight(650.).child(a.title.clone()))
                                    .child(div().text_color(p.text_3).text_size(fs(10.5)).child(if a.unlocks.is_empty() { p2_i18n::t("lib.ach.nobodyShort") } else { p2_i18n::tv("lib.ach.earnedCount", &[("n", &a.unlocks.len().to_string())]) })),
                            )
                            .when(unlocked, |s| s.child(div().size(px(20.)).flex().items_center().justify_center().rounded_full().bg(p.success_soft).text_color(p.success_text).text_size(fs(11.)).css_weight(800.).child("✓"))),
                    );
                }
                ach = ach.child(list);
            }
        }
        // İncelemeler
        let mut rev = panel(p);
        let mut head = h3(&p2_i18n::t("lib.rev.title"), p);
        if let Some(h) = hub.filter(|h| !h.reviews.is_empty()) {
            let positive = h.reviews.iter().filter(|r| r.recommended).count() as f32 / h.reviews.len() as f32;
            head = head.child(count(p2_i18n::tv("lib.rev.positive", &[("n", &((positive * 100.).round() as u32).to_string())]), Some(if positive >= 0.7 { "good" } else if positive >= 0.4 { "mixed" } else { "bad" }), p));
        }
        rev = rev.child(head);
        if let Some(h) = hub {
            rev = rev.child(self.review_form(window, p, cx));
            for (i, r) in h.reviews.iter().filter(|r| r.user_id != self.me).enumerate() {
                let _ = i;
                rev = rev.child(
                    div()
                        .flex()
                        .gap(px(10.))
                        .py(px(10.))
                        .border_t_1()
                        .border_color(p.border_subtle)
                        .child(lib_avatar(&r.user.id, &r.user.display_name, r.user.avatar_url.as_deref(), 28., 9.5, p))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.))
                                        .flex_wrap()
                                        .child(div().text_color(p.text_strong).text_size(fs(12.5)).font_weight(FontWeight::BOLD).child(r.user.display_name.clone()))
                                        .child(div().text_size(fs(11.)).css_weight(650.).text_color(if r.recommended { p.success_text } else { p.danger_text }).child(if r.recommended { p2_i18n::t("lib.rev.recommends") } else { p2_i18n::t("lib.rev.notRecommends") }))
                                        .child(div().text_color(p.text_3).text_size(fs(10.5)).child(time_ago(&r.updated_at))),
                                )
                                .when_some(r.body.clone().filter(|b| !b.is_empty()), |s, b| s.child(div().mt(px(4.)).text_color(p.text).text_size(fs(12.5)).line_height(relative(1.55)).child(b))),
                        ),
                );
            }
            if h.reviews.is_empty() {
                rev = rev.child(hint(p2_i18n::t("lib.rev.empty"), p));
            }
        }
        // Görevler
        let mut tasks = panel(p).child(h3(&p2_i18n::t("lib.tasks.title"), p).child(h3_action("lib-board", &p2_i18n::t("lib.tasks.openBoard"), p).on_click(cx.listener(|_, _, _, cx| cx.emit(LibraryEvent::OpenBoard)))));
        match &self.tasks {
            None => tasks = tasks.child(hint(p2_i18n::t("lib.tasks.loading"), p)),
            Some((rows, boards)) => {
                const ORDER: [&str; 4] = ["IN_PROGRESS", "REVIEW", "TODO", "BACKLOG"];
                const PRIO: [&str; 4] = ["URGENT", "HIGH", "MEDIUM", "LOW"];
                let rank = |list: &[&str], v: &str| list.iter().position(|x| *x == v).map_or(-1, |i| i as i32);
                let mut open: Vec<&TaskRow> = rows.iter().filter(|t| t.status != "DONE").collect();
                open.sort_by_key(|t| (rank(&ORDER, &t.status), rank(&PRIO, &t.priority), t.number));
                let done = rows.len() - open.len();
                if rows.is_empty() {
                    tasks = tasks.child(hint(p2_i18n::t("lib.tasks.empty"), p));
                } else if open.is_empty() {
                    tasks = tasks.child(hint(p2_i18n::tv("lib.tasks.noneOpen", &[("done", &done.to_string())]), p));
                }
                if !open.is_empty() {
                    let mut list = div().flex().flex_col().gap(px(4.));
                    for t in open.iter().take(8) {
                        let chip = |label: &str, kind: &str| {
                            let (bg, fg) = match kind {
                                "IN_PROGRESS" => (p.accent_soft, p.text_accent),
                                "REVIEW" | "HIGH" => (p.warning_soft, p.warning_text),
                                "TODO" => (p.bg_hover, p.text_2),
                                "URGENT" => (p.danger_soft, p.danger_text),
                                _ => (p.bg_hover, p.text_3),
                            };
                            div().px(px(6.)).py(px(2.)).rounded(radius::PILL).bg(bg).text_color(fg).text_size(fs(9.)).font_weight(FontWeight::BOLD).tracking(0.04).whitespace_nowrap().child(tr_upper(label))
                        };
                        let status = match t.status.as_str() {
                            "BACKLOG" | "TODO" | "IN_PROGRESS" | "REVIEW" | "DONE" => p2_client_core::status_label(&t.status).to_string(),
                            s => s.to_string(),
                        };
                        let prio = match t.priority.as_str() {
                            "URGENT" | "HIGH" => Some(p2_client_core::priority_label(&t.priority).to_string()),
                            _ => None,
                        };
                        let (hover_bg, bs) = (p.bg_hover, p.border_strong);
                        let board = t.board.clone();
                        list = list.child(
                            div()
                                .id(SharedString::from(format!("lib-task-{}", t.number)))
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .w_full()
                                .py(px(7.))
                                .px(px(9.))
                                .border_1()
                                .border_color(p.border_subtle)
                                .rounded(radius::SM)
                                .bg(p.bg_3)
                                .cursor_pointer()
                                .hover(move |s| s.bg(hover_bg).border_color(bs).left(px(2.)))
                                .when(*boards > 1, |s| s.tooltip(move |w, cx| p2_ui::controls::tooltip(board.clone(), w, cx)))
                                .on_click(cx.listener(|_, _, _, cx| cx.emit(LibraryEvent::OpenBoard)))
                                .child(div().text_color(p.text_3).text_size(fs(10.)).child(format!("#{}", t.number)))
                                .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().text_color(p.text).text_size(fs(12.)).font_weight(FontWeight::SEMIBOLD).child(t.title.clone()))
                                .child(div().flex().items_center().gap(px(4.)).when_some(prio, |s, l| s.child(chip(&l, &t.priority))).child(chip(&status, &t.status))),
                        );
                    }
                    tasks = tasks.child(list);
                }
                if !rows.is_empty() {
                    let more = open.len().saturating_sub(8);
                    tasks = tasks.child(
                        div()
                            .flex()
                            .justify_between()
                            .items_center()
                            .gap(px(10.))
                            .mt(px(8.))
                            .text_color(p.text_3)
                            .text_size(fs(11.))
                            .child(p2_i18n::tv("lib.tasks.summary", &[("open", &open.len().to_string()), ("done", &done.to_string())]))
                            .when(more > 0, |s| s.child(div().id("lib-task-more").text_color(p.text_accent).font_weight(FontWeight::SEMIBOLD).cursor_pointer().hover(|s| s.underline()).child(p2_i18n::tv("lib.tasks.more", &[("n", &more.to_string())])).on_click(cx.listener(|_, _, _, cx| cx.emit(LibraryEvent::OpenBoard))))),
                    );
                }
            }
        }
        div()
            .w(px(320.))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap(px(14.))
            .child(players.p2_enter(("lib-side-1", self.project), Entrance::Rise, delay(0)))
            .child(ach.p2_enter(("lib-side-2", self.project), Entrance::Rise, delay(1)))
            .child(rev.p2_enter(("lib-side-3", self.project), Entrance::Rise, delay(2)))
            .child(tasks.p2_enter(("lib-side-4", self.project), Entrance::Rise, delay(3)))
            .into_any_element()
    }

    /// ReviewForm: kendi incelemem (göster/düzenle) ya da yeni form.
    fn review_form(&self, window: &Window, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let mine = self.hub.as_ref().and_then(|h| h.reviews.iter().find(|r| r.user_id == self.me)).cloned();
        let open = self.review_open.unwrap_or(mine.is_none());
        if let (Some(m), false) = (&mine, open) {
            let (soft, strong, dsoft, dtext) = (p.accent_soft, p.text_strong, p.danger_soft, p.danger_text);
            return div()
                .flex()
                .flex_col()
                .py(px(10.))
                .px(px(12.))
                .mb(px(8.))
                .border_1()
                .border_color(p.success_border)
                .rounded(radius::MD)
                .bg(p.success_soft)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .flex_wrap()
                        .child(div().text_color(p.text_strong).text_size(fs(12.5)).font_weight(FontWeight::BOLD).child(p2_i18n::t("lib.rev.yours")))
                        .child(div().text_size(fs(11.)).css_weight(650.).text_color(if m.recommended { p.success_text } else { p.danger_text }).child(if m.recommended { p2_i18n::t("lib.rev.youRecommend") } else { p2_i18n::t("lib.rev.youNotRecommend") })),
                )
                .when_some(m.body.clone().filter(|b| !b.is_empty()), |s, b| s.child(div().mt(px(4.)).text_color(p.text).text_size(fs(12.5)).line_height(relative(1.55)).child(b)))
                .child(
                    div()
                        .mt(px(6.))
                        .flex()
                        .gap(px(2.))
                        .child(div().id("lib-rev-edit").px(px(7.)).py(px(2.)).rounded(radius::XS).text_color(p.text_3).text_size(fs(11.)).font_weight(FontWeight::SEMIBOLD).cursor_pointer().hover(move |s| s.bg(soft).text_color(strong)).child(p2_i18n::t("lib.edit")).on_click({
                            let m = m.clone();
                            cx.listener(move |this, _, window, cx| {
                                this.review_open = Some(true);
                                this.review_rec = m.recommended;
                                let body = m.body.clone().unwrap_or_default();
                                this.review_body.update(cx, |s, cx| s.set_value(body, window, cx));
                                cx.notify();
                            })
                        }))
                        .child(div().id("lib-rev-del").px(px(7.)).py(px(2.)).rounded(radius::XS).text_color(p.text_3).text_size(fs(11.)).font_weight(FontWeight::SEMIBOLD).cursor_pointer().hover(move |s| s.bg(dsoft).text_color(dtext)).child(p2_i18n::t("lib.delete")).on_click(cx.listener(|this, _, window, cx| this.delete_review(window, cx)))),
                )
                .into_any_element();
        }
        let rec = self.review_rec;
        let choice = |id: &'static str, label: String, up: bool| {
            let active = rec == up;
            let hover_bg = p.bg_hover;
            div()
                .id(id)
                .flex_1()
                .min_h(px(32.))
                .flex()
                .items_center()
                .justify_center()
                .border_1()
                .rounded(radius::SM)
                .text_size(fs(11.5))
                .css_weight(650.)
                .cursor_pointer()
                .map(|s| match (active, up) {
                    (true, true) => s.border_color(p.success_border).bg(p.success_soft).text_color(p.success_text),
                    (true, false) => s.border_color(p.danger_border).bg(p.danger_soft).text_color(p.danger_text),
                    _ => s.border_color(p.border).bg(p.bg_3).text_color(p.text_2).hover(move |s| s.bg(hover_bg)),
                })
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.review_rec = up;
                    cx.notify();
                }))
        };
        let focused = self.review_body.read(cx).focus_handle(cx).is_focused(window);
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .mb(px(8.))
            .child(div().flex().gap(px(6.)).child(choice("lib-rev-up", p2_i18n::t("lib.rev.choiceUp"), true)).child(choice("lib-rev-down", p2_i18n::t("lib.rev.choiceDown"), false)))
            .child(
                div()
                    .min_h(px(63.9))
                    .py(px(8.))
                    .px(px(10.))
                    .border_1()
                    .border_color(if focused { p.border_accent } else { p.border })
                    .when(focused, |s| s.shadow(shadows(&[p.ring])))
                    .rounded(radius::SM)
                    .bg(p.bg_3)
                    .text_color(p.text)
                    .text_size(fs(12.))
                    .line_height(relative(1.5))
                    // XSmall: bileşenin iç dolgusu yok; 12px×.85, line-height 1.5 = 15,3px (3 satır + 8/8 dolgu + kenarlık = 63,9).
                    .child(gpui_kit::component::Sizable::with_size(Textarea::new(&self.review_body).appearance(false), gpui_kit::component::Size::XSmall).p(px(0.)).text_size(fs(12.)).line_height(px(15.3)).text_color(p.text)),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(6.))
                    .when(mine.is_some(), |s| {
                        s.child(foot_btn("lib-rev-cancel", p2_i18n::t("lib.cancelEdit"), false, false, p).h(px(30.)).px(px(12.)).text_size(fs(11.5)).on_click(cx.listener(|this, _, _, cx| {
                            this.review_open = Some(false);
                            cx.notify();
                        })))
                    })
                    .child(
                        secondary("lib-rev-save", if self.review_busy { p2_i18n::t("lib.saving") } else if mine.is_some() { p2_i18n::t("lib.rev.update") } else { p2_i18n::t("lib.rev.publish") }, Sec::Accent, self.review_busy, p)
                            .min_h(px(30.))
                            .px(px(12.))
                            .text_size(fs(11.5))
                            .on_click(cx.listener(|this, _, window, cx| this.save_review(window, cx))),
                    ),
            )
            .into_any_element()
    }

    /// Kütüphanede olmayan proje kartı.
    fn not_app(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let project = &self.projects[self.project];
        let publish = self.can_publish();
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .py(px(40.))
            .px(px(24.))
            .child(
                div()
                    .max_w(px(460.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(12.))
                    .py(px(28.))
                    .px(px(30.))
                    .border_1()
                    .border_color(p.border)
                    .rounded(radius::XL)
                    .bg(p.bg_1)
                    .child(project_icon(project, 64., 16., 20., true, p))
                    .child(div().mt(px(4.)).text_color(p.text_strong).text_size(font_size::XL).font_weight(FontWeight::BOLD).child(p2_i18n::tv("lib.notApp.title", &[("name", &project.name)])))
                    .child(div().text_center().text_color(p.text_2).text_size(font_size::SM).line_height(relative(1.5)).child(if publish { p2_i18n::t("lib.notApp.bodyPublish") } else { p2_i18n::t("lib.notApp.bodyNoPublish") }))
                    .when(publish, |s| {
                        s.child(primary("lib-add-app", p2_i18n::t("lib.notApp.add"), false, p).on_click(cx.listener(|this, _, window, cx| {
                            let (api, project) = (this.api.clone(), this.pid());
                            task::run(window, cx, move || api.update_project_settings(&project, serde_json::json!({ "libraryEnabled": true })), |this, result, _, cx| match result {
                                Ok(saved) => {
                                    if let Some(p) = this.projects.iter_mut().find(|p| p.id == saved.id) {
                                        p.library_enabled = true;
                                    }
                                    cx.notify();
                                }
                                Err(e) => this.fail_api(e, &p2_i18n::t("lib.err.addFailed"), cx),
                            });
                        })))
                    }),
            )
            .into_any_element()
    }

    fn main_area(&self, window: &mut Window, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let project = &self.projects[self.project];
        if !project.library_enabled {
            return div().flex_1().min_w_0().h_full().bg(p.bg_0).child(self.not_app(p, cx)).into_any_element();
        }
        let width = self.content_width(window);
        let offset = -f32::from(self.scroll.offset().y);
        let mut content = div()
            .id("library-content")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .on_scroll(cx.listener(|_, _, _, cx| cx.notify()))
            .flex()
            .flex_col()
            .pb(px(48.))
            .child(self.hero(width, p, cx))
            .child(self.actionbar("flow", width, p, cx));
        if self.state.root.is_empty() {
            content = content.child(
                div()
                    .mt(px(16.))
                    .mx(px(28.))
                    .w(px(width - 56.))
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .py(px(14.))
                    .px(px(16.))
                    .border_1()
                    .border_color(p.border_accent)
                    .rounded(radius::MD)
                    .bg(p.accent_soft)
                    .child(div().text_color(p.text_strong).text_size(fs(12.5)).font_weight(FontWeight::BOLD).child(p2_i18n::t("lib.root.title")))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .text_color(p.text_2)
                            .text_size(fs(11.5))
                            .line_height(relative(1.5))
                            .child(p2_i18n::t("lib.root.before"))
                            // Satır içi <code>: dolgusu satır kutusunu büyütmez (Electron kutusu 13px, satır 14,66px).
                            .child(div().px(px(6.)).py(px(1.)).line_height(px(11.)).rounded(px(5.)).bg(css_rgba(0, 0, 0, 0.25)).text_size(fs(11.)).font_family(p2_ui::tokens::FONT_MONO).child("P2ProjectLibrary"))
                            .child(p2_i18n::t("lib.root.after")),
                    )
                    .child(div().mt(px(4.)).flex().child(secondary("lib-root", p2_i18n::t("lib.root.choose"), Sec::Plain, false, p).on_click(cx.listener(|this, _, window, cx| this.choose_root(window, cx))))),
            );
        }
        content = content.child(div().w(px(width)).pt(px(22.)).px(px(28.)).flex().items_start().gap(px(18.)).child(div().flex_1().min_w_0().child(self.activity_panel(p, cx))).child(self.side_column(window, p, cx)));
        // Kaydırma çubuğu (10px, thumb .18 / hover .32) — Chromium çubuğu düzende yer kaplar.
        let bounds = self.scroll.bounds();
        let view_h = f32::from(bounds.size.height);
        let max = f32::from(self.scroll.max_offset().y);
        let thumb = (view_h > 0. && max > 0.).then(|| {
            let total = view_h + max;
            let th = (view_h * view_h / total).max(20.);
            let top = (offset / max).clamp(0., 1.) * (view_h - th);
            div().absolute().right_0().top(px(top)).w(px(10.)).h(px(th)).rounded(px(5.)).bg(css_rgba(120, 170, 255, 0.18))
        });
        // Yapışkan eylem çubuğu: kapak kaydırılınca üstte kalır.
        let sticky = (offset >= 330.).then(|| div().absolute().top_0().left_0().child(self.actionbar("sticky", width, p, cx)));
        div().relative().flex_1().min_w_0().h_full().overflow_hidden().bg(p.bg_0).child(content).children(sticky).children(thumb).into_any_element()
    }
}

/// `.library-game-state`: 8px yeşil nokta, 3px `success-soft` halka.
fn state_dot(p: &Palette) -> gpui_kit::Div {
    div().size(px(8.)).flex_shrink_0().rounded_full().bg(p.success).shadow(shadows(&[p2_ui::tokens::Shadow::new(0., 0., 0., 3., p.success_soft)]))
}

impl Render for LibraryPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        let side = self.side_panel(window, &p, cx);
        let main = self.main_area(window, &p, cx);
        let overlays = self.overlays(window, &p, cx);
        div()
            .id("library-page")
            .relative()
            .size_full()
            .flex()
            .bg(p.bg_0)
            .text_color(p.text)
            .child(side)
            .child(main)
            .children(overlays)
            .p2_enter("library-page", Entrance::Fade, Duration::ZERO)
    }
}

#[allow(dead_code)]
fn _px(v: Pixels) -> f32 {
    f32::from(v)
}

/// .library-post-content .rich-message: düz paragraflar 13px (kalıtılan, satır 1,6) ve paragraf arası 6px.
/// `rich::render_message` sohbet boyunu (fs-lg 13,5) zorladığı için düz metin bloklarını burada çizeriz;
/// kod/dosya bloğu içeren gönderi ortak çiziciye bırakılır.
fn post_content(content: &str, p: &Palette) -> AnyElement {
    use gpui_kit::{FontStyle, HighlightStyle, StyledText};
    use p2_ui::rich::{Block, Span, parse_message};
    let blocks = parse_message(content);
    if blocks.iter().any(|b| !matches!(b, Block::Text { .. })) {
        return p2_ui::rich::render_message(content, p);
    }
    let style = |s: Span| match s {
        Span::Bold => HighlightStyle { font_weight: Some(FontWeight::BOLD), color: Some(p.text_strong), ..Default::default() },
        Span::Italic => HighlightStyle { font_style: Some(FontStyle::Italic), ..Default::default() },
        Span::Code => HighlightStyle { background_color: Some(p.code_bg), color: Some(p.text_accent), ..Default::default() },
        Span::Mention => HighlightStyle { background_color: Some(p.mention_bg), color: Some(p.text_accent), font_weight: Some(FontWeight::MEDIUM), ..Default::default() },
    };
    let mut col = div().flex().flex_col().gap(px(6.));
    for b in blocks {
        if let Block::Text { text, spans } = b {
            col = col.child(div().child(StyledText::new(text).with_highlights(spans.into_iter().map(|(r, s)| (r, style(s))))));
        }
    }
    col.into_any_element()
}
