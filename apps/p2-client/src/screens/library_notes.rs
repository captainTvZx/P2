//! Proje notları paneli (ProjectNotesPanel.tsx + project-notes.css, geniş düzen): solda liste
//! (sabitliler önce), sağda başlık + editör. Kayıt otomatik (800 ms); çakışmada (409) sunucudaki
//! sürüm yüklenir ve satır içi uyarı gösterilir.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::Focusable as _;
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, Entity, FontWeight, SharedString, Subscription, Task, Window, div, px, relative};
use p2_client_core::{Api, ProjectNote};
use p2_ui::P2Text as _;
use p2_ui::controls::{BadgeTone, BtnSize, BtnVariant, badge, button, empty};
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::{Palette, font_size, radius, shadows};
use p2_ui::{Entrance, P2Motion as _};

use super::editor::{self, NoteBox};
use super::ui::{PLUS, ico, note_time_ago};
use crate::task;

pub const NOTE_CONFLICT_TEXT: &str = "Not başka bir cihazda değişti, yeniden yüklendi";
const PIN: &str = r#"<path d="M8.5 3.5h7l-1 6.5 3.5 3.5v2h-6v5.5l-.5 1-.5-1V15.5h-6v-2l3.5-3.5z"/>"#;
const BACK: &str = r#"<path d="M19 12H5M11 6l-6 6 6 6"/>"#;
const TRASH: &str = r#"<path d="M4 7h16M9.5 7V4h5v3M6.5 7l1 13.5h9l1-13.5M10 11v6M14 11v6"/>"#;

fn title_of(n: &ProjectNote) -> String {
    let t = n.title.trim();
    if t.is_empty() { "Adsız not".into() } else { t.to_string() }
}

/// notesModel.ts `sortNotes`: sabitliler önce, sonra son güncellenen.
fn sort(notes: &mut [ProjectNote]) {
    notes.sort_by(|a, b| b.pinned.cmp(&a.pinned).then_with(|| b.updated_at.cmp(&a.updated_at)));
}

pub struct NotesPanel {
    api: Arc<Api>,
    org: String,
    project: String,
    me: String,
    manage: bool,
    notes: Vec<ProjectNote>,
    loading: bool,
    selected: Option<String>,
    title: Entity<InputState>,
    body: NoteBox,
    /// Editöre yüklenmiş not (değişiklik olaylarını kayda çevirmek için).
    bound: Option<String>,
    saving: HashSet<String>,
    pending: Option<Task<()>>,
    hint: Option<(String, bool)>,
    confirm_delete: bool,
    /// Dar düzen (< 520 px): liste ↔ editör arasında geçiş.
    narrow: bool,
    show_list: bool,
    _subs: Vec<Subscription>,
}

