//! Görev panosu — BoardPage.tsx + board.css'in birebir karşılığı: pano sekmeleri (sağ tık / çift tık /
//! ⋯ menüsü, yeni pano, yeniden adlandır, sil), başlık (sprint hedefi + ilerleme çubuğu), görünüm
//! (Pano/Liste), sprint ve kişi seçimi (yerel `select` görünümü), arama, "Henüz sprint yok" notu,
//! kanban (sürükle-bırak: hedef sütun vurgusu, kesikli bırakma yuvası, soluklaşan kart, 900 ms parlama),
//! liste görünümü, kart/satır sağ tık menüsü (aç, düzenle, duruma taşı, sprint'e taşı, sil), görev
//! ayrıntısı (sağ panel ya da ortada geniş; `board_task.rs`), görev penceresi (`board_editor.rs`),
//! sprint penceresi (`board_sprint.rs`), onay pencereleri, gerçek zamanlı güncellemeler
//! (task:updated/moved/deleted, task:comment*, sprint:*, board:*).

use std::sync::Arc;
use p2_ui::P2Fx as _; // backdrop_blur
use std::time::Duration;

use gpui_kit::component::input::{Input, InputState, TextareaState};
use gpui_kit::prelude::*;
use gpui_kit::{Animation, AnimationExt as _, AnyElement, Context, Entity, MouseButton, Pixels, Point, SharedString, Subscription, Window, div, point, px};
use p2_client_core::{Api, ApiError, BOARD_COLUMNS, BoardContent, BoardInfo, Member, Task, TaskComment};
use p2_ui::P2Text as _;
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::{FONT_MONO, Palette, Shadow, font_size, fs, radius, shadows};
use p2_ui::{Entrance, P2Motion as _};
use serde_json::{Value, json};

use super::board_editor::{BoardRef, EditorEvent, TaskEditor};
use super::chat::view::{Tone, menu_hr, menu_item, menu_panel, place_menu};
use super::{submit_on_enter, text_input, value};
use crate::task;

#[path = "board_ui.rs"]
pub(super) mod ui;
#[path = "board_task.rs"]
mod task_panel;
#[path = "board_sprint.rs"]
mod sprint;

use ui::*;

/// Sürüklenen kart (GPUI sürükle-bırak yükü + sürükleme önizlemesi).
#[derive(Clone)]
pub struct DraggedTask {
    id: String,
    title: String,
    number: i64,
    priority: String,
}

impl Render for DraggedTask {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        // Chromium'un sürükleme görüntüsü: kartın yarı saydam kopyası.
        let (outer, strip, inner) = left_toned(priority_edge(&self.priority, &p), p.border, radius::MD);
        outer
            .w(px(260.))
            .opacity(0.8)
            .shadow(p.shadow_lg())
            .child(strip)
            .child(
                inner
                    .bg(p.bg_3)
                    .px(px(10.))
                    .pt(px(9.))
                    .pb(px(8.))
                    .flex()
                    .flex_col()
                    .gap(px(7.))
                    .child(mono(format!("#{}", self.number), 10., p.text_3))
                    .child(div().text_size(fs(12.)).line_height(gpui_kit::relative(1.45)).text_color(p.text).child(self.title.clone())),
            )
    }
}

#[derive(Clone, Copy, PartialEq)]
enum View {
    Board,
    List,
}

/// Açık onay penceresi (ConfirmDialog).
#[derive(Clone)]
enum Confirm {
    Board(BoardInfo),
    Task(Task),
    Sprint(p2_client_core::Sprint),
}

/// Açık yerel `select` listesi.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum OpenSelect {
    Scope,
    Assignee,
    PanelAssignee,
    PanelSprint,
}

/// Tek alanlı ad penceresi (NameDialog): yeni pano ya da yeniden adlandırma.
struct NameDialog {
    board: Option<BoardInfo>,
    input: Entity<InputState>,
}

pub struct BoardPage {
    /// Uygulama ayarları (`task_view`) için kök görünüm. Pencere kökü gpui-kit `Root` sarmalayıcısı
    /// olduğundan `window.root::<ClientApp>()` her zaman boş döner; tanıtıcı açıkça taşınır.
    app: gpui_kit::WeakEntity<crate::app::ClientApp>,
    ctx_api: Arc<Api>,
    org: String,
    project: String,
    project_name: String,
    me: String,
    members: Vec<Member>,
    can_manage: bool,
    can_create: bool,
    boards: Vec<BoardInfo>,
    board_id: Option<String>,
    content: Option<BoardContent>,
    /// "backlog" ya da sprint kimliği.
    scope: String,
    scope_chosen: bool,
    /// "all" | "mine" | kişi kimliği
    assignee: String,
    search: Entity<InputState>,
    view: View,
    selected: Option<String>,
    comments: Vec<TaskComment>,
    comment: Entity<TextareaState>,
    comment_edit: Option<(String, Entity<TextareaState>)>,
    comment_menu: Option<(String, Point<Pixels>)>,
    /// Yorum yazma alanının emoji paneli (chat_emoji.rs `EmojiPanel`, salt kullanım).
    comment_emoji: Option<super::chat::emoji::EmojiPanel>,
    /// Emoji düğmesinin pencere sınırları (`anchorFrom(button.getBoundingClientRect())`).
    emoji_anchor: std::rc::Rc<std::cell::Cell<Option<gpui_kit::Bounds<Pixels>>>>,
    /// `@` önerilerinde etkin satır (`mentionIndex`).
    mention_ix: usize,
    /// attachments.tsx `useAttachments`: yüklenmiş ekler, yükleniyor, hata.
    comment_files: Vec<p2_client_core::UploadedFile>,
    comment_uploading: bool,
    comment_upload_error: Option<String>,
    sending: bool,
    editor: Option<(Entity<TaskEditor>, Subscription)>,
    sprints_open: bool,
    sprint_ui: sprint::SprintUi,
    open_select: Option<OpenSelect>,
    board_menu: Option<(BoardInfo, Point<Pixels>, bool)>,
    task_menu: Option<(String, Point<Pixels>)>,
    name_dialog: Option<NameDialog>,
    confirm: Option<Confirm>,
    /// Sürüklenen kart ve bırakma yuvası (sütun, önüne konacak kart).
    dragging: Option<String>,
    slot: Option<(String, Option<String>)>,
    flash: Option<(String, usize)>,
    menu_epoch: usize,
    busy: bool,
    error: Option<String>,
    loading: bool,
    _subs: Vec<Subscription>,
}

