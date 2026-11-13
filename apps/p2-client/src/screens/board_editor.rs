//! Görev oluştur / düzenle penceresi — BoardPage.tsx `TaskDialog` (`.share-modal.board-dialog`, 560px):
//! başlık, açıklama (`.board-editor`: biçim araçları + etiketle), Atanan / Öncelik / Tür / Etiketler /
//! Sütun / Sprint / Puan / Bitiş (3 sütunlu `.board-grid`), altta Sil · Vazgeç · Oluştur/Kaydet.

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, Entity, EventEmitter, MouseButton, SharedString, Subscription, Window, div, px};
use p2_client_core::{Api, BOARD_COLUMNS, Member, Sprint, TASK_KINDS, TASK_PRIORITIES, Task, kind_label, parse_labels, priority_label};
use p2_ui::P2Fx as _; // backdrop_blur
use p2_ui::P2Text as _;
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::{Palette, font_size, fs, radius, shadows};
use p2_ui::{Entrance, P2Motion as _};
use serde_json::{Value, json};

use super::board::ui::{Btn, board_button, column_label, native_select, select_list};
use super::text_input;
use crate::task;

pub enum EditorEvent {
    /// Kaydedilen görev; `true` yeni oluşturuldu.
    Saved(Task, bool),
    Delete(Task),
    Cancelled,
}

/// Panonun bağlamı: hangi pano, kimler, hangi sprint'ler.
#[derive(Clone)]
pub struct BoardRef {
    pub api: Arc<Api>,
    pub org: String,
    pub project: String,
    pub board: String,
}

#[derive(Clone, Copy, PartialEq)]
enum Pick {
    Assignee,
    Priority,
    Kind,
    Column,
    Sprint,
}

pub struct TaskEditor {
    ctx: BoardRef,
    editing: Option<Task>,
    members: Vec<Member>,
    sprints: Vec<Sprint>,
    can_delete: bool,
    title: Entity<InputState>,
    description: Entity<TextareaState>,
    points: Entity<InputState>,
    due: Entity<InputState>,
    labels: Entity<InputState>,
    status: String,
    priority: String,
    kind: String,
    assignee: Option<String>,
    sprint: Option<String>,
    open: Option<Pick>,
    busy: bool,
    error: Option<String>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<EditorEvent> for TaskEditor {}

/// "2026-11-20T12:00:00.000Z" → "2026-11-20"
pub fn date_part(iso: &str) -> String {
    iso.chars().take(10).collect()
}

/// "2026-11-20" (ya da "20.11.2026") → ISO; geçersizse None. Gün kaymasın diye öğlen UTC.
pub fn parse_day(text: &str) -> Option<String> {
    let t = text.trim();
    let (y, m, d) = if let Some((d, rest)) = t.split_once('.') {
        let (m, y) = rest.split_once('.')?;
        (y.parse::<u32>().ok()?, m.parse::<u32>().ok()?, d.parse::<u32>().ok()?)
    } else {
        let mut it = t.split('-');
        (it.next()?.parse().ok()?, it.next()?.parse().ok()?, it.next()?.parse().ok()?)
    };
    chrono::NaiveDate::from_ymd_opt(y as i32, m, d).map(|_| format!("{y:04}-{m:02}-{d:02}T12:00:00.000Z"))
}

impl TaskEditor {
    #[allow(clippy::too_many_arguments)]
    pub fn new(ctx: BoardRef, editing: Option<Task>, status: &str, sprint: Option<String>, members: Vec<Member>, sprints: Vec<Sprint>, can_delete: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let title = text_input(window, cx, "Ne yapılacak?");
        let description = cx.new(|cx| TextareaState::new(window, cx).auto_grow(1, 14).placeholder("Ayrıntılar, kabul kriterleri… Dosya ve görselleri buraya sürükleyebilirsin."));
        let points = text_input(window, cx, "—");
        let due = text_input(window, cx, "gg.aa.yyyy");
        let labels = text_input(window, cx, "ui, ses, seviye-3");
        let mut subs = Vec::new();
        for i in [&title, &points, &due, &labels] {
            subs.push(cx.subscribe_in(i, window, |this, state, ev: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = ev {
                    if *state == this.title {
                        this.save(window, cx);
                    }
                }
                cx.notify();
            }));
        }
        let mut s = Self {
            ctx,
            editing: None,
            members,
            sprints,
            can_delete,
            title,
            description,
            points,
            due,
            labels,
            status: status.to_string(),
            priority: "MEDIUM".into(),
            kind: "TASK".into(),
            assignee: None,
            sprint,
            open: None,
            busy: false,
            error: None,
            _subs: subs,
        };
        if let Some(t) = editing {
            let due = t.due_at.as_deref().map(date_part).and_then(|d| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok()).map(|d| d.format("%d.%m.%Y").to_string()).unwrap_or_default();
            let (ti, de, po, la) = (t.title.clone(), t.description.clone().unwrap_or_default(), t.points.map(|p| p.to_string()).unwrap_or_default(), t.label_list().join(", "));
            s.title.update(cx, |i, cx| i.set_value(ti, window, cx));
            s.description.update(cx, |i, cx| i.set_value(de, window, cx));
            s.points.update(cx, |i, cx| i.set_value(po, window, cx));
            s.due.update(cx, |i, cx| i.set_value(due, window, cx));
            s.labels.update(cx, |i, cx| i.set_value(la, window, cx));
            s.status = t.column().to_string();
            s.priority = t.priority.clone();
            s.kind = t.kind().to_string();
            s.assignee = t.assignee_id.clone();
            s.sprint = t.sprint_id.clone();
            s.editing = Some(t);
        }
        s.title.update(cx, |i, cx| i.focus(window, cx));
        s
    }

