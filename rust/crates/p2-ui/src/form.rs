//! Form denetimleri — Electron'un hesaplanmış stilleriyle birebir (dom-styles.mjs ile ölçüldü).
//!
//! | Electron | Rust |
//! |---|---|
//! | `.auth-card input` (h 40, p 13/14, r 11, bg-3, `border`; odak `border-accent` + `ring`) | [`input`]`(.., InputKind::Auth, ..)` |
//! | `.p2-input` / `.p2-select` (p 9/12, r 8, bg-3, `500 11,05px`; hover `border-strong`) | [`input`]`(.., InputKind::P2, ..)` |
//! | `input[type=checkbox]` + `accent-color` (Chromium yerel, 13×13, r 2) | [`native_checkbox`] |
//! | `.p2-toggle` (36×20 hap; topuz 16, `translateX(16px)` 220ms ease-pop) | [`toggle`] |
//! | `.p2-radio` benzeri yuvarlak seçim | [`radio`] |
//!
//! Odak halkası GPUI'de `box-shadow 0 0 0 3px` ile çizilir (CSS `--p2-ring`'in aynısı).

use gpui_kit::component::input::{Input, InputState};
use gpui_kit::prelude::*;
use gpui_kit::AnimationExt as _;
use gpui_kit::{Animation, App, Div, Entity, FontWeight, Hsla, Stateful, Window, div, px};

use crate::motion::{EASE_POP, M_BASE};
use crate::tokens::{Palette, font_size, fs, radius, shadows};

#[derive(Clone, Copy, PartialEq)]
pub enum InputKind {
    /// `.auth-card input`, `.setup-* input` (giriş/kurulum ekranları). Yazı üst etiketten kalıtılır: 12px×0,85, 600.
    Auth,
    /// `.p2-input` (uygulama içi).
    P2,
}

/// P2 görünümlü tek satırlı girdi. `window` odak durumunu okumak için (odak halkası).
pub fn input(state: &Entity<InputState>, kind: InputKind, p: &Palette, window: &Window, cx: &App) -> Input {
    let focused = gpui_kit::Focusable::focus_handle(state.read(cx), cx).is_focused(window);
        let base = Input::new(state).focus_bordered(false).bordered(false).appearance(false);
    let base = match kind {
        InputKind::Auth => Styled::h(base, px(40.)).px(px(14.)).py(px(13.)).rounded(radius::MD).text_size(fs(12.)).font_weight(FontWeight::SEMIBOLD),
        InputKind::P2 => Styled::h(base, px(34.)).px(px(12.)).py(px(9.)).rounded(radius::SM).text_size(font_size::MD).font_weight(FontWeight::MEDIUM),
    };
    let base = base.line_height(gpui_kit::relative(crate::tokens::LINE_HEIGHT_NORMAL)).bg(p.bg_3).text_color(p.text).border_1();
    if focused {
        base.border_color(p.border_accent).shadow(shadows(&[p.ring]))
    } else {
        // CSS :hover (border-strong) — Input hover durumu sunmuyor; çağıran sarmalayıcıda group_hover kullanabilir.
        base.border_color(p.border)
    }
}

/// Chromium'un yerel onay kutusu, `accent-color: var(--p2-accent)` (13×13, köşe 2px, beyaz tik).
/// Piksel ölçümü: Electron giriş ekranı "Beni hatırla". Kapalıyken Chromium açık şema kutusu.
pub fn native_checkbox(checked: bool, p: &Palette) -> Div {
    const TICK: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 13 13"><path d="M2.5 6.6 5.2 9.3 10.3 2.7" fill="none" stroke="black" stroke-width="1.9"/></svg>"#;
    let b = div().size(px(13.)).flex_shrink_0().rounded(px(2.));
    if checked {
        b.bg(p.accent).child(gpui_kit::svg().data(TICK).size(px(13.)).text_color(gpui_kit::white()))
    } else {
        b.bg(gpui_kit::white()).border_1().border_color(gpui_kit::rgb(0x767676))
    }
}

/// `.p2-toggle`: 36×20 hap, kapalı `navy-500`, açık `accent-grad`; topuz 16×16 beyaz,
/// `0 1px 3px rgba(0,0,0,.4)`, 220ms ease-pop kayma. `epoch` her değişimde artırılırsa canlandırılır.
pub fn toggle(id: impl Into<gpui_kit::ElementId>, on: bool, epoch: usize, p: &Palette) -> Stateful<Div> {
    let knob_shadow = shadows(&[crate::tokens::Shadow::new(0., 1., 3., 0., crate::tokens::css_rgba(0, 0, 0, 0.4))]);
    let id = id.into();
    let knob = div()
        .absolute()
        .top(px(2.))
        .size(px(16.))
        .rounded_full()
        .bg(gpui_kit::white())
        .shadow(knob_shadow)
        .with_animation(("p2-toggle-knob", epoch), Animation::new(if epoch == 0 { std::time::Duration::from_millis(1) } else { M_BASE }), move |el, t| {
            let k = EASE_POP.ease(t);
            let x = if on { 16. * k } else { 16. * (1. - k) };
            el.left(px(2. + x))
        });
    div()
        .id(id)
        .relative()
        .w(px(36.))
        .h(px(20.))
        .flex_shrink_0()
        .rounded_full()
        .cursor_pointer()
        .map(|s| if on { s.bg(p.accent_grad.background()) } else { s.bg(p.navy_500) })
        .child(knob)
}

/// Yuvarlak seçim (radyo): 16px halka `border-strong`, seçiliyken vurgu + 8px iç nokta.
pub fn radio(selected: bool, p: &Palette) -> Div {
    div()
        .size(px(16.))
        .flex_shrink_0()
        .rounded_full()
        .border_1()
        .flex()
        .items_center()
        .justify_center()
        .map(|s| if selected { s.border_color(p.accent).child(div().size(px(8.)).rounded_full().bg(p.accent)) } else { s.border_color(p.border_strong) })
}

/// `.auth-card label`: üstte etiket (12px×0,85, 600, `text-2`), 7px boşluk, altta denetim.
pub fn auth_label(label: &str, control: impl IntoElement, p: &Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(7.))
        .text_size(fs(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(p.text_2)
        .child(div().h(px(12.)).line_height(px(12.)).child(label.to_string()))
        .child(control)
}

/// Renk yardımcı: CSS `filter: brightness(k)` yaklaşığı (hover'da düğmeler).
pub fn brighten(c: Hsla, k: f32) -> Hsla {
    let mut r: gpui_kit::Rgba = c.into();
    r.r = (r.r * k).min(1.);
    r.g = (r.g * k).min(1.);
    r.b = (r.b * k).min(1.);
    r.into()
}