impl BoardPage {
    #[allow(clippy::too_many_arguments)]
    pub fn new(app: gpui_kit::WeakEntity<crate::app::ClientApp>, api: Arc<Api>, org: String, project: String, project_name: String, me: String, members: Vec<Member>, can_manage: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = text_input(window, cx, "Görev ara");
        let comment = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, 9).submit_on_enter(true).placeholder("Bu görev hakkında yaz…"));
        let subs = vec![
            cx.subscribe_in(&search, window, |_, _, _: &gpui_kit::component::input::InputEvent, _, cx| cx.notify()),
            cx.subscribe_in(&comment, window, |this, _, ev: &gpui_kit::component::input::InputEvent, window, cx| {
                match ev {
                    // Öneri açıkken Enter etiketi ekler (BoardPage.tsx `onKeyDown`), yoksa gönderir.
                    gpui_kit::component::input::InputEvent::PressEnter { .. } => {
                        if !this.pick_active_mention(window, cx) {
                            this.send_comment(window, cx);
                        }
                    }
                    gpui_kit::component::input::InputEvent::Change => this.mention_ix = 0,
                    _ => {}
                }
                cx.notify();
            }),
        ];
        // Görünüm (Electron localStorage `p2:board-view`); parite kancası kayıtlı seçimi geçersiz kılar.
        let view = match std::env::var("P2_AUTOPILOT_BOARD_VIEW").ok().or_else(|| memory_get("p2:board-view")).as_deref() {
            Some("list") => View::List,
            _ => View::Board,
        };
        let mut page = Self {
            app,
            ctx_api: api,
            org,
            project,
            project_name,
            me,
            members,
            can_manage,
            can_create: true,
            boards: Vec::new(),
            board_id: None,
            content: None,
            scope: "backlog".into(),
            scope_chosen: false,
            assignee: "all".into(),
            search,
            view,
            selected: None,
            comments: Vec::new(),
            comment,
            comment_edit: None,
            comment_menu: None,
            comment_emoji: None,
            emoji_anchor: Default::default(),
            mention_ix: 0,
            comment_files: Vec::new(),
            comment_uploading: false,
            comment_upload_error: None,
            sending: false,
            editor: None,
            sprints_open: false,
            sprint_ui: sprint::SprintUi::new(window, cx),
            open_select: None,
            board_menu: None,
            task_menu: None,
            name_dialog: None,
            confirm: None,
            dragging: None,
            slot: None,
            flash: None,
            menu_epoch: 0,
            busy: false,
            error: None,
            loading: true,
            _subs: subs,
        };
        page.load_boards(window, cx);
        page
    }

    fn ctx(&self) -> Option<BoardRef> {
        Some(BoardRef { api: self.ctx_api.clone(), org: self.org.clone(), project: self.project.clone(), board: self.board_id.clone()? })
    }

    fn fail(&mut self, e: ApiError, cx: &mut Context<Self>) {
        self.busy = false;
        // Bildirimler sağ üstte (toasts.rs); panonun üstünde ayrı kırmızı şerit yok.
        crate::toasts::error(if e.message.is_empty() { "İşlem tamamlanamadı.".to_string() } else { e.message });
        cx.notify();
    }

    fn toast(text: impl Into<String>) {
        let text: String = text.into();
        crate::toasts::push(p2_ui::controls::ToastTone::Ok, None, &text);
    }

    fn load_boards(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (api, org, project) = (self.ctx_api.clone(), self.org.clone(), self.project.clone());
        task::run(window, cx, move || api.boards(&org, &project), |this, result, window, cx| match result {
            Ok(mut boards) => {
                boards.sort_by_key(|b| b.position);
                this.boards = boards;
                // Proje bazında hatırlanan pano (Electron `p2:board:<proje>`); silinmişse ilk panoya düşülür.
                let saved = memory_get(&format!("p2:board:{}", this.project)).filter(|id| this.boards.iter().any(|b| &b.id == id));
                match saved.or_else(|| this.boards.first().map(|b| b.id.clone())) {
                    Some(first) => this.open_board(first, window, cx),
                    None => {
                        this.loading = false;
                        cx.notify();
                    }
                }
            }
            Err(e) => {
                this.loading = false;
                // Boş durum metni sayfada kalır; hata ayrıca sağ üstte bildirilir.
                let text = if e.message.is_empty() { "Panolar yüklenemedi.".to_string() } else { e.message };
                crate::toasts::error(&text);
                this.error = Some(text);
                cx.notify();
            }
        });
    }

    fn open_board(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        memory_set(&format!("p2:board:{}", self.project), &id);
        self.board_id = Some(id);
        self.selected = None;
        self.scope_chosen = false;
        self.reload(window, cx);
    }

    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ctx) = self.ctx() else { return };
        cx.notify();
        // Parite kancası (veri yazmaz): P2_AUTOPILOT_BOARD_DEMO=<{sprints,tasks} JSON> sunucunun pano içeriği
        // yerine kullanılır (Electron tarafında aynı gövde CDP sahte yanıtıyla gelir).
        let demo = std::env::var("P2_AUTOPILOT_BOARD_DEMO").ok().and_then(|d| serde_json::from_str::<Value>(&d).ok());
        let info = self.current_board();
        task::run(
            window,
            cx,
            move || match (demo, info) {
                (Some(mut d), Some(info)) => {
                    d["board"] = serde_json::to_value(&info).unwrap_or_default();
                    serde_json::from_value::<BoardContent>(d).map_err(|e| ApiError { message: e.to_string(), ..Default::default() })
                }
                _ => ctx.api.board(&ctx.org, &ctx.project, &ctx.board),
            },
            |this, result, window, cx| {
            this.loading = false;
            match result {
                Ok(content) => {
                    crate::trace(&format!("pano: {} {} görev, {} sprint", content.board.name, content.tasks.len(), content.sprints.len()));
                    // Kapsam: kullanıcı seçmediyse aktif sprint, yoksa backlog.
                    if !this.scope_chosen {
                        this.scope = content.sprints.iter().find(|s| s.status == "ACTIVE").map(|s| s.id.clone()).unwrap_or_else(|| "backlog".into());
                    }
                    this.content = Some(content);
                    this.error = None;
                    this.autopilot(window, cx);
                }
                Err(e) => this.fail(e, cx),
            }
            cx.notify();
        });
    }

    /// Otomatik pilot (testler + parite kancaları, veri yazmayanlar ayrıca belirtildi):
    /// P2_AUTOPILOT_TASK=<başlık> görev açar (yazar), P2_AUTOPILOT_MOVE=<başlık>|<SÜTUN> kartı taşır (yazar),
    /// P2_AUTOPILOT_OPEN_TASK=<başlık> ayrıntıyı açar; P2_AUTOPILOT_BOARD_UI=<durum> yalnız görünüm açar:
    /// `task-menu:<başlık>@x,y`, `board-menu`, `new-task`, `edit:<başlık>`, `sprints`, `sprint-form`, `new-board`,
    /// `delete-task:<başlık>`, `scope-select`, `assignee-select`.
    fn autopilot(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        static DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if DONE.swap(true, std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        let find = |this: &Self, title: &str| this.content.as_ref().and_then(|c| c.tasks.iter().find(|t| t.title == title).cloned());
        if let Ok(title) = std::env::var("P2_AUTOPILOT_TASK") {
            self.open_editor(None, "TODO", window, cx);
            if let Some((editor, _)) = &self.editor {
                editor.update(cx, |e, cx| e.autopilot(&title, window, cx));
            }
        }
        if let Ok(spec) = std::env::var("P2_AUTOPILOT_MOVE") {
            if let Some((title, status)) = spec.split_once('|') {
                if let Some(t) = find(self, title) {
                    self.drop_task(t.id, status.to_string(), None, window, cx);
                }
            }
        }
        if let Ok(title) = std::env::var("P2_AUTOPILOT_OPEN_TASK") {
            if let Some(t) = find(self, &title) {
                self.select(t.id, window, cx);
            }
        }
        if let Ok(spec) = std::env::var("P2_AUTOPILOT_BOARD_UI") {
            let (what, arg) = spec.split_once(':').unwrap_or((spec.as_str(), ""));
            match what {
                "task-menu" => {
                    let (title, at) = arg.split_once('@').unwrap_or((arg, "640,320"));
                    let (x, y) = at.split_once(',').and_then(|(x, y)| Some((x.parse::<f32>().ok()?, y.parse::<f32>().ok()?))).unwrap_or((640., 320.));
                    if let Some(t) = find(self, title) {
                        self.task_menu = Some((t.id, point(px(x), px(y))));
                    }
                }
                "board-menu" => {
                    if let Some(b) = self.current_board() {
                        self.board_menu = Some((b, point(px(0.), px(0.)), true));
                    }
                }
                "new-task" => self.open_editor(None, "TODO", window, cx),
                "edit" => {
                    if let Some(t) = find(self, arg) {
                        self.open_editor(Some(t), "TODO", window, cx);
                    }
                }
                "sprints" => self.open_sprints(window, cx),
                "sprint-form" => {
                    self.open_sprints(window, cx);
                    self.sprint_ui.screen = sprint::Screen::Form;
                }
                "new-board" => self.open_name_dialog(None, window, cx),
                "delete-task" => {
                    if let Some(t) = find(self, arg) {
                        self.confirm = Some(Confirm::Task(t));
                    }
                }
                // Yorum yazma alanı: emoji paneli açık / `@` önerileri (görev P2_AUTOPILOT_OPEN_TASK ile açık).
                "comment-emoji" => self.toggle_comment_emoji(window, cx),
                "comment-mention" => self.comment.update(cx, |s, cx| {
                    s.set_value("@", window, cx);
                    s.focus(window, cx);
                }),
                "scope-select" => self.open_select = Some(OpenSelect::Scope),
                "assignee-select" => self.open_select = Some(OpenSelect::Assignee),
                _ => {}
            }
            crate::trace(&format!("pano: görünüm {spec}"));
        }
        cx.notify();
    }

    fn current_board(&self) -> Option<BoardInfo> {
        self.boards.iter().find(|b| Some(&b.id) == self.board_id.as_ref()).cloned()
    }

    // ------------------------------------------------------------ görünür görevler

    fn visible(&self, cx: &gpui_kit::App) -> Vec<&Task> {
        let Some(content) = &self.content else { return Vec::new() };
        let needle = value(&self.search, cx).trim().to_lowercase();
        content
            .tasks
            .iter()
            .filter(|t| if self.scope == "backlog" { t.sprint_id.is_none() } else { t.sprint_id.as_deref() == Some(self.scope.as_str()) })
            .filter(|t| match self.assignee.as_str() {
                "all" => true,
                "mine" => t.assignee_id.as_deref() == Some(self.me.as_str()),
                id => t.assignee_id.as_deref() == Some(id),
            })
            .filter(|t| needle.is_empty() || format!("{} {}", t.number, t.title).to_lowercase().contains(&needle))
            .collect()
    }

    fn column<'a>(tasks: &[&'a Task], status: &str) -> Vec<&'a Task> {
        let mut out: Vec<&Task> = tasks.iter().copied().filter(|t| t.column() == status).collect();
        out.sort_by_key(|t| (t.position, t.number));
        out
    }

    fn member(&self, id: Option<&str>) -> Option<&Member> {
        id.and_then(|id| self.members.iter().find(|m| m.id == id))
    }

    fn sprint_name(&self, id: Option<&str>) -> String {
        id.and_then(|id| self.content.as_ref()?.sprints.iter().find(|s| s.id == id)).map(|s| s.name.clone()).unwrap_or_else(|| "Backlog".into())
    }

    fn upsert(&mut self, task: Task) {
        if let Some(content) = &mut self.content {
            if task.board_id != content.board.id && !task.board_id.is_empty() {
                return;
            }
            match content.tasks.iter_mut().find(|t| t.id == task.id) {
                Some(t) => *t = task,
                None => content.tasks.push(task),
            }
        }
    }

    fn can_edit(&self, t: &Task) -> bool {
        self.can_manage || t.created_by_id == self.me || t.assignee_id.as_deref() == Some(self.me.as_str())
    }

    // ------------------------------------------------------------ eylemler

    /// Kartı `status` sütununa, `before` kartının önüne (yoksa sona) koyar.
    fn drop_task(&mut self, id: String, status: String, before: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ctx) = self.ctx() else { return };
        let visible: Vec<Task> = self.visible(cx).into_iter().cloned().collect();
        let refs: Vec<&Task> = visible.iter().collect();
        let current: Vec<String> = Self::column(&refs, &status).into_iter().map(|t| t.id.clone()).collect();
        let mut order: Vec<String> = current.iter().filter(|t| **t != id).cloned().collect();
        let at = before.as_ref().and_then(|b| order.iter().position(|t| t == b)).unwrap_or(order.len());
        order.insert(at, id.clone());
        let same_column = self.content.as_ref().and_then(|c| c.tasks.iter().find(|t| t.id == id)).is_some_and(|t| t.status == status);
        if same_column && order == current {
            return;
        }
        // İyimser güncelleme: sunucu yanıtı (ve task:moved) gelince kesinleşir.
        if let Some(content) = &mut self.content {
            for (ix, tid) in order.iter().enumerate() {
                if let Some(t) = content.tasks.iter_mut().find(|t| &t.id == tid) {
                    t.position = ix as i64;
                    if *tid == id {
                        t.status = status.clone();
                    }
                }
            }
        }
        self.flash_card(id.clone(), cx);
        cx.notify();
        let number = self.content.as_ref().and_then(|c| c.tasks.iter().find(|t| t.id == id)).map(|t| t.number).unwrap_or_default();
        let st = status.clone();
        task::run(window, cx, move || ctx.api.move_task(&ctx.org, &ctx.project, &ctx.board, &id, &st, &order), move |this, result, window, cx| match result {
            Ok(tasks) => {
                crate::trace(&format!("pano: taşındı #{number} → {status}"));
                for t in tasks {
                    this.upsert(t);
                }
                cx.notify();
            }
            Err(e) => {
                this.fail(e, cx);
                this.reload(window, cx);
            }
        });
    }

    /// `.board-card.flash` (900 ms halka).
    fn flash_card(&mut self, id: String, cx: &mut Context<Self>) {
        let epoch = self.flash.as_ref().map_or(0, |f| f.1 + 1);
        self.flash = Some((id, epoch));
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(900)).await;
            let _ = this.update(cx, |this, cx| {
                if this.flash.as_ref().is_some_and(|f| f.1 == epoch) {
                    this.flash = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn update_task(&mut self, id: String, patch: Value, done: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ctx) = self.ctx() else { return };
        self.busy = true;
        cx.notify();
        task::run(window, cx, move || ctx.api.update_task(&ctx.org, &ctx.project, &ctx.board, &id, patch), move |this, result, _, cx| {
            this.busy = false;
            match result {
                Ok(t) => {
                    crate::trace(&format!("pano: görev güncellendi #{}", t.number));
                    this.upsert(t);
                    if let Some(text) = &done {
                        Self::toast(text.clone());
                    }
                }
                Err(e) => this.fail(e, cx),
            }
            cx.notify();
        });
    }

    fn select(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.selected = Some(id.clone());
        self.comments.clear();
        self.comment_edit = None;
        self.open_select = None;
        cx.notify();
        if std::env::var("P2_AUTOPILOT_BOARD_DEMO").is_ok() {
            crate::trace("pano: görev açıldı, 0 yorum");
            return;
        }
        let Some(ctx) = self.ctx() else { return };
        task::run(window, cx, move || (id.clone(), ctx.api.task_comments(&ctx.org, &ctx.project, &ctx.board, &id)), |this, (id, result), _, cx| {
            if this.selected.as_deref() != Some(id.as_str()) {
                return;
            }
            if let Ok(comments) = result {
                crate::trace(&format!("pano: görev açıldı, {} yorum", comments.len()));
                this.comments = comments;
                cx.notify();
            }
        });
    }

    fn open_editor(&mut self, editing: Option<Task>, status: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ctx) = self.ctx() else { return };
        let sprint = (self.scope != "backlog").then(|| self.scope.clone());
        let sprints = self.content.as_ref().map(|c| c.sprints.clone()).unwrap_or_default();
        let can_delete = editing.as_ref().is_some_and(|t| self.can_edit(t));
        let members = self.members.clone();
        let status = status.to_string();
        let editor = cx.new(|cx| TaskEditor::new(ctx, editing, &status, sprint, members, sprints, can_delete, window, cx));
        let sub = cx.subscribe_in(&editor, window, |this, _, event: &EditorEvent, window, cx| {
            this.editor = None;
            match event {
                EditorEvent::Saved(task, created) => {
                    crate::trace(&format!("pano: görev kaydedildi #{} «{}»", task.number, task.title));
                    if *created {
                        Self::toast(format!("#{} “{}” oluşturuldu.", task.number, task.title));
                    }
                    this.upsert(task.clone());
                }
                EditorEvent::Delete(task) => this.delete_task(task.id.clone(), window, cx),
                EditorEvent::Cancelled => {}
            }
            cx.notify();
        });
        self.editor = Some((editor, sub));
        self.selected = None;
        cx.notify();
    }

    fn delete_task(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ctx) = self.ctx() else { return };
        self.confirm = None;
        task::run(window, cx, move || (id.clone(), ctx.api.delete_task(&ctx.org, &ctx.project, &ctx.board, &id)), |this, (id, result), _, cx| match result {
            Ok(()) => {
                if let Some(c) = &mut this.content {
                    c.tasks.retain(|t| t.id != id);
                }
                if this.selected.as_deref() == Some(id.as_str()) {
                    this.selected = None;
                }
                Self::toast("Görev silindi.");
                cx.notify();
            }
            Err(e) => this.fail(e, cx),
        });
    }

    fn open_name_dialog(&mut self, board: Option<BoardInfo>, window: &mut Window, cx: &mut Context<Self>) {
        let input = text_input(window, cx, "Örn. Oyun takımı");
        if let Some(b) = &board {
            let name = b.name.clone();
            input.update(cx, |i, cx| i.set_value(name, window, cx));
        }
        input.update(cx, |i, cx| i.focus(window, cx));
        self._subs.push(submit_on_enter(&input, window, cx, |this, window, cx| this.save_name(window, cx)));
        self._subs.push(cx.subscribe_in(&input, window, |_, _, _: &gpui_kit::component::input::InputEvent, _, cx| cx.notify()));
        self.name_dialog = Some(NameDialog { board, input });
        cx.notify();
    }

    fn save_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = &self.name_dialog else { return };
        let name = value(&d.input, cx).trim().to_string();
        if name.chars().count() < 2 {
            return;
        }
        let target = d.board.clone();
        self.name_dialog = None;
        self.busy = true;
        let (api, org, project) = (self.ctx_api.clone(), self.org.clone(), self.project.clone());
        task::run(
            window,
            cx,
            move || match &target {
                Some(b) => api.rename_board(&org, &project, &b.id, &name).map(|b| (b, false)),
                None => api.create_board(&org, &project, &name).map(|b| (b, true)),
            },
            |this, result, window, cx| {
                this.busy = false;
                match result {
                    Ok((board, created)) => {
                        let id = board.id.clone();
                        match this.boards.iter_mut().find(|b| b.id == board.id) {
                            Some(b) => *b = board,
                            None => this.boards.push(board),
                        }
                        this.boards.sort_by_key(|b| b.position);
                        if created {
                            this.open_board(id, window, cx);
                        }
                        if let Some(c) = &mut this.content {
                            if let Some(b) = this.boards.iter().find(|b| b.id == c.board.id) {
                                c.board = b.clone();
                            }
                        }
                    }
                    Err(e) => this.fail(e, cx),
                }
                cx.notify();
            },
        );
        cx.notify();
    }

    fn delete_board(&mut self, board: BoardInfo, window: &mut Window, cx: &mut Context<Self>) {
        self.confirm = None;
        let (api, org, project) = (self.ctx_api.clone(), self.org.clone(), self.project.clone());
        let id = board.id.clone();
        task::run(window, cx, move || api.delete_board(&org, &project, &id), move |this, result, window, cx| match result {
            Ok(()) => {
                this.boards.retain(|b| b.id != board.id);
                if this.board_id.as_deref() == Some(board.id.as_str()) {
                    match this.boards.first().map(|b| b.id.clone()) {
                        Some(first) => this.open_board(first, window, cx),
                        None => {
                            memory_set(&format!("p2:board:{}", this.project), "");
                            this.board_id = None;
                            this.content = None;
                        }
                    }
                }
                cx.notify();
            }
            Err(e) => this.fail(e, cx),
        });
    }

    fn open_sprints(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let has = self.content.as_ref().is_some_and(|c| !c.sprints.is_empty());
        self.sprint_ui.reset(if has { sprint::Screen::List } else { sprint::Screen::Form }, None, window, cx);
        self.sprints_open = true;
        cx.notify();
    }

    fn close_overlays(&mut self) -> bool {
        let any = self.open_select.is_some() || self.board_menu.is_some() || self.task_menu.is_some() || self.comment_menu.is_some() || self.comment_emoji.is_some();
        self.open_select = None;
        self.board_menu = None;
        self.task_menu = None;
        self.comment_menu = None;
        self.comment_emoji = None;
        any
    }

    /// Gerçek zamanlı olaylar (çalışma alanından).
    pub fn on_event(&mut self, event: &str, payload: &Value, window: &mut Window, cx: &mut Context<Self>) {
        let board = self.board_id.clone().unwrap_or_default();
        let ours = |v: &Value| v.as_str().is_none_or(|b| b == board);
        match event {
            "task:updated" => {
                if let Ok(task) = serde_json::from_value::<Task>(payload["task"].clone()) {
                    if task.board_id == board {
                        crate::trace(&format!("pano: olay task:updated #{} {}", task.number, task.status));
                        self.upsert(task);
                    }
                }
            }
            "task:moved" if ours(&payload["boardId"]) => {
                if let Ok(tasks) = serde_json::from_value::<Vec<Task>>(payload["tasks"].clone()) {
                    crate::trace(&format!("pano: olay task:moved ({} görev)", tasks.len()));
                    for t in tasks {
                        self.upsert(t);
                    }
                }
            }
            "task:deleted" if ours(&payload["boardId"]) => {
                let id = payload["taskId"].as_str().unwrap_or_default();
                if let Some(c) = &mut self.content {
                    c.tasks.retain(|t| t.id != id);
                }
                if self.selected.as_deref() == Some(id) {
                    self.selected = None;
                }
            }
            "task:comment" => {
                if let Ok(c) = serde_json::from_value::<TaskComment>(payload["comment"].clone()) {
                    if self.selected.as_deref() == Some(c.task_id.as_str()) {
                        match self.comments.iter_mut().find(|x| x.id == c.id) {
                            Some(x) => *x = c,
                            None => self.comments.push(c),
                        }
                    }
                }
            }
            "task:comment:reactions" => {
                let id = payload["commentId"].as_str().unwrap_or_default();
                let mut out: Vec<p2_client_core::Reaction> = Vec::new();
                for row in payload["rows"].as_array().cloned().unwrap_or_default() {
                    let emoji = row["emoji"].as_str().unwrap_or_default().to_string();
                    let mine = row["userId"].as_str() == Some(self.me.as_str());
                    match out.iter_mut().find(|r| r.emoji == emoji) {
                        Some(r) => {
                            r.count += 1;
                            r.reacted |= mine;
                        }
                        None => out.push(p2_client_core::Reaction { emoji, count: 1, reacted: mine, user_id: None }),
                    }
                }
                if let Some(c) = self.comments.iter_mut().find(|c| c.id == id) {
                    c.reactions = out;
                }
            }
            "task:comment:deleted" => {
                let id = payload["commentId"].as_str().unwrap_or_default();
                self.comments.retain(|c| c.id != id);
            }
            "sprint:updated" | "sprint:deleted" if ours(&payload["boardId"]) || ours(&payload["sprint"]["boardId"]) => self.reload(window, cx),
            "board:updated" => {
                if let Ok(b) = serde_json::from_value::<BoardInfo>(payload["board"].clone()) {
                    if b.project_id == self.project || b.project_id.is_empty() {
                        match self.boards.iter_mut().find(|x| x.id == b.id) {
                            Some(x) => *x = b,
                            None => self.boards.push(b),
                        }
                        self.boards.sort_by_key(|b| b.position);
                    }
                }
            }
            "board:deleted" => {
                let id = payload["boardId"].as_str().unwrap_or_default().to_string();
                self.boards.retain(|b| b.id != id);
                if self.board_id.as_deref() == Some(id.as_str()) {
                    if let Some(first) = self.boards.first().map(|b| b.id.clone()) {
                        self.open_board(first, window, cx);
                    }
                }
            }
            _ => {}
        }
        cx.notify();
    }

    // ------------------------------------------------------------ çizim: üst kısım

    /// `.board-tabs`: proje adı (vurgu noktalı), pano hapları, ＋, sağda ⋯.
    fn tabs(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let mut list = div().flex_none().flex().items_center().gap(px(5.)).p(px(3.)).border_1().border_color(p.border_subtle).rounded(radius::MD).bg(p.bg_2);
        for b in &self.boards {
            let on = self.board_id.as_deref() == Some(b.id.as_str());
            let (id, menu_b, dbl_b) = (b.id.clone(), b.clone(), b.clone());
            let (hover, strong) = (p.bg_hover, p.text_strong);
            let manage = self.can_manage;
            list = list.child(
                div()
                    .id(SharedString::from(format!("tab-{}", b.id)))
                    .relative()
                    .h(px(28.))
                    .px(px(14.))
                    .flex()
                    .items_center()
                    .border_1()
                    .rounded(radius::SM)
                    .text_size(fs(11.5))
                    .css_weight(650.)
                    .whitespace_nowrap()
                    .cursor_pointer()
                    .map(|s| if on { s.border_color(p.border).bg(p.bg_3).text_color(p.text_strong).shadow(shadows(&[p.inset_top])) } else { s.border_color(gpui_kit::transparent_black()).text_color(p.text_3).hover(move |s| s.bg(hover).text_color(strong)) })
                    .child(b.name.clone())
                    .when(on, |s| s.child(div().absolute().bottom(px(2.)).left_0().right_0().flex().justify_center().child(div().w(px(18.)).h(px(2.)).rounded(px(2.)).bg(p.accent_bar.background()))))
                    .on_click(cx.listener(move |this, ev: &gpui_kit::ClickEvent, window, cx| {
                        if ev.click_count() >= 2 && manage {
                            this.open_name_dialog(Some(dbl_b.clone()), window, cx);
                        } else if this.board_id.as_deref() != Some(id.as_str()) {
                            this.open_board(id.clone(), window, cx);
                        }
                    }))
                    .when(manage, |s| {
                        s.on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, ev: &gpui_kit::MouseDownEvent, _, cx| {
                                this.close_overlays();
                                this.menu_epoch += 1;
                                this.board_menu = Some((menu_b.clone(), ev.position, false));
                                cx.notify();
                            }),
                        )
                    }),
            );
        }
        if self.can_manage {
            let (ba, ta) = (p.border_accent, p.text_accent);
            list = list.child(
                div()
                    .id("tab-add")
                    .size(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_1()
                    .border_color(gpui_kit::transparent_black())
                    .rounded(radius::SM)
                    .text_size(fs(13.))
                    .css_weight(650.)
                    .text_color(p.text_3)
                    .cursor_pointer()
                    .hover(move |s| s.border_color(ba).text_color(ta))
                    .child("＋")
                    .on_click(cx.listener(|this, _, window, cx| this.open_name_dialog(None, window, cx))),
            );
        }
        let project = div()
            .flex_none()
            .max_w(px(220.))
            .flex()
            .items_center()
            .gap(px(7.))
            .child(div().flex_none().size(px(6.)).rounded_full().bg(p.accent_hover).shadow(shadows(&[Shadow::new(0., 0., 6., 0., p.accent_hover)])))
            .child(div().truncate().text_size(fs(11.5)).font_weight(gpui_kit::FontWeight::BOLD).text_color(p.text_2).child(self.project_name.clone()));
        let tools = (self.can_manage && self.current_board().is_some()).then(|| {
            board_button("board-menu-toggle", "⋯", Btn::Subtle, false, p)
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(|this, _, _, cx| {
                    let open = this.board_menu.as_ref().is_some_and(|m| m.2);
                    this.close_overlays();
                    if !open {
                        if let Some(b) = this.current_board() {
                            this.menu_epoch += 1;
                            this.board_menu = Some((b, point(px(0.), px(0.)), true));
                        }
                    }
                    cx.notify();
                }))
        });
        // Çok pano varsa haplar yatay rayda kayar (altta kaydırma çubuğu). Çubuk oluğu (`RAIL_GUTTER`)
        // satırın altına eklenip eksi kenar boşluğuyla geri alınır; proje adı ve ⋯ aynı alt boşlukla
        // hapların ortasına hizalı kalır → yerleşim eskisiyle birebir.
        let gutter = p2_ui::fx::RAIL_GUTTER;
        div()
            .flex_none()
            .pt(px(10.))
            .px(px(22.))
            .mb(px(-gutter))
            .flex()
            .items_center()
            .gap(px(12.))
            .child(project.mb(px(gutter)))
            .child(p2_ui::components::h_rail("board-tabs-rail").flex_1().min_w_0().child(list))
            .children(tools.map(|t| t.mb(px(gutter))))
            .into_any_element()
    }

    /// `.board-head`: başlık + kapsam açıklaması (sprint ilerlemesi) ve denetimler.
    fn head(&self, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let sprints = self.content.as_ref().map(|c| c.sprints.clone()).unwrap_or_default();
        let current = self.current_board();
        let title = match &current {
            Some(b) => b.name.clone(),
            None if !self.project_name.is_empty() => format!("{} · Görevler", self.project_name),
            None => "Görevler".into(),
        };
        let scope_sprint = sprints.iter().find(|s| s.id == self.scope).cloned();
        let subtitle = if self.scope == "backlog" { "Sprint’e alınmamış işler".to_string() } else { scope_sprint.as_ref().and_then(|s| s.goal.clone()).filter(|g| !g.is_empty()).unwrap_or_else(|| "Sprint panosu".into()) };
        let progress = scope_sprint.as_ref().and_then(|s| sprint_progress(s.starts_at.as_deref(), s.ends_at.as_deref()));
        let title_block = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .child(div().h(p2_ui::text::scaled(px(20.))).text_size(fs(19.)).font_weight(gpui_kit::FontWeight::BOLD).tracking(-0.02).text_color(p.text_strong).mb(px(3.)).truncate().child(title))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .min_w_0()
                    .h(p2_ui::text::scaled(px(11.)))
                    .text_size(fs(11.5))
                    .text_color(p.text_3)
                    .child(div().truncate().child(subtitle))
                    .when_some(progress, |s, (pct, left)| {
                        s.child(
                            div()
                                .flex_none()
                                .ml(px(10.))
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(div().relative().w(px(96.)).h(px(4.)).rounded(px(99.)).bg(p.navy_600).overflow_hidden().child(div().absolute().left_0().top_0().bottom_0().w(px(96. * pct / 100.)).rounded(px(99.)).bg(p.accent_bar.background())))
                                .child(div().text_size(fs(10.5)).css_weight(650.).text_color(p.text_3).whitespace_nowrap().child(if left > 0 { format!("{left} gün kaldı") } else { "süre doldu".into() })),
                        )
                    }),
            );

        // Görünüm düğmeleri
        let view = self.view;
        let view_btn = |id: &'static str, label: &'static str, on: bool| {
            let strong = p.text_strong;
            div()
                .id(id)
                .h(px(26.))
                .px(px(12.))
                .flex()
                .items_center()
                .rounded(radius::SM)
                .text_size(fs(11.5))
                .css_weight(650.)
                .cursor_pointer()
                .map(|s| if on { s.bg(p.bg_3).text_color(p.text_strong).shadow(shadows(&[p.inset_top])) } else { s.text_color(p.text_3).hover(move |s| s.text_color(strong)) })
                .child(label)
        };
        let views = div()
            .flex_none()
            .flex()
            .p(px(3.))
            .border_1()
            .border_color(p.border_subtle)
            .rounded(radius::MD)
            .bg(p.bg_2)
            .child(view_btn("view-board", "Pano", view == View::Board).on_click(cx.listener(|this, _, _, cx| {
                this.view = View::Board;
                memory_set("p2:board-view", "board");
                cx.notify();
            })))
            .child(view_btn("view-list", "Liste", view == View::List).on_click(cx.listener(|this, _, _, cx| {
                this.view = View::List;
                memory_set("p2:board-view", "list");
                cx.notify();
            })));

        // Sprint ve kişi seçimleri (yerel select)
        let scope_opts: Vec<(String, String)> = sprints
            .iter()
            .map(|s| (s.id.clone(), format!("{}{}", s.name, if s.status == "ACTIVE" { " · aktif" } else if s.status == "DONE" { " · bitti" } else { "" })))
            .chain(std::iter::once(("backlog".to_string(), "Backlog".to_string())))
            .collect();
        let who_opts: Vec<(String, String)> = [("all".to_string(), "Herkes".to_string()), ("mine".to_string(), "Bana atananlar".to_string())].into_iter().chain(self.members.iter().map(|m| (m.id.clone(), m.display_name.clone()))).collect();
        let scope_select = self.select_field(OpenSelect::Scope, "scope", &scope_opts, &self.scope.clone(), p, window, cx, |this, v, _, cx| {
            this.scope = v;
            this.scope_chosen = true;
            cx.notify();
        });
        let who_select = self.select_field(OpenSelect::Assignee, "who", &who_opts, &self.assignee.clone(), p, window, cx, |this, v, _, cx| {
            this.assignee = v;
            cx.notify();
        });

        // `.admin-search` (168 × 32)
        let focused = gpui_kit::Focusable::focus_handle(self.search.read(cx), cx).is_focused(window);
        let search = div()
            .flex_none()
            .w(px(168.))
            .h(px(32.))
            .px(px(8.))
            .flex()
            .items_center()
            .border_1()
            .rounded(radius::SM)
            .bg(p.bg_3)
            .map(|s| if focused { s.border_color(p.border_accent).shadow(shadows(&[p.ring])) } else { s.border_color(p.border) })
            .child(div().w(px(22.)).flex_none().text_color(p.text_4).child("⌕"))
            .child(div().flex_1().min_w_0().child(Input::new(&self.search).appearance(false).bordered(false).focus_bordered(false).text_size(fs(12.)).text_color(p.text).px(px(2.)).h(p2_ui::text::scaled(px(16.)))));

        let controls = div()
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(7.))
            .child(views)
            .child(scope_select)
            .child(who_select)
            .child(search)
            .when(self.can_manage, |s| s.child(board_button("sprints", "Sprint’ler", Btn::Plain, false, p).on_click(cx.listener(|this, _, window, cx| this.open_sprints(window, cx)))))
            .when(self.can_create, |s| s.child(board_button("new-task", "Yeni görev", Btn::Primary, false, p).on_click(cx.listener(|this, _, window, cx| this.open_editor(None, "TODO", window, cx)))));

        div()
            .flex_none()
            .pt(px(18.))
            .pb(px(14.))
            .px(px(22.))
            .flex()
            .items_center()
            .gap(px(16.))
            .border_b_1()
            .border_color(p.border_subtle)
            .child(title_block)
            .child(controls)
            .into_any_element()
    }

    /// Yerel `select` + açıkken seçenek listesi. `pick(değer)`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn select_field(
        &self,
        which: OpenSelect,
        id: &'static str,
        options: &[(String, String)],
        current: &str,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
        pick: impl Fn(&mut Self, String, &mut Window, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        let labels: Vec<String> = options.iter().map(|o| o.1.clone()).collect();
        let selected = options.iter().position(|o| o.0 == current).unwrap_or(0);
        let width = select_width(window, &labels, fs(11.5), 650., 8., 200.);
        let open = self.open_select == Some(which);
        let field = native_select(SharedString::from(format!("sel-{id}")), labels.get(selected).cloned().unwrap_or_default(), width, 30., 8., fs(11.5), 650., open, p)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, _, cx| {
                let was = this.open_select == Some(which);
                this.close_overlays();
                this.open_select = (!was).then_some(which);
                cx.notify();
            }));
        let mut wrap = div().relative().flex_none().child(field);
        if open {
            let pick = std::rc::Rc::new(pick);
            let (list, rows) = select_list(id, &labels, selected, width, p);
            let mut list = list.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
            for (row, (value, _)) in rows.into_iter().zip(options.iter().cloned()) {
                let pick = pick.clone();
                list = list.child(row.on_click(cx.listener(move |this, _, window, cx| {
                    this.open_select = None;
                    pick(this, value.clone(), window, cx);
                })));
            }
            wrap = wrap.child(div().absolute().top(px(32.)).left_0().child(gpui_kit::deferred(gpui_kit::anchored().child(list)).with_priority(2)));
        }
        wrap.into_any_element()
    }

    // ------------------------------------------------------------ çizim: kanban

    fn card(&self, task: &Task, status: &str, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let owner = self.member(task.assignee_id.as_deref());
        let id = task.id.clone();
        let drag = DraggedTask { id: task.id.clone(), title: task.title.clone(), number: task.number, priority: task.priority.clone() };
        let dragging = self.dragging.as_deref() == Some(task.id.as_str());
        let group = SharedString::from(format!("card-{}", task.id));
        let (outer, strip, inner) = left_toned(priority_edge(&task.priority, p), p.border, radius::MD);
        let (bs, hover_bg) = (p.border_strong, opaque(p.bg_hover, p.bg_3));
        let md = p.shadow_md();
        let late = is_late(task);
        let dot = priority_dot(&task.priority, p);
        let (menu_task, st, tid) = (task.id.clone(), status.to_string(), task.id.clone());
        let card = outer
            .id(SharedString::from(format!("card-{}", task.id)))
            .group(group.clone())
            .flex_none()
            .cursor_pointer()
            // `.board-card:hover:not(.dragging) { transform: translateY(-2px) }` (140ms ease-out geçişli).
            .when(!dragging, |s| s.hover(move |s| s.translate(0., -2.).bg(bs).shadow(md.clone())))
            // `.board-card.dragging { opacity: .55; transform: rotate(2.2deg) scale(1.02) }`. Döndürme
            // BİLEREK yok: yamalı çizicide dörtgenler eksene hizalı kalır, yalnız yazı/ikon dönerdi
            // (TOKENS.md §6 "Kalan" 1) — yarım döndürme Electron'dan daha çok sapar.
            .when(dragging, |s| s.opacity(0.55).shadow(p.shadow_lg()).scale(1.02))
            .child(if dragging { strip } else { strip.group_hover(group.clone(), move |s| s.bg(bs)) })
            .child(
                inner
                    .bg(p.bg_3)
                    .when(!dragging, |s| s.group_hover(group.clone(), move |s| s.bg(hover_bg)))
                    .px(px(10.))
                    .pt(px(9.))
                    .pb(px(8.))
                    .flex()
                    .flex_col()
                    .gap(px(7.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .child(mono(format!("#{}", task.number), 10., p.text_3))
                            .children(kind_badge(task.kind(), p))
                            .when_some(task.points, |s, pts| s.child(div().flex_none().px(px(6.)).py(px(1.)).rounded(radius::PILL).bg(p.bg_hover).text_color(p.text_2).text_size(fs(9.5)).font_weight(gpui_kit::FontWeight::BOLD).child(format!("{pts}p"))))
                            .child(div().flex_1())
                            .child(div().flex_none().size(px(7.)).rounded_full().bg(dot).shadow(shadows(&[Shadow::new(0., 0., 6., 0., dot)]))),
                    )
                    .child(div().text_size(fs(12.)).line_height(gpui_kit::relative(1.45)).text_color(p.text).child(task.title.clone()))
                    .children(label_chips(task, 3, false, p))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(7.))
                            .child(board_avatar(owner, 24., p))
                            .when(task.description.as_deref().is_some_and(|d| !d.is_empty()), |s| s.child(div().text_size(fs(11.)).text_color(p.text_3).child("≡")))
                            .when_some(task.due_at.clone(), |s, d| s.child(div().ml_auto().text_size(fs(10.)).text_color(if late { p.danger_text } else { p.text_3 }).child(day_text(&d)))),
                    ),
            )
            .on_click(cx.listener(move |this, _, window, cx| this.select(id.clone(), window, cx)))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, ev: &gpui_kit::MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.close_overlays();
                    this.menu_epoch += 1;
                    this.task_menu = Some((menu_task.clone(), ev.position));
                    cx.notify();
                }),
            )
            .on_drag(drag, |d: &DraggedTask, _, _, cx| cx.new(|_| d.clone()))
            .on_drag_move::<DraggedTask>(cx.listener(move |this, ev: &gpui_kit::DragMoveEvent<DraggedTask>, _, cx| {
                let dragged = ev.drag(cx).id.clone();
                if this.dragging.as_deref() != Some(dragged.as_str()) {
                    this.dragging = Some(dragged);
                    cx.notify();
                }
                if !ev.bounds.contains(&ev.event.position) {
                    return;
                }
                // Fare kartın üst yarısındaysa önüne, alt yarısındaysa arkasına (BoardPage.tsx `aim`).
                let upper = ev.event.position.y < ev.bounds.origin.y + ev.bounds.size.height / 2.;
                let next = if upper {
                    Some(tid.clone())
                } else {
                    let visible: Vec<Task> = this.visible(cx).into_iter().cloned().collect();
                    let refs: Vec<&Task> = visible.iter().collect();
                    let col = Self::column(&refs, &st);
                    col.iter().position(|t| t.id == tid).and_then(|i| col.get(i + 1)).map(|t| t.id.clone())
                };
                let slot = Some((st.clone(), next));
                if this.slot != slot {
                    this.slot = slot;
                    cx.notify();
                }
            }));
        match &self.flash {
            Some((fid, epoch)) if *fid == task.id => {
                let (ring, glow_c) = (p.accent_hover, p.accent_glow);
                card.with_animation(("card-flash", *epoch), Animation::new(Duration::from_millis(900)), move |el, t| {
                    // 0 → %25 tepe → 1: halka + parıltı söner.
                    let k = if t < 0.25 { t / 0.25 } else { (1. - t) / 0.75 };
                    let mut g = glow_c;
                    g.color.a *= k;
                    let mut r = ring;
                    r.a *= k;
                    el.shadow(shadows(&[Shadow::new(0., 0., 0., 2., r), g]))
                })
                .into_any_element()
            }
            // `board-card-in var(--m-base) var(--ease-pop)`: translateY(6px) scale(.98) → none, opaklık 0 → 1.
            _ => card
                .with_animation(("board-card", task.number as usize), Animation::new(p2_ui::motion::M_BASE), |el, t| {
                    let k = p2_ui::motion::EASE_POP.ease(t);
                    el.opacity(k.clamp(0., 1.)).translate(0., 6. * (1. - k)).scale(0.98 + 0.02 * k)
                })
                .into_any_element(),
        }
    }

    fn drop_slot(p: &Palette) -> AnyElement {
        div().flex_none().h(px(54.)).border(px(1.5)).border_dashed().border_color(p.accent_hover).rounded(radius::MD).bg(p.accent_soft).into_any_element()
    }

    fn columns(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let tasks = self.visible(cx);
        let mut row = div().id("board-columns").flex_1().min_h_0().flex().gap(px(12.)).pt(px(14.)).px(px(22.)).pb(px(18.)).overflow_x_scroll();
        for (ix, status) in BOARD_COLUMNS.into_iter().enumerate() {
            let cards = Self::column(&tasks, status);
            let points: i64 = cards.iter().filter_map(|t| t.points).sum();
            let st = status.to_string();
            let over = self.dragging.is_some() && self.slot.as_ref().is_some_and(|s| s.0 == status);
            let (edge, tone) = if over { (p.border_accent, p.border_accent) } else { (p.border, column_tone(status, p)) };
            let (outer, strip, inner) = top_toned(tone, edge, radius::LG);
            let fill = if over { opaque(p.accent_soft, p.bg_0) } else { opaque(p.bg_0, p.bg_0) };
            let mut list = div().id(SharedString::from(format!("col-list-{status}"))).flex_1().min_h_0().overflow_y_scroll().p(px(10.)).flex().flex_col().gap(px(8.));
            let slot_here = |before: Option<&str>| self.dragging.is_some() && self.slot.as_ref().is_some_and(|s| s.0 == status && s.1.as_deref() == before);
            for t in &cards {
                if slot_here(Some(t.id.as_str())) {
                    list = list.child(Self::drop_slot(p));
                }
                list = list.child(self.card(t, status, p, cx));
            }
            if slot_here(None) {
                list = list.child(Self::drop_slot(p));
            }
            if cards.is_empty() && !slot_here(None) {
                list = list.child(div().my(px(9.35)).py(px(14.)).px(px(8.)).flex().justify_center().text_size(fs(11.)).text_color(p.text_3).child(if self.can_create { "Kart yok — buraya sürükle" } else { "Kart yok" }));
            }
            let (hsoft, strong) = (p.accent_soft, p.text_strong);
            let st2 = st.clone();
            row = row.child(
                outer
                    .id(SharedString::from(format!("col-{status}")))
                    .flex_1()
                    .min_w(px(250.))
                    .h_full()
                    .flex()
                    .flex_col()
                    .when(over, |s| s.shadow(shadows(&[p.accent_glow])))
                    .child(strip)
                    .child(inner.map(|s| if over { s.bg(fill) } else { s.bg(opaque_gradient(&p.glass, p.bg_0)).shadow(shadows(&[p.inset_top])) }).child(
                        div()
                            .flex_none()
                            .py(px(10.))
                            .px(px(12.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .border_b_1()
                            .border_color(p.border_subtle)
                            .child(div().flex_none().text_size(fs(12.)).font_weight(gpui_kit::FontWeight::BOLD).text_color(p.text_strong).child(column_label(status)))
                            .child(div().flex_1().min_w_0().text_size(fs(10.)).text_color(p.text_3).child(if points > 0 { format!("{} · {points} puan", cards.len()) } else { cards.len().to_string() }))
                            .when(self.can_create, |s| {
                                s.child(
                                    div()
                                        .id(SharedString::from(format!("add-{status}")))
                                        .flex_none()
                                        .size(px(24.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(7.))
                                        .text_size(fs(13.))
                                        .line_height(fs(13.))
                                        .text_color(p.text_3)
                                        .cursor_pointer()
                                        .hover(move |s| s.bg(hsoft).text_color(strong))
                                        .child("＋")
                                        .tooltip(|window, cx| p2_ui::controls::tooltip("Bu sütuna görev ekle", window, cx))
                                        .on_click(cx.listener(move |this, _, window, cx| this.open_editor(None, status, window, cx))),
                                )
                            }),
                    ).child(list))
                    .on_drag_move::<DraggedTask>(cx.listener(move |this, ev: &gpui_kit::DragMoveEvent<DraggedTask>, _, cx| {
                        let inside = ev.bounds.contains(&ev.event.position);
                        if inside {
                            let dragged = ev.drag(cx).id.clone();
                            if this.dragging.as_deref() != Some(dragged.as_str()) {
                                this.dragging = Some(dragged);
                            }
                            // Sütunun boş yerine: sona (kart dinleyicisi önünü/arkasını kendisi seçer).
                            if this.slot.as_ref().is_none_or(|s| s.0 != st) {
                                this.slot = Some((st.clone(), None));
                                cx.notify();
                            }
                        } else if this.slot.as_ref().is_some_and(|s| s.0 == st) {
                            // Bu sütundan çıkıldı (başka sütun kendi yuvasını yazar).
                        }
                    }))
                    .on_drop(cx.listener(move |this, d: &DraggedTask, window, cx| {
                        let target = this.slot.take().filter(|s| s.0 == st2).unwrap_or((st2.clone(), None));
                        this.dragging = None;
                        this.drop_task(d.id.clone(), target.0, target.1, window, cx);
                    }))
                    .p2_enter(("board-column", ix), Entrance::Rise, Duration::from_millis(40 * ix as u64)),
            );
        }
        row.into_any_element()
    }

    fn list_view(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let tasks = self.visible(cx);
        let mut out = div().id("board-list").flex_1().min_h_0().overflow_y_scroll().pt(px(14.)).px(px(22.)).pb(px(18.)).flex().flex_col();
        for status in BOARD_COLUMNS {
            let cards = Self::column(&tasks, status);
            if cards.is_empty() {
                continue;
            }
            let points: i64 = cards.iter().filter_map(|t| t.points).sum();
            let (outer, strip, inner) = left_toned(column_tone(status, p), p.border, radius::LG);
            let (hbg, strong) = (p.bg_hover, p.text_strong);
            let mut rows = div().flex().flex_col();
            for (i, t) in cards.iter().enumerate() {
                let (id, mid) = (t.id.clone(), t.id.clone());
                let picked = self.selected.as_deref() == Some(t.id.as_str());
                let late = is_late(t);
                rows = rows.child(
                    div()
                        .id(SharedString::from(format!("row-{}", t.id)))
                        .py(px(8.))
                        .px(px(12.))
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .when(i > 0, |s| s.border_t_1().border_color(p.border_subtle))
                        .cursor_pointer()
                        .map(|s| if picked { s.bg(p.bg_selected) } else { s.hover(move |s| s.bg(hbg)) })
                        .child(div().w(px(46.)).flex_none().child(mono(format!("#{}", t.number), 10., p.text_3)))
                        .child(div().flex_1().min_w_0().truncate().text_size(fs(12.)).text_color(p.text).child(t.title.clone()))
                        .child(div().w(px(120.)).flex_none().truncate().text_size(fs(10.5)).text_color(p.text_3).child(self.sprint_name(t.sprint_id.as_deref())))
                        .child(div().w(px(74.)).flex_none().flex().child(priority_chip(&t.priority, p)))
                        .child(div().w(px(44.)).flex_none().truncate().text_size(fs(10.5)).text_color(p.text_3).child(t.points.map(|x| format!("{x}p")).unwrap_or_else(|| "—".into())))
                        .child(div().w(px(62.)).flex_none().truncate().text_size(fs(10.5)).text_color(if late { p.danger_text } else { p.text_3 }).child(t.due_at.as_deref().map(day_text).unwrap_or_else(|| "—".into())))
                        .child(div().w(px(24.)).flex_none().child(board_avatar(self.member(t.assignee_id.as_deref()), 22., p)))
                        .on_click(cx.listener(move |this, _, window, cx| this.select(id.clone(), window, cx)))
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, ev: &gpui_kit::MouseDownEvent, _, cx| {
                                cx.stop_propagation();
                                this.close_overlays();
                                this.menu_epoch += 1;
                                this.task_menu = Some((mid.clone(), ev.position));
                                cx.notify();
                            }),
                        ),
                );
            }
            out = out.child(
                outer
                    .flex_none()
                    .mb(px(14.))
                    .child(strip)
                    .child(inner.bg(opaque_gradient(&p.glass, p.bg_0)).child(
                        div()
                            .py(px(9.))
                            .px(px(12.))
                            .flex()
                            .items_center()
                            .gap(px(9.))
                            .border_b_1()
                            .border_color(p.border_subtle)
                            .child(div().text_size(fs(12.)).font_weight(gpui_kit::FontWeight::BOLD).text_color(p.text_strong).child(column_label(status)))
                            .child(div().text_size(fs(10.5)).text_color(p.text_3).child(if points > 0 { format!("{} · {points} puan", cards.len()) } else { cards.len().to_string() }))
                            .when(self.can_create, |s| {
                                s.child(
                                    div()
                                        .id(SharedString::from(format!("list-add-{status}")))
                                        .ml_auto()
                                        .h(px(24.))
                                        .px(px(9.))
                                        .flex()
                                        .items_center()
                                        .border_1()
                                        .border_color(p.border)
                                        .rounded(px(7.))
                                        .bg(p.bg_3)
                                        .text_size(fs(10.5))
                                        .css_weight(650.)
                                        .text_color(p.text)
                                        .cursor_pointer()
                                        .hover(move |s| s.bg(hbg).text_color(strong))
                                        .child("＋ Görev")
                                        .on_click(cx.listener(move |this, _, window, cx| this.open_editor(None, status, window, cx))),
                                )
                            }),
                    )
                    .child(rows))
                    .p2_enter(("board-list-group", status.len()), Entrance::Rise, Duration::ZERO),
            );
        }
        if tasks.is_empty() {
            out = out.child(div().py(px(14.)).px(px(8.)).flex().justify_center().text_size(fs(11.)).text_color(p.text_3).child("Bu görünümde görev yok."));
        }
        out.into_any_element()
    }

    // ------------------------------------------------------------ çizim: menüler ve pencereler

    /// Pencere koordinatlı katman: dışına basınca kapanır.
    fn layer(id: &'static str, child: AnyElement, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let size = window.viewport_size();
        gpui_kit::deferred(
            gpui_kit::anchored().position_mode(gpui_kit::AnchoredPositionMode::Window).position(point(px(0.), px(0.))).child(
                div()
                    .id(id)
                    .relative()
                    .w(size.width)
                    .h(size.height)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.close_overlays();
                            cx.notify();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this, _, _, cx| {
                            this.close_overlays();
                            cx.notify();
                        }),
                    )
                    .child(child),
            ),
        )
        .with_priority(5)
        .into_any_element()
    }

    fn board_menu_view(&self, p: &Palette, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (board, at, anchored) = self.board_menu.clone()?;
        let single = self.boards.len() < 2;
        let h = 6. * 2. + 2. + 34. * 3. + 11.;
        let (x, y) = if anchored {
            // ⋯ düğmesinin altına, sağ kenara hizalı (anchorFrom(..., true)).
            let vw = f32::from(window.viewport_size().width);
            place_menu(vw - 22., 38. + 10. + 3. + 30. + 4., 38. + 10. + 3., true, 230., h, window)
        } else {
            place_menu(f32::from(at.x), f32::from(at.y), f32::from(at.y), false, 230., h, window)
        };
        let (b1, b2, b3) = (board.clone(), board.clone(), board);
        let menu = menu_panel("board-context-menu", 230., 6., p)
            .child(menu_item("bm-open", "↗", "Panoyu aç", Tone::Normal, None, 34., p).on_click(cx.listener(move |this, _, window, cx| {
                this.board_menu = None;
                if this.board_id.as_deref() != Some(b1.id.as_str()) {
                    this.open_board(b1.id.clone(), window, cx);
                }
                cx.notify();
            })))
            .child(menu_item("bm-rename", "✎", "Panoyu yeniden adlandır", Tone::Normal, None, 34., p).on_click(cx.listener(move |this, _, window, cx| {
                this.board_menu = None;
                this.open_name_dialog(Some(b2.clone()), window, cx);
            })))
            .child(menu_hr(p))
            .child(menu_item("bm-delete", "×", if single { "Son pano silinemez" } else { "Panoyu sil" }, if single { Tone::Disabled } else { Tone::Danger }, None, 34., p).when(!single, |s| {
                s.on_click(cx.listener(move |this, _, _, cx| {
                    this.board_menu = None;
                    this.confirm = Some(Confirm::Board(b3.clone()));
                    cx.notify();
                }))
            }))
            .p2_enter(("board-menu", self.menu_epoch), Entrance::ScaleIn, Duration::ZERO);
        Some(Self::layer("board-menu-layer", div().absolute().left(px(x)).top(px(y)).child(menu).into_any_element(), window, cx))
    }

    fn task_menu_view(&self, p: &Palette, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (id, at) = self.task_menu.clone()?;
        let t = self.content.as_ref()?.tasks.iter().find(|t| t.id == id)?.clone();
        let can_edit = self.can_edit(&t);
        let sprints: Vec<p2_client_core::Sprint> = self.content.as_ref().map(|c| c.sprints.iter().filter(|s| s.status != "DONE" && Some(&s.id) != t.sprint_id.as_ref()).cloned().collect()).unwrap_or_default();
        let label_h = 19.;
        let h = 6. * 2. + 2. + 34. + if can_edit { 34. + 11. + label_h + 34. * 4. + 11. + label_h + 34. * (1 + sprints.len()) as f32 + 11. + 34. } else { 0. };
        let (x, y) = place_menu(f32::from(at.x), f32::from(at.y), f32::from(at.y), false, 245., h, window);
        let label = |text: &str| div().mt(px(6.)).mx(px(9.)).mb(px(3.)).text_size(font_size::XXS).css_weight(800.).tracking(0.12).text_color(p.text_accent).child(tr_upper(text));
        let (open_id, edit_t) = (t.id.clone(), t.clone());
        let mut menu = menu_panel("board-task-menu", 245., 6., p).child(menu_item("tm-open", "↗", "Görevi aç", Tone::Normal, None, 34., p).on_click(cx.listener(move |this, _, window, cx| {
            this.task_menu = None;
            this.select(open_id.clone(), window, cx);
        })));
        if can_edit {
            menu = menu
                .child(menu_item("tm-edit", "✎", "Düzenle", Tone::Normal, None, 34., p).on_click(cx.listener(move |this, _, window, cx| {
                    this.task_menu = None;
                    this.open_editor(Some(edit_t.clone()), "TODO", window, cx);
                })))
                .child(menu_hr(p))
                .child(label("Duruma taşı"));
            for status in BOARD_COLUMNS {
                let here = t.column() == status;
                let (tid, number) = (t.id.clone(), t.number);
                menu = menu.child(menu_item(SharedString::from(format!("tm-st-{status}")), "→", column_label(status), if here { Tone::Disabled } else { Tone::Normal }, None, 34., p).when(!here, |s| {
                    s.on_click(cx.listener(move |this, _, window, cx| {
                        this.task_menu = None;
                        this.drop_task(tid.clone(), status.to_string(), None, window, cx);
                        Self::toast(format!("#{number} “{}” sütununa taşındı.", column_label(status)));
                    }))
                }));
            }
            menu = menu.child(menu_hr(p)).child(label("Sprint’e taşı"));
            let (tid, number, in_sprint) = (t.id.clone(), t.number, t.sprint_id.is_some());
            menu = menu.child(menu_item("tm-backlog", "↩", "Backlog", if in_sprint { Tone::Normal } else { Tone::Disabled }, None, 34., p).when(in_sprint, |s| {
                s.on_click(cx.listener(move |this, _, window, cx| {
                    this.task_menu = None;
                    this.update_task(tid.clone(), json!({ "sprintId": null }), Some(format!("#{number} “Backlog” alanına taşındı.")), window, cx);
                }))
            }));
            for s in sprints {
                let (tid, sid, name) = (t.id.clone(), s.id.clone(), s.name.clone());
                let trail = (s.status == "ACTIVE").then(|| div().flex_none().text_size(fs(8.)).css_weight(600.).text_color(p.text_4).child("aktif").into_any_element());
                menu = menu.child(menu_item(SharedString::from(format!("tm-sp-{}", s.id)), "→", s.name.clone(), Tone::Normal, trail, 34., p).on_click(cx.listener(move |this, _, window, cx| {
                    this.task_menu = None;
                    this.update_task(tid.clone(), json!({ "sprintId": sid }), Some(format!("#{number} “{name}” alanına taşındı.")), window, cx);
                })));
            }
            let del = t.clone();
            menu = menu.child(menu_hr(p)).child(menu_item("tm-delete", "×", "Görevi sil", Tone::Danger, None, 34., p).on_click(cx.listener(move |this, _, _, cx| {
                this.task_menu = None;
                this.confirm = Some(Confirm::Task(del.clone()));
                cx.notify();
            })));
        }
        let menu = menu.p2_enter(("task-menu", self.menu_epoch), Entrance::ScaleIn, Duration::ZERO);
        Some(Self::layer("task-menu-layer", div().absolute().left(px(x)).top(px(y)).child(menu).into_any_element(), window, cx))
    }

    /// `.share-layer` + `.share-modal.board-dialog` (görev, ad ve sprint pencereleri ortak kalıp).
    pub(super) fn share_modal(id: &'static str, width: f32, title: impl Into<SharedString>, hint: Option<SharedString>, p: &Palette, window: &Window) -> (gpui_kit::Stateful<gpui_kit::Div>, gpui_kit::Div) {
        let vw = f32::from(window.viewport_size().width);
        let vh = f32::from(window.viewport_size().height);
        let layer = div().id(id).absolute().inset_0().flex().items_center().justify_center().px(px(24.)).pb(px(24.)).bg(p.overlay).backdrop_blur(p2_ui::fx::BLUR_SM).occlude();
        let card = div()
            .relative()
            .overflow_hidden()
            .w(px(width.min(vw - 48.)))
            .max_h(px(vh - 90.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .pt(px(16.))
            .px(px(18.))
            .pb(px(14.))
            .border_1()
            .border_color(p.border_strong)
            .rounded(radius::XL)
            .bg(p.glass_strong.background())
            .shadow(p.shadow_lg())
            .text_color(p.text)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(10.))
                    .child(div().flex().flex_col().child(div().text_size(fs(14.)).font_weight(gpui_kit::FontWeight::BOLD).text_color(p.text_strong).child(title.into())).when_some(hint, |s, h| s.child(div().mt(px(2.)).text_size(fs(10.5)).text_color(p.text_3).child(h)))),
            );
        (layer, card)
    }

    /// `.share-modal > header button` (30 × 30, ×).
    pub(super) fn close_x(id: &'static str, p: &Palette) -> gpui_kit::Stateful<gpui_kit::Div> {
        let (soft, strong) = (p.accent_soft, p.text_strong);
        div().id(id).flex_none().size(px(30.)).flex().items_center().justify_center().rounded(radius::XS).text_size(fs(19.)).text_color(p.text_3).cursor_pointer().hover(move |s| s.bg(soft).text_color(strong)).child("×")
    }

    /// `.board-field`: üstte etiket (`600 fs-sm` text-3), 4px aralık.
    pub(super) fn field(label: &str, control: impl IntoElement, p: &Palette) -> gpui_kit::Div {
        div().flex().flex_col().gap(px(4.)).min_w_0().child(div().text_size(font_size::SM).css_weight(600.).text_color(p.text_3).child(label.to_string())).child(control)
    }

    /// `.board-field input`: p 8 10, r 8, bg-3, `12px` 1.5; odakta `border-accent` + halka.
    pub(super) fn field_input(state: &Entity<InputState>, p: &Palette, window: &Window, cx: &gpui_kit::App) -> AnyElement {
        let focused = gpui_kit::Focusable::focus_handle(state.read(cx), cx).is_focused(window);
        div()
            .h(px(33.))
            .px(px(10.))
            .flex()
            .items_center()
            .border_1()
            .rounded(radius::SM)
            .bg(p.bg_3)
            .map(|s| if focused { s.border_color(p.border_accent).shadow(shadows(&[p.ring])) } else { s.border_color(p.border) })
            .child(Input::new(state).appearance(false).bordered(false).focus_bordered(false).text_size(fs(12.)).text_color(p.text).px_0().h(p2_ui::text::scaled(px(18.))))
            .into_any_element()
    }

    fn name_dialog_view(&self, p: &Palette, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let d = self.name_dialog.as_ref()?;
        let creating = d.board.is_none();
        let (layer, card) = Self::share_modal(
            "name-dialog",
            420.,
            if creating { "Yeni pano" } else { "Panoyu yeniden adlandır" },
            creating.then(|| SharedString::from("Her takım kendi panosunu kullanabilir: “Oyun”, “Sanat”, “Backend” gibi.")),
            p,
            window,
        );
        let ok = value(&d.input, cx).trim().chars().count() >= 2;
        let close = |this: &mut Self, _: &gpui_kit::ClickEvent, _: &mut Window, cx: &mut Context<Self>| {
            this.name_dialog = None;
            cx.notify();
        };
        let card = card
            .child(div().absolute().top(px(16.)).right(px(18.)).child(Self::close_x("nd-x", p).on_click(cx.listener(close))))
            .child(Self::field("Ad", Self::field_input(&d.input, p, window, cx), p))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(board_button("nd-cancel", "Vazgeç", Btn::Plain, false, p).on_click(cx.listener(close)))
                    .child(board_button("nd-save", if creating { "Pano oluştur" } else { "Kaydet" }, Btn::Primary, self.busy || !ok, p).when(ok, |s| s.on_click(cx.listener(|this, _, window, cx| this.save_name(window, cx))))),
            )
            .p2_enter("name-dialog-card", Entrance::PopElastic, Duration::ZERO);
        Some(
            layer
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.name_dialog = None;
                        cx.notify();
                    }),
                )
                .child(card)
                .into_any_element(),
        )
    }

    /// ConfirmDialog.tsx (Modal sm, `.confirm-modal`).
    fn confirm_view(&self, p: &Palette, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let c = self.confirm.clone()?;
        let (title, body, label) = match &c {
            Confirm::Board(b) => ("Pano silinsin mi?".to_string(), format!("“{}” panosu, içindeki sprint ve görevlerle birlikte silinecek. Bu işlem geri alınamaz.", b.name), "Panoyu sil"),
            Confirm::Task(t) => ("Görev silinsin mi?".to_string(), if self.selected.as_deref() == Some(t.id.as_str()) { format!("“{}” panodan kaldırılacak.", t.title) } else { format!("#{} “{}” panodan kaldırılacak.", t.number, t.title) }, "Görevi sil"),
            Confirm::Sprint(s) => ("Sprint silinsin mi?".to_string(), format!("“{}” silinecek. Görevleri silinmez, backlog’a döner.", s.name), "Sprint’i sil"),
        };
        use p2_ui::controls::{BtnSize, BtnVariant, ModalSize, button, modal, modal_body, modal_foot, modal_head};
        let dismiss = |this: &mut Self, _: &gpui_kit::ClickEvent, _: &mut Window, cx: &mut Context<Self>| {
            this.confirm = None;
            cx.notify();
        };
        let close = button("bc-x", BtnVariant::Ghost, BtnSize::Sm, true, false, p).child("×").on_click(cx.listener(dismiss));
        let yes = button("bc-yes", BtnVariant::Danger, BtnSize::Md, false, false, p).child(label).on_click(cx.listener(move |this, _, window, cx| {
            this.confirm = None;
            match c.clone() {
                Confirm::Board(b) => this.delete_board(b, window, cx),
                Confirm::Task(t) => this.delete_task(t.id, window, cx),
                Confirm::Sprint(s) => this.delete_sprint(s, window, cx),
            }
            cx.notify();
        }));
        let card = modal(ModalSize::Sm, window, p)
            .child(modal_head(title, Some(body.into()), Some(close.into_any_element()), p))
            .child(modal_body())
            .child(modal_foot(p).child(button("bc-no", BtnVariant::Secondary, BtnSize::Md, false, false, p).child("Vazgeç").on_click(cx.listener(dismiss))).child(yes))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .p2_enter("board-confirm-card", Entrance::PopElastic, Duration::ZERO);
        Some(
            div()
                .id("board-confirm")
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                
                .px(px(24.))
                .pb(px(24.))
                .bg(p.overlay).backdrop_blur(p2_ui::fx::BLUR_SM)
                .occlude()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.confirm = None;
                        cx.notify();
                    }),
                )
                .child(card)
                .into_any_element(),
        )
    }
}

