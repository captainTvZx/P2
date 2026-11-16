//! Grup oluşturma penceresi — GroupCreator.tsx: kişileri seç, istersen bir ad ver.
//! Ad boş bırakılırsa sunucu üyelerin adlarından üretir. Görünüm call.css `.share-layer` /
//! `.share-modal` + direct.css `.group-modal` (F: parite).

use std::collections::HashSet;
use p2_ui::P2Fx as _; // backdrop_blur
use std::sync::Arc;

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::{Context, Entity, EventEmitter, FontWeight, MouseButton, SharedString, Subscription, Window, div, px};
use p2_client_core::{Api, ChatGroup, Member};
use p2_ui::P2Text as _;
use p2_ui::controls::lh;
use p2_ui::form::InputKind;
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::{fs, radius, shadows};
use p2_ui::{Entrance, P2Motion as _};

use super::{text_input, value};
use crate::task;

pub enum GroupCreatorEvent {
    Created(ChatGroup),
    Cancelled,
}

pub struct GroupCreator {
    api: Arc<Api>,
    org: String,
    project: String,
    candidates: Vec<Member>,
    /// Seçim sırası (Electron: tıklama sırası).
    selected: Vec<String>,
    name: Entity<InputState>,
    search: Entity<InputState>,
    busy: bool,
    error: Option<String>,
    _subs: Vec<Subscription>,
}

impl EventEmitter<GroupCreatorEvent> for GroupCreator {}

impl GroupCreator {
    pub fn new(api: Arc<Api>, org: String, project: String, candidates: Vec<Member>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name = text_input(window, cx, "İsteğe bağlı — boş bırakırsan isimlerden oluşur");
        let search = text_input(window, cx, "Kişi ara");
        let subs = vec![
            cx.subscribe_in(&name, window, |_, state, ev: &InputEvent, window, cx| {
                // `value.slice(0,60)`
                if matches!(ev, InputEvent::Change) {
                    let v = state.read(cx).value().to_string();
                    if v.chars().count() > 60 {
                        let cut: String = v.chars().take(60).collect();
                        state.update(cx, |s, cx| s.set_value(cut, window, cx));
                    }
                }
                cx.notify();
            }),
            cx.subscribe(&search, |_, _, _: &InputEvent, cx| cx.notify()),
        ];
        Self { api, org, project, candidates, selected: Vec::new(), name, search, busy: false, error: None, _subs: subs }
    }

    /// Otomatik pilot: kullanıcı adlarıyla seç ve oluştur. `@view` yalnız pencereyi açık bırakır
    /// (veri yazmaz; parite ekranı `group-creator`).
    pub fn autopilot(&mut self, name: &str, usernames: &[&str], window: &mut Window, cx: &mut Context<Self>) {
        if usernames == ["@view"] {
            crate::trace("grup: oluşturucu açık (görünüm)");
            return;
        }
        self.selected = self.candidates.iter().filter(|m| usernames.contains(&m.username.as_str())).map(|m| m.id.clone()).collect();
        let name = name.to_string();
        self.name.update(cx, |s, cx| s.set_value(name, window, cx));
        self.create(window, cx);
    }

    fn create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected.is_empty() || self.busy {
            return;
        }
        self.busy = true;
        self.error = None;
        cx.notify();
        let (api, org, project) = (self.api.clone(), self.org.clone(), self.project.clone());
        let name = value(&self.name, cx).trim().to_string();
        let ids = self.selected.clone();
        task::run(window, cx, move || api.create_group(&org, &project, &name, &ids), |this, result, _, cx| {
            this.busy = false;
            match result {
                Ok(group) => cx.emit(GroupCreatorEvent::Created(group)),
                Err(e) => this.error = Some(if e.message.is_empty() { "Grup oluşturulamadı.".into() } else { e.message }),
            }
            cx.notify();
        });
    }
}

impl Render for GroupCreator {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        let count = self.selected.len();
        let needle = value(&self.search, cx).trim().to_lowercase();
        let visible: Vec<&Member> = self.candidates.iter().filter(|m| needle.is_empty() || m.display_name.to_lowercase().contains(&needle) || m.username.to_lowercase().contains(&needle)).collect();

