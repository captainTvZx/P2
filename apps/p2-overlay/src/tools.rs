//! Overlay araç pencerelerinin içerikleri — Electron `src/overlay/tools/*.tsx` + CSS'leri.
//! Her araç `OverlayRoot::tool_body` içinde çizilir; `ghost` (panel kapalı, sabitli) salt-okunurdur.

use std::time::{Duration, Instant};

use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, BoxShadow, Context, Entity, FontWeight, Hsla, MouseButton, SharedString, Subscription, Window, div, point, px};
use p2_client_core::{ProjectNote, Task};
use crate::ui::avatar;
use p2_ui::controls::{BadgeTone, Presence, TabsStyle, badge, card, status_dot, tab, tabs};
use p2_ui::form::toggle;
use p2_ui::tokens::{FONT_MONO, Palette, font_size, radius};

use crate::icons::{self, ic};
use crate::ui::{Btn, Cmd, Loadable, OverlayRoot, State, UpperTr as _, btn, empty, input, keys_row, slider_track};
use crate::wm::{self, ToolId, WinState};

pub const STATUS: [(&str, &str); 5] = [("BACKLOG", "ovr.status.backlog"), ("TODO", "ovr.status.todo"), ("IN_PROGRESS", "ovr.status.inProgress"), ("REVIEW", "ovr.status.review"), ("DONE", "ovr.status.done")];
pub const LIST_ORDER: [&str; 5] = ["IN_PROGRESS", "REVIEW", "TODO", "BACKLOG", "DONE"];
pub const BOARD_COLUMNS: [&str; 4] = ["TODO", "IN_PROGRESS", "REVIEW", "DONE"];
pub const PRIORITY: [(&str, &str); 4] = [("LOW", "ovr.priority.low"), ("MEDIUM", "ovr.priority.medium"), ("HIGH", "ovr.priority.high"), ("URGENT", "ovr.priority.urgent")];
// İkinci/üçüncü alanlar i18n anahtarı (uzun ad, kısa rozet).
pub const KINDS: [(&str, &str, &str); 5] = [("TASK", "ovr.kind.task", "ovr.kind.taskShort"), ("BUG", "ovr.kind.bug", "ovr.kind.bugShort"), ("FEATURE", "ovr.kind.feature", "ovr.kind.featureShort"), ("IMPROVEMENT", "ovr.kind.improvement", "ovr.kind.improvementShort"), ("DOCS", "ovr.kind.docs", "ovr.kind.docsShort")];
const CORNERS: [(&str, &str); 4] = [("top-right", "ovr.corner.topRight"), ("top-left", "ovr.corner.topLeft"), ("bottom-right", "ovr.corner.bottomRight"), ("bottom-left", "ovr.corner.bottomLeft")];
const HUD_POSITIONS: [(&str, &str); 3] = [("top-left", "ovr.corner.topLeft"), ("top-center", "ovr.corner.topCenter"), ("top-right", "ovr.corner.topRight")];
const NOTE_AUTOSAVE: Duration = Duration::from_millis(800);
const NARROW: f32 = 520.;

pub fn status_label(s: &str) -> String {
    p2_i18n::t(STATUS.iter().find(|(k, _)| *k == s).map_or("ovr.status.todo", |(_, v)| *v))
}
/// (değer, i18n anahtarı) listesini (değer, çevrilmiş etiket) seçeneklerine çevirir.
fn opts(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter().map(|(a, b)| (a.to_string(), p2_i18n::t(b))).collect()
}

#[derive(Clone, Copy, PartialEq)]
pub enum TasksView {
    List,
    Board,
}

pub struct ToolsUi {
    pub tasks_view: TasksView,
    pub only_mine: bool,
    pub board: String,
    pub creating: bool,
    /// Düzenlenen görev (yeni görev formu düzenleme kipinde) ve silme onayı.
    pub editing: Option<String>,
    pub delete_confirm: bool,
    pub draft: (String, String, String),
    pub task_title: Entity<InputState>,
    pub task_desc: Entity<TextareaState>,
    pub task_labels: Entity<InputState>,
    pub note_sel: Option<String>,
    pub note_list: bool,
    pub note_title: Entity<InputState>,
    pub note_body: Entity<TextareaState>,
    pub note_bound: Option<String>,
    pub note_dirty: Option<Instant>,
    pub note_auto: bool,
    pub note_confirm: bool,
    pub team_open: Option<String>,
    pub invite: bool,
    pub url_input: Entity<InputState>,
    pub url: String,
    pub stop_confirm: bool,
    /// Sohbet yazma kutusu.
    pub chat_input: Entity<InputState>,
    /// Tarayıcı: son kare ve görünüm klavye odağında mı.
    pub browser_frame: Option<std::sync::Arc<gpui_kit::RenderImage>>,
    pub browser_focus: bool,
}

impl ToolsUi {
    pub fn new(window: &mut Window, cx: &mut Context<OverlayRoot>) -> (Self, Vec<Subscription>) {
        let task_title = input(window, cx, &p2_i18n::t("ovr.tasks.titlePlaceholder"));
        let task_labels = input(window, cx, &p2_i18n::t("ovr.tasks.labelsPlaceholder"));
        let task_desc = cx.new(|cx| TextareaState::new(window, cx).auto_grow(3, 8).placeholder(p2_i18n::t("ovr.tasks.descPlaceholder")));
        let note_title = input(window, cx, &p2_i18n::t("ovr.notes.titlePlaceholder"));
        let note_body = cx.new(|cx| TextareaState::new(window, cx).auto_grow(8, 400).placeholder(p2_i18n::t("ovr.notes.bodyPlaceholder")));
        let url_input = input(window, cx, &p2_i18n::t("ovr.browser.urlPlaceholder"));
        let chat_input = input(window, cx, &p2_i18n::t("ovr.chat.placeholder"));
        let subs = vec![
            cx.subscribe_in(&task_title, window, |this: &mut OverlayRoot, _, ev: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = ev {
                    this.submit_task(window, cx);
                }
            }),
            cx.subscribe_in(&note_title, window, |this: &mut OverlayRoot, _, ev: &InputEvent, _, _| {
                if matches!(ev, InputEvent::Change) && this.ui.note_bound.is_some() {
                    this.ui.note_dirty = Some(Instant::now());
                }
            }),
            cx.subscribe_in(&note_body, window, |this: &mut OverlayRoot, _, ev: &InputEvent, _, _| {
                if matches!(ev, InputEvent::Change) && this.ui.note_bound.is_some() {
                    this.ui.note_dirty = Some(Instant::now());
                }
            }),
            // Enter: seçili kanala gönder, kutu boşalır.
            cx.subscribe_in(&chat_input, window, |this: &mut OverlayRoot, state, ev: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = ev {
                    let text = state.read(cx).value().trim().to_string();
                    if let (false, Some(channel)) = (text.is_empty(), this.st.chat_sel.clone()) {
                        this.pending.push(Cmd::ChatSend { channel, text });
                        state.update(cx, |s, cx| s.set_value("", window, cx));
                        cx.notify();
                    }
                }
            }),
            cx.subscribe_in(&url_input, window, |this: &mut OverlayRoot, state, ev: &InputEvent, _, cx| {
                if let InputEvent::PressEnter { .. } = ev {
                    this.ui.url = normalize_url(&state.read(cx).value());
                    this.pending.push(Cmd::Browser(crate::browser::BrowserCmd::Navigate(this.ui.url.clone())));
                    cx.notify();
                }
            }),
        ];
        let ui = Self {
            tasks_view: TasksView::List,
            only_mine: false,
            board: "*".into(),
            creating: false,
            editing: None,
            delete_confirm: false,
            draft: ("TODO".into(), "MEDIUM".into(), "TASK".into()),
            task_title,
            task_desc,
            task_labels,
            note_sel: None,
            note_list: true,
            note_title,
            note_body,
            note_bound: None,
            note_dirty: None,
            note_auto: false,
            note_confirm: false,
            team_open: None,
            invite: false,
            url_input,
            url: "https://www.google.com".into(),
            stop_confirm: false,
            chat_input,
            browser_frame: None,
            browser_focus: false,
        };
        (ui, subs)
    }

    pub fn on_hidden(&mut self) {
        self.creating = false;
        self.team_open = None;
        self.invite = false;
        self.stop_confirm = false;
    }

    /// Esc: açık form/odaklı alan kapanır. İşlendiyse true.
    pub fn escape(&mut self, _st: &mut State, window: &mut Window, cx: &mut Context<OverlayRoot>) -> bool {
        if self.creating {
            self.creating = false;
            return true;
        }
        use gpui_kit::Focusable as _;
        let focused = [self.task_title.read(cx).focus_handle(cx), self.note_title.read(cx).focus_handle(cx), self.url_input.read(cx).focus_handle(cx), self.note_body.read(cx).focus_handle(cx)];
        if focused.iter().any(|f| f.is_focused(window)) {
            window.blur(cx);
            return true;
        }
        false
    }
}

fn normalize_url(raw: &str) -> String {
    let v = raw.trim();
    if v.is_empty() {
        return String::new();
    }
    if v.contains("://") {
        return v.to_string();
    }
    if v.contains('.') && !v.contains(' ') {
        return format!("https://{v}");
    }
    format!("https://www.google.com/search?q={}", v.replace(' ', "+"))
}

/// Not özeti zamanı (ProjectNotesPanel.tsx `noteTimeAgo`).
pub fn time_ago(iso: &str) -> String {
    let Some(secs) = parse_iso(iso) else { return String::new() };
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let diff = now - secs;
    if diff < 60 {
        p2_i18n::t("ovr.time.justNow")
    } else if diff < 3600 {
        p2_i18n::tv("ovr.time.minAgo", &[("n", &(diff / 60).max(1).to_string())])
    } else if diff < 86400 {
        p2_i18n::tv("ovr.time.hourAgo", &[("n", &(diff / 3600).to_string())])
    } else if diff < 172800 {
        p2_i18n::t("ovr.time.yesterday")
    } else {
        p2_i18n::tv("ovr.time.dayAgo", &[("n", &(diff / 86400).to_string())])
    }
}

fn parse_iso(s: &str) -> Option<i64> {
    // 2026-11-13T10:20:30.000Z
    let (d, t) = s.split_once('T')?;
    let mut dp = d.split('-').map(|x| x.parse::<i64>().ok());
    let (y, m, day) = (dp.next()??, dp.next()??, dp.next()??);
    let mut tp = t.trim_end_matches('Z').split(':');
    let (h, mi) = (tp.next()?.parse::<i64>().ok()?, tp.next()?.parse::<i64>().ok()?);
    let sec = tp.next().and_then(|x| x.split('.').next()).and_then(|x| x.parse::<i64>().ok()).unwrap_or(0);
    let (y2, m2) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let doy = (153 * m2 + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + h * 3600 + mi * 60 + sec)
}

fn note_title(n: &ProjectNote) -> String {
    let t = n.title.trim();
    if t.is_empty() { p2_i18n::t("ovr.notes.untitled") } else { t.to_string() }
}

fn status_color(s: &str, p: &Palette) -> Hsla {
    match s {
        "TODO" => p.slate_300,
        "IN_PROGRESS" => p.accent,
        "REVIEW" => p.warning,
        "DONE" => p.success,
        _ => p.text_4,
    }
}
fn priority_color(s: &str, p: &Palette) -> Hsla {
    match s {
        "URGENT" => p.danger,
        "HIGH" => p.warning,
        "MEDIUM" => p.accent,
        _ => p.text_4,
    }
}

/// `.p2-select` benzeri tetik (h 32, p 0 26 0 9, r 6, fs-sm) + sağda chevron; tıklayınca DOM içi liste.
fn select_box(key: String, label: String, h: f32, r: gpui_kit::Pixels, text: gpui_kit::Pixels, open: bool, p: &Palette) -> gpui_kit::Stateful<gpui_kit::Div> {
    div()
        .id(SharedString::from(format!("sel-{key}")))
        .relative()
        .h(px(h))
        .pl(px(9.))
        .pr(px(26.))
        .flex()
        .items_center()
        .rounded(r)
        .border_1()
        .border_color(if open { p.border_accent } else { p.border })
        .when(open, |d| d.shadow(p2_ui::tokens::shadows(&[p.ring])))
        .bg(p.bg_3)
        .text_size(text)
        .font_weight(FontWeight::MEDIUM)
        .text_color(p.text)
        .cursor_pointer()
        .child(div().truncate().child(label))
        .child(div().absolute().right(px(9.)).top_0().bottom_0().flex().items_center().child(ic(icons::CHEVRON, 1.6 * 2., 12., p.text_accent)))
}

impl OverlayRoot {
    /// Pencere başlığı (setTitle karşılığı).
    pub fn tool_title(&self, tool: ToolId) -> String {
        let st = &self.st;
        match tool {
            ToolId::NewTask if self.ui.editing.is_some() => p2_i18n::t("ovr.tasks.editTitle"),
            ToolId::Tasks => {
                let n = st.tasks.ready().map(|d| d.all().filter(|t| t.assignee_id.as_deref() == Some(st.user_id.as_str()) && t.status != "DONE").count()).unwrap_or(0);
                if n > 0 { p2_i18n::tv("ovr.tasks.titleCount", &[("n", &n.to_string())]) } else { p2_i18n::t("ovr.tasks.title") }
            }
            ToolId::Notes => match st.notes.ready() {
                Some(n) if !n.is_empty() => p2_i18n::tv("ovr.notes.titleCount", &[("n", &n.len().to_string())]),
                _ => p2_i18n::t("ovr.notes.title"),
            },
            ToolId::Team => match st.team.ready() {
                Some(n) if n.iter().any(|m| m.id != st.user_id) => p2_i18n::tv("ovr.team.titleCount", &[("n", &n.iter().filter(|m| m.id != st.user_id).count().to_string())]),
                _ => p2_i18n::t("ovr.team.title"),
            },
            ToolId::Voice if st.call.active && !st.call.title.is_empty() => st.call.title.clone(),
            _ => p2_i18n::t(wm::meta(tool).title),
        }
    }

    pub fn tool_body(&self, tool: ToolId, win: &WinState, ghost: bool, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let mini = win.pinned && win.minimized;
        match tool {
            ToolId::Tasks => self.tasks_body(win, ghost, mini, p, window, cx),
            ToolId::Notes => self.notes_body(win, ghost, mini, p, window, cx),
            ToolId::Perf => Some(self.perf_body(ghost, mini, p, cx)),
            ToolId::Voice => self.voice_body(ghost, mini, p, cx),
            ToolId::Team => Some(self.team_body(ghost, mini, p, cx)),
            ToolId::Summary => Some(self.summary_body(ghost, mini, p, cx)),
            ToolId::Capture => (!mini).then(|| self.capture_body(ghost, p, cx)),
            ToolId::Library => (!win.pinned).then(|| self.library_body(p, cx)),
            ToolId::Browser => (!mini).then(|| self.browser_body(ghost, p, window, cx)),
            ToolId::Chat => (!mini).then(|| self.chat_body(p, cx)),
            ToolId::NewTask => (!mini).then(|| self.new_task_body(p, cx)),
        }
        .filter(|_| !win.minimized || mini)
    }