impl Render for BoardPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        // Sürükleme bitti (pencere dışına bırakma dahil): yuva ve soluk kart kalmasın.
        if !cx.has_active_drag() && (self.dragging.is_some() || self.slot.is_some()) {
            self.dragging = None;
            self.slot = None;
        }
        let root = div()
            .id("board-page")
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(p.bg_0)
            .font_family(p2_ui::tokens::FONT)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.close_overlays() {
                        cx.notify();
                    }
                }),
            )
            .p2_enter("board-page", Entrance::Fade, Duration::ZERO);
        if self.loading && self.content.is_none() {
            return root.child(div().py(px(14.)).px(px(8.)).flex().justify_center().text_size(fs(11.)).text_color(p.text_3).child("Pano yükleniyor…")).into_any_element();
        }
        if self.board_id.is_none() {
            return root.child(div().py(px(14.)).px(px(8.)).flex().justify_center().text_size(fs(11.)).text_color(p.text_3).child(self.error.clone().unwrap_or_else(|| "Bu projede henüz pano yok.".into()))).into_any_element();
        }
        let no_sprints = self.content.as_ref().is_some_and(|c| c.sprints.is_empty());
        let notes = div()
            .flex_none()
            .flex()
            .flex_col()
            .when(no_sprints && self.can_manage, |s| {
                s.child(div().py(px(9.)).px(px(22.)).border_b_1().border_color(p.border_subtle).text_size(fs(11.)).text_color(p.text_3).child("Henüz sprint yok. “Sprint’ler” bölümünden ilk sprint’i açabilirsin; görevler o zamana kadar backlog’da birikir."))
            });
        let body = if self.view == View::Board { self.columns(&p, cx) } else { self.list_view(&p, cx) };
        let tabs = (!self.boards.is_empty()).then(|| self.tabs(&p, cx));
        let head = self.head(&p, window, cx);
        let selected = self.selected.as_ref().and_then(|id| self.content.as_ref()?.tasks.iter().find(|t| &t.id == id).cloned());
        let detail = selected.map(|t| self.task_panel(&t, &p, window, cx));
        let sprints = self.sprints_open.then(|| self.sprint_dialog(&p, window, cx)).map(|d| window_cover(d, window));
        let board_menu = self.board_menu_view(&p, window, cx);
        let task_menu = self.task_menu_view(&p, window, cx);
        let name = self.name_dialog_view(&p, window, cx).map(|d| window_cover(d, window));
        let confirm = self.confirm_view(&p, window, cx).map(|d| window_cover(d, window));
        root.children(tabs)
            .child(head)
            .child(notes)
            .child(body)
            .children(detail)
            .when_some(self.editor.as_ref().map(|e| e.0.clone()), |s, e| s.child(window_cover(e, window)))
            .children(sprints)
            .children(name)
            .children(confirm)
            .children(board_menu)
            .children(task_menu)
            .into_any_element()
    }
}

/// Electron'un pano `localStorage` anahtarlarının (`p2:board:<proje>`, `p2:board-view`) karşılığı:
/// profil klasöründe tek bir anahtar/değer JSON dosyası.
fn memory_path() -> std::path::PathBuf {
    p2_client_core::store::profile_dir().join("board-memory.json")
}

fn memory_get(key: &str) -> Option<String> {
    let map: serde_json::Map<String, Value> = std::fs::read(memory_path()).ok().and_then(|b| serde_json::from_slice(&b).ok())?;
    map.get(key).and_then(|v| v.as_str()).filter(|v| !v.is_empty()).map(str::to_string)
}

fn memory_set(key: &str, value: &str) {
    let path = memory_path();
    let mut map: serde_json::Map<String, Value> = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
    if map.get(key).and_then(|v| v.as_str()) == Some(value) {
        return;
    }
    map.insert(key.to_string(), Value::String(value.to_string()));
    if let Ok(bytes) = serde_json::to_vec(&map) {
        let _ = std::fs::write(path, bytes);
    }
}

#[allow(dead_code)]
fn _mono_font() -> &'static str {
    FONT_MONO
}