        // `.group-people`: en çok 260px, p 4, kenarlık `border-subtle`, r 11, bg-0.
        let mut people = div().id("group-people").max_h(px(260.)).overflow_y_scroll().flex().flex_col().gap(px(1.)).p(px(4.)).border_1().border_color(p.border_subtle).rounded(radius::MD).bg(p.bg_0);
        for member in &visible {
            let on = self.selected.contains(&member.id);
            let id = member.id.clone();
            let hover = p.bg_hover;
            let hue = p.hues[p2_ui::components::avatar_hue(&member.id) % 10];
            let face = match crate::avatars::image(&member.id) {
                Some(img) => gpui_kit::img(gpui_kit::ImageSource::Render(img)).size_full().rounded(px(7.)).object_fit(gpui_kit::ObjectFit::Cover).into_any_element(),
                None => p2_ui::components::initials_label(p2_ui::components::user_initials(&member.display_name)).into_any_element(),
            };
            people = people.child(
                div()
                    .id(SharedString::from(format!("pick-{}", member.id)))
                    .flex_none()
                    .h(px(34.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(radius::XS)
                    .cursor_pointer()
                    .map(|s| if on { s.bg(p.accent_soft).text_color(p.text_strong) } else { s.text_color(p.text).hover(move |s| s.bg(hover)) })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(i) = this.selected.iter().position(|x| x == &id) {
                            this.selected.remove(i);
                        } else {
                            this.selected.push(id.clone());
                        }
                        cx.notify();
                    }))
                    // `.group-person .person-avatar`: 22px, r 7, 8,5px 800.
                    .child(
                        div()
                            .size(px(22.))
                            .flex_none()
                            .overflow_hidden()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(7.))
                            .bg(gpui_kit::linear_gradient(145., gpui_kit::linear_color_stop(hue.bg, 0.), gpui_kit::linear_color_stop(hue.bg2, 1.)))
                            .text_color(hue.fg)
                            .text_size(fs(8.5))
                            .line_height(px(22.))
                            .font_weight(FontWeight::EXTRA_BOLD)
                            .tracking(-0.02)
                            .child(face),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .items_baseline()
                            .gap(px(6.))
                            .child(div().text_size(fs(11.5)).line_height(lh(fs(11.5))).css_weight(600.).truncate().child(member.display_name.clone()))
                            .child(div().flex_none().text_size(fs(9.5)).line_height(lh(fs(9.5))).text_color(p.text_3).child(format!("@{}", member.username))),
                    )
                    // `.group-check`: 14px, r 4; seçiliyken vurgu + beyaz ✓.
                    .child(
                        div()
                            .ml_auto()
                            .size(px(14.))
                            .flex_none()
                            .rounded(px(4.))
                            .border_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .map(|s| if on { s.border_color(p.accent).bg(p.accent).text_color(gpui_kit::white()).text_size(fs(9.)).line_height(px(12.)).child("✓") } else { s.border_color(p.navy_500) }),
                    ),
            );
        }
        if visible.is_empty() {
            people = people.child(div().py(px(36.)).flex().justify_center().text_size(fs(11.)).text_color(p.text_3).child("Kişi bulunamadı."));
        }

        let field = |state: &Entity<InputState>| p2_ui::form::input(state, InputKind::P2, &p, window, cx).h(px(32.)).px(px(10.)).py(px(0.)).text_size(fs(11.5)).font_weight(FontWeight::NORMAL);
        let (soft, strong, hover) = (p.accent_soft, p.text_strong, p.bg_hover);
        let disabled = self.busy || count == 0;
        let card = div()
            .id("group-card")
            .relative()
            .w(px(440f32.min(f32::from(window.viewport_size().width) - 48.)))
            .max_h(px((f32::from(window.viewport_size().height) * 0.78).min(600.)))
            .flex()
            .flex_col()
            .gap(px(10.))
            .pt(px(16.))
            .px(px(16.))
            .pb(px(14.))
            .overflow_hidden()
            .rounded(radius::XL)
            .border_1()
            .border_color(p.border_strong)
            .bg(p.glass_strong.background())
            .shadow(p.shadow_lg())
            .text_color(p.text)
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(10.))
                    .child(div().flex().flex_col().child(div().text_size(fs(14.)).line_height(lh(fs(14.))).font_weight(FontWeight::BOLD).text_color(p.text_strong).child("Grup oluştur")).child(div().mt(px(2.)).text_size(fs(10.5)).line_height(lh(fs(10.5))).text_color(p.text_3).child("Birlikte yazışacağın kişileri seç")))
                    .child(
                        div()
                            .id("group-close")
                            .size(px(30.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(radius::XS)
                            .text_color(p.text_3)
                            .text_size(fs(19.))
                            .cursor_pointer()
                            .hover(move |s| s.bg(soft).text_color(strong))
                            .child("×")
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(GroupCreatorEvent::Cancelled))),
                    ),
            )
            .child(div().flex().flex_col().gap(px(5.)).text_size(fs(10.)).line_height(lh(fs(10.))).css_weight(650.).text_color(p.text_3).child("Grup adı").child(field(&self.name)))
            .child(field(&self.search))
            .child(people)
            .when_some(self.error.clone(), |s, e| s.child(div().text_size(fs(10.)).text_color(p.danger_text).child(e)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.))
                    .child(div().mr_auto().text_size(fs(10.)).line_height(lh(fs(10.))).text_color(p.text_3).child(format!("{count} kişi seçildi")))
                    .child(
                        div()
                            .id("group-cancel")
                            .h(px(36.))
                            .px(px(16.))
                            .flex()
                            .items_center()
                            .border_1()
                            .border_color(p.border)
                            .rounded(radius::SM)
                            .text_color(p.text_2)
                            .text_size(fs(12.))
                            .css_weight(650.)
                            .cursor_pointer()
                            .hover(move |s| s.bg(hover).text_color(strong))
                            .child("Vazgeç")
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(GroupCreatorEvent::Cancelled))),
                    )
                    .child(
                        div()
                            .id("group-create")
                            .h(px(36.))
                            .px(px(18.))
                            .flex()
                            .items_center()
                            .border_1()
                            .border_color(gpui_kit::transparent_black())
                            .rounded(radius::SM)
                            .bg(p.accent_grad.background())
                            .text_color(p.text_on_accent)
                            .text_size(fs(12.))
                            .font_weight(FontWeight::BOLD)
                            .shadow(shadows(&[p.accent_glow_strong, p.inset_top]))
                            .map(|s| if disabled { s.opacity(0.55) } else { s.cursor_pointer() })
                            .child(if self.busy { "Oluşturuluyor…" } else { "Grubu oluştur" })
                            .on_click(cx.listener(|this, _, window, cx| this.create(window, cx))),
                    ),
            )
            .p2_enter("group-creator", Entrance::PopElastic, std::time::Duration::ZERO);

        // `.share-layer`: p 38 24 24 (pencereye göre; çalışma alanı başlık çubuğunun altında başlar).
        div()
            .id("group-scrim")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .px(px(24.))
            .pb(px(24.))
            .bg(p.overlay).backdrop_blur(p2_ui::fx::BLUR_SM)
            .occlude()
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| cx.emit(GroupCreatorEvent::Cancelled)))
            .child(card)
    }
}