    /// Otomatik pilot: başlık ver ve kaydet.
    pub fn autopilot(&mut self, title: &str, window: &mut Window, cx: &mut Context<Self>) {
        let title = title.to_string();
        self.title.update(cx, |i, cx| i.set_value(title, window, cx));
        self.save(window, cx);
    }

    fn fail(&mut self, message: impl Into<String>, cx: &mut Context<Self>) {
        self.error = Some(message.into());
        cx.notify();
    }

    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let title = self.title.read(cx).value().trim().to_string();
        if title.chars().count() < 2 {
            return;
        }
        let points_text: String = self.points.read(cx).value().chars().filter(|c| c.is_ascii_digit()).take(3).collect();
        let due_text = self.due.read(cx).value().to_string();
        let due = if due_text.trim().is_empty() {
            None
        } else {
            match parse_day(&due_text) {
                Some(d) => Some(d),
                None => return self.fail("Bitiş tarihi gg.aa.yyyy biçiminde olmalı.", cx),
            }
        };
        let body = json!({
            "title": title,
            "description": self.description.read(cx).value().to_string(),
            "assigneeId": self.assignee.clone().unwrap_or_default(),
            "priority": self.priority,
            "status": self.status,
            "kind": self.kind,
            "labels": parse_labels(&self.labels.read(cx).value()),
            "points": if points_text.is_empty() { Value::Null } else { json!(points_text.parse::<i64>().unwrap_or(0)) },
            "dueAt": due.map(Value::from).unwrap_or(Value::Null),
            "sprintId": self.sprint.clone().map(Value::from).unwrap_or(Value::Null),
        });
        self.busy = true;
        self.error = None;
        cx.notify();
        let ctx = self.ctx.clone();
        let editing = self.editing.clone();
        task::run(
            window,
            cx,
            move || match &editing {
                Some(t) => ctx.api.update_task(&ctx.org, &ctx.project, &ctx.board, &t.id, body).map(|t| (t, false)),
                None => ctx.api.create_task(&ctx.org, &ctx.project, &ctx.board, body).map(|t| (t, true)),
            },
            |this, result, _, cx| {
                this.busy = false;
                match result {
                    Ok((task, created)) => cx.emit(EditorEvent::Saved(task, created)),
                    Err(e) => this.fail(if e.message.is_empty() { "İşlem tamamlanamadı.".to_string() } else { e.message }, cx),
                }
                cx.notify();
            },
        );
    }

    fn options(&self, which: Pick) -> (Vec<(String, String)>, String) {
        match which {
            Pick::Assignee => (std::iter::once((String::new(), "Atanmadı".to_string())).chain(self.members.iter().map(|m| (m.id.clone(), m.display_name.clone()))).collect(), self.assignee.clone().unwrap_or_default()),
            Pick::Priority => (TASK_PRIORITIES.iter().map(|s| (s.to_string(), priority_label(s).to_string())).collect(), self.priority.clone()),
            Pick::Kind => (TASK_KINDS.iter().map(|s| (s.to_string(), kind_label(s).to_string())).collect(), self.kind.clone()),
            Pick::Column => (BOARD_COLUMNS.iter().map(|s| (s.to_string(), column_label(s).to_string())).collect(), self.status.clone()),
            Pick::Sprint => (std::iter::once((String::new(), "Backlog".to_string())).chain(self.sprints.iter().map(|s| (s.id.clone(), s.name.clone()))).collect(), self.sprint.clone().unwrap_or_default()),
        }
    }

    fn set(&mut self, which: Pick, v: String) {
        match which {
            Pick::Assignee => self.assignee = (!v.is_empty()).then_some(v),
            Pick::Priority => self.priority = v,
            Pick::Kind => self.kind = v,
            Pick::Column => self.status = v,
            Pick::Sprint => self.sprint = (!v.is_empty()).then_some(v),
        }
    }

    /// `.board-field select` (p 8 10, `12px`, r 8, bg-3) + açılır liste.
    fn select(&self, which: Pick, id: &'static str, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let (opts, cur) = self.options(which);
        let labels: Vec<String> = opts.iter().map(|o| o.1.clone()).collect();
        let selected = opts.iter().position(|o| o.0 == cur).unwrap_or(0);
        let open = self.open == Some(which);
        let field = native_select(SharedString::from(format!("te-{id}")), labels.get(selected).cloned().unwrap_or_default(), 0., 32., 10., fs(12.), 400., open, p)
            .w_full()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, _, cx| {
                this.open = if this.open == Some(which) { None } else { Some(which) };
                cx.notify();
            }));
        let mut wrap = div().relative().w_full().child(field);
        if open {
            let (list, rows) = select_list(id, &labels, selected, 160., p);
            let mut list = list.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
            for (row, (v, _)) in rows.into_iter().zip(opts.into_iter()) {
                list = list.child(row.on_click(cx.listener(move |this, _, _, cx| {
                    this.open = None;
                    this.set(which, v.clone());
                    cx.notify();
                })));
            }
            wrap = wrap.child(div().absolute().top(px(34.)).left_0().child(gpui_kit::deferred(gpui_kit::anchored().child(list)).with_priority(5)));
        }
        wrap.into_any_element()
    }

    fn input_box(state: &Entity<InputState>, p: &Palette, window: &Window, cx: &gpui_kit::App) -> AnyElement {
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
            .child(Input::new(state).appearance(false).bordered(false).focus_bordered(false).text_size(fs(12.)).text_color(p.text).px_0().h(px(18.)))
            .into_any_element()
    }

    fn wrap(&mut self, marker: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        self.description.update(cx, |s, cx| {
            let value = s.value().to_string();
            let range = s.selected_range();
            let (a, b) = (range.start.min(value.len()), range.end.min(value.len()));
            s.set_value(format!("{}{marker}{}{marker}{}", &value[..a], &value[a..b], &value[b..]), window, cx);
            let end = a + marker.len() + (b - a);
            s.set_selected_range(end..end, cx);
            s.focus(window, cx);
        });
    }

    fn insert(&mut self, text: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        self.description.update(cx, |s, cx| {
            s.insert(text, window, cx);
            s.focus(window, cx);
        });
    }
}