    // ==================================================================== Görevler
    fn tasks_body(&self, win: &WinState, ghost: bool, mini: bool, p: &Palette, _window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let st = &self.st;
        let me = st.user_id.as_str();
        let instance = wm::instance_of(&win.id).map(String::from);
        let board_id = instance.clone().unwrap_or_else(|| self.ui.board.clone());
        let data = match &st.tasks {
            Loadable::Ready(d) => d,
            Loadable::Failed(e) => return Some(empty(Some(&p2_i18n::t("ovr.tasks.loadFailed")), Some(e), p).into_any_element()),
            _ => return Some(empty(None, Some(&p2_i18n::t("ovr.tasks.loading")), p).into_any_element()),
        };
        let mine: Vec<&Task> = data.all().filter(|t| t.assignee_id.as_deref() == Some(me) && t.status != "DONE").collect();
        if mini {
            let first = mine.first()?;
            return Some(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .max_w(px(320.))
                    .py(px(4.))
                    .px(px(10.))
                    .rounded(radius::PILL)
                    .bg(p.bg_2)
                    .text_size(font_size::SM)
                    .child(badge(mine.len().to_string(), BadgeTone::Soft, p))
                    .child(dot(priority_color(&first.priority, p), p.bg_2))
                    .child(div().truncate().font_weight(FontWeight::BOLD).text_color(p.text_strong).child(first.title.clone()))
                    .into_any_element(),
            );
        }
        let board = data.boards.iter().find(|b| b.id == board_id);
        let scope = data.board_tasks(&board_id);
        let only_mine = self.ui.only_mine && !ghost;
        let filtered: Vec<&Task> = scope.iter().copied().filter(|t| !only_mine || (t.assignee_id.as_deref() == Some(me) && t.status != "DONE")).collect();
        let mine_count = scope.iter().filter(|t| t.assignee_id.as_deref() == Some(me) && t.status != "DONE").count();
        let names: std::collections::HashMap<&str, &str> = data.boards.iter().map(|b| (b.id.as_str(), b.name.as_str())).collect();
        let show_board = self.ui.tasks_view == TasksView::Board && !ghost;

        let mut root = div().flex().flex_col().gap(px(if ghost { 6. } else { 8. })).h_full().min_h_0().pt(px(if ghost { 6. } else { 8. })).px(px(if ghost { 12. } else { 10. })).pb(px(if ghost { 12. } else { 10. }));
        if ghost {
            let open = filtered.iter().filter(|t| t.status != "DONE").count();
            let mine_n = filtered.iter().filter(|t| t.assignee_id.as_deref() == Some(me) && t.status != "DONE").count();
            root = root.child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(10.))
                    .py(px(2.))
                    .px(px(4.))
                    .border_b_1()
                    .border_color(p.border_subtle)
                    .child(div().text_size(font_size::LG).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(board.map_or(p2_i18n::t("ovr.tasks.title"), |b| b.name.clone())))
                    .child(div().text_size(font_size::XS).font_weight(FontWeight::SEMIBOLD).text_color(p.text_2).child(p2_i18n::tv("ovr.tasks.ghostSummary", &[("open", &open.to_string()), ("mine", &mine_n.to_string())]))),
            );
        } else {
            let mut head = div().flex().flex_wrap().items_center().gap_x(px(8.)).gap_y(px(6.)).flex_none().min_h(px(38.));
            if instance.is_none() {
                let mut options = vec![("*".to_string(), p2_i18n::t("ovr.tasks.allBoards"))];
                options.extend(data.boards.iter().map(|b| (b.id.clone(), b.name.clone())));
                let label = options.iter().find(|(v, _)| *v == board_id).map_or(p2_i18n::t("ovr.tasks.allBoards"), |(_, l)| l.clone());
                let open = self.select.as_ref().is_some_and(|s| s.key == "tasks:board");
                let sel = board_id.clone();
                head = head.child(
                    div().flex_1().min_w(px(120.)).max_w(px(220.)).child(
                        select_box("tasks:board".into(), label, 32., radius::XS, font_size::SM, open, p).child(self.mark("tasks:board")).on_click(cx.listener(move |this, _, _, cx| {
                            this.open_select("tasks:board", options.clone(), sel.clone());
                            cx.notify();
                        })),
                    ),
                );
            }
            let view = self.ui.tasks_view;
            let seg = |id: &'static str, body: &'static str, label: String, active: bool| {
                let color = if active { p.text_strong } else { p.text_3 };
                tab(id, "", active, TabsStyle::Segment, 0, p)
                    .relative()
                    .h(px(30.))
                    .px(px(10.))
                    .gap(px(5.))
                    .text_size(font_size::SM)
                    .child(ic(body, 2.0, 14., color))
                    .child(div().child(label))
                    .child(self.mark(format!("tasks:{id}")))
            };
            head = head
                .child(
                    tabs(TabsStyle::Segment, p)
                        .flex_none()
                        .child(seg("list", icons::LIST, p2_i18n::t("ovr.tasks.viewList"), view == TasksView::List).on_click(cx.listener(|this, _, _, cx| {
                            this.ui.tasks_view = TasksView::List;
                            cx.notify();
                        })))
                        .child(seg("board", icons::BOARD, p2_i18n::t("ovr.tasks.viewBoard"), view == TasksView::Board).on_click(cx.listener(|this, _, _, cx| {
                            this.ui.tasks_view = TasksView::Board;
                            cx.notify();
                        }))),
                )
                .child(self.filter_chip(mine_count, p, cx))
                .child(div().flex_1().min_w_0());
            if instance.is_none() {
                if let Some(b) = board {
                    let bid = b.id.clone();
                    head = head.child(btn("tasks-window", "", Btn::IconGhost, p).w(px(30.)).h(px(30.)).child(ic(icons::WINDOW, 1.8, 16., p.text_2)).on_click(cx.listener(move |this, _, _, cx| {
                        this.st.store.open_instance(ToolId::Tasks, &bid);
                        cx.notify();
                    })));
                }
            }
            let can = !data.boards.is_empty();
            head = head.child(
                btn("tasks-new", "", Btn::Primary, p)
                    .h(px(32.))
                    .pl(px(9.))
                    .pr(px(12.))
                    .gap(px(5.))
                    .shadow(vec![])
                    .when(!can, |d| d.opacity(0.5))
                    .child(ic(icons::PLUS, 2.2, 16., p.text_on_accent))
                    .child(p2_i18n::t("ovr.tasks.new"))
                    .child(self.mark("tasks:new"))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if can && this.ui.creating {
                            // Form zaten açık: öne gelir.
                            this.open_window("newtask");
                            this.focus_window("newtask");
                        } else if can {
                            this.ui.creating = true;
                            this.ui.editing = None;
                            this.ui.delete_confirm = false;
                            this.open_window("newtask");
                            this.focus_window("newtask");
                            this.ui.draft = ("TODO".into(), "MEDIUM".into(), "TASK".into());
                            this.ui.task_title.update(cx, |s, cx| {
                                s.set_value("", window, cx);
                                s.focus(window, cx);
                            });
                            this.ui.task_desc.update(cx, |s, cx| s.set_value("", window, cx));
                            this.ui.task_labels.update(cx, |s, cx| s.set_value("", window, cx));
                        }
                        cx.notify();
                    })),
            );
            root = root.child(head);
        }

        if !show_board {
            let mut body = div().flex_1().min_h_0().flex().flex_col().gap(px(if ghost { 2. } else { 4. }));
            if filtered.is_empty() {
                let (t, s) = if only_mine {
                    ("ovr.tasks.emptyMine", "ovr.tasks.emptyMineHint")
                } else if ghost {
                    ("ovr.tasks.empty", "ovr.tasks.emptyGhostHint")
                } else {
                    ("ovr.tasks.empty", "ovr.tasks.emptyHint")
                };
                body = body.child(empty(Some(&p2_i18n::t(t)), Some(&p2_i18n::t(s)), p));
            }
            for (gi, status) in LIST_ORDER.iter().enumerate() {
                let items: Vec<&&Task> = filtered.iter().filter(|t| t.status == *status).collect();
                if items.is_empty() {
                    continue;
                }
                let mut group = div().flex().flex_col().gap(px(if ghost { 3. } else { 4. })).when(gi > 0, |d| d.mt(px(if ghost { 8. } else { 6. })));
                group = group.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .pt(px(6.))
                        .px(px(4.))
                        .pb(px(4.))
                        .child(status_pill(status, 20., 9., font_size::XS, p))
                        .child(div().text_size(font_size::XS).font_weight(FontWeight::BOLD).text_color(p.text_3).child(items.len().to_string())),
                );
                for t in items {
                    let board_name = (board_id == "*").then(|| names.get(t.board_id.as_str()).copied().unwrap_or("")).filter(|n| !n.is_empty());
                    group = group.child(self.task_row(t, me, board_name, ghost, p, cx));
                }
                body = body.child(group);
            }
            root = root.child(body);
        } else {
            let mut cols = div().flex().gap(px(6.)).flex_1().min_h_0();
            for status in BOARD_COLUMNS {
                let items: Vec<&&Task> = filtered.iter().filter(|t| t.status == status).collect();
                let mut col = div()
                    .flex_1()
                    .min_w(px(132.))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .p(px(6.))
                    .rounded(radius::MD)
                    .bg(p.bg_2)
                    .border_1()
                    .border_color(p.border_subtle)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .pt(px(2.))
                            .px(px(4.))
                            .pb(px(4.))
                            .text_size(font_size::XXS)
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.text_3)
                            .child(div().size(px(6.)).rounded_full().bg(status_color(status, p)))
                            .child(status_label(status).pipe_upper())
                            .child(div().ml_auto().text_color(p.text_4).child(items.len().to_string())),
                    );
                let mut list = div().flex().flex_col().gap(px(4.)).min_h(px(48.)).flex_1();
                for t in &items {
                    list = list.child(self.task_card(t, me, p));
                }
                if items.is_empty() {
                    list = list.child(div().py(px(12.)).px(px(4.)).text_center().text_size(font_size::XS).text_color(p.text_4).child(p2_i18n::t("ovr.tasks.columnEmpty")));
                }
                col = col.child(list);
                cols = cols.child(col);
            }
            root = root.child(cols);
        }
        Some(root.into_any_element())
    }

    fn filter_chip(&self, count: usize, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let on = self.ui.only_mine;
        let (hb, hc) = (p.bg_hover, p.text_strong);
        div()
            .id("tasks-mine")
            .relative()
            .h(px(30.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(5.))
            .rounded(radius::PILL)
            .border_1()
            .border_color(if on { p.border_accent } else { p.border })
            .when(on, |d| d.bg(p.accent_soft))
            .text_color(if on { p.text_accent } else { p.text_2 })
            .text_size(font_size::XS)
            .font_weight(FontWeight::SEMIBOLD)
            .cursor_pointer()
            .when(!on, |d| d.hover(move |s| s.bg(hb).text_color(hc)))
            .child(p2_i18n::t("ovr.tasks.mine"))
            .when(count > 0, |d| d.child(badge(count.to_string(), BadgeTone::Soft, p).h(px(16.)).min_w(px(16.)).px(px(5.)).text_size(font_size::XXS)))
            .child(self.mark("tasks:mine"))
            .on_click(cx.listener(|this, _, _, cx| {
                this.ui.only_mine = !this.ui.only_mine;
                cx.notify();
            }))
    }

    fn task_row(&self, t: &Task, me: &str, board_name: Option<&str>, ghost: bool, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let mine = t.assignee_id.as_deref() == Some(me);
        let done = t.status == "DONE";
        let bg = if ghost { if mine { p.accent_soft } else { p.bg_3 } } else { p.bg_2 };
        let left = if mine { p.accent } else { status_color(&t.status, p) };
        let kind = t.kind();
        let labels = t.label_list();
        let mut title = div().flex().items_center().gap(px(5.)).min_w_0().text_size(font_size::MD).font_weight(FontWeight::BOLD).text_color(p.text_strong);
        title = title.child(div().flex_none().font_weight(FontWeight::MEDIUM).text_color(p.text_4).child(format!("#{}", t.number)));
        if kind != "TASK" {
            title = title.child(kind_badge(kind, p));
        }
        title = title.child(div().truncate().child(t.title.clone()));
        let mut meta = div().flex().items_center().gap(px(6.)).min_w_0().text_size(font_size::XS).text_color(if ghost { p.text_2 } else { p.text_3 }).whitespace_nowrap().overflow_hidden();
        if let Some(n) = board_name {
            meta = meta.child(div().truncate().child(n.to_string()));
        }
        if !labels.is_empty() {
            meta = meta.child(label_chips(&labels, 3, p));
        }
        let has_meta = board_name.is_some() || !labels.is_empty();
        let main = div().flex_1().min_w_0().flex().flex_col().gap(px(3.)).py(px(2.)).child(title).when(has_meta, |d| d.child(meta));
        let border = if ghost { p.border } else { p.border_subtle };
        let mut row = div()
            .id(SharedString::from(format!("row-{}", t.id)))
            .relative()
            .flex()
            .items_center()
            .gap(px(10.))
            .min_h(px(if ghost { 40. } else { 48. }))
            .py(px(if ghost { 5. } else { 6. }))
            .pr(px(8.))
            .pl(px(if ghost { 10. } else { 12. }))
            .border_1()
            .border_color(if ghost && mine { p.border_accent } else { border })
            .rounded(radius::SM)
            .bg(bg)
            .when(done, |d| d.opacity(if ghost { 0.55 } else { 0.75 }))
            // 3 px sol şerit (border-left; "mine" 2 px vurgu)
            .child(div().absolute().left(px(-1.)).top(px(-1.)).bottom(px(-1.)).w(px(if mine { 2. } else { 3. })).rounded_l(radius::SM).bg(left))
            .child(dot(priority_color(&t.priority, p), bg))
            .child(main);
        if let Some(a) = t.assignee_id.as_deref() {
            let name = self.member_name(a);
            row = row.child(avatar(a, &name, 20., p));
        }
        if !ghost {
            let task = t.clone();
            row = row.cursor_pointer().child(self.mark(format!("task-row:{}", t.title))).on_click(cx.listener(move |this, _, window, cx| this.edit_task(&task, window, cx)));
            let open = self.select.as_ref().is_some_and(|s| s.key == format!("task-status:{}", t.title));
            let key = format!("task-status:{}", t.title);
            let (sel, tid) = (t.status.clone(), t.id.clone());
            row = row.hover({
                let (hb, hbc) = (p.bg_hover, p.border_strong);
                move |s| s.bg(hb).border_color(hbc)
            });
            row = row.child(
                div().flex_none().w(px(118.)).child(select_box(key.clone(), status_label(&t.status).into(), 32., radius::XS, font_size::SM, open, p).child(self.mark(key.clone())).on_click(cx.listener(
                    move |this, _, _, cx| {
                        // Satırın tıklaması (düzenleme) tetiklenmesin.
                        cx.stop_propagation();
                        this.ui_task_target = Some(tid.clone());
                        this.open_select(key.clone(), opts(&STATUS), sel.clone());
                        cx.notify();
                    },
                ))),
            );
        }
        row.into_any_element()
    }

    fn task_card(&self, t: &Task, me: &str, p: &Palette) -> AnyElement {
        let mine = t.assignee_id.as_deref() == Some(me);
        let kind = t.kind();
        let labels = t.label_list();
        let (hb, hbc) = (p.bg_hover, p.border_strong);
        div()
            .id(SharedString::from(format!("card-{}", t.id)))
            .relative()
            .flex()
            .items_start()
            .gap(px(6.))
            .w_full()
            .py(px(7.))
            .px(px(8.))
            .border_1()
            .border_color(p.border_subtle)
            .rounded(radius::SM)
            .bg(p.bg_3)
            .text_size(font_size::SM)
            .font_weight(FontWeight(550.))
            .text_color(p.text)
            .hover(move |s| s.bg(hb).border_color(hbc))
            .when(mine, |d| d.child(div().absolute().left(px(-1.)).top(px(-1.)).bottom(px(-1.)).w(px(2.)).rounded_l(radius::SM).bg(p.accent)))
            .child(div().mt(px(4.)).child(dot(priority_color(&t.priority, p), p.bg_3)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(div().line_height(gpui_kit::relative(1.3)).text_color(p.text_strong).child(format!("#{} {}", t.number, t.title)))
                    .when(kind != "TASK" || !labels.is_empty(), |d| d.child(div().flex().flex_wrap().items_center().gap(px(3.)).when(kind != "TASK", |d| d.child(kind_badge(kind, p))).child(label_chips(&labels, 2, p)))),
            )
            .when_some(t.assignee_id.as_deref(), |d, a| d.child(avatar(a, &self.member_name(a), 20., p)))
            .into_any_element()
    }

    pub fn member_name(&self, id: &str) -> String {
        if id == self.st.user_id {
            return self.st.user_name.clone();
        }
        self.st.team.ready().and_then(|m| m.iter().find(|m| m.id == id)).map_or(p2_i18n::t("ovr.member"), |m| m.display_name.clone())
    }

    /// Göreve tıklanınca: "Yeni görev" penceresi düzenleme kipinde, alanlar doldurulmuş açılır.
    pub fn edit_task(&mut self, t: &Task, window: &mut Window, cx: &mut Context<Self>) {
        self.ui.creating = true;
        self.ui.editing = Some(t.id.clone());
        self.ui.delete_confirm = false;
        self.ui.draft = (t.status.clone(), t.priority.clone(), t.kind().to_string());
        let (title, desc, labels) = (t.title.clone(), t.description.clone().unwrap_or_default(), t.label_list().join(", "));
        self.ui.task_title.update(cx, |s, cx| s.set_value(title, window, cx));
        self.ui.task_desc.update(cx, |s, cx| s.set_value(desc, window, cx));
        self.ui.task_labels.update(cx, |s, cx| s.set_value(labels, window, cx));
        self.open_window("newtask");
        self.focus_window("newtask");
        self.pending.push(Cmd::Log(format!("görev düzenleniyor: #{}", t.number)));
        cx.notify();
    }

    pub fn submit_task(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.ui.creating {
            return;
        }
        let title = self.ui.task_title.read(cx).value().trim().to_string();
        if title.chars().count() < 2 {
            return;
        }
        let Some(data) = self.st.tasks.ready() else { return };
        let board = data.boards.iter().find(|b| b.id == self.ui.board).or(data.boards.first()).map(|b| b.id.clone());
        let Some(board) = board else { return };
        let desc = self.ui.task_desc.read(cx).value().to_string();
        let labels: Vec<String> = self.ui.task_labels.read(cx).value().split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).take(10).collect();
        let (status, priority, kind) = self.ui.draft.clone();
        if let Some(tid) = self.ui.editing.take() {
            if let Some(t) = data.all().find(|t| t.id == tid) {
                let patch = serde_json::json!({ "title": title, "priority": priority, "kind": kind, "labels": labels, "description": desc });
                let (b, from) = (t.board_id.clone(), t.status.clone());
                self.pending.push(Cmd::UpdateTask { board: b.clone(), task: tid.clone(), patch });
                if from != status {
                    self.pending.push(Cmd::MoveTask { board: b, task: tid, from, to: status });
                }
            }
            self.ui.creating = false;
            self.close_window("newtask");
            self.ui.task_title.update(cx, |s, cx| s.set_value("", window, cx));
            cx.notify();
            return;
        }
        let mut input = serde_json::json!({ "title": title, "status": status, "priority": priority, "kind": kind, "labels": labels, "assigneeId": self.st.user_id });
        if !desc.trim().is_empty() {
            input["description"] = serde_json::Value::String(desc);
        }
        self.pending.push(Cmd::CreateTask { board, input });
        self.ui.creating = false;
        self.close_window("newtask");
        self.ui.task_title.update(cx, |s, cx| s.set_value("", window, cx));
        cx.notify();
    }

    /// "Yeni görev" penceresi: başlık, açıklama, tür/öncelik/sütun, etiketler; Enter ya da Oluştur.
    fn new_task_body(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let Some(data) = self.st.tasks.ready() else { return empty(Some(&p2_i18n::t("ovr.loading")), None, p).into_any_element() };
        let Some(board) = data.boards.iter().find(|b| b.id == self.ui.board).or(data.boards.first()) else { return empty(Some(&p2_i18n::t("ovr.tasks.noBoards")), None, p).into_any_element() };
        let (status, priority, kind) = &self.ui.draft;
        let field = |label: &str, el: AnyElement| div().flex_1().min_w_0().flex().flex_col().gap(px(4.)).child(crate::ui::kick(label.to_string(), p).text_color(p.text_3)).child(el);
        let sel = |key: &'static str, list: Vec<(String, String)>, value: &str| {
            let label = list.iter().find(|(v, _)| v == value).map_or(String::new(), |(_, l)| l.clone());
            let open = self.select.as_ref().is_some_and(|s| s.key == key);
            let v = value.to_string();
            select_box(key.into(), label, 32., radius::XS, font_size::SM, open, p).child(self.mark(key)).on_click(cx.listener(move |this, _, _, cx| {
                this.open_select(key, list.clone(), v.clone());
                cx.notify();
            }))
        };
        let valid = self.ui.task_title.read(cx).value().trim().chars().count() >= 2;
        let editing = self.ui.editing.as_ref().and_then(|id| data.all().find(|t| &t.id == id));
        let edit_actions = editing.map(|t| {
            let (b, tid) = (t.board_id.clone(), t.id.clone());
            let mine = t.assignee_id.as_deref() == Some(self.st.user_id.as_str());
            let me = self.st.user_id.clone();
            let (b2, tid2) = (b.clone(), tid.clone());
            let confirm = self.ui.delete_confirm;
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(
                    btn("ot-delete", if confirm { p2_i18n::t("ovr.tasks.deleteConfirm") } else { p2_i18n::t("ovr.tasks.delete") }, Btn::Danger, p).child(self.mark("tasks:delete")).on_click(cx.listener(move |this, _, _, cx| {
                        if this.ui.delete_confirm {
                            this.ui.delete_confirm = false;
                            this.ui.editing = None;
                            this.ui.creating = false;
                            this.pending.push(Cmd::DeleteTask { board: b.clone(), task: tid.clone() });
                            this.close_window("newtask");
                        } else {
                            this.ui.delete_confirm = true;
                        }
                        cx.notify();
                    })),
                )
                .child(
                    btn("ot-assign", if mine { p2_i18n::t("ovr.tasks.unassign") } else { p2_i18n::t("ovr.tasks.assignMe") }, Btn::Ghost, p).child(self.mark("tasks:assign")).on_click(cx.listener(move |this, _, _, cx| {
                        let value = if mine { serde_json::Value::Null } else { serde_json::Value::String(me.clone()) };
                        this.pending.push(Cmd::UpdateTask { board: b2.clone(), task: tid2.clone(), patch: serde_json::json!({ "assigneeId": value }) });
                        cx.notify();
                    })),
                )
        });
        let form = div()
            .id("ot-modal")
            .flex()
            .flex_col()
            .gap(px(10.))
            .p(px(14.))
            .child(div().text_size(font_size::XS).text_color(p.text_3).child(match editing {
                Some(t) => p2_i18n::tv("ovr.tasks.editing", &[("n", &t.number.to_string())]),
                None => p2_i18n::tv("ovr.tasks.addsToBoard", &[("board", &board.name)]),
            }))
            .child(div().relative().h(px(34.)).child(field_input(&self.ui.task_title, p)).child(self.mark("tasks:title")))
            .child(
                div()
                    .py(px(6.))
                    .px(px(8.))
                    .border_1()
                    .border_color(p.border)
                    .rounded(radius::SM)
                    .bg(p.bg_3)
                    .text_size(font_size::SM)
                    .child(Textarea::new(&self.ui.task_desc).appearance(false).bordered(false)),
            )
            .child(div().flex().items_start().gap(px(8.)).child(field(&p2_i18n::t("ovr.tasks.fieldKind"), sel("draft:kind", KINDS.iter().map(|(a, b, _)| (a.to_string(), p2_i18n::t(b))).collect(), kind).into_any_element())).child(field(&p2_i18n::t("ovr.tasks.fieldPriority"), sel("draft:priority", opts(&PRIORITY), priority).into_any_element())).child(field(&p2_i18n::t("ovr.tasks.fieldColumn"), sel("draft:status", opts(&STATUS), status).into_any_element())))
            .child(div().flex().items_start().gap(px(8.)).child(field(&p2_i18n::t("ovr.tasks.fieldLabels"), div().h(px(32.)).child(field_input(&self.ui.task_labels, p)).into_any_element())))
            .child(
                div().flex().items_center().justify_end().gap(px(6.)).children(edit_actions).child(div().flex_1()).child(
                    btn("ot-create", if editing.is_some() { p2_i18n::t("ovr.tasks.save") } else { p2_i18n::t("ovr.tasks.create") }, Btn::Primary, p).when(!valid, |d| d.opacity(0.5)).child(self.mark("tasks:create")).on_click(cx.listener(|this, _, window, cx| this.submit_task(window, cx))),
                ),
            );
        form.into_any_element()
    }

    /// Açılır listeden seçim (selectShim `commit`).
    pub fn on_select(&mut self, key: &str, value: &str, _window: &mut Window, cx: &mut Context<Self>) {
        match key {
            "tasks:board" => self.ui.board = value.to_string(),
            "draft:kind" => self.ui.draft.2 = value.to_string(),
            "draft:priority" => self.ui.draft.1 = value.to_string(),
            "draft:status" => self.ui.draft.0 = value.to_string(),
            "hud:position" => {
                let mut look = self.st.look.clone();
                look.hud_position = value.to_string();
                self.pending.push(Cmd::Look(look));
            }
            "lib:corner" => {
                let mut look = self.st.look.clone();
                look.toast_corner = value.to_string();
                self.pending.push(Cmd::Look(look));
            }
            k if k.starts_with("task-status:") => {
                if let (Some(tid), Some(data)) = (self.ui_task_target.take(), self.st.tasks.ready()) {
                    if let Some(t) = data.all().find(|t| t.id == tid) {
                        if t.status != value {
                            self.pending.push(Cmd::MoveTask { board: t.board_id.clone(), task: t.id.clone(), from: t.status.clone(), to: value.to_string() });
                        }
                    }
                }
            }
            _ => {}
        }
        cx.notify();
    }

    // ==================================================================== Notlar
    fn notes_body(&self, win: &WinState, ghost: bool, mini: bool, p: &Palette, _window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let notes = match &self.st.notes {
            Loadable::Ready(n) => n,
            Loadable::Failed(e) => return Some(empty(Some(&p2_i18n::t("ovr.notes.loadFailed")), Some(e), p).into_any_element()),
            _ => return Some(empty(None, Some(&p2_i18n::t("ovr.notes.loading")), p).into_any_element()),
        };
        let pinned = notes.iter().find(|n| n.pinned);
        if mini {
            let n = pinned?;
            return Some(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .py(px(4.))
                    .px(px(8.))
                    .rounded(radius::PILL)
                    .bg(p.bg_2)
                    .text_color(p.text_strong)
                    .text_size(font_size::XS)
                    .font_weight(FontWeight(650.))
                    .child(ic(icons::NOTES, 1.8, 14., p.text_accent))
                    .child(div().truncate().child(note_title(n)))
                    .into_any_element(),
            );
        }
        if ghost {
            let mut d = div().flex().flex_col().gap(px(6.)).h_full().min_h_0().py(px(8.)).px(px(10.));
            match pinned {
                None => d = d.child(empty(Some(&p2_i18n::t("ovr.notes.noPinned")), Some(&p2_i18n::t("ovr.notes.noPinnedHint")), p)),
                Some(n) => {
                    d = d
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .child(div().flex_1().min_w_0().truncate().text_size(font_size::SM).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(note_title(n)))
                                .child(vis_badge(&n.visibility, p))
                                .child(div().text_size(font_size::XXS).text_color(p.text_4).child(time_ago(&n.updated_at))),
                        )
                        .child(div().flex_1().min_h_0().overflow_hidden().text_size(font_size::SM).line_height(gpui_kit::relative(1.45)).text_color(p.text).child(if n.body.trim().is_empty() { p2_i18n::t("ovr.notes.emptyBody") } else { n.body.clone() }));
                }
            }
            return Some(d.into_any_element());
        }
        let narrow = win.w - 2. - 20. < NARROW;
        let selected = self.ui.note_sel.as_ref().and_then(|id| notes.iter().find(|n| &n.id == id));
        let show_list = narrow && (self.ui.note_list || selected.is_none());
        let saving = selected.is_some_and(|n| self.st.saving_notes.contains(&n.id) || self.ui.note_dirty.is_some());

        let mut bar = div().flex().flex_wrap().items_center().gap(px(6.)).min_h(px(32.));
        if narrow && selected.is_some() && !self.ui.note_list {
            bar = bar.child(btn("pn-back", "", Btn::IconGhost, p).w(px(30.)).h(px(30.)).child(ic(icons::BACK, 2., 19., p.text_2)).child(self.mark("notes:back")).on_click(cx.listener(|this, _, _, cx| {
                this.ui.note_list = true;
                cx.notify();
            })));
        }
        bar = bar.child(
            btn("pn-new", "", Btn::Primary, p)
                .h(px(30.))
                .pl(px(8.))
                .pr(px(11.))
                .gap(px(4.))
                .shadow(vec![])
                .child(ic(icons::PLUS, 2.2, 15., p.text_on_accent))
                .child(p2_i18n::t("ovr.notes.new"))
                .child(self.mark("notes:new"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.pending.push(Cmd::CreateNote);
                    cx.notify();
                })),
        );
        if let Some(n) = selected.filter(|_| !show_list) {
            let (a, b) = (n.id.clone(), n.id.clone());
            let private = n.visibility != "TEAM";
            bar = bar.child(
                tabs(TabsStyle::Segment, p)
                    .child(tab("pn-private", p2_i18n::t("ovr.notes.private"), private, TabsStyle::Segment, 0, p).h(px(24.)).px(px(9.)).text_size(font_size::XS).on_click(cx.listener(move |this, _, _, cx| {
                        this.pending.push(Cmd::SaveNote { id: a.clone(), patch: serde_json::json!({ "visibility": "PRIVATE" }) });
                        cx.notify();
                    })))
                    .child(tab("pn-team", p2_i18n::t("ovr.notes.team"), !private, TabsStyle::Segment, 0, p).h(px(24.)).px(px(9.)).text_size(font_size::XS).on_click(cx.listener(move |this, _, _, cx| {
                        this.pending.push(Cmd::SaveNote { id: b.clone(), patch: serde_json::json!({ "visibility": "TEAM" }) });
                        cx.notify();
                    }))),
            );
            let (pid, pinned_now, did) = (n.id.clone(), n.pinned, n.id.clone());
            bar = bar
                .child(btn("pn-pin", "", Btn::IconGhost, p).w(px(30.)).h(px(30.)).when(pinned_now, |d| d.bg(p.accent_soft)).child(ic(icons::NOTE_PIN, 2., 19., if pinned_now { p.text_accent } else { p.text_2 })).on_click(cx.listener(move |this, _, _, cx| {
                    this.pending.push(Cmd::SaveNote { id: pid.clone(), patch: serde_json::json!({ "pinned": !pinned_now }) });
                    cx.notify();
                })))
                .child(btn("pn-del", "", Btn::IconGhost, p).w(px(30.)).h(px(30.)).child(ic(icons::TRASH, 2., 19., p.text_2)).on_click(cx.listener(move |this, _, _, cx| {
                    let _ = &did;
                    this.ui.note_confirm = true;
                    cx.notify();
                })));
        }
        bar = bar.child(div().flex_1());
        if narrow && show_list && !notes.is_empty() {
            bar = bar.child(div().text_size(font_size::XS).text_color(p.text_4).child(p2_i18n::tv("ovr.notes.count", &[("n", &notes.len().to_string())])));
        }
        let mut root = div().flex().flex_col().gap(px(8.)).h_full().min_h_0().pt(px(8.)).px(px(10.)).pb(px(10.)).child(bar);
        if self.ui.note_confirm {
            if let Some(n) = selected {
                let id = n.id.clone();
                root = root.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .py(px(6.))
                        .px(px(10.))
                        .border_1()
                        .border_color(p.danger_border)
                        .rounded(radius::SM)
                        .bg(p.danger_soft)
                        .text_color(p.danger_text)
                        .text_size(font_size::XS)
                        .child(p2_i18n::tv("ovr.notes.deleteConfirm", &[("title", &note_title(n))]))
                        .child(div().flex_1())
                        .child(btn("pn-cancel", p2_i18n::t("ovr.cancel"), Btn::Ghost, p).on_click(cx.listener(|this, _, _, cx| {
                            this.ui.note_confirm = false;
                            cx.notify();
                        })))
                        .child(btn("pn-delete", p2_i18n::t("ovr.delete"), Btn::Danger, p).on_click(cx.listener(move |this, _, _, cx| {
                            this.ui.note_confirm = false;
                            this.pending.push(Cmd::DeleteNote(id.clone()));
                            cx.notify();
                        }))),
                );
            }
        }
        let list = || {
            let mut l = div().flex().flex_col().gap(px(2.)).min_h_0().pr(px(2.));
            if notes.is_empty() {
                l = l.child(empty(Some(&p2_i18n::t("ovr.notes.none")), Some(&p2_i18n::t("ovr.notes.noneHint")), p).py(px(18.)).px(px(10.)));
            }
            for n in notes {
                let active = self.ui.note_sel.as_deref() == Some(n.id.as_str());
                let id = n.id.clone();
                let untitled = n.title.trim().is_empty();
                l = l.child(
                    div()
                        .id(SharedString::from(format!("pn-{}", n.id)))
                        .relative()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .w_full()
                        .py(px(8.))
                        .px(px(10.))
                        .border_1()
                        .border_color(if active { p.border_accent } else { gpui_kit::transparent_black() })
                        .rounded(radius::SM)
                        .when(active, |d| d.bg(p.bg_selected))
                        .when(!active, |d| d.hover(|s| s.bg(p.bg_hover)))
                        .cursor_pointer()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(2.))
                                .child(div().truncate().text_size(font_size::SM).font_weight(if untitled { FontWeight::MEDIUM } else { FontWeight(650.) }).text_color(if untitled { p.text_3 } else { p.text_strong }).when(untitled, |d| d.italic()).child(note_title(n)))
                                .child(div().truncate().text_size(font_size::XXS).text_color(p.text_3).child(time_ago(&n.updated_at))),
                        )
                        .child(div().flex().flex_col().items_end().gap(px(3.)).when(n.pinned, |d| d.child(ic(icons::NOTE_PIN, 2., 12., p.text_accent))).child(vis_badge(&n.visibility, p)))
                        .child(self.mark(format!("note:{}", note_title(n))))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.select_note(&id, window, cx);
                            cx.notify();
                        })),
                );
            }
            l
        };
        let editor = selected.map(|n| {
            div()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(div().h(px(36.)).child(field_input(&self.ui.note_title, p).h(px(36.)).px(px(12.)).text_size(font_size::LG).font_weight(FontWeight::BOLD)).child(self.mark("notes:title")))
                .child(
                    div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .flex_col()
                        .border_1()
                        .border_color(p.border)
                        .rounded(radius::MD)
                        .bg(p.bg_3)
                        .overflow_hidden()
                        .child(self.fmt_bar(p))
                        .child(div().relative().flex_1().min_h(px(120.)).py(px(10.)).px(px(12.)).text_size(font_size::MD).font_weight(FontWeight::MEDIUM).line_height(gpui_kit::relative(1.55)).child(Textarea::new(&self.ui.note_body).appearance(false).bordered(false)).child(self.mark("notes:body"))),
                )
                .child(
                    div()
                        .flex()
                        .flex_none()
                        .items_center()
                        .gap(px(8.))
                        .min_h(px(18.))
                        .text_size(font_size::XXS)
                        .text_color(p.text_4)
                        .child(vis_badge(&n.visibility, p))
                        .child(if n.author_id == self.st.user_id { p2_i18n::t("ovr.notes.yours") } else { p2_i18n::tv("ovr.notes.author", &[("name", &n.author.display_name)]) })
                        .child(format!("· {}", time_ago(&n.updated_at)))
                        .child(div().flex_1())
                        .child(div().flex().items_center().child(div().mr(px(5.)).size(px(6.)).rounded_full().bg(if saving { p.warning } else { p.success })).child(p2_i18n::t(if saving { "ovr.notes.saving" } else { "ovr.notes.saved" }))),
                )
        });
        let body = if narrow {
            if show_list { div().flex_1().min_h_0().flex().flex_col().child(list()) } else { div().flex_1().min_h_0().flex().flex_col().children(editor) }
        } else {
            div().flex_1().min_h_0().flex().gap(px(10.)).child(div().w(px(220.)).flex_none().child(list())).child(editor.map(|e| e.into_any_element()).unwrap_or_else(|| empty(Some(&p2_i18n::t("ovr.notes.pick")), Some(&p2_i18n::t("ovr.notes.pickHint")), p).flex_1().into_any_element()))
        };
        Some(root.child(body).into_any_element())
    }

    fn fmt_bar(&self, p: &Palette) -> impl IntoElement {
        let item = |id: &'static str, body: &'static str, label: String, stroke: f32| {
            let (hb, hc) = (p.accent_soft, p.text_strong);
            div()
                .id(id)
                .group(id)
                .h(px(26.))
                .px(px(8.))
                .flex()
                .items_center()
                .gap(px(5.))
                .rounded(radius::XS)
                .text_color(p.text_2)
                .text_size(font_size::XS)
                .font_weight(FontWeight::SEMIBOLD)
                .cursor_pointer()
                .hover(move |s| s.bg(hb).text_color(hc))
                .child(p2_ui::icons::Icon::raw(body).stroke(stroke).el().size(px(16.)).text_color(p.text_2).group_hover(id, move |s| s.text_color(hc)))
                .child(label)
        };
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(2.))
            .h(px(34.))
            .px(px(6.))
            .border_b_1()
            .border_color(p.border_subtle)
            .bg(p.bg_2)
            .child(item("fmt-b", icons::BOLD, p2_i18n::t("ovr.fmt.bold"), 2.2))
            .child(item("fmt-i", icons::ITALIC, p2_i18n::t("ovr.fmt.italic"), 1.9))
            .child(item("fmt-l", icons::FMT_LIST, p2_i18n::t("ovr.fmt.list"), 1.9))
            .child(item("fmt-k", icons::LINK, p2_i18n::t("ovr.fmt.link"), 1.9))
            .child(div().ml_auto().pr(px(4.)).text_size(font_size::XXS).font_weight(FontWeight::BOLD).text_color(p.text_4).child("MARKDOWN"))
    }

    pub fn select_note(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.flush_note(cx);
        self.ui.note_sel = Some(id.to_string());
        self.ui.note_list = false;
        self.ui.note_confirm = false;
        self.bind_note(window, cx);
    }

    /// Seçili notun başlık/gövdesini alanlara yazar (yalnız seçim değişince).
    pub fn bind_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.ui.note_sel.clone() else { return };
        if self.ui.note_bound.as_deref() == Some(id.as_str()) {
            return;
        }
        let Some(n) = self.st.notes.ready().and_then(|l| l.iter().find(|n| n.id == id)).cloned() else { return };
        self.ui.note_bound = None;
        self.ui.note_title.update(cx, |s, cx| s.set_value(n.title.clone(), window, cx));
        self.ui.note_body.update(cx, |s, cx| s.set_value(n.body.clone(), window, cx));
        self.ui.note_bound = Some(id);
        self.ui.note_dirty = None;
    }

    /// Bekleyen otomatik kaydı hemen gönderir.
    pub fn flush_note(&mut self, cx: &mut Context<Self>) {
        if self.ui.note_dirty.take().is_none() {
            return;
        }
        let Some(id) = self.ui.note_bound.clone() else { return };
        let title = self.ui.note_title.read(cx).value().to_string();
        let body = self.ui.note_body.read(cx).value().to_string();
        let Some(n) = self.st.notes.ready().and_then(|l| l.iter().find(|n| n.id == id)) else { return };
        let mut patch = serde_json::Map::new();
        if n.title != title {
            patch.insert("title".into(), title.into());
        }
        if n.body != body {
            patch.insert("body".into(), body.into());
        }
        if !patch.is_empty() {
            self.pending.push(Cmd::SaveNote { id, patch: serde_json::Value::Object(patch) });
        }
    }

    /// Zamanlayıcılar (host her döngüde çağırır): not otomatik kaydı (800 ms).
    pub fn tick(&mut self, cx: &mut Context<Self>) -> bool {
        if self.ui.note_dirty.is_some_and(|t| t.elapsed() >= NOTE_AUTOSAVE) {
            self.flush_note(cx);
            return true;
        }
        false
    }

    // ==================================================================== Performans
    fn perf_body(&self, ghost: bool, mini: bool, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let s = &self.st.perf;
        let hist = &self.st.history;
        let fps = hist.last().map(|_| s.fps.round() as i64);
        let ping = self.st.ping;
        let avg = (!hist.is_empty()).then(|| (hist.iter().sum::<f32>() / hist.len() as f32).round() as i64);
        let min = hist.iter().copied().fold(None, |m: Option<f32>, v| Some(m.map_or(v, |m| m.min(v)))).map(|v| v as i64);
        let gb = |mb: f32| (mb / 1024. * 10.).round() / 10.;
        let game_cpu = s.game_cpu.map(|c| c.round() as i64);
        let game_ram = s.game_ram_mb;
        let loss = self.st.loss;
        if mini {
            let pill = div()
                .flex()
                .items_center()
                .gap(px(4.))
                .py(px(4.))
                .px(px(10.))
                .rounded(radius::PILL)
                .border_1()
                .border_color(p.border)
                .bg(p.bg_2)
                .text_color(p.text_strong)
                .font_weight(FontWeight::BOLD)
                .child(fps.map_or("—".into(), |v| v.to_string()))
                .child(div().mr(px(6.)).text_color(p.text_3).font_weight(FontWeight::SEMIBOLD).child("FPS"))
                .child(div().w(px(1.)).h(px(12.)).mx(px(4.)).bg(p.border))
                .child(ping.map_or("—".into(), |v| v.to_string()))
                .child(div().mr(px(6.)).text_color(p.text_3).font_weight(FontWeight::SEMIBOLD).child("ms"));
            return pill.into_any_element();
        }
        let stat = |label: &str, value: String, unit: Option<&str>, low: bool, small: String, big: bool| {
            card(false, false, p)
                .flex()
                .flex_col()
                .gap(px(2.))
                .py(px(10.))
                .px(px(12.))
                .min_w_0()
                .child(crate::ui::kick(label.to_string(), p))
                .child(
                    div()
                        .flex()
                        .items_baseline()
                        .text_size(if big { font_size::XXXL } else { font_size::XXL })
                        .font_weight(FontWeight::EXTRA_BOLD)
                        .text_color(if low { p.warning_text } else { p.text_strong })
                        .child(value)
                        .when_some(unit, |d, u| d.child(div().text_size(font_size::SM).text_color(p.text_3).font_weight(FontWeight::SEMIBOLD).child(format!(" {u}")))),
                )
                .child(div().text_size(font_size::XXS).text_color(p.text_3).child(small))
        };
        let row2 = div()
            .flex()
            .gap(px(8.))
            .child(stat("FPS", fps.map_or("—".into(), |v| v.to_string()), None, fps.is_some_and(|f| f < 30), avg.map_or(p2_i18n::t("ovr.perf.waitingFrames"), |a| p2_i18n::tv("ovr.perf.avgMin", &[("avg", &a.to_string()), ("min", &min.unwrap_or(0).to_string())])), true).flex_1())
            .child(stat("Ping", ping.map_or("—".into(), |v| v.to_string()), Some("ms"), ping.is_some_and(|v| v > 150), p2_i18n::t("ovr.perf.p2Server"), true).flex_1());
        let ram_small = p2_i18n::tv("ovr.perf.sysRam", &[("used", &gb(s.sys_ram_used_mb as f32).to_string()), ("total", &(s.sys_ram_total_mb as f32 / 1024.).round().to_string())]);
        let cells: Vec<AnyElement> = vec![
            stat(&p2_i18n::t("ovr.perf.gameCpu"), game_cpu.map_or("—".into(), |v| v.to_string()), Some("%"), game_cpu.is_some_and(|c| c > 90), s.sys_cpu.map_or(p2_i18n::t("ovr.perf.measuring"), |c| p2_i18n::tv("ovr.perf.sysCpu", &[("n", &c.round().to_string())])), false).into_any_element(),
            stat(&p2_i18n::t("ovr.perf.gameRam"), game_ram.map_or("—".into(), |m| if m >= 1024 { format!("{}", gb(m as f32)) } else { m.to_string() }), Some(if game_ram.is_some_and(|m| m < 1024) { "MB" } else { "GB" }), false, ram_small, false).into_any_element(),
            stat("GPU", "—".into(), Some("%"), false, p2_i18n::t("ovr.perf.noNvidia"), false).into_any_element(),
            stat(&p2_i18n::t("ovr.perf.packetLoss"), loss.map_or("—".into(), |l| format!("{}", (l * 10.).round() / 10.)), Some("%"), loss.is_some_and(|l| l > 5.), if loss.is_some() { p2_i18n::t("ovr.perf.p2Server") } else { p2_i18n::t("ovr.perf.measuringLoss") }, false).into_any_element(),
        ];
        // .overlay-perf-grid.small: repeat(auto-fit, minmax(104px, 1fr)), aralık 8.
        let mut grid = div().flex().flex_wrap().gap(px(8.));
        for c in cells {
            grid = grid.child(div().flex_grow(1.).flex_basis(px(104.)).min_w(px(104.)).max_w(px(140.)).flex().flex_col().child(c));
        }
        let chart = self.perf_chart(p);
        let mut root = div().flex().flex_col().gap(px(10.)).p(px(10.)).min_h_full().flex_none().child(row2).child(grid).child(chart);
        if !ghost {
            let look = self.st.look.clone();
            let toggle_row = |id: &'static str, label: String, on: bool, f: fn(&mut p2_overlay_ipc::proto::Look)| {
                let look = look.clone();
                div()
                    .id(id)
                    .relative()
                    .min_h(px(36.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(10.))
                    .rounded(radius::SM)
                    .text_size(font_size::MD)
                    .font_weight(FontWeight(550.))
                    .text_color(p.text_2)
                    .child(label)
                    .child(toggle(SharedString::from(format!("{id}-t")), on, 0, p))
                    .child(self.mark(format!("perf:{id}")))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let mut l = look.clone();
                        f(&mut l);
                        this.pending.push(Cmd::Look(l));
                        cx.notify();
                    }))
            };
            let pos_label = p2_i18n::t(HUD_POSITIONS.iter().find(|(v, _)| *v == look.hud_position).map_or("ovr.corner.topLeft", |(_, l)| *l));
            let open = self.select.as_ref().is_some_and(|s| s.key == "hud:position");
            let cur = look.hud_position.clone();
            root = root.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(toggle_row("hud", p2_i18n::t("ovr.perf.hudToggle"), look.hud, |l| l.hud = !l.hud))
                    .child(
                        div()
                            .min_h(px(36.))
                            .px(px(10.))
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(10.))
                            .text_size(font_size::MD)
                            .font_weight(FontWeight(550.))
                            .text_color(p.text_2)
                            .child(p2_i18n::t("ovr.hudPosition"))
                            .child(div().flex_1().max_w(px(282.)).child(select_box("hud:position".into(), pos_label, 28., radius::SM, font_size::XS, open, p).child(self.mark("hud:position")).on_click(cx.listener(move |this, _, _, cx| {
                                this.open_select("hud:position", opts(&HUD_POSITIONS), cur.clone());
                                cx.notify();
                            })))),
                    )
                    .child(toggle_row("details", p2_i18n::t("ovr.perf.detailsToggle"), look.hud_details, |l| l.hud_details = !l.hud_details))
                    .child(toggle_row("motion", p2_i18n::t("ovr.reduceMotion"), look.reduce_motion, |l| l.reduce_motion = !l.reduce_motion))
                    .child(div().text_size(font_size::XXS).line_height(gpui_kit::relative(1.45)).text_color(p.text_4).child(p2_i18n::t("ovr.perf.explain"))),
            );
        }
        root.into_any_element()
    }

    /// `.overlay-perf-chart-wrap`: 96 px, son 60 sn FPS alan grafiği (svg), etiket ve tepe.
    fn perf_chart(&self, p: &Palette) -> impl IntoElement {
        let hist = &self.st.history;
        let peak = hist.iter().copied().fold(60f32, f32::max);
        // viewBox 0 0 60 40, preserveAspectRatio=none, çizgi 1.6 px (non-scaling-stroke): gerçek piksel boyutunda çizilir.
        let over = self.scrolls.borrow().get("perf").is_some_and(|h| f32::from(h.max_offset().y) > 0.5);
        let cw = self.st.store.win("perf").map_or(400., |w| w.w) - 2. - 20. - if over { 10. } else { 0. } - 2.;
        let ch = 94f32;
        let mut svg = String::new();
        if hist.len() > 1 {
            let off = 60. - hist.len() as f32;
            let pt = |i: usize, v: f32| ((off + i as f32) / 60. * cw, (40. - (v / peak * 36.).round()) / 40. * ch);
            let pts: Vec<String> = hist.iter().enumerate().map(|(i, v)| { let (x, y) = pt(i, *v); format!("{x:.2},{y:.2}") }).collect();
            let (fc, fa) = p2_ui::icons::svg_color(p.accent_soft);
            let (sc, sa) = p2_ui::icons::svg_color(p.accent);
            svg = format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {cw} {ch}" preserveAspectRatio="none" width="{w2}" height="{h2}"><path d="M{x0:.2},{ch} L{} L{x1:.2},{ch} Z" fill="{fc}" fill-opacity="{fa}"/><polyline points="{}" fill="none" stroke="{sc}" stroke-opacity="{sa}" stroke-width="1.6" stroke-linejoin="round"/></svg>"##,
                pts.join(" L"),
                pts.join(" "),
                w2 = cw * 2.,
                h2 = ch * 2.,
                x0 = off / 60. * cw,
                x1 = 59. / 60. * cw,
            );
        }
        div()
            .relative()
            .flex_none()
            .h(px(96.))
            .border_1()
            .border_color(p.border_subtle)
            .rounded(radius::SM)
            .bg(p.bg_1)
            .overflow_hidden()
            .when(!svg.is_empty(), |d| d.child(p2_ui::icons::svg_image(svg).absolute().left_0().top_0().w(px(cw)).h(px(ch))))
            .child(div().absolute().left(px(8.)).top(px(6.)).text_size(font_size::XXS).font_weight(FontWeight::BOLD).text_color(p.text_4).child(p2_i18n::tv("ovr.perf.chartLabel", &[("n", &hist.len().to_string())])))
            .when(!hist.is_empty(), |d| {
                d.child(div().absolute().right(px(8.)).bottom(px(6.)).py(px(1.)).px(px(5.)).rounded(radius::XS).bg(p.bg_1).font_family(FONT_MONO).text_size(font_size::XXS).font_weight(FontWeight::BOLD).text_color(p.text_3).child(p2_i18n::tv("ovr.perf.peak", &[("n", &(peak as i64).to_string())])))
            })
    }

    // ==================================================================== Sesli oda
    fn voice_body(&self, ghost: bool, mini: bool, p: &Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        let call = &self.st.call;
        if mini {
            let speaking: Vec<_> = call.people.iter().filter(|x| x.speaking).collect();
            if !call.active || speaking.is_empty() {
                return None;
            }
            let mut pill = div().flex().items_center().gap(px(8.)).py(px(4.)).pl(px(6.)).pr(px(10.)).border_1().border_color(p.border).rounded(radius::PILL).bg(p.bg_2).shadow(p.shadow_sm());
            for x in speaking {
                pill = pill.child(div().flex().items_center().gap(px(6.)).child(avatar(&x.user_id, &x.name, 28., p)).child(div().text_size(font_size::SM).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(x.name.clone())));
            }
            return Some(pill.into_any_element());
        }
        if ghost {
            if !call.active {
                return Some(div().flex().items_center().gap(px(8.)).py(px(8.)).px(px(10.)).text_color(p.text_3).text_size(font_size::SM).child(icons::call("volume", 16., p.text_3)).child(p2_i18n::t("ovr.voice.notInRoom")).into_any_element());
            }
            let mut list = div().flex().flex_col().gap(px(2.)).p(px(6.));
            for x in &call.people {
                list = list.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .py(px(4.))
                        .px(px(8.))
                        .rounded(radius::SM)
                        .when(x.speaking, |d| d.bg(p.accent_soft))
                        .child(avatar(&x.user_id, &x.name, 28., p))
                        .child(div().flex_1().min_w_0().truncate().text_size(font_size::MD).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(x.name.clone()))
                        .when(x.muted || x.deafened, |d| d.child(icons::call(if x.deafened { "headphonesOff" } else { "micOff" }, 14., p.danger_text))),
                );
            }
            return Some(list.into_any_element());
        }
        let mut root = div().flex().flex_col().gap(px(10.)).h_full().min_h_0().p(px(10.));
        if call.active {
            let n = call.people.len();
            root = root.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .pt(px(2.))
                    .px(px(2.))
                    .child(div().size(px(10.)).flex_none().rounded_full().bg(p.success).shadow(vec![BoxShadow { color: p.success_soft, offset: point(px(0.), px(0.)), blur_radius: px(0.), spread_radius: px(3.), inset: false }]))
                    .child(div().flex_1().min_w_0().flex().flex_col().gap(px(1.)).child(div().truncate().text_size(font_size::MD).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(if call.title.is_empty() { p2_i18n::t("ovr.tool.voice") } else { call.title.clone() })).child(div().text_size(font_size::XS).text_color(p.text_3).child(p2_i18n::tv("ovr.voice.people", &[("n", &n.to_string())]))))
                    .child(btn("voice-open", "", Btn::IconGhost, p).child(icons::call("expand", 15., p.text_2)).on_click(cx.listener(|this, _, _, cx| {
                        this.pending.push(Cmd::Action("call-open".into()));
                        cx.notify();
                    }))),
            );
            let mut grid = div().flex().flex_wrap().gap(px(8.)).flex_1().min_h_0().p(px(2.));
            for x in &call.people {
                let self_ = x.user_id == self.st.user_id || x.name == self.st.user_name;
                grid = grid.child(
                    div()
                        .relative()
                        .flex_grow(1.)
                        .flex_basis(px(118.))
                        .min_w(px(118.))
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(6.))
                        .pt(px(12.))
                        .px(px(8.))
                        .pb(px(10.))
                        .border_1()
                        .border_color(if x.speaking { p.success } else { p.border_subtle })
                        .rounded(radius::MD)
                        .bg(if x.speaking { p.accent_soft } else { p.bg_2 })
                        .overflow_hidden()
                        .child(avatar(&x.user_id, &x.name, 52., p).when(x.speaking, |a| a.shadow(vec![BoxShadow { color: p.success, offset: point(px(0.), px(0.)), blur_radius: px(0.), spread_radius: px(3.), inset: false }])))
                        .child(div().max_w_full().truncate().text_size(font_size::SM).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(format!("{}{}", x.name, if self_ { p2_i18n::t("ovr.youSuffix") } else { String::new() })))
                        .child(div().flex().gap(px(4.)).min_h(px(14.)).when(x.muted, |d| d.child(icons::call("micOff", 14., p.danger_text))).when(x.deafened, |d| d.child(icons::call("headphonesOff", 14., p.danger_text))))
                        .child(div().absolute().left_0().right_0().bottom_0().h(px(3.)).bg(p.bg_0)),
                );
            }
            root = root.child(grid);
            if call.people.len() <= 1 {
                root = root.child(div().py(px(2.)).px(px(4.)).text_size(font_size::XS).text_color(p.text_4).child(p2_i18n::t("ovr.voice.alone")));
            }
        } else {
            let mut rooms = div().flex_1().min_h_0().flex().flex_col().gap(px(8.)).child(crate::ui::kick(if self.st.voice_channels.is_empty() { p2_i18n::t("ovr.voice.channels") } else { p2_i18n::tv("ovr.voice.channelsCount", &[("n", &self.st.voice_channels.len().to_string())]) }, p).px(px(2.)));
            for (id, name) in &self.st.voice_channels {
                let cid = id.clone();
                let (hover, strong) = (p.accent_soft, p.text_strong);
                rooms = rooms.child(
                    div()
                        .id(SharedString::from(format!("join-{id}")))
                        .group(SharedString::from(format!("room-{id}")))
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .h(px(40.))
                        .px(px(10.))
                        .rounded(radius::MD)
                        .cursor_pointer()
                        .text_color(p.text_2)
                        .hover(move |s| s.bg(hover).text_color(strong))
                        .child(icons::call("volume", 18., p.text_3))
                        .child(div().flex_1().min_w_0().truncate().text_size(font_size::MD).font_weight(FontWeight::SEMIBOLD).child(name.clone()))
                        .child(div().text_size(font_size::XS).font_weight(FontWeight::BOLD).text_color(p.text_accent).child(p2_i18n::t("ovr.voice.join")))
                        .child(self.mark(format!("voice:join:{name}")))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.pending.push(Cmd::Action(format!("call-join:{cid}")));
                            cx.notify();
                        })),
                );
            }
            if self.st.voice_channels.is_empty() {
                rooms = rooms.child(empty(Some(&p2_i18n::t("ovr.voice.noChannels")), Some(&p2_i18n::t("ovr.voice.noChannelsHint")), p));
            }
            root = root.child(rooms);
        }
        if self.ui.invite {
            let mut picker = div().flex().flex_col().gap(px(2.)).max_h(px(190.)).p(px(8.)).border_1().border_color(p.border).rounded(radius::MD).bg(p.bg_2).child(div().pb(px(4.)).px(px(4.)).text_size(font_size::XXS).font_weight(FontWeight::EXTRA_BOLD).text_color(p.text_4).child(p2_i18n::t(if call.active { "ovr.voice.addToRoomKicker" } else { "ovr.voice.callKicker" })));
            let people: Vec<Member> = self.st.team.ready().map(|m| m.iter().filter(|m| m.id != self.st.user_id).cloned().collect()).unwrap_or_default();
            for m in &people {
                let (id, active) = (m.id.clone(), call.active);
                picker = picker.child(
                    div()
                        .id(SharedString::from(format!("inv-{}", m.id)))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .py(px(6.))
                        .px(px(8.))
                        .rounded(radius::SM)
                        .cursor_pointer()
                        .hover(|s| s.bg(p.accent_soft))
                        .child(avatar(&m.id, &m.display_name, 20., p))
                        .child(m.display_name.clone())
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.pending.push(Cmd::Action(format!("{}:{id}", if active { "call-invite" } else { "call-start" })));
                            this.ui.invite = false;
                            cx.notify();
                        })),
                );
            }
            if people.is_empty() {
                picker = picker.child(div().py(px(2.)).px(px(4.)).text_size(font_size::XS).text_color(p.text_4).child(p2_i18n::t("ovr.voice.teamNotLoaded")));
            }
            root = root.child(picker);
        }
        root = root.child(self.voice_bar(p, cx));
        Some(root.into_any_element())
    }

    /// `.overlay-voice-bar`: Discord tarzı alt çubuk — solda bağlantı durumu, sağda yuvarlak
    /// simge düğmeleri (mikrofon, kulaklık, davet/ara, ayrıl). Oyun içinde çalışmayan kamera ve
    /// ekran paylaşımı yok.
    fn voice_bar(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let call = &self.st.call;
        let round = |id: &'static str, icon: &'static str, tip: String, active: bool, danger: bool| {
            let (bg, fg) = if danger { (p.danger, p.text_on_accent) } else if active { (p.bg_3, p.text_strong) } else { (p.danger_soft, p.danger_text) };
            let hover = if danger { p.danger_deep } else { p.accent_soft_2 };
            div()
                .id(id)
                .relative()
                .size(px(34.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(bg)
                .cursor_pointer()
                .hover(move |s| s.bg(hover))
                .child(icons::call(icon, 16., fg))
                .tooltip(move |window, cx| p2_ui::controls::tooltip(tip.clone(), window, cx))
                .child(self.mark(format!("voice:{id}")))
        };
        let act = |name: &'static str| {
            cx.listener(move |this: &mut OverlayRoot, _: &gpui_kit::ClickEvent, _: &mut Window, cx: &mut Context<OverlayRoot>| {
                this.pending.push(Cmd::Action(name.into()));
                cx.notify();
            })
        };
        let status = if call.active {
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(1.))
                .child(div().text_size(font_size::XS).font_weight(FontWeight::BOLD).text_color(p.success_text).child(p2_i18n::t("ovr.voice.connected")))
                .child(div().truncate().text_size(font_size::XS).text_color(p.text_3).child(if call.title.is_empty() { p2_i18n::t("ovr.tool.voice") } else { call.title.clone() }))
        } else {
            div().flex_1().min_w_0().text_size(font_size::XS).text_color(p.text_4).child(p2_i18n::t("ovr.voice.notInRoom"))
        };
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .pt(px(10.))
            .mt_auto()
            .border_t_1()
            .border_color(p.border_subtle)
            .child(status)
            .child(round("mute", if call.muted { "micOff" } else { "mic" }, p2_i18n::t(if call.muted { "ovr.voice.muted" } else { "ovr.voice.mic" }), !call.muted, false).on_click(act("call-mute")))
            .child(round("deafen", if call.deafened { "headphonesOff" } else { "headphones" }, p2_i18n::t(if call.deafened { "ovr.voice.deafened" } else { "ovr.voice.headphones" }), !call.deafened, false).on_click(act("call-deafen")))
            .child(round("invite", "personAdd", p2_i18n::t(if call.active { "ovr.voice.invite" } else { "ovr.voice.call" }), true, false).on_click(cx.listener(|this, _, _, cx| {
                this.ui.invite = !this.ui.invite;
                cx.notify();
            })))
            .when(call.active, |d| d.child(round("leave", "hangup", p2_i18n::t("ovr.voice.leave"), true, true).on_click(act("call-leave"))))
    }

    // ==================================================================== Ekip
    fn team_body(&self, ghost: bool, mini: bool, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let members = match &self.st.team {
            Loadable::Ready(m) => m,
            _ => return empty(None, Some(&p2_i18n::t("ovr.team.waiting")), p).into_any_element(),
        };
        let presence = |s: &str| match s {
            "ONLINE" => Presence::Online,
            "AWAY" => Presence::Away,
            "BUSY" => Presence::Busy,
            _ => Presence::Offline,
        };
        let label = |s: &str| {
            p2_i18n::t(match s {
                "ONLINE" => "ovr.presence.online",
                "AWAY" => "ovr.presence.away",
                "BUSY" => "ovr.presence.busy",
                _ => "ovr.presence.offline",
            })
        };
        let rank = |m: &Member| if m.status == "ONLINE" || m.status == "AWAY" || m.status == "BUSY" { 0 } else { 5 };
        let mut sorted: Vec<&Member> = members.iter().filter(|m| m.id != self.st.user_id).collect();
        sorted.sort_by(|a, b| rank(a).cmp(&rank(b)).then(a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase())));
        let online: Vec<&Member> = sorted.iter().copied().filter(|m| rank(m) < 5).collect();
        let offline: Vec<&Member> = sorted.iter().copied().filter(|m| rank(m) >= 5).collect();
        let av = |m: &Member, size: f32| div().relative().child(avatar(&m.id, &m.display_name, size, p)).child(div().absolute().right(px(-2.)).bottom(px(-2.)).child(status_dot(presence(&m.status), Some(p.bg_1), p)));
        if mini {
            let mut d = div().flex().flex_wrap().gap(px(4.)).p(px(4.));
            for m in &online {
                d = d.child(av(m, 20.));
            }
            return d.into_any_element();
        }
        let mut root = div().flex().flex_col().gap(px(4.)).p(px(6.)).min_h_full().flex_none();
        if sorted.is_empty() {
            root = root.child(empty(Some(&p2_i18n::t("ovr.team.empty")), Some(&p2_i18n::t("ovr.team.emptyHint")), p));
        }
        let row = |m: &Member| {
            let self_ = m.id == self.st.user_id;
            let open = self.ui.team_open.as_deref() == Some(m.id.as_str()) && !ghost;
            let mut r = div()
                .id(SharedString::from(format!("team-{}", m.id)))
                .relative()
                .flex()
                .items_center()
                .gap(px(10.))
                .py(px(6.))
                .px(px(8.))
                .rounded(radius::SM)
                .text_size(font_size::MD)
                .when(open, |d| d.bg(p.hover_soft).rounded_b(px(0.)))
                .hover(|s| s.bg(p.hover_soft))
                .child(av(m, 28.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(div().truncate().font_weight(FontWeight::BOLD).text_color(p.text_strong).child(format!("{}{}", m.display_name, if self_ { p2_i18n::t("ovr.youSuffix") } else { String::new() })))
                        .child(div().truncate().text_size(font_size::XS).text_color(p.text_3).child(label(&m.status))),
                );
            if !ghost {
                let (cid, pid, oid) = (m.id.clone(), m.id.clone(), m.id.clone());
                let in_call = self.st.call.active;
                let mut actions = div().flex().gap(px(4.)).flex_none();
                if !self_ {
                    actions = actions
                        .child(btn(SharedString::from(format!("tm-msg-{}", m.id)), "", Btn::IconGhost, p).w(px(28.)).h(px(28.)).child(ic(icons::CHAT, 1.8, 16., p.text_2)).on_click(cx.listener(move |this, _, _, cx| {
                            this.pending.push(Cmd::Action(format!("open-dm:{cid}")));
                            cx.notify();
                        })))
                        .child(btn(SharedString::from(format!("tm-call-{}", m.id)), "", Btn::IconGhost, p).w(px(28.)).h(px(28.)).child(icons::call(if in_call { "personAdd" } else { "phone" }, 16., p.text_2)).on_click(cx.listener(move |this, _, _, cx| {
                            this.pending.push(Cmd::Action(format!("{}:{pid}", if in_call { "call-invite" } else { "call-start" })));
                            cx.notify();
                        })));
                }
                actions = actions.child(btn(SharedString::from(format!("tm-prof-{}", m.id)), "", Btn::IconGhost, p).w(px(28.)).h(px(28.)).when(open, |d| d.bg(p.accent_soft)).child(ic(icons::PROFILE, 1.8, 15., if open { p.text_accent } else { p.text_2 })).on_click(cx.listener(move |this, _, _, cx| {
                    this.ui.team_open = if this.ui.team_open.as_deref() == Some(oid.as_str()) { None } else { Some(oid.clone()) };
                    cx.notify();
                })));
                r = r.child(actions);
            }
            let mut item = div().flex().flex_col().child(r);
            if open {
                let role = match m.role.as_str() {
                    "OWNER" => p2_i18n::t("ovr.role.owner"),
                    "ADMIN" => p2_i18n::t("ovr.role.admin"),
                    "" => String::new(),
                    _ => p2_i18n::t("ovr.member"),
                };
                let meta_row = |k: &str, v: String| div().flex().gap(px(8.)).text_size(font_size::XS).child(div().flex_none().w(px(52.)).text_size(font_size::XXS).font_weight(FontWeight::BOLD).text_color(p.text_4).child(k.pipe_upper())).child(div().min_w_0().truncate().text_color(p.text).child(v));
                let oid = m.id.clone();
                item = item.child(
                    card(false, false, p)
                        .mx(px(4.))
                        .mb(px(6.))
                        .p(px(10.))
                        .rounded_t(px(0.))
                        .flex()
                        .flex_col()
                        .gap(px(8.))
                        .child(div().flex().items_center().gap(px(10.)).child(av(m, 36.)).child(div().flex_1().min_w_0().child(div().text_size(font_size::MD).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(m.display_name.clone())).when(!m.username.is_empty(), |d| d.child(div().text_color(p.text_3).child(format!("@{}", m.username))))))
                        .child(div().flex().flex_col().gap(px(4.)).when_some(m.title.clone().filter(|t| !t.is_empty()), |d, t| d.child(meta_row(&p2_i18n::t("ovr.team.jobTitle"), t))).when(!role.is_empty(), |d| d.child(meta_row(&p2_i18n::t("ovr.team.role"), role.clone()))).child(meta_row(&p2_i18n::t("ovr.team.status"), label(&m.status))))
                        .child(div().flex().flex_wrap().gap(px(6.)).child(btn(SharedString::from(format!("tm-app-{}", m.id)), p2_i18n::t("ovr.openInApp"), Btn::Ghost, p).on_click(cx.listener(move |this, _, _, cx| {
                            this.pending.push(Cmd::Action(format!("open-profile:{oid}")));
                            cx.notify();
                        })))),
                );
            }
            item
        };
        if !online.is_empty() {
            root = root.child(div().py(px(4.)).px(px(8.)).text_size(font_size::XS).text_color(p.text_3).child(p2_i18n::tv("ovr.team.onlineCount", &[("n", &online.len().to_string())])));
            for m in &online {
                root = root.child(row(m));
            }
        }
        if !offline.is_empty() {
            root = root.child(div().pt(px(8.)).px(px(8.)).pb(px(4.)).text_size(font_size::XS).text_color(p.text_3).child(p2_i18n::tv("ovr.team.offlineCount", &[("n", &offline.len().to_string())])));
            for m in &offline {
                root = root.child(row(m));
            }
        }
        root.into_any_element()
    }

    // ==================================================================== Oyun özeti
    fn summary_body(&self, ghost: bool, mini: bool, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let st = &self.st;
        let name = if st.project_name.is_empty() { p2_i18n::t("ovr.project") } else { st.project_name.clone() };
        let installed = st.installed.clone();
        // library.ts `formatDuration`.
        let fmt_dur = |s: u64| {
            if s < 60 {
                return if s == 0 { "—".to_string() } else { p2_i18n::tv("ovr.dur.sec", &[("s", &s.to_string())]) };
            }
            let (h, m) = (s / 3600, ((s % 3600) as f32 / 60.).round() as u64);
            let (hs, ms) = (h.to_string(), m.to_string());
            if h >= 100 || (h > 0 && m == 0) {
                p2_i18n::tv("ovr.dur.h", &[("h", &hs)])
            } else if h > 0 {
                p2_i18n::tv("ovr.dur.hm", &[("h", &hs), ("m", &ms)])
            } else {
                p2_i18n::tv("ovr.dur.m", &[("m", &ms)])
            }
        };
        let (play, team_play) = match &st.hub { Some((m, t, _)) => (*m, *t), None => (installed.as_ref().map_or(0, |i| i.play_seconds), 0) };
        if mini {
            return div()
                .flex()
                .items_center()
                .gap(px(8.))
                .py(px(4.))
                .px(px(10.))
                .rounded(radius::PILL)
                .bg(p.bg_2)
                .text_size(font_size::SM)
                .child(div().font_weight(FontWeight::BOLD).text_color(p.text_strong).child(name))
                .when_some(installed.as_ref(), |d, i| d.child(div().font_family(FONT_MONO).child(format!("v{}", i.version))))
                .child(div().text_color(p.text_3).child(fmt_dur(play)))
                .into_any_element();
        }
        let initial = name.chars().next().map(|c| c.pipe_upper().to_string()).unwrap_or_default();
        let chip = |text: String, live: bool| {
            div()
                .h(px(16.))
                .px(px(7.))
                .flex()
                .items_center()
                .rounded(radius::PILL)
                .border_1()
                .border_color(if live { p.success_border } else { p.border_subtle })
                .bg(if live { p.success_soft } else { p.bg_3 })
                .text_size(font_size::XXS)
                .font_weight(FontWeight::BOLD)
                .text_color(if live { p.success_text } else { p.text_2 })
                .when(live, |d| d.child(div().mr(px(5.)).size(px(6.)).rounded_full().bg(p.success)))
                .child(text)
        };
        let head = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .py(px(10.))
            .px(px(12.))
            .border_1()
            .border_color(p.border_subtle)
            .rounded(radius::MD)
            .bg(p.bg_2)
            .child(div().size(px(40.)).flex_none().rounded(radius::SM).bg(p.accent_soft).text_color(p.text_accent).flex().items_center().justify_center().text_size(font_size::LG).font_weight(FontWeight::EXTRA_BOLD).child(initial))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(div().truncate().text_size(font_size::LG).font_weight(FontWeight(750.)).text_color(p.text_strong).child(name.clone()))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(6.))
                            .text_size(font_size::XS)
                            .text_color(p.text_3)
                            .when_some(installed.as_ref(), |d, i| d.child(div().font_family(FONT_MONO).font_weight(FontWeight::SEMIBOLD).text_color(p.text).child(format!("v{}", i.version))).child(chip(channel_label(&i.channel), false)))
                            .when(installed.is_none(), |d| d.child(p2_i18n::t("ovr.notInstalled")))
                            .child(chip(p2_i18n::t("ovr.running"), true)),
                    ),
            );
        let stat = |label: &str, value: String| {
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .py(px(8.))
                .px(px(10.))
                .rounded(radius::SM)
                .bg(p.bg_2)
                .border_1()
                .border_color(p.border_subtle)
                .child(div().truncate().text_size(font_size::XXS).font_weight(FontWeight::BOLD).text_color(p.text_3).child(label.pipe_upper()))
                .child(div().truncate().text_size(font_size::LG).font_weight(FontWeight(750.)).text_color(p.text_strong).child(value))
        };
        let section = |title: &str, extra: Option<AnyElement>, body: AnyElement| {
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(div().flex().items_center().gap(px(6.)).px(px(2.)).child(div().flex_1().min_w_0().text_size(font_size::XXS).font_weight(FontWeight::EXTRA_BOLD).text_color(p.text_accent).child(title.pipe_upper())).children(extra))
                .child(body)
        };
        let note = |t: &str| div().py(px(8.)).px(px(10.)).rounded(radius::SM).bg(p.bg_2).text_color(p.text_4).text_size(font_size::XS).child(t.to_string()).into_any_element();
        let mut root = div().flex().flex_col().gap(px(8.)).p(px(8.)).min_h_full().flex_none().child(head);
        if ghost {
            return root.child(div().flex().gap(px(6.)).child(stat(&p2_i18n::t("ovr.summary.yourPlaytime"), fmt_dur(play))).child(stat(&p2_i18n::t("ovr.summary.playing"), "1".into()))).into_any_element();
        }
        root = root
            .child(div().flex().gap(px(6.)).child(stat(&p2_i18n::t("ovr.summary.yourPlaytime"), fmt_dur(play))).child(stat(&p2_i18n::t("ovr.summary.teamTotal"), fmt_dur(team_play))).child(stat(&p2_i18n::t("ovr.summary.playing"), "1".into())))
            .child(section(&p2_i18n::t("ovr.summary.whoIsPlaying"), Some(div().text_size(font_size::XXS).font_weight(FontWeight::BOLD).text_color(p.text_4).child("1").into_any_element()), div().flex().items_center().gap(px(8.)).py(px(6.)).px(px(8.)).rounded(radius::SM).bg(p.bg_2).border_1().border_color(p.border_subtle).child(avatar(&st.user_id, &st.user_name, 28., p)).child(div().flex_1().min_w_0().truncate().text_size(font_size::SM).text_color(p.text_2).child(st.user_name.clone())).into_any_element()));
        let live = st.live_release.clone();
        root = root.child(section(
            &p2_i18n::t("ovr.summary.releaseNotes"),
            live.as_ref().map(|(v, c, _)| div().text_size(font_size::XXS).font_weight(FontWeight::BOLD).text_color(p.text_4).child(format!("v{v} · {}", channel_label(c))).into_any_element()),
            match &live {
                None => note(&p2_i18n::t("ovr.summary.noRelease")),
                Some((_, _, n)) if n.trim().is_empty() => note(&p2_i18n::t("ovr.summary.noReleaseNotes")),
                Some((_, _, n)) => div().py(px(8.)).px(px(10.)).rounded(radius::SM).bg(p.bg_2).border_1().border_color(p.border_subtle).text_size(font_size::SM).line_height(gpui_kit::relative(1.45)).child(n.clone()).into_any_element(),
            },
        ));
        root = root.child(section(
            &p2_i18n::t("ovr.summary.recentPosts"),
            Some(btn("ov-all", p2_i18n::t("ovr.summary.all"), Btn::Ghost, p).h(px(22.)).px(px(8.)).text_size(font_size::XS).on_click(cx.listener(|this, _, _, cx| {
                this.pending.push(Cmd::Action("open-overview".into()));
                cx.notify();
            })).into_any_element()),
            self.hub_posts(p),
        ));
        root.into_any_element()
    }

    // ==================================================================== Ekran görüntüsü
    fn capture_body(&self, ghost: bool, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let shots = &self.st.shots;
        let card_of = |s: &crate::ui::ShotRec| {
            let name = std::path::Path::new(&s.path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            div()
                .flex_none()
                .w(px(196.))
                .flex()
                .flex_col()
                .gap(px(5.))
                .p(px(6.))
                .border_1()
                .border_color(p.border)
                .rounded(radius::MD)
                .bg(p.bg_2)
                .child(div().w_full().h(px(103.)).rounded(radius::SM).bg(p.bg_0).flex().items_center().justify_center().child(div().text_size(font_size::XXS).text_color(p.text_4).child(p2_i18n::t("ovr.capture.noPreview"))))
                .child(div().flex().flex_col().gap(px(1.)).child(div().truncate().text_size(font_size::XS).font_weight(FontWeight::SEMIBOLD).text_color(p.text_strong).child(name)).child(div().text_size(font_size::XXS).text_color(p.text_3).child(format!("{}×{}", s.width, s.height))))
        };
        if ghost {
            let mut strip = div().flex().gap(px(8.)).p(px(8.));
            for s in shots.iter().take(4) {
                strip = strip.child(card_of(s));
            }
            if shots.is_empty() {
                strip = strip.child(div().text_size(font_size::XS).text_color(p.text_3).child(p2_i18n::t("ovr.capture.noneYet")));
            }
            return strip.into_any_element();
        }
        // Yatay ray: sığmayan çekimler yana kayar, altta kaydırma çubuğu (overlay'de genel çubuk
        // ayarı kapalı olduğundan bu öğede açıkça açılır).
        use p2_ui::P2Transition as _;
        let mut strip = p2_ui::components::h_rail("oc-shots-rail").gap(px(8.)).auto_scrollbar();
        for s in shots {
            strip = strip.child(card_of(s));
        }
        let h4 = |t: &str, n: usize| div().flex().items_baseline().text_size(font_size::XS).font_weight(FontWeight::BOLD).text_color(p.text_3).child(t.pipe_upper()).when(n > 0, |d| d.child(div().font_weight(FontWeight::MEDIUM).text_color(p.text_4).child(format!(" · {n}"))));
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(8.))
            .min_h_full()
            .min_h_0()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .flex_none()
                    .child(div().text_size(font_size::XS).text_color(p.text_3).line_height(gpui_kit::relative(1.4)).child(p2_i18n::t("ovr.capture.howTo")))
                    .child(div().flex_1().min_w_0())
                    .child(p2_ui::controls::button("oc-bug", p2_ui::controls::BtnVariant::Default, p2_ui::controls::BtnSize::Sm, false, false, p).child(p2_i18n::t("ovr.capture.reportBug")).on_click(cx.listener(|this, _, _, cx| {
                        // Hata bildir: Yeni görev penceresi "Hata" türüyle açılır.
                        this.ui.creating = true;
                        this.ui.draft = ("TODO".into(), "HIGH".into(), "BUG".into());
                        this.open_window("newtask");
                        this.focus_window("newtask");
                        cx.notify();
                    }))),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .gap(px(12.))
                    .pr(px(2.))
                    .child(div().flex().flex_col().gap(px(6.)).child(h4(&p2_i18n::t("ovr.capture.recent"), shots.len())).child(if shots.is_empty() {
                        div().text_size(font_size::XS).text_color(p.text_3).line_height(gpui_kit::relative(1.4)).child(p2_i18n::t("ovr.capture.noneYetHint")).into_any_element()
                    } else {
                        strip.into_any_element()
                    }))
                    .child(div().flex().flex_col().gap(px(6.)).child(h4(&p2_i18n::t("ovr.capture.gallery"), 0)).child(empty(None, Some(&p2_i18n::t("ovr.capture.galleryHint")), p))),
            )
            .into_any_element()
    }

    // ==================================================================== Ayarlar (Kütüphane hızlı menüsü)
    fn library_body(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let st = &self.st;
        let look = st.look.clone();
        let name = if st.project_name.is_empty() { p2_i18n::t("ovr.project") } else { st.project_name.clone() };
        let h4 = |t: &str| div().px(px(2.)).text_size(font_size::XXS).font_weight(FontWeight::EXTRA_BOLD).text_color(p.text_accent).child(t.pipe_upper());
        let installed = st.installed.clone();
        let game = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .py(px(10.))
            .px(px(12.))
            .border_1()
            .border_color(p.border_subtle)
            .rounded(radius::MD)
            .bg(p.bg_2)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(3.))
                    .child(div().truncate().text_size(font_size::LG).font_weight(FontWeight(750.)).text_color(p.text_strong).child(name))
                    .child(div().flex().items_center().gap(px(6.)).text_size(font_size::XS).text_color(p.text_3).map(|d| match &installed {
                        Some(i) => d.child(div().font_family(FONT_MONO).font_weight(FontWeight::SEMIBOLD).text_color(p.text).child(format!("v{}", i.version))).child(channel_label(&i.channel)),
                        None => d.child(p2_i18n::t("ovr.notInstalled")),
                    })),
            )
            .child(
                div()
                    .h(px(16.))
                    .px(px(7.))
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .rounded(radius::PILL)
                    .border_1()
                    .border_color(p.success_border)
                    .bg(p.success_soft)
                    .text_size(font_size::XXS)
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.success_text)
                    .child(div().size(px(6.)).rounded_full().bg(p.success))
                    .child(p2_i18n::t("ovr.running")),
            );
        // .lt-actions: repeat(auto-fit, minmax(118px, 1fr)), aralık 6.
        let inner = self.st.store.win("library").map_or(380., |w| w.w) - 2. - 16.;
        let cols = ((inner + 6.) / (118. + 6.)).floor().max(1.);
        let lib_btn_w = ((inner - 6. * (cols - 1.)) / cols).floor();
        let action = |id: &'static str, body: &'static str, label: String| {
            p2_ui::controls::button(id, p2_ui::controls::BtnVariant::Secondary, p2_ui::controls::BtnSize::Sm, false, false, p).relative().w(px(lib_btn_w)).h(px(32.)).text_size(font_size::SM).gap(px(0.)).child(div().mr(px(6.)).child(ic(body, 1.8, 15., p.text))).child(label).child(self.mark(format!("lib:{id}")))
        };
        let actions: AnyElement = if self.ui.stop_confirm {
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .py(px(8.))
                .px(px(10.))
                .rounded(radius::SM)
                .bg(p.danger_soft)
                .border_1()
                .border_color(p.danger_border)
                .text_color(p.danger_text)
                .text_size(font_size::SM)
                .child(div().flex_1().min_w_0().child(p2_i18n::t("ovr.lib.stopConfirm")))
                .child(btn("lt-stop-yes", p2_i18n::t("ovr.lib.stopYes"), Btn::Danger, p).on_click(cx.listener(|this, _, _, cx| {
                    this.ui.stop_confirm = false;
                    this.pending.push(Cmd::Action("game-stop".into()));
                    cx.notify();
                })))
                .child(btn("lt-stop-no", p2_i18n::t("ovr.cancel"), Btn::Ghost, p).on_click(cx.listener(|this, _, _, cx| {
                    this.ui.stop_confirm = false;
                    cx.notify();
                })))
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_wrap()
                .gap(px(6.))
                .child(action("stop", icons::POWER, p2_i18n::t("ovr.lib.stopGame")).on_click(cx.listener(|this, _, _, cx| {
                    this.ui.stop_confirm = true;
                    cx.notify();
                })))
                .child(action("folder", icons::FOLDER, p2_i18n::t("ovr.toast.openFolder")).on_click(cx.listener(|this, _, _, cx| {
                    this.pending.push(Cmd::Action("open-folder".into()));
                    cx.notify();
                })))
                .child(action("shot", icons::CAPTURE, p2_i18n::t("ovr.takeScreenshot")).on_click(cx.listener(|this, _, _, cx| {
                    this.pending.push(Cmd::Shot);
                    cx.notify();
                })))
                .into_any_element()
        };
        let lrow = || div().flex().items_center().gap(px(10.)).min_h(px(36.)).py(px(6.)).px(px(10.)).rounded(radius::SM).bg(p.bg_2).border_1().border_color(p.border_subtle);
        let text = |b: &str, s: Option<&str>| div().flex_1().min_w_0().flex().flex_col().gap(px(1.)).child(div().truncate().text_size(font_size::SM).font_weight(FontWeight::SEMIBOLD).text_color(p.text).child(b.to_string())).when_some(s, |d, s| d.child(div().truncate().text_size(font_size::XXS).text_color(p.text_4).child(s.to_string())));
        let slider = |label: &str, value: i32, min: f32, max: f32, unit: &str, key: &'static str| {
            let t = (value as f32 - min) / (max - min);
            div()
                .flex()
                .flex_col()
                .gap(px(6.))
                .py(px(6.))
                .px(px(10.))
                .rounded(radius::SM)
                .bg(p.bg_2)
                .border_1()
                .border_color(p.border_subtle)
                .child(div().flex().items_center().justify_between().gap(px(8.)).child(div().text_size(font_size::SM).font_weight(FontWeight::SEMIBOLD).text_color(p.text).child(label.to_string())).child(div().font_family(FONT_MONO).text_size(font_size::XS).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(format!("{value}{unit}"))))
                .child(div().relative().h(px(14.)).flex().items_center().child(slider_track(t, p)).child(self.mark(key)))
        };
        let toggle_row = |id: &'static str, b: &str, s: &str, on: bool, f: fn(&mut p2_overlay_ipc::proto::Look)| {
            let look = look.clone();
            lrow().id(id).relative().cursor_pointer().hover(|st| st.bg(p.bg_hover).border_color(p.border_strong)).child(text(b, Some(s))).child({ let _ = on; lib_toggle(p) }).child(self.mark(format!("lib:{id}"))).on_click(cx.listener(move |this, _, _, cx| {
                let mut l = look.clone();
                f(&mut l);
                this.pending.push(Cmd::Look(l));
                cx.notify();
            }))
        };
        let select_row = |key: &'static str, b: &str, s: &str, list: &[(&str, &str)], value: &str| {
            let label = p2_i18n::t(list.iter().find(|(v, _)| *v == value).map_or(list[0].1, |(_, l)| *l));
            let open = self.select.as_ref().is_some_and(|x| x.key == key);
            let (o, v) = (opts(list), value.to_string());
            lrow().child(text(b, Some(s))).child(div().max_w(px(140.)).child(select_box(key.into(), label, 28., radius::XS, font_size::SM, open, p).child(self.mark(key)).on_click(cx.listener(move |this, _, _, cx| {
                this.open_select(key, o.clone(), v.clone());
                cx.notify();
            }))))
        };
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .p(px(8.))
            .min_h_full()
            .min_h_0()
            .child(div().flex().flex_col().gap(px(6.)).child(h4(&p2_i18n::t("ovr.game"))).child(game).child(actions))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.))
                    .child(h4("Overlay"))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.))
                            .child(slider(&p2_i18n::t("ovr.lib.windowOpacity"), (look.opacity * 100.).round() as i32, 35., 100., "%", "lib:opacity"))
                            .child(select_row("lib:corner", &p2_i18n::t("ovr.lib.toastCorner"), &p2_i18n::t("ovr.lib.toastCornerHint"), &CORNERS, &look.toast_corner))
                            .child(toggle_row("hud", &p2_i18n::t("ovr.lib.hud"), &p2_i18n::t("ovr.lib.hudHint"), look.hud, |l| l.hud = !l.hud))
                            .child(select_row("hud:position", &p2_i18n::t("ovr.hudPosition"), &p2_i18n::t("ovr.lib.hudPositionHint"), &HUD_POSITIONS, &look.hud_position))
                            .child(toggle_row("details", &p2_i18n::t("ovr.lib.details"), &p2_i18n::t("ovr.lib.detailsHint"), look.hud_details, |l| l.hud_details = !l.hud_details))
                            .child(toggle_row("motion", &p2_i18n::t("ovr.reduceMotion"), &p2_i18n::t("ovr.lib.reduceMotionHint"), look.reduce_motion, |l| l.reduce_motion = !l.reduce_motion))
                            .child(slider(&p2_i18n::t("ovr.lib.uiScale"), (look.scale * 100.).round() as i32, 80., 150., "%", "lib:scale")),
                    )
                    .child(div().flex().justify_end().child(btn("lt-all", p2_i18n::t("ovr.lib.allSettings"), Btn::Ghost, p).on_click(cx.listener(|this, _, _, cx| {
                        this.pending.push(Cmd::Action("open-settings:library".into()));
                        cx.notify();
                    })))),
            )
            .into_any_element()
    }

    // ==================================================================== Tarayıcı
    /// Oyun içi tarayıcı (WebView2, `browser.rs`): araç çubuğu + sayfanın canlı görüntüsü. Görünüme
    /// tıklayınca klavye sayfaya gider; adres çubuğunda Enter gezinir.
    fn browser_body(&self, ghost: bool, p: &Palette, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        use crate::browser::BrowserCmd as B;
        let send = |cmd: B| {
            cx.listener(move |this: &mut OverlayRoot, _: &gpui_kit::ClickEvent, _: &mut Window, cx: &mut Context<OverlayRoot>| {
                this.pending.push(Cmd::Browser(cmd.clone()));
                cx.notify();
            })
        };
        let nav = |id: &'static str, body: &'static str, stroke: f32| btn(id, "", Btn::IconGhost, p).child(ic(body, stroke, 16., p.text_2));
        let bar = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .py(px(6.))
            .px(px(8.))
            .border_b_1()
            .border_color(p.border_subtle)
            .bg(p.bg_2)
            .child(nav("br-back", icons::NAV_BACK, 2.).on_click(send(B::Back)))
            .child(nav("br-fwd", icons::NAV_FWD, 2.).on_click(send(B::Forward)))
            .child(btn("br-reload", "", Btn::IconGhost, p).child(icons::composer("refresh", 1.8, 16., p.text_2)).on_click(send(B::Reload)))
            .child(nav("br-home", icons::HOME, 1.9).on_click(cx.listener(|this, _, window, cx| {
                this.ui.url = "https://www.google.com".into();
                this.ui.url_input.update(cx, |s, cx| s.set_value("https://www.google.com", window, cx));
                this.pending.push(Cmd::Browser(B::Navigate(this.ui.url.clone())));
                cx.notify();
            })))
            .child(div().flex_1().min_w_0().h(px(30.)).child(field_input(&self.ui.url_input, p).h(px(30.))))
            .child(btn("br-ext", "", Btn::IconGhost, p).child(ic(icons::OPEN, 1.8, 16., p.text_2)).on_click(cx.listener(move |this, _, _, cx| {
                this.pending.push(Cmd::Action(format!("open-url:{}", this.ui.url)));
                cx.notify();
            })));
        // Görünüm içi nokta (mantıksal piksel = tarayıcı pikseli).
        let point_in = |this: &OverlayRoot, pos: gpui_kit::Point<gpui_kit::Pixels>| -> Option<(i32, i32)> {
            let b = *this.measured.borrow().get("br-view")?;
            Some((f32::from(pos.x - b.origin.x).round() as i32, f32::from(pos.y - b.origin.y).round() as i32))
        };
        use windows::Win32::UI::WindowsAndMessaging::{WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP};
        let mouse = move |this: &mut OverlayRoot, msg: u32, pos: gpui_kit::Point<gpui_kit::Pixels>, wheel: i32| {
            if let Some((x, y)) = point_in(this, pos) {
                this.pending.push(Cmd::Browser(B::Mouse { msg, x, y, wheel }));
            }
        };
        let view = div()
            .id("br-view")
            .relative()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .bg(gpui_kit::white())
            // Pencerenin alt köşeleri (13 px): görüntü köşeden taşmasın.
            .rounded_b(px(13.))
            .child(self.mark("br-view"))
            .children(self.ui.browser_frame.clone().map(|img| gpui_kit::img(gpui_kit::ImageSource::Render(img)).absolute().top_0().left_0().size_full().rounded_b(px(13.)).object_fit(gpui_kit::ObjectFit::Fill)))
            .when(self.ui.browser_frame.is_none(), |d| d.bg(p.bg_0).child(div().absolute().inset_0().flex().items_center().justify_center().child(empty(Some(&p2_i18n::t("ovr.loading")), None, p))))
            // Odak kenarlığı yok: görünümün içine çizilen kenarlık sayfayı 2 px küçültüp bulanıklaştırıyordu.
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &gpui_kit::MouseDownEvent, _, cx| {
                this.ui.browser_focus = true;
                mouse(this, WM_LBUTTONDOWN, ev.position, 0);
                cx.notify();
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(move |this, ev: &gpui_kit::MouseUpEvent, _, _| mouse(this, WM_LBUTTONUP, ev.position, 0)))
            .on_mouse_down(MouseButton::Right, cx.listener(move |this, ev: &gpui_kit::MouseDownEvent, _, cx| {
                this.ui.browser_focus = true;
                mouse(this, WM_RBUTTONDOWN, ev.position, 0);
                cx.notify();
            }))
            .on_mouse_up(MouseButton::Right, cx.listener(move |this, ev: &gpui_kit::MouseUpEvent, _, _| mouse(this, WM_RBUTTONUP, ev.position, 0)))
            .on_mouse_move(cx.listener(move |this, ev: &gpui_kit::MouseMoveEvent, _, _| mouse(this, WM_MOUSEMOVE, ev.position, 0)))
            .on_scroll_wheel(cx.listener(move |this, ev: &gpui_kit::ScrollWheelEvent, _, cx| {
                let dy = match ev.delta {
                    gpui_kit::ScrollDelta::Lines(l) => l.y * 120.,
                    gpui_kit::ScrollDelta::Pixels(px) => f32::from(px.y) * 3.,
                };
                mouse(this, WM_MOUSEWHEEL, ev.position, dy.round() as i32);
                cx.stop_propagation();
            }));
        div().flex().flex_col().size_full().min_h_0().when(!ghost, |d| d.child(bar)).child(view).into_any_element()
    }

    // ==================================================================== Sohbet
    /// Discord düzeni: solda yazılı kanallar, sağda yazara göre gruplu mesajlar ve "#kanal'a mesaj"
    /// kutusu. Panel açıkken 3 sn'de bir yenilenir; Enter gönderir.
    fn chat_body(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let sel = self.st.chat_sel.clone();
        let mut list = div().w(px(168.)).flex_none().h_full().flex().flex_col().gap(px(1.)).py(px(8.)).px(px(6.)).border_r_1().border_color(p.border_subtle).bg(p.bg_1).rounded_bl(px(13.)).child(
            div().px(px(8.)).pb(px(6.)).text_size(px(10.)).font_weight(FontWeight::BOLD).text_color(p.text_4).child(p2_i18n::upper(&p2_i18n::t("ovr.chat.channels"))),
        );
        if self.st.chat_channels.is_empty() {
            list = list.child(div().px(px(8.)).text_size(font_size::SM).text_color(p.text_4).child(p2_i18n::t("ovr.chat.noChannels")));
        }
        for (id, name) in &self.st.chat_channels {
            let on = sel.as_deref() == Some(id.as_str());
            let cid = id.clone();
            let (hover, strong) = (p.accent_soft, p.text_strong);
            list = list.child(
                div()
                    .id(SharedString::from(format!("chat-ch-{id}")))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .h(px(30.))
                    .px(px(8.))
                    .rounded(radius::SM)
                    .cursor_pointer()
                    .text_size(font_size::SM)
                    .text_color(if on { p.text_strong } else { p.text_3 })
                    .when(on, |d| d.bg(p.accent_soft_2))
                    .when(!on, |d| d.hover(move |s| s.bg(hover).text_color(strong)))
                    .child(div().text_color(p.text_4).child("#"))
                    .child(div().flex_1().min_w_0().truncate().child(name.clone()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.pending.push(Cmd::ChatSelect(cid.clone()));
                        cx.notify();
                    })),
            );
        }
        let channel_name = sel.as_ref().and_then(|s| self.st.chat_channels.iter().find(|c| &c.0 == s)).map(|c| c.1.clone()).unwrap_or_default();
        let me = self.st.user_id.clone();
        let mut msgs = div().flex_1().min_h_0().overflow_hidden().flex().flex_col().justify_end().px(px(14.)).pb(px(8.));
        match &self.st.chat {
            Loadable::Ready(lines) if lines.is_empty() => msgs = msgs.child(empty(Some(&p2_i18n::t("ovr.chat.empty")), Some(&p2_i18n::tv("ovr.chat.emptyHint", &[("channel", channel_name.as_str())])), p)),
            Loadable::Ready(lines) => {
                let mut prev: Option<&str> = None;
                for line in lines {
                    let head = prev != Some(line.author_id.as_str());
                    prev = Some(line.author_id.as_str());
                    let mine = line.author_id == me;
                    let text = div().text_size(font_size::SM).line_height(gpui_kit::relative(1.4)).text_color(p.text).child(line.content.clone());
                    msgs = msgs.child(if head {
                        div()
                            .flex()
                            .gap(px(10.))
                            .mt(px(10.))
                            .child(avatar(&line.author_id, &line.author, 32., p).flex_none())
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
                                            .items_baseline()
                                            .gap(px(8.))
                                            .child(div().text_size(font_size::SM).font_weight(FontWeight::BOLD).text_color(if mine { p.text_accent } else { p.text_strong }).child(line.author.clone()))
                                            .child(div().text_size(px(10.5)).text_color(p.text_4).child(time_ago(&line.at))),
                                    )
                                    .child(text),
                            )
                            .into_any_element()
                    } else {
                        div().pl(px(42.)).mt(px(2.)).child(text).into_any_element()
                    });
                }
            }
            Loadable::Failed(e) => msgs = msgs.child(empty(Some(&p2_i18n::t("ovr.chat.failed")), Some(e), p)),
            _ => msgs = msgs.child(empty(Some(&p2_i18n::t("ovr.loading")), None, p)),
        }
        let composer = div().relative().px(px(12.)).pb(px(12.)).child(field_input(&self.ui.chat_input, p).h(px(40.))).child(self.mark("chat:input"));
        div()
            .flex()
            .h_full()
            .min_h_0()
            .child(list)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .h(px(38.))
                            .px(px(14.))
                            .border_b_1()
                            .border_color(p.border_subtle)
                            .child(div().text_color(p.text_4).child("#"))
                            .child(div().font_weight(FontWeight::BOLD).text_color(p.text_strong).truncate().child(channel_name)),
                    )
                    .child(msgs)
                    .when(sel.is_some(), |d| d.child(composer)),
            )
            .into_any_element()
    }
}

