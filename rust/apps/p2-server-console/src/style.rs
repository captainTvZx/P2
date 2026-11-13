//! server-console.css token'ları ve yardımcı öğeler. Değerler CSS'teki `:root` bloğuyla birebir.
//!
//! Yazı ağırlıkları: Chrome "Segoe UI" ile 600 → Semibold, 650/700 → Bold, 800/900 → Black seçer;
//! GPUI'ye aynı yüzler açıkça verilir.

use gpui_kit::prelude::*;
use gpui_kit::{BoxShadow, Div, FontWeight, Hsla, Rgba, SharedString, div, linear_color_stop, linear_gradient, point, px};

pub fn c(hex: u32) -> Hsla {
    gpui_kit::rgb(hex).into()
}
pub fn ca(r: u8, g: u8, b: u8, a: f32) -> Hsla {
    Rgba { r: r as f32 / 255., g: g as f32 / 255., b: b as f32 / 255., a }.into()
}

pub const FONT: &str = "Segoe UI";
pub const MONO: &str = "Cascadia Mono";

pub const W600: FontWeight = FontWeight::SEMIBOLD;
/// CSS 650 ve 700.
pub const W650: FontWeight = FontWeight::BOLD;
/// CSS 800 ve 900.
pub const W800: FontWeight = FontWeight::BLACK;

pub fn bg_0() -> Hsla {
    c(0x070b14)
}
pub fn bg_2() -> Hsla {
    c(0x0e1626)
}
pub fn bg_3() -> Hsla {
    c(0x121c30)
}
pub fn bg_hover() -> Hsla {
    c(0x182440)
}
pub fn line() -> Hsla {
    c(0x1f2f52)
}
pub fn text() -> Hsla {
    c(0xd6e4ff)
}
pub fn text_strong() -> Hsla {
    c(0xeef5ff)
}
pub fn text_2() -> Hsla {
    ca(214, 228, 255, 0.78)
}
pub fn text_3() -> Hsla {
    c(0x7d93b8)
}
pub fn text_4() -> Hsla {
    c(0x4d6690)
}
pub fn text_accent() -> Hsla {
    c(0x7fb6ff)
}
pub fn accent() -> Hsla {
    c(0x2c7bff)
}
pub fn accent_hover() -> Hsla {
    c(0x4f9dff)
}
pub fn accent_soft() -> Hsla {
    ca(44, 123, 255, 0.16)
}
pub fn border_subtle() -> Hsla {
    ca(120, 170, 255, 0.10)
}
pub fn border() -> Hsla {
    ca(120, 170, 255, 0.18)
}
pub fn border_strong() -> Hsla {
    ca(96, 170, 255, 0.38)
}
pub fn success_soft() -> Hsla {
    ca(61, 220, 132, 0.16)
}
pub fn success_text() -> Hsla {
    c(0x8fe8b4)
}
pub fn danger() -> Hsla {
    c(0xff5c6c)
}
pub fn danger_soft() -> Hsla {
    ca(255, 92, 108, 0.16)
}
pub fn danger_border() -> Hsla {
    ca(255, 92, 108, 0.42)
}
pub fn danger_text() -> Hsla {
    c(0xff9aa4)
}

pub const R_SM: f32 = 8.;
pub const R_MD: f32 = 11.;
pub const R_LG: f32 = 14.;
pub const R_XL: f32 = 18.;