fn field(label: &str, body: impl IntoElement, p: &Palette) -> gpui_kit::Div {
    div().flex().flex_col().gap(px(4.)).min_w_0().child(div().text_size(font_size::SM).css_weight(600.).text_color(p.text_3).child(label.to_string())).child(body)
}

fn tool(id: &'static str, icon: &'static str, tip: &'static str, p: &Palette) -> gpui_kit::Stateful<gpui_kit::Div> {
    let (soft, strong) = (p.accent_soft, p.text_strong);
    div()
        .id(id)
        .flex_none()
        .size(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(7.))
        .text_color(p.text_3)
        .cursor_pointer()
        .hover(move |s| s.bg(soft).text_color(strong))
        .child(p2_ui::icons::icon(icon).stroke(1.9).el().size(px(15.)).text_color(p.text_3))
        .tooltip(move |window, cx| p2_ui::controls::tooltip(tip, window, cx))
}

impl Render for TaskEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        let vw = f32::from(window.viewport_size().width);
        let vh = f32::from(window.viewport_size().height);
        let ok = self.title.read(cx).value().trim().chars().count() >= 2;
        let desc_focus = gpui_kit::Focusable::focus_handle(self.description.read(cx), cx).is_focused(window);
        let (title, hint) = match &self.editing {
            Some(t) => (format!("#{} görevi düzenle", t.number), "Değişiklikler herkese anında yansır."),
            None => ("Yeni görev".to_string(), "Panoya yeni bir kart ekler."),
        };
        let (soft, strong) = (p.accent_soft, p.text_strong);
        let header = div()
            .flex()
            .items_start()
            .justify_between()
            .gap(px(10.))
            .child(div().flex().flex_col().child(div().text_size(fs(14.)).font_weight(gpui_kit::FontWeight::BOLD).text_color(p.text_strong).child(title)).child(div().mt(px(2.)).text_size(fs(10.5)).text_color(p.text_3).child(hint)))
            .child(
                div()
                    .id("te-x")
                    .flex_none()
                    .size(px(30.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(radius::XS)
                    .text_size(fs(19.))
                    .text_color(p.text_3)
                    .cursor_pointer()
                    .hover(move |s| s.bg(soft).text_color(strong))
                    .child("×")
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(EditorEvent::Cancelled))),
            );
        let editor = div()
            .relative()
            .flex()
            .flex_col()
            .gap(px(8.))
            .p(px(8.))
            .border_1()
            .rounded(radius::MD)
            .bg(p.bg_3)
            .map(|s| if desc_focus { s.border_color(p.border_accent).shadow(shadows(&[p.ring])) } else { s.border_color(p.border) })
            .child(div().id("te-desc").min_h(px(96.)).max_h(px(240.)).overflow_y_scroll().py(px(4.)).px(px(2.)).text_size(fs(12.)).line_height(gpui_kit::relative(1.6)).text_color(p.text).child(Textarea::new(&self.description).appearance(false).p(px(0.)).text_size(fs(12.)).line_height(px(16.32)).text_color(p.text)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(tool("te-bold", "composer:bold", "Kalın (Ctrl+B)", &p).on_click(cx.listener(|this, _, window, cx| this.wrap("**", window, cx))))
                    .child(tool("te-italic", "composer:italic", "İtalik (Ctrl+I)", &p).on_click(cx.listener(|this, _, window, cx| this.wrap("_", window, cx))))
                    .child(tool("te-link", "composer:link", "Bağlantı ekle", &p).on_click(cx.listener(|this, _, window, cx| this.insert("[bağlantı](https://)", window, cx))))
                    .child(tool("te-mention", "composer:mention", "Birini etiketle", &p).on_click(cx.listener(|this, _, window, cx| this.insert("@", window, cx))))
                    .child(div().flex_1())
                    .child(tool("te-attach", "composer:plus", "Dosya ekle", &p)),
            );
        // `.board-grid`: auto-fit minmax(150px,1fr) → 560px pencerede 3 sütun.
        let cell = |el: AnyElement| div().w(px(167.3)).flex_none().child(el);
        let grid = div()
            .flex()
            .flex_wrap()
            .gap(px(10.))
            .child(cell(field("Atanan", self.select(Pick::Assignee, "assignee", &p, cx), &p).into_any_element()))
            .child(cell(field("Öncelik", self.select(Pick::Priority, "priority", &p, cx), &p).into_any_element()))
            .child(cell(field("Tür", self.select(Pick::Kind, "kind", &p, cx), &p).into_any_element()))
            .child(cell(field("Etiketler", Self::input_box(&self.labels, &p, window, cx), &p).into_any_element()))
            .child(cell(field("Sütun", self.select(Pick::Column, "column", &p, cx), &p).into_any_element()))
            .child(cell(field("Sprint", self.select(Pick::Sprint, "sprint", &p, cx), &p).into_any_element()))
            .child(cell(field("Puan", Self::input_box(&self.points, &p, window, cx), &p).into_any_element()))
            .child(cell(field("Bitiş", Self::input_box(&self.due, &p, window, cx), &p).into_any_element()));
        let editing = self.editing.clone();
        let footer = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .when_some(editing.filter(|_| self.can_delete), |s, t| s.child(board_button("te-del", "Sil", Btn::Danger, self.busy, &p).on_click(cx.listener(move |_, _, _, cx| cx.emit(EditorEvent::Delete(t.clone()))))))
            .child(div().flex_1())
            .child(board_button("te-cancel", "Vazgeç", Btn::Plain, false, &p).on_click(cx.listener(|_, _, _, cx| cx.emit(EditorEvent::Cancelled))))
            .child(board_button("te-save", if self.editing.is_some() { "Kaydet" } else { "Oluştur" }, Btn::Primary, self.busy || !ok, &p).when(ok, |s| s.on_click(cx.listener(|this, _, window, cx| this.save(window, cx)))));
        let card = div()
            .id("task-editor")
            .relative()
            .w(px(560f32.min(vw - 48.)))
            .max_h(px(vh - 90.))
            .overflow_y_scroll()
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
            .occlude()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    if this.open.take().is_some() {
                        cx.notify();
                    }
                }),
            )
            .child(header)
            .child(field("Başlık", Self::input_box(&self.title, &p, window, cx), &p))
            .child(field("Açıklama", editor, &p))
            .child(grid)
            .when_some(self.error.clone(), |s, e| s.child(div().text_size(fs(11.)).text_color(p.danger_text).child(e)))
            .child(footer)
            .p2_enter("task-editor", Entrance::PopElastic, Duration::ZERO);
        div()
            .id("task-editor-layer")
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
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| cx.emit(EditorEvent::Cancelled)))
            .child(card)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days() {
        assert_eq!(parse_day("2026-11-20").as_deref(), Some("2026-11-20T12:00:00.000Z"));
        assert_eq!(parse_day("20.11.2026").as_deref(), Some("2026-11-20T12:00:00.000Z"));
        assert_eq!(parse_day("2026-02-30"), None);
        assert_eq!(date_part("2026-11-20T12:00:00.000Z"), "2026-11-20");
    }
}