use p2_client_core::Member;

fn channel_label(c: &str) -> String {
    match c {
        "" | "live" | "stable" => p2_i18n::t("ovr.channel.stable"),
        "beta" => "Beta".into(),
        "dev" => p2_i18n::t("ovr.channel.dev"),
        other => other.to_string(),
    }
}

fn dot(color: Hsla, ring: Hsla) -> impl IntoElement {
    div().flex_none().size(px(9.)).rounded_full().bg(color).shadow(vec![BoxShadow { color: ring, offset: point(px(0.), px(0.)), blur_radius: px(0.), spread_radius: px(2.), inset: false }])
}

fn status_pill(status: &str, h: f32, pad: f32, size: gpui_kit::Pixels, p: &Palette) -> impl IntoElement {
    let backlog = status == "BACKLOG";
    div()
        .flex_none()
        .h(px(h))
        .px(px(pad))
        .flex()
        .items_center()
        .rounded(radius::PILL)
        .bg(if backlog { p.navy_500 } else { status_color(status, p) })
        .text_color(if backlog { p.text } else { p.text_on_accent })
        .text_size(size)
        .font_weight(FontWeight::BOLD)
        .line_height(px(h))
        .child(status_label(status))
}

fn kind_badge(kind: &str, p: &Palette) -> impl IntoElement {
    let short = KINDS.iter().find(|(k, _, _)| *k == kind).map_or(String::new(), |(_, _, s)| p2_i18n::t(s));
    let bg = match kind {
        "BUG" => p.danger,
        "FEATURE" => p.accent,
        "IMPROVEMENT" => p.warning,
        "DOCS" => p.success,
        _ => p.text_4,
    };
    div().flex_none().h(px(15.)).px(px(5.)).flex().items_center().rounded(radius::PILL).bg(bg).text_color(p.text_on_accent).text_size(font_size::XXS).font_weight(FontWeight::EXTRA_BOLD).child(short.pipe_upper())
}