impl NotesPanel {
    pub fn new(api: Arc<Api>, org: String, project: String, me: String, manage: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("Başlık"));
        let body = NoteBox::new("Notunu yaz… (biçim, emoji, görsel ve dosya eklenebilir)", false, window, cx);
        let subs = vec![
            cx.subscribe_in(&title, window, |this, _, e: &InputEvent, window, cx| {
                if matches!(e, InputEvent::Change) {
                    this.edited(window, cx);
                }
            }),
            cx.subscribe_in(&body.text, window, |this, _, e: &InputEvent, window, cx| {
                if matches!(e, InputEvent::Change) {
                    this.edited(window, cx);
                }
            }),
        ];
        let mut panel = Self { api, org, project, me, manage, notes: Vec::new(), loading: true, selected: None, title, body, bound: None, saving: HashSet::new(), pending: None, hint: None, confirm_delete: false, narrow: false, show_list: true, _subs: subs };
        panel.narrow = Self::measure(window) < 520.;
        panel.reload(window, cx);
        panel
    }

    fn reload(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.project.clone());
        task::run(window, cx, move || api.notes(&org, &project), |this, result, window, cx| {
            this.loading = false;
            match result {
                Ok(mut list) => {
                    sort(&mut list);
                    crate::trace(&format!("kütüphane: {} not", list.len()));
                    this.notes = list;
                    if this.selected.as_ref().is_none_or(|id| !this.notes.iter().any(|n| &n.id == id)) {
                        // Dar düzende ilk not otomatik seçilmez (ProjectNotesPanel.tsx).
                        this.selected = if this.narrow { None } else { this.notes.first().map(|n| n.id.clone()) };
                    }
                    this.bind(window, cx);
                }
                Err(e) => this.show_hint(if e.message.is_empty() { "Notlar yüklenemedi.".into() } else { e.message }, true, window, cx),
            }
            cx.notify();
        });
    }

    fn selected(&self) -> Option<&ProjectNote> {
        self.selected.as_ref().and_then(|id| self.notes.iter().find(|n| &n.id == id))
    }

    fn editable(&self, n: &ProjectNote) -> bool {
        n.author_id == self.me || (n.visibility == "TEAM" && self.manage)
    }

    /// Seçili notu editöre yükler (yalnız seçim değişince).
    fn bind(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(n) = self.selected().cloned() else {
            self.bound = None;
            return;
        };
        if self.bound.as_deref() == Some(n.id.as_str()) {
            return;
        }
        self.bound = None;
        self.title.update(cx, |s, cx| s.set_value(n.title.clone(), window, cx));
        self.body.set(&n.body, window, cx);
        self.bound = Some(n.id.clone());
        self.confirm_delete = false;
    }

    fn show_hint(&mut self, text: String, danger: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.hint = Some((text.clone(), danger));
        cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(6)).await;
            let _ = this.update(cx, |this, cx| {
                if this.hint.as_ref().is_some_and(|h| h.0 == text) {
                    this.hint = None;
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Başlık/gövde değişti: iyimser güncelle, 800 ms sonra kaydet.
    fn edited(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.bound.clone() else { return };
        let title = self.title.read(cx).value().to_string();
        let body = self.body.value(cx);
        let Some(n) = self.notes.iter_mut().find(|n| n.id == id) else { return };
        if n.title == title && n.body == body {
            return;
        }
        n.title = title;
        n.body = body;
        self.pending = Some(cx.spawn_in(window, async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(800)).await;
            let _ = this.update_in(cx, |this, window, cx| this.flush(id, window, cx));
        }));
        cx.notify();
    }

    fn patch(&mut self, id: String, patch: serde_json::Value, window: &mut Window, cx: &mut Context<Self>) {
        let Some(version) = self.notes.iter().find(|n| n.id == id).map(|n| n.version) else { return };
        let mut patch = patch;
        patch["version"] = version.into();
        self.saving.insert(id.clone());
        cx.notify();
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.project.clone());
        task::run(window, cx, move || api.update_note(&org, &project, &id, patch).map_err(|e| (id, e)), |this, result, window, cx| {
            match result {
                Ok(saved) => {
                    this.saving.remove(&saved.id);
                    if let Some(n) = this.notes.iter_mut().find(|n| n.id == saved.id) {
                        // Yanıt gelene kadar yazılanlar korunur; sürüm ve görünürlük sunucudan.
                        n.version = saved.version;
                        n.visibility = saved.visibility.clone();
                        n.pinned = saved.pinned;
                        n.updated_at = saved.updated_at.clone();
                    }
                    sort(&mut this.notes);
                    crate::trace("kütüphane: not kaydedildi");
                }
                Err((id, e)) => {
                    this.saving.remove(&id);
                    if e.status == 409 {
                        this.bound = None;
                        this.show_hint(NOTE_CONFLICT_TEXT.into(), false, window, cx);
                        this.reload(window, cx);
                    } else {
                        this.show_hint(if e.message.is_empty() { "Not kaydedilemedi.".into() } else { e.message }, true, window, cx);
                    }
                }
            }
            cx.notify();
        });
    }

    fn flush(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(n) = self.notes.iter().find(|n| n.id == id) else { return };
        let patch = serde_json::json!({ "title": n.title, "body": n.body });
        self.patch(id, patch, window, cx);
    }

    /// Panel genişliği: içerik − sütun boşlukları − sağ sütun (320) − panel iç boşluğu.
    fn measure(window: &Window) -> f32 {
        let side = crate::resize::value(&crate::resize::LIBRARY_SIDE);
        f32::from(window.viewport_size().width) - 72. - side - 10. - 56. - 18. - 320. - 36.
    }

    fn create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.project.clone());
        task::run(window, cx, move || api.create_note(&org, &project, "", "", false), |this, result, window, cx| match result {
            Ok(n) => {
                this.selected = Some(n.id.clone());
                this.show_list = false;
                this.notes.insert(0, n);
                sort(&mut this.notes);
                this.bind(window, cx);
                this.title.update(cx, |s, cx| s.focus(window, cx));
                cx.notify();
            }
            Err(e) => this.show_hint(if e.message.is_empty() { "Not oluşturulamadı.".into() } else { e.message }, true, window, cx),
        });
    }

    fn remove(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.project.clone());
        self.notes.retain(|n| n.id != id);
        self.selected = self.notes.first().map(|n| n.id.clone());
        self.bound = None;
        self.bind(window, cx);
        task::run(window, cx, move || api.delete_note(&org, &project, &id), |this, result, window, cx| {
            if let Err(e) = result {
                this.show_hint(if e.message.is_empty() { "Not silinemedi.".into() } else { e.message }, true, window, cx);
                this.reload(window, cx);
            }
        });
        cx.notify();
    }

    fn vis_badge(v: &str, p: &Palette) -> gpui_kit::Div {
        if v == "TEAM" { badge("Ekip", BadgeTone::Soft, p) } else { badge("Özel", BadgeTone::Muted, p) }
    }
}