/// `--p2-accent-grad` (135°, #4f9dff → #2c7bff); `bright`: `filter: brightness(1.08)`.
pub fn accent_grad(bright: bool) -> gpui_kit::Background {
    let (a, b) = if bright { (ca(85, 170, 255, 1.), ca(48, 133, 255, 1.)) } else { (c(0x4f9dff), c(0x2c7bff)) };
    linear_gradient(135., linear_color_stop(a, 0.), linear_color_stop(b, 1.))
}
/// `--p2-accent-bar` (90°, #4f9dff → #8fd0ff).
pub fn accent_bar() -> gpui_kit::Background {
    linear_gradient(90., linear_color_stop(c(0x4f9dff), 0.), linear_color_stop(c(0x8fd0ff), 1.))
}
/// `--p2-glass` (135°).
pub fn glass() -> gpui_kit::Background {
    linear_gradient(135., linear_color_stop(ca(14, 22, 38, 0.96), 0.), linear_color_stop(ca(8, 12, 22, 0.94), 1.))
}
/// `--p2-glass-strong` (160°; ortadaki #0e1626 durağı iki duraklı GPUI degradesinde düşer).
pub fn glass_strong() -> gpui_kit::Background {
    linear_gradient(160., linear_color_stop(c(0x131d31), 0.), linear_color_stop(c(0x0a1020), 1.))
}

fn shadow(y: f32, blur: f32, spread: f32, color: Hsla) -> BoxShadow {
    BoxShadow { color, offset: point(px(0.), px(y)), blur_radius: px(blur), spread_radius: px(spread), inset: false }
}
/// `--p2-shadow-md`, `--p2-inset-top` ile.
pub fn shadow_md() -> Vec<BoxShadow> {
    vec![shadow(12., 30., 0., ca(0, 0, 0, 0.45)), shadow(0., 0., 1., ca(0, 0, 0, 0.3)), inset_top()]
}
/// `--p2-shadow-lg`.
pub fn shadow_lg() -> Vec<BoxShadow> {
    vec![shadow(18., 44., 0., ca(0, 0, 0, 0.55)), shadow(0., 0., 1., ca(0, 0, 0, 0.35)), shadow(0., 34., 0., ca(40, 120, 255, 0.18)), inset_top()]
}
/// `.primary` gölgesi: `0 6px 18px rgba(20,90,220,.35)`.
pub fn shadow_primary() -> Vec<BoxShadow> {
    vec![shadow(6., 18., 0., ca(20, 90, 220, 0.35)), inset_top()]
}
/// `.logo` gölgesi.
pub fn shadow_logo() -> Vec<BoxShadow> {
    vec![shadow(6., 18., 0., ca(20, 90, 220, 0.45)), shadow(0., 0., 1., ca(120, 180, 255, 0.25))]
}

/// `--p2-inset-top`: `inset 0 1px 0 rgba(255,255,255,.08)`.
pub fn inset_top() -> BoxShadow {
    BoxShadow { inset: true, ..shadow(1., 0., 0., ca(255, 255, 255, 0.08)) }
}

/// `letter-spacing`: GPUI'de yok; her karakter ayrı öğe olarak, sağına aralık eklenerek dizilir.
pub fn spaced(text: &str, spacing: f32) -> Div {
    let mut row = div().flex().flex_row().flex_none();
    for ch in text.chars() {
        let cell = if ch == ' ' { div().child("\u{00a0}") } else { div().child(SharedString::from(ch.to_string())) };
        row = row.child(cell.mr(px(spacing)));
    }
    row
}

/// tr dilinde `text-transform: uppercase` (i → İ, ı → I).
pub fn upper_tr(text: &str) -> String {
    text.chars()
        .flat_map(|ch| match ch {
            'i' => vec!['İ'],
            'ı' => vec!['I'],
            other => other.to_uppercase().collect(),
        })
        .collect()
}

/// `.card h2`: 10px, 800, .18em aralık, büyük harf, vurgu rengi, alt boşluk 14.
pub fn card_title(text: &str) -> Div {
    spaced(&upper_tr(text), 1.8).text_size(px(10.)).line_height(px(14.5)).font_weight(W800).text_color(text_accent())
}

/// `.card`.
pub fn card() -> Div {
    div()
        .relative()
        .overflow_hidden()
        .flex()
        .flex_col()
        .bg(glass())
        .border_1()
        .border_color(border())
        .rounded(px(R_LG))
        .p(px(18.))
        .shadow(shadow_md())
}