fn label_chips(labels: &[String], max: usize, p: &Palette) -> impl IntoElement {
    let mut d = div().flex().flex_wrap().gap(px(3.)).min_w_0();
    let chip = |t: String| div().h(px(15.)).max_w(px(96.)).px(px(6.)).flex().items_center().border_1().border_color(p.border).rounded(radius::PILL).bg(p.bg_2).text_color(p.text_2).text_size(font_size::XXS).font_weight(FontWeight::SEMIBOLD).truncate().child(t);
    for l in labels.iter().take(max) {
        d = d.child(chip(l.clone()));
    }
    if labels.len() > max {
        d = d.child(chip(format!("+{}", labels.len() - max)));
    }
    d
}

fn vis_badge(v: &str, p: &Palette) -> impl IntoElement {
    if v == "TEAM" { badge(p2_i18n::t("ovr.notes.team"), BadgeTone::Soft, p) } else { badge(p2_i18n::t("ovr.notes.private"), BadgeTone::Muted, p) }.h(px(16.)).px(px(6.)).text_size(font_size::XXS)
}

/// `.p2-input` (h 32–36, p 0 10, r 8, bg-3, 500 fs-md); odakta vurgu kenarlığı + halka.
pub fn field_input(state: &Entity<InputState>, p: &Palette) -> Input {
    Input::new(state)
        .appearance(false)
        .bordered(false)
        .focus_bordered(false)
        .h(px(34.))
        .px(px(10.))
        .rounded(radius::SM)
        .bg(p.bg_3)
        .border_1()
        .border_color(p.border)
        .text_size(font_size::MD)
        .font_weight(FontWeight::MEDIUM)
        .text_color(p.text)
}