impl Render for NotesPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        let p = &p;
        let selected = self.selected().cloned();
        let editable = selected.as_ref().is_some_and(|n| self.editable(n));
        let saving = selected.as_ref().is_some_and(|n| self.saving.contains(&n.id));
        let icon_btn = |id: &'static str, body: &'static str, on: bool| {
            button(id, BtnVariant::Ghost, BtnSize::Sm, true, false, p).px(px(0.)).when(on, |s| s.bg(p.accent_soft).text_color(p.text_accent)).child(ico(body, 2., 17., if on { p.text_accent } else { p.text_2 }))
        };
        let mut toolbar = div().flex().items_center().gap(px(6.)).flex_wrap().min_h(px(30.)).child(
            button("pn-new", BtnVariant::Primary, BtnSize::Sm, false, false, p).child(ico(PLUS, 2., 15., p.text_on_accent)).child("Yeni not").on_click(cx.listener(|this, _, window, cx| this.create(window, cx))),
        );
        self.narrow = Self::measure(window) < 520.;
        let narrow = self.narrow;
        let list_pane = narrow && (self.show_list || selected.is_none());
        if narrow && selected.is_some() && !self.show_list {
            toolbar = div().flex().items_center().gap(px(6.)).flex_wrap().min_h(px(30.))
                .child(icon_btn("pn-back", BACK, false).tooltip(|w, cx| p2_ui::controls::tooltip("Listeye dön", w, cx)).on_click(cx.listener(|this, _, _, cx| {
                    this.show_list = true;
                    cx.notify();
                })))
                .child(button("pn-new", BtnVariant::Primary, BtnSize::Sm, false, false, p).child(ico(PLUS, 2., 15., p.text_on_accent)).child("Yeni not").on_click(cx.listener(|this, _, window, cx| this.create(window, cx))));
        }
        if let (Some(n), true, false) = (&selected, editable, list_pane) {
            let tab = |id: &'static str, label: &'static str, v: &'static str| {
                let active = n.visibility == v;
                let (hover_bg, text) = (p.bg_hover, p.text);
                let nid = n.id.clone();
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
                    .tooltip(move |w, cx| p2_ui::controls::tooltip(if v == "TEAM" { "Projedeki herkes görür" } else { "Yalnız sen görürsün" }, w, cx))
                    .child(label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(n) = this.notes.iter_mut().find(|n| n.id == nid) {
                            n.visibility = v.into();
                        }
                        this.patch(nid.clone(), serde_json::json!({ "visibility": v }), window, cx);
                    }))
            };
            let (pid, did, pinned) = (n.id.clone(), n.id.clone(), n.pinned);
            toolbar = toolbar
                .child(div().flex().gap(px(4.)).p(px(3.)).rounded(radius::MD).bg(p.bg_2).border_1().border_color(p.border_subtle).child(tab("pn-private", "Özel", "PRIVATE")).child(tab("pn-team", "Ekip", "TEAM")))
                .child(
                    icon_btn("pn-pin", PIN, pinned)
                        .tooltip(move |w, cx| p2_ui::controls::tooltip(if pinned { "Sabitlemeyi kaldır" } else { "Sabitle (listede üstte kalır; overlay sabitli görünümde gösterilir)" }, w, cx))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if let Some(n) = this.notes.iter_mut().find(|n| n.id == pid) {
                                n.pinned = !pinned;
                            }
                            sort(&mut this.notes);
                            this.patch(pid.clone(), serde_json::json!({ "pinned": !pinned }), window, cx);
                        })),
                )
                .child(icon_btn("pn-trash", TRASH, false).tooltip(|w, cx| p2_ui::controls::tooltip("Notu sil", w, cx)).on_click(cx.listener(move |this, _, _, cx| {
                    let _ = &did;
                    this.confirm_delete = true;
                    cx.notify();
                })));
        }
        toolbar = toolbar.child(div().flex_1());
        if selected.is_some() {
            toolbar = toolbar.child(div().text_size(font_size::XS).whitespace_nowrap().text_color(if saving { p.text_3 } else { p.text_4 }).child(if saving { "Kaydediliyor…" } else { "Kaydedildi" }));
        }
        let hint_bar = |_text: String, danger: bool| {
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .py(px(6.))
                .px(px(10.))
                .border_1()
                .border_color(if danger { p.danger_border } else { p.border })
                .rounded(radius::SM)
                .bg(if danger { p.danger_soft } else { p.warning_soft })
                .text_color(if danger { p.danger_text } else { p.warning_text })
                .text_size(font_size::XS)
        };
        let hint = self.hint.clone().map(|(t, danger)| {
            hint_bar(t.clone(), danger)
                .child(t)
                .child(div().flex_1())
                .child(button("pn-hint-ok", BtnVariant::Ghost, BtnSize::Sm, false, false, p).child("Tamam").on_click(cx.listener(|this, _, _, cx| {
                    this.hint = None;
                    cx.notify();
                })))
                .p2_enter("pn-hint", Entrance::Rise, Duration::ZERO)
        });
        let confirm = (self.confirm_delete).then_some(selected.clone()).flatten().map(|n| {
            let id = n.id.clone();
            hint_bar(String::new(), true)
                .child(format!("\"{}\" silinecek.", title_of(&n)))
                .child(div().flex_1())
                .child(button("pn-del-no", BtnVariant::Ghost, BtnSize::Sm, false, false, p).child("Vazgeç").on_click(cx.listener(|this, _, _, cx| {
                    this.confirm_delete = false;
                    cx.notify();
                })))
                .child(button("pn-del-yes", BtnVariant::Danger, BtnSize::Sm, false, false, p).child("Sil").on_click(cx.listener(move |this, _, window, cx| {
                    this.confirm_delete = false;
                    this.remove(id.clone(), window, cx);
                })))
        });
        // Liste
        let mut list = div().id("pn-list").w(px(220.)).flex_shrink_0().h_full().overflow_y_scroll().pr(px(2.)).flex().flex_col().gap(px(2.));
        if self.loading && self.notes.is_empty() {
            list = list.child(empty("", Some("Notlar yükleniyor…".into()), p).py(px(18.)).px(px(10.)));
        } else if self.notes.is_empty() {
            list = list.child(empty("Henüz not yok", Some("\"Yeni not\" ile başla. Özel notları yalnız sen görürsün; Ekip notları projedeki herkese açıktır.".into()), p).py(px(18.)).px(px(10.)).text_center());
        }
        for n in &self.notes {
            let active = self.selected.as_deref() == Some(n.id.as_str());
            let id = n.id.clone();
            let untitled = n.title.trim().is_empty();
            let hover_bg = p.bg_hover;
            let who = if n.visibility == "TEAM" && n.author_id != self.me { format!(" · {}", n.author.display_name) } else { String::new() };
            list = list.child(
                div()
                    .id(SharedString::from(format!("pn-item-{}", n.id)))
                    .w_full()
                    .py(px(7.))
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .border_1()
                    .rounded(radius::SM)
                    .cursor_pointer()
                    .map(|s| if active { s.bg(p.bg_selected).border_color(p.border_accent) } else { s.border_color(gpui_kit::transparent_black()).hover(move |s| s.bg(hover_bg)) })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.selected = Some(id.clone());
                        this.show_list = false;
                        this.bind(window, cx);
                        cx.notify();
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(div().overflow_hidden().whitespace_nowrap().text_ellipsis().text_size(font_size::SM).map(|s| if untitled { s.text_color(p.text_3).font_weight(FontWeight::MEDIUM).italic() } else { s.text_color(p.text_strong).css_weight(650.) }).child(title_of(n)))
                            .child(div().overflow_hidden().whitespace_nowrap().text_ellipsis().text_size(font_size::XXS).text_color(p.text_3).child(format!("{}{who}", note_time_ago(&n.updated_at)))),
                    )
                    .child(div().flex().flex_col().items_end().gap(px(3.)).when(n.pinned, |s| s.child(div().text_color(p.text_accent).child(ico(PIN, 2., 12., p.text_accent)))).child(Self::vis_badge(&n.visibility, p).h(p2_ui::text::scaled(px(16.))).text_size(font_size::XXS))),
            );
        }
        // Editör
        let editor_col: AnyElement = match &selected {
            None => empty("Not seç", Some("Soldaki listeden bir not seç ya da yeni bir not oluştur.".into()), p).into_any_element(),
            Some(n) if editable => {
                let focused = self.title.read(cx).focus_handle(cx).is_focused(window);
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .child(
                        div()
                            .h(px(32.))
                            .px(px(12.))
                            .flex()
                            .items_center()
                            .border_1()
                            .border_color(if focused { p.border_accent } else { p.border })
                            .when(focused, |s| s.shadow(shadows(&[p.ring])))
                            .rounded(radius::SM)
                            .bg(p.bg_3)
                            .text_size(font_size::MD)
                            .css_weight(650.)
                            .text_color(p.text)
                            .child(Input::new(&self.title).appearance(false).text_size(font_size::MD)),
                    )
                    .child(div().flex_1().min_h_0().child(editor::note_box("pn-body", &self.body, self.api.clone(), |v: &mut Self| Some(&mut v.body), None, false, p, cx)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .text_size(font_size::XXS)
                            .text_color(p.text_4)
                            .child(Self::vis_badge(&n.visibility, p))
                            .child(if n.author_id == self.me { div().child("Senin notun") } else { div().flex().gap(px(4.)).child("Yazan:").child(div().text_color(p.text_3).font_weight(FontWeight::SEMIBOLD).child(n.author.display_name.clone())) })
                            .child(format!("· {}", note_time_ago(&n.updated_at))),
                    )
                    .into_any_element()
            }
            Some(n) => div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(
                    div()
                        .id("pn-read")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .py(px(10.))
                        .px(px(12.))
                        .border_1()
                        .border_color(p.border)
                        .rounded(radius::MD)
                        .bg(p.bg_2)
                        .child(div().mb(px(8.)).text_size(font_size::MD).text_color(p.text_strong).font_weight(FontWeight::BOLD).child(title_of(n)))
                        .child(div().text_size(font_size::SM).line_height(relative(1.5)).child(p2_ui::rich::render_message(&n.body, p))),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .text_size(font_size::XXS)
                        .text_color(p.text_4)
                        .child(Self::vis_badge(&n.visibility, p))
                        .child(div().flex().gap(px(4.)).child("Yazan:").child(div().text_color(p.text_3).font_weight(FontWeight::SEMIBOLD).child(n.author.display_name.clone())))
                        .child(format!("· {}", note_time_ago(&n.updated_at)))
                        .child("· salt okunur"),
                )
                .into_any_element(),
        };
        // `.library-notes-tab .pn { height: clamp(360px, 60vh, 720px) }`
        let vh = f32::from(window.viewport_size().height);
        div()
            .h(px((vh * 0.6).clamp(360., 720.)))
            .flex()
            .flex_col()
            .gap(px(8.))
            .text_color(p.text)
            .child(toolbar)
            .children(hint)
            .children(confirm)
            .child(div().flex_1().min_h_0().flex().gap(px(10.)).map(|s| {
                if !narrow {
                    s.child(list).child(div().flex_1().min_w_0().min_h_0().flex().flex_col().gap(px(8.)).child(editor_col))
                } else if list_pane {
                    s.child(list.w_full().flex_1())
                } else {
                    s.child(div().flex_1().min_w_0().min_h_0().flex().flex_col().gap(px(8.)).child(editor_col))
                }
            }))
    }
}
