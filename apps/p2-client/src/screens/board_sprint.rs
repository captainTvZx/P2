//! Sprint penceresi — BoardPage.tsx `SprintDialog`: liste (durum, hedef, aralık, görev sayısı, kalan gün;
//! Görev aktar / Düzenle / Başlat / Erken bitir / Sil), form (ad, hedef, `RangeCalendar` iki tıkla aralık +
//! 1 hafta / 2 hafta / 1 ay), backlog'dan aktarma (`BacklogPicker`) ve sprint'i bitirme (`FinishSprint`).

use chrono::{Datelike as _, NaiveDate};

use super::*;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Screen {
    List,
    Form,
    Transfer,
    Finish,
}

pub(super) struct SprintUi {
    pub screen: Screen,
    editing: Option<p2_client_core::Sprint>,
    name: Entity<InputState>,
    goal: Entity<InputState>,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    month: NaiveDate,
    target: Option<p2_client_core::Sprint>,
    picked: Vec<String>,
    search: Entity<InputState>,
    finishing: Option<p2_client_core::Sprint>,
    finish_target: String,
    new_name: Entity<InputState>,
}

fn first_of(d: NaiveDate) -> NaiveDate {
    d.with_day(1).unwrap_or(d)
}

fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

fn add_months(d: NaiveDate, n: i32) -> NaiveDate {
    let m = d.year() * 12 + d.month0() as i32 + n;
    NaiveDate::from_ymd_opt(m.div_euclid(12), m.rem_euclid(12) as u32 + 1, 1).unwrap_or(d)
}

impl SprintUi {
    pub fn new(window: &mut Window, cx: &mut Context<BoardPage>) -> Self {
        Self {
            screen: Screen::List,
            editing: None,
            name: text_input(window, cx, "Sprint 1"),
            goal: text_input(window, cx, "Bu sprint’te ne bitecek?"),
            from: None,
            to: None,
            month: first_of(today()),
            target: None,
            picked: Vec::new(),
            search: text_input(window, cx, "Backlog’da ara"),
            finishing: None,
            finish_target: "backlog".into(),
            new_name: text_input(window, cx, ""),
        }
    }

    pub fn reset(&mut self, screen: Screen, editing: Option<p2_client_core::Sprint>, window: &mut Window, cx: &mut Context<BoardPage>) {
        self.screen = screen;
        let (name, goal) = editing.as_ref().map(|s| (s.name.clone(), s.goal.clone().unwrap_or_default())).unwrap_or_default();
        self.name.update(cx, |i, cx| i.set_value(name, window, cx));
        self.goal.update(cx, |i, cx| i.set_value(goal, window, cx));
        self.from = editing.as_ref().and_then(|s| s.starts_at.as_deref()).and_then(local).map(|d| d.date_naive());
        self.to = editing.as_ref().and_then(|s| s.ends_at.as_deref()).and_then(local).map(|d| d.date_naive());
        self.month = first_of(self.from.unwrap_or_else(today));
        self.editing = editing;
        if screen == Screen::Form {
            self.name.update(cx, |i, cx| i.focus(window, cx));
        }
    }
}