#[allow(dead_code)]
fn _keys(p: &Palette) -> impl IntoElement {
    keys_row(&[], p)
}

/// Ayarlar penceresindeki anahtar: LibraryWindow.tsx'te `<span class="p2-toggle">` `.lt-row > span` kuralıyla
/// `flex: 1; display: grid` alır (satırın yarısına yayılır) ve `aria-checked` taşımadığından hep kapalı çizilir.
/// Electron'daki görünüm birebir kopyalanır (ayar durumu satıra tıklayınca değişir).
fn lib_toggle(p: &Palette) -> impl IntoElement {
    div()
        .relative()
        .flex_1()
        .min_w_0()
        .h(px(20.))
        .rounded(radius::PILL)
        .bg(p.navy_500)
        .child(div().absolute().top(px(2.)).left(px(2.)).size(px(16.)).rounded_full().bg(gpui_kit::white()).shadow(vec![BoxShadow { color: crate::ui::rgba(0, 0, 0, 0.4), offset: point(px(0.), px(1.)), blur_radius: px(3.), spread_radius: px(0.), inset: false }]))
}

impl OverlayRoot {
    /// `.ov-post` listesi (en çok 5; uzun gövde 66 px'te kırpılır).
    fn hub_posts(&self, p: &Palette) -> AnyElement {
        let posts = self.st.hub.as_ref().map(|h| h.2.clone()).unwrap_or_default();
        if posts.is_empty() {
            return div().py(px(8.)).px(px(10.)).rounded(radius::SM).bg(p.bg_2).text_color(p.text_4).text_size(font_size::XS).child(p2_i18n::t(if self.st.hub.is_none() { "ovr.loading" } else { "ovr.summary.noPosts" })).into_any_element();
        }
        let mut list = div().flex().flex_col().gap(px(6.));
        for post in posts {
            let long = post.content.chars().count() > 220 || post.content.lines().count() > 4;
            // MessageRichText: **kalın** işaretleri düz metne indirgenir.
            let text = post.content.replace("**", "");
            list = list.child(
                div()
                    .flex()
                    .gap(px(8.))
                    .py(px(8.))
                    .px(px(10.))
                    .rounded(radius::SM)
                    .bg(p.bg_2)
                    .border_1()
                    .border_color(p.border_subtle)
                    .child(avatar(&post.author_id, &post.author, 28., p))
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
                                    .gap(px(6.))
                                    .min_w_0()
                                    .text_size(font_size::XS)
                                    .text_color(p.text_3)
                                    .child(div().truncate().text_size(font_size::SM).font_weight(FontWeight::BOLD).text_color(p.text_strong).child(post.author.clone()))
                                    .when(post.release, |d| {
                                        d.child(div().h(px(16.)).px(px(7.)).flex().items_center().rounded(radius::PILL).border_1().border_color(p.success_border).bg(p.success_soft).text_size(font_size::XXS).font_weight(FontWeight::BOLD).text_color(p.success_text).child(div().mr(px(5.)).size(px(6.)).rounded_full().bg(p.success)).child(p2_i18n::t("ovr.summary.releaseBadge")))
                                    })
                                    .child(div().ml_auto().whitespace_nowrap().child(time_ago(&post.created_at))),
                            )
                            .child(div().text_size(font_size::SM).line_height(gpui_kit::relative(1.45)).text_color(p.text).when(long, |d| d.max_h(px(66.)).overflow_hidden()).child(text)),
                    ),
            );
        }
        list.into_any_element()
    }
}