impl BoardPage {
    fn sprint_save(&mut self, id: Option<String>, body: Value, note: Option<String>, back: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ctx) = self.ctx() else { return };
        self.busy = true;
        cx.notify();
        task::run(
            window,
            cx,
            move || match &id {
                Some(id) => ctx.api.update_sprint(&ctx.org, &ctx.project, &ctx.board, id, body),
                None => ctx.api.create_sprint(&ctx.org, &ctx.project, &ctx.board, body),
            },
            move |this, result, window, cx| {
                this.busy = false;
                match result {
                    Ok(s) => {
                        crate::trace(&format!("pano: sprint kaydedildi «{}»", s.name));
                        if let Some(n) = &note {
                            Self::toast(n.clone());
                        }
                        if back {
                            this.sprint_ui.editing = None;
                            this.sprint_ui.screen = Screen::List;
                        }
                        this.reload(window, cx);
                    }
                    Err(e) => this.fail(e, cx),
                }
                cx.notify();
            },
        );
    }

    pub(super) fn delete_sprint(&mut self, s: p2_client_core::Sprint, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ctx) = self.ctx() else { return };
        let id = s.id.clone();
        task::run(window, cx, move || ctx.api.delete_sprint(&ctx.org, &ctx.project, &ctx.board, &id), move |this, result, window, cx| match result {
            Ok(()) => {
                Self::toast(format!("“{}” silindi, görevleri backlog’a döndü.", s.name));
                this.reload(window, cx);
            }
            Err(e) => this.fail(e, cx),
        });
    }

    fn save_sprint_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = value(&self.sprint_ui.name, cx).trim().to_string();
        if name.chars().count() < 2 {
            return;
        }
        let goal = value(&self.sprint_ui.goal, cx).trim().to_string();
        let mut body = json!({ "name": name, "goal": goal });
        // Gün başı ve gün sonu (yerel saat): sprint seçilen günlerin tamamını kapsar.
        if let Some(d) = self.sprint_ui.from.and_then(|d| d.and_hms_opt(0, 0, 0)).and_then(|d| d.and_local_timezone(chrono::Local).single()) {
            body["startsAt"] = json!(d.with_timezone(&chrono::Utc).to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
        }
        if let Some(d) = self.sprint_ui.to.and_then(|d| d.and_hms_opt(23, 59, 59)).and_then(|d| d.and_local_timezone(chrono::Local).single()) {
            body["endsAt"] = json!(d.with_timezone(&chrono::Utc).to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
        }
        let editing = self.sprint_ui.editing.as_ref().map(|s| s.id.clone());
        let note = if editing.is_some() { format!("“{name}” güncellendi.") } else { format!("“{name}” oluşturuldu.") };
        self.sprint_save(editing, body, Some(note), true, window, cx);
    }

    fn transfer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(ctx), Some(target)) = (self.ctx(), self.sprint_ui.target.clone()) else { return };
        let ids = self.sprint_ui.picked.clone();
        if ids.is_empty() {
            return;
        }
        self.busy = true;
        let sid = target.id.clone();
        task::run(
            window,
            cx,
            move || ids.iter().map(|id| ctx.api.update_task(&ctx.org, &ctx.project, &ctx.board, id, json!({ "sprintId": sid }))).collect::<Result<Vec<_>, _>>(),
            move |this, result, _, cx| {
                this.busy = false;
                match result {
                    Ok(tasks) => {
                        Self::toast(format!("{} görev “{}” sprint’ine alındı.", tasks.len(), target.name));
                        for t in tasks {
                            this.upsert(t);
                        }
                        this.sprint_ui.target = None;
                        this.sprint_ui.screen = Screen::List;
                    }
                    Err(e) => this.fail(e, cx),
                }
                cx.notify();
            },
        );
    }

    fn finish(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(ctx), Some(sprint)) = (self.ctx(), self.sprint_ui.finishing.clone()) else { return };
        let target = self.sprint_ui.finish_target.clone();
        let new_name = value(&self.sprint_ui.new_name, cx).trim().to_string();
        if target == "new" && new_name.chars().count() < 2 {
            return;
        }
        let unfinished: Vec<String> = self.content.as_ref().map(|c| c.tasks.iter().filter(|t| t.sprint_id.as_deref() == Some(sprint.id.as_str()) && t.status != "DONE").map(|t| t.id.clone()).collect()).unwrap_or_default();
        self.busy = true;
        let sid = sprint.id.clone();
        task::run(
            window,
            cx,
            move || -> Result<(), ApiError> {
                let to: Value = match target.as_str() {
                    "backlog" => Value::Null,
                    "new" => json!(ctx.api.create_sprint(&ctx.org, &ctx.project, &ctx.board, json!({ "name": new_name }))?.id),
                    id => json!(id),
                };
                for id in &unfinished {
                    ctx.api.update_task(&ctx.org, &ctx.project, &ctx.board, id, json!({ "sprintId": to }))?;
                }
                ctx.api.update_sprint(&ctx.org, &ctx.project, &ctx.board, &sid, json!({ "status": "DONE", "endsAt": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true) }))?;
                Ok(())
            },
            move |this, result, window, cx| {
                this.busy = false;
                match result {
                    Ok(()) => {
                        Self::toast(format!("“{}” bitirildi; tamamlanmamış görevler aktarıldı.", sprint.name));
                        this.sprint_ui.finishing = None;
                        this.sprint_ui.screen = Screen::List;
                        this.reload(window, cx);
                    }
                    Err(e) => this.fail(e, cx),
                }
                cx.notify();
            },
        );
    }

    /// `RangeCalendar`: pazartesiden başlayan ay ızgarası, iki tıkla aralık.
    fn calendar(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let ui = &self.sprint_ui;
        let (from, to, month) = (ui.from, ui.to, ui.month);
        let (hb, soft, strong) = (p.bg_hover, p.accent_soft, p.text_strong);
        let nav = |id: &'static str, glyph: &'static str, delta: i32| {
            div()
                .id(id)
                .w(px(28.))
                .h(px(26.))
                .flex()
                .items_center()
                .justify_center()
                .border_1()
                .border_color(p.border)
                .rounded(px(7.))
                .bg(p.bg_hover)
                .text_size(fs(14.))
                .text_color(p.text)
                .cursor_pointer()
                .hover(move |s| s.bg(soft).text_color(strong))
                .child(glyph)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.sprint_ui.month = add_months(this.sprint_ui.month, delta);
                    cx.notify();
                }))
        };
        let header = div().flex().items_center().child(nav("cal-prev", "‹", -1)).child(div().flex_1().flex().justify_center().text_size(fs(12.)).font_weight(gpui_kit::FontWeight::BOLD).text_color(p.text_strong).child(month_year(month))).child(nav("cal-next", "›", 1));
        let mut week = div().flex().gap(px(3.));
        for d in ["Pt", "Sa", "Ça", "Pe", "Cu", "Ct", "Pz"] {
            week = week.child(div().flex_1().flex().justify_center().text_size(fs(9.5)).font_weight(gpui_kit::FontWeight::BOLD).text_color(p.text_accent).child(d));
        }
        let shift = month.weekday().num_days_from_monday() as i32;
        let days = add_months(month, 1).pred_opt().map(|d| d.day()).unwrap_or(30) as i32;
        let cells = ((shift + days + 6) / 7) * 7;
        let today = today();
        let mut grid = div().flex().flex_col().gap(px(3.));
        let mut row = div().flex().gap(px(3.));
        for i in 0..cells {
            let day = i - shift + 1;
            let cell: AnyElement = if day < 1 || day > days {
                div().flex_1().h(px(30.)).into_any_element()
            } else {
                let key = month.with_day(day as u32).unwrap_or(month);
                let picked = Some(key) == from || Some(key) == to;
                let between = matches!((from, to), (Some(a), Some(b)) if key > a && key < b);
                let start = Some(key) == from && to.is_some();
                let end = Some(key) == to;
                let r = radius::SM;
                div()
                    .id(SharedString::from(format!("cal-{key}")))
                    .flex_1()
                    .h(px(30.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_1()
                    .border_color(if key == today { p.border_strong } else { gpui_kit::transparent_black() })
                    .text_size(fs(11.5))
                    .when(key == today, |s| s.css_weight(750.))
                    .map(|s| {
                        if picked {
                            s.bg(p.accent).text_color(p.text_on_accent).css_weight(750.)
                        } else if between {
                            s.bg(p.accent_soft).text_color(p.text_strong)
                        } else {
                            s.text_color(p.text).hover(move |s| s.bg(hb))
                        }
                    })
                    .map(|s| match (start, end, between) {
                        (_, _, true) => s,
                        (true, true, _) => s.rounded(r),
                        (true, false, _) => s.rounded_l(r),
                        (false, true, _) => s.rounded_r(r),
                        _ => s.rounded(r),
                    })
                    .cursor_pointer()
                    .child(day.to_string())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let ui = &mut this.sprint_ui;
                        // İlk tık başlangıç, ikinci tık bitiş (geriye tıklanırsa yeniden başlar).
                        match (ui.from, ui.to) {
                            (Some(a), None) if key >= a => ui.to = Some(key),
                            _ => {
                                ui.from = Some(key);
                                ui.to = None;
                            }
                        }
                        cx.notify();
                    }))
                    .into_any_element()
            };
            row = row.child(cell);
            if i % 7 == 6 {
                grid = grid.child(row);
                row = div().flex().gap(px(3.));
            }
        }
        let quick = |id: &'static str, label: &'static str, n: i64| {
            div()
                .id(id)
                .h(px(24.))
                .px(px(9.))
                .flex()
                .items_center()
                .border_1()
                .border_color(p.border)
                .rounded(px(99.))
                .bg(p.bg_hover)
                .text_size(fs(10.5))
                .css_weight(650.)
                .text_color(p.text)
                .cursor_pointer()
                .hover(move |s| s.bg(soft).text_color(strong))
                .child(label)
                .on_click(cx.listener(move |this, _, _, cx| {
                    let start = this.sprint_ui.from.unwrap_or(today);
                    this.sprint_ui.from = Some(start);
                    this.sprint_ui.to = start.checked_add_days(chrono::Days::new((n - 1) as u64));
                    cx.notify();
                }))
        };
        let summary = match (from, to) {
            (Some(a), Some(b)) => format!("{} → {} · {} gün", day_month(a), day_month(b), (b - a).num_days() + 1),
            (Some(a), None) => format!("{} → bitişi seç", day_month(a)),
            _ => "Başlangıç gününe tıkla".into(),
        };
        div()
            .flex()
            .flex_col()
            .gap(px(7.))
            .p(px(10.))
            .border_1()
            .border_color(p.border)
            .rounded(radius::MD)
            .bg(p.bg_3)
            .child(header)
            .child(week)
            .child(grid)
            .child(
                div()
                    .pt(px(3.))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(6.))
                    .border_t_1()
                    .border_color(p.border_subtle)
                    .child(quick("cal-1w", "1 hafta", 7))
                    .child(quick("cal-2w", "2 hafta", 14))
                    .child(quick("cal-1m", "1 ay", 30))
                    .child(div().flex_1())
                    .child(div().text_size(fs(10.5)).css_weight(650.).text_color(p.text_3).child(summary)),
            )
            .into_any_element()
    }

    pub(super) fn sprint_dialog(&self, p: &Palette, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let ui = &self.sprint_ui;
        let sprints = self.content.as_ref().map(|c| c.sprints.clone()).unwrap_or_default();
        let tasks = self.content.as_ref().map(|c| c.tasks.clone()).unwrap_or_default();
        let backlog: Vec<Task> = tasks.iter().filter(|t| t.sprint_id.is_none()).cloned().collect();
        let (title, hint) = match ui.screen {
            Screen::List => ("Sprint’ler".to_string(), "Aynı anda tek sprint aktif olur; yeni sprint başlayınca diğeri planlanana döner."),
            Screen::Form => (if ui.editing.is_some() { "Sprint’i düzenle".into() } else { "Yeni sprint".into() }, "Ad, hedef ve takvimden tarih aralığı."),
            Screen::Finish => ("Sprint’i bitir".into(), "Bitmemiş görevlerin nereye taşınacağını seç."),
            Screen::Transfer => (format!("Backlog’dan aktar{}", ui.target.as_ref().map(|s| format!(" · {}", s.name)).unwrap_or_default()), "Sprint’e alınacak görevleri seç."),
        };
        let (layer, card) = Self::share_modal("sprint-dialog", 620., title, Some(hint.into()), p, window);
        let close = |this: &mut Self, _: &gpui_kit::ClickEvent, _: &mut Window, cx: &mut Context<Self>| {
            this.sprints_open = false;
            cx.notify();
        };
        let mut card = card.child(div().absolute().top(px(16.)).right(px(18.)).child(Self::close_x("sp-x", p).on_click(cx.listener(close))));
        let footer = || div().flex().items_center().justify_end().gap(px(8.));
        match ui.screen {
            Screen::List => {
                let mut list = div().id("sprint-list").max_h(px(240.)).overflow_y_scroll().flex().flex_col().gap(px(7.));
                for s in &sprints {
                    let count = tasks.iter().filter(|t| t.sprint_id.as_deref() == Some(s.id.as_str())).count();
                    let range = match (&s.starts_at, &s.ends_at) {
                        (Some(a), Some(b)) => format!("{} – {}", day_text(a), day_text(b)),
                        (None, Some(b)) => format!("bitiş {}", day_text(b)),
                        _ => "tarih yok".into(),
                    };
                    let left = (s.status == "ACTIVE").then(|| sprint_progress(s.starts_at.as_deref(), s.ends_at.as_deref())).flatten().map(|(_, l)| if l > 0 { format!(" · {l} gün kaldı") } else { " · süre doldu".into() }).unwrap_or_default();
                    let active = s.status == "ACTIVE";
                    let mut actions = div().flex_none().flex().gap(px(6.));
                    let small = |id: String, label: &'static str, kind: Btn, disabled: bool| sized_button(SharedString::from(id), label, kind, disabled, 26., 9., fs(10.5), p);
                    if s.status != "DONE" {
                        let target = s.clone();
                        let none = backlog.is_empty();
                        actions = actions.child(small(format!("sp-move-{}", s.id), "Görev aktar", Btn::Plain, self.busy || none).when(!none, |b| {
                            b.on_click(cx.listener(move |this, _, _, cx| {
                                this.sprint_ui.target = Some(target.clone());
                                this.sprint_ui.picked.clear();
                                this.sprint_ui.screen = Screen::Transfer;
                                cx.notify();
                            }))
                        }));
                    }
                    let edit = s.clone();
                    actions = actions.child(small(format!("sp-edit-{}", s.id), "Düzenle", Btn::Plain, self.busy).on_click(cx.listener(move |this, _, window, cx| {
                        this.sprint_ui.reset(Screen::Form, Some(edit.clone()), window, cx);
                        cx.notify();
                    })));
                    if s.status == "PLANNED" {
                        let (id, name) = (s.id.clone(), s.name.clone());
                        actions = actions.child(small(format!("sp-start-{}", s.id), "Başlat", Btn::Plain, self.busy).on_click(cx.listener(move |this, _, window, cx| this.sprint_save(Some(id.clone()), json!({ "status": "ACTIVE" }), Some(format!("“{name}” başlatıldı.")), false, window, cx))));
                    }
                    if active {
                        let fin = s.clone();
                        actions = actions.child(small(format!("sp-finish-{}", s.id), "Erken bitir", Btn::Plain, self.busy).on_click(cx.listener(move |this, _, window, cx| {
                            let name = format!("{} devamı", fin.name);
                            this.sprint_ui.new_name.update(cx, |i, cx| i.set_value(name, window, cx));
                            this.sprint_ui.finishing = Some(fin.clone());
                            this.sprint_ui.finish_target = "backlog".into();
                            this.sprint_ui.screen = Screen::Finish;
                            cx.notify();
                        })));
                    }
                    let del = s.clone();
                    actions = actions.child(small(format!("sp-del-{}", s.id), "Sil", Btn::Danger, self.busy).on_click(cx.listener(move |this, _, _, cx| {
                        this.confirm = Some(Confirm::Sprint(del.clone()));
                        cx.notify();
                    })));
                    list = list.child(
                        div()
                            .py(px(9.))
                            .px(px(11.))
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .border_1()
                            .rounded(radius::MD)
                            .map(|d| if active { d.border_color(p.border_accent).bg(p.accent_soft) } else { d.border_color(p.border).bg(p.bg_3) })
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .flex_col()
                                    .child(div().text_size(fs(12.)).font_weight(gpui_kit::FontWeight::BOLD).text_color(p.text_strong).child(s.name.clone()))
                                    .child(div().truncate().text_size(fs(10.5)).text_color(p.text_3).child(s.goal.clone().filter(|g| !g.is_empty()).unwrap_or_else(|| "Hedef yazılmadı".into())))
                                    .child(div().mt(px(2.)).text_size(fs(10.)).text_color(p.text_3).child(format!("{range} · {count} görev{left}"))),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(fs(10.))
                                    .map(|d| match s.status.as_str() {
                                        "ACTIVE" => d.text_color(p.text_accent).font_weight(gpui_kit::FontWeight::BOLD),
                                        "DONE" => d.text_color(p.text_3).opacity(0.7),
                                        _ => d.text_color(p.text_3),
                                    })
                                    .child(match s.status.as_str() {
                                        "ACTIVE" => "Aktif",
                                        "DONE" => "Bitti",
                                        _ => "Planlandı",
                                    }),
                            )
                            .child(actions),
                    );
                }
                if sprints.is_empty() {
                    list = list.child(div().py(px(14.)).px(px(8.)).flex().justify_center().text_size(fs(11.)).text_color(p.text_3).child("Henüz sprint yok."));
                }
                card = card.child(list).child(
                    footer().child(board_button("sp-close", "Kapat", Btn::Plain, false, p).on_click(cx.listener(close))).child(board_button("sp-new", "＋ Yeni sprint", Btn::Primary, false, p).on_click(cx.listener(|this, _, window, cx| {
                        this.sprint_ui.reset(Screen::Form, None, window, cx);
                        cx.notify();
                    }))),
                );
            }
            Screen::Form => {
                let ok = value(&ui.name, cx).trim().chars().count() >= 2;
                let first = sprints.is_empty();
                card = card
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(10.))
                            .child(Self::field("Sprint adı", Self::field_input(&ui.name, p, window, cx), p))
                            .child(Self::field("Hedef", Self::field_input(&ui.goal, p, window, cx), p))
                            .child(self.calendar(p, cx)),
                    )
                    .child(
                        footer()
                            .child(board_button("sf-cancel", "Vazgeç", Btn::Plain, false, p).on_click(cx.listener(move |this, _, _, cx| {
                                this.sprint_ui.editing = None;
                                // İlk sprint henüz yoksa dönülecek liste yok: pencere kapanır.
                                if first {
                                    this.sprints_open = false;
                                } else {
                                    this.sprint_ui.screen = Screen::List;
                                }
                                cx.notify();
                            })))
                            .child(board_button("sf-save", if ui.editing.is_some() { "Kaydet" } else { "Sprint oluştur" }, Btn::Primary, self.busy || !ok, p).when(ok, |b| b.on_click(cx.listener(|this, _, window, cx| this.save_sprint_form(window, cx))))),
                    );
            }
            Screen::Transfer => {
                let needle = value(&ui.search, cx).trim().to_lowercase();
                let list_items: Vec<&Task> = backlog.iter().filter(|t| needle.is_empty() || format!("{} {}", t.number, t.title).to_lowercase().contains(&needle)).collect();
                let all = !list_items.is_empty() && ui.picked.len() == list_items.len();
                let ids: Vec<String> = list_items.iter().map(|t| t.id.clone()).collect();
                let focused = gpui_kit::Focusable::focus_handle(ui.search.read(cx), cx).is_focused(window);
                let search = div()
                    .flex_1()
                    .h(px(32.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .border_1()
                    .rounded(radius::SM)
                    .bg(p.bg_3)
                    .map(|s| if focused { s.border_color(p.border_accent).shadow(shadows(&[p.ring])) } else { s.border_color(p.border) })
                    .child(div().w(px(22.)).flex_none().text_color(p.text_4).child("⌕"))
                    .child(div().flex_1().child(Input::new(&ui.search).appearance(false).bordered(false).focus_bordered(false).text_size(fs(12.)).text_color(p.text).px_0().h(p2_ui::text::scaled(px(16.)))));
                let mut list = div().id("backlog-list").max_h(px(320.)).overflow_y_scroll().pr(px(2.)).flex().flex_col().gap(px(5.));
                for t in &list_items {
                    let on = ui.picked.contains(&t.id);
                    let id = t.id.clone();
                    let (bs, hb) = (p.border_strong, p.bg_hover);
                    list = list.child(
                        div()
                            .id(SharedString::from(format!("bl-{}", t.id)))
                            .py(px(8.))
                            .px(px(10.))
                            .flex()
                            .items_center()
                            .gap(px(9.))
                            .border_1()
                            .rounded(radius::MD)
                            .cursor_pointer()
                            .map(|d| if on { d.border_color(p.border_accent).bg(p.accent_soft) } else { d.border_color(p.border).bg(p.bg_3).hover(move |s| s.border_color(bs).bg(hb)) })
                            .child(div().w(px(18.)).flex_none().child(p2_ui::form::native_checkbox(on, p)))
                            .child(div().w(px(42.)).flex_none().child(mono(format!("#{}", t.number), 10., p.text_3)))
                            .child(div().flex_1().min_w_0().truncate().text_size(fs(12.)).text_color(p.text).child(t.title.clone()))
                            .child(priority_chip(&t.priority, p))
                            .when_some(t.points, |s, pts| s.child(div().text_size(fs(10.5)).text_color(p.text_3).child(format!("{pts}p"))))
                            .child(div().w(px(20.)).flex_none().child(board_avatar(self.member(t.assignee_id.as_deref()), 20., p)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                let picked = &mut this.sprint_ui.picked;
                                if let Some(i) = picked.iter().position(|x| *x == id) {
                                    picked.remove(i);
                                } else {
                                    picked.push(id.clone());
                                }
                                cx.notify();
                            })),
                    );
                }
                if list_items.is_empty() {
                    list = list.child(div().py(px(14.)).px(px(8.)).flex().justify_center().text_size(fs(11.)).text_color(p.text_3).child(if backlog.is_empty() { "Backlog boş." } else { "Eşleşen görev yok." }));
                }
                let points: i64 = ui.picked.iter().filter_map(|id| backlog.iter().find(|t| &t.id == id)?.points).sum();
                let target = ui.target.as_ref().map(|s| s.name.clone()).unwrap_or_default();
                let none = ui.picked.is_empty();
                card = card
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(10.))
                            .child(
                                div().flex().items_center().gap(px(8.)).child(search).child(board_button("bl-all", if all { "Seçimi kaldır" } else { "Tümünü seç" }, Btn::Plain, false, p).on_click(cx.listener(move |this, _, _, cx| {
                                    this.sprint_ui.picked = if all { Vec::new() } else { ids.clone() };
                                    cx.notify();
                                }))),
                            )
                            .child(list),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(div().text_size(fs(11.5)).css_weight(650.).text_color(p.text_3).child(format!("{} görev{}", ui.picked.len(), if points > 0 { format!(" · {points} puan") } else { String::new() })))
                            .child(div().flex_1())
                            .child(board_button("bl-cancel", "Vazgeç", Btn::Plain, false, p).on_click(cx.listener(|this, _, _, cx| {
                                this.sprint_ui.target = None;
                                this.sprint_ui.screen = Screen::List;
                                cx.notify();
                            })))
                            .child(board_button("bl-move", format!("“{target}” sprint’ine al"), Btn::Primary, self.busy || none, p).when(!none, |b| b.on_click(cx.listener(|this, _, window, cx| this.transfer(window, cx))))),
                    );
            }
            Screen::Finish => {
                let Some(sprint) = ui.finishing.clone() else { return div().into_any_element() };
                let unfinished = tasks.iter().filter(|t| t.sprint_id.as_deref() == Some(sprint.id.as_str()) && t.status != "DONE").count();
                let opts: Vec<(String, String)> = std::iter::once(("backlog".to_string(), "Backlog’a taşı".to_string()))
                    .chain(sprints.iter().filter(|s| s.id != sprint.id && s.status != "DONE").map(|s| (s.id.clone(), format!("{}{}", s.name, if s.status == "ACTIVE" { " · aktif" } else { "" }))))
                    .chain(std::iter::once(("new".to_string(), "Yeni sprint oluştur ve taşı".to_string())))
                    .collect();
                let new_ok = ui.finish_target != "new" || value(&ui.new_name, cx).trim().chars().count() >= 2;
                let mut body = div().flex().flex_col().gap(px(12.)).child(
                    div()
                        .p(px(12.))
                        .border_1()
                        .border_color(p.border)
                        .rounded(radius::MD)
                        .bg(p.bg_3)
                        .child(div().text_size(fs(13.)).font_weight(gpui_kit::FontWeight::BOLD).text_color(p.text_strong).child(sprint.name.clone()))
                        .child(div().mt(px(3.)).text_size(fs(11.)).font_weight(gpui_kit::FontWeight::BOLD).text_color(p.text_accent).child(format!("{unfinished} tamamlanmamış görev")))
                        .child(div().mt(px(8.)).text_size(fs(11.)).line_height(gpui_kit::relative(1.45)).text_color(p.text_3).child("Tamamlanan görevler bu sprint’in geçmişinde kalır. Diğer görevler seçtiğin hedefe taşınır.")),
                );
                if unfinished > 0 {
                    let cur = ui.finish_target.clone();
                    body = body.child(Self::field(
                        "Tamamlanmamış görevler",
                        self.wide_select_plain("finish-target", &opts, &cur, p, cx, |this, v, _, cx| {
                            this.sprint_ui.finish_target = v;
                            cx.notify();
                        }),
                        p,
                    ));
                    if ui.finish_target == "new" {
                        body = body.child(Self::field("Yeni sprint adı", Self::field_input(&ui.new_name, p, window, cx), p));
                    }
                }
                card = card.child(body).child(
                    footer()
                        .child(board_button("fs-cancel", "Vazgeç", Btn::Plain, false, p).on_click(cx.listener(|this, _, _, cx| {
                            this.sprint_ui.finishing = None;
                            this.sprint_ui.screen = Screen::List;
                            cx.notify();
                        })))
                        .child(board_button("fs-ok", "Sprint’i bitir", Btn::Primary, self.busy || !new_ok, p).when(new_ok, |b| b.on_click(cx.listener(|this, _, window, cx| this.finish(window, cx))))),
                );
            }
        }
        layer
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.sprints_open = false;
                    cx.notify();
                }),
            )
            .child(card.p2_enter("sprint-dialog-card", Entrance::PopElastic, Duration::ZERO))
            .into_any_element()
    }

    /// Pencere içindeki tam genişlik `select` (`.board-field select`: p 8 10, `12px`).
    fn wide_select_plain(&self, id: &'static str, options: &[(String, String)], current: &str, p: &Palette, cx: &mut Context<Self>, pick: impl Fn(&mut Self, String, &mut Window, &mut Context<Self>) + 'static) -> AnyElement {
        let labels: Vec<String> = options.iter().map(|o| o.1.clone()).collect();
        let selected = options.iter().position(|o| o.0 == current).unwrap_or(0);
        // Seçim döngüsü: pencerede açılır liste yerine her tıkta sonraki seçenek (Chromium listesinin
        // yerini tutar; değer kümesi küçük).
        let values: Vec<String> = options.iter().map(|o| o.0.clone()).collect();
        native_select(SharedString::from(format!("sel-{id}")), labels.get(selected).cloned().unwrap_or_default(), 0., 32., 10., fs(12.), 400., false, p)
            .w_full()
            .on_click(cx.listener(move |this, _, window, cx| {
                let next = values.get((selected + 1) % values.len().max(1)).cloned().unwrap_or_default();
                pick(this, next, window, cx);
            }))
            .into_any_element()
    }
}
