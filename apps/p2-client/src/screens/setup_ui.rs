//! setup.css ortak parçaları (kurulum türü, sunucu sorunu, ilk yönetici, kuruluş ekranları):
//! `.setup-page` (radyal vurgu zemini + p 30, kart ortada), `.setup-shell` (cam kart, p 34 40, r 18,
//! `p2-pop-elastic`, 0,3 sn gecikmeli parlama), `.setup-shell > header` (logo 44 + eyebrow + h2 27px).

use std::time::Duration;

use gpui_kit::AnimationExt as _;
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Div, FontWeight, Hsla, Window, div, px};
use p2_ui::P2Text as _;
use p2_ui::components::{glass_card, logo_image, radial_background};
use p2_ui::text::normal_line_height;
use p2_ui::tokens::{KICKER_TRACKING_EM, Palette, fs};
use p2_ui::{Entrance, P2Motion as _};

/// `.setup-page`: `radial-gradient(circle at X% Y%, <renk> 0, transparent <durak>), bg-0`; içerik ortada.
pub fn setup_page(window: &Window, p: &Palette, at: (f32, f32), tint: Hsla, stop: f32, shell: impl IntoElement) -> AnyElement {
    let viewport = window.viewport_size();
    let (w, h) = (f32::from(viewport.width), f32::from(viewport.height) - crate::titlebar::HEIGHT);
    // `transparent`'a doğru (premultiplied) karışım = opak zeminde renk → zemin geçişi.
    let bg = radial_background(w, h, at, &[(tint, 0.), (p.bg_0, stop)], p.bg_0);
    div()
        .id("setup-page")
        .relative()
        .size_full()
        .overflow_y_scroll()
        .child(div().absolute().top_0().left_0().child(bg))
        .child(div().relative().min_h(px(h)).flex().items_center().justify_center().p(px(30.)).child(shell))
        .into_any_element()
}

/// `.setup-shell`: genişlik min(max, 100%), p `pad_y`/`pad_x`, `glass-strong`, `shadow-lg`, r 18; parlama 0,3 sn.
pub fn setup_shell(id: &'static str, max_w: f32, pad_y: f32, pad_x: f32, p: &Palette) -> Div {
    glass_card(p).w(px(max_w)).py(px(pad_y)).px(px(pad_x)).flex().flex_col().child(shine(id, max_w, 300))
}

/// Kartı `p2-pop-elastic` (620ms) ile canlandırır; `key` görünüm değişince farklı olmalı.
pub fn shell_enter(shell: Div, key: (&'static str, usize)) -> AnyElement {
    shell.p2_enter(key, Entrance::PopElastic, Duration::ZERO).into_any_element()
}

/// `::after` parlama şeridi (38%, `rgba(120,180,255,.08)`, 1,1 sn ease-out, `delay_ms` gecikme).
pub fn shine(id: &'static str, card_w: f32, delay_ms: u64) -> impl IntoElement {
    let band = card_w * 0.38;
    let delay = delay_ms as f32 / 1000.;
    let total = delay + 1.1;
    let glow = gpui_kit::rgba(0x78b4ff14);
    let clear = gpui_kit::rgba(0x78b4ff00);
    let half = move |from: gpui_kit::Rgba, to: gpui_kit::Rgba| div().w(px(band / 2.)).h_full().bg(gpui_kit::linear_gradient(90., gpui_kit::linear_color_stop(from, 0.), gpui_kit::linear_color_stop(to, 1.)));
    div()
        .absolute()
        .top_0()
        .bottom_0()
        .left(px(-band * 1.2))
        .w(px(band))
        .flex()
        .child(half(clear, glow))
        .child(half(glow, clear))
        .with_animation(id, gpui_kit::Animation::new(Duration::from_secs_f32(total)), move |el, t| {
            let raw = ((t * total - delay) / 1.1).clamp(0., 1.);
            let k = p2_ui::motion::CubicBezier::new(0., 0., 0.58, 1.).ease(raw);
            el.left(px(band * (-1.2 + 3.8 * k)))
        })
}

/// `.eyebrow` (kurulum kartlarında margin 0): 9,5px×0,85, 800, .18em, `text-accent`.
pub fn eyebrow(text: impl Into<gpui_kit::SharedString>, p: &Palette) -> Div {
    let text: gpui_kit::SharedString = text.into();
    let size = fs(9.5);
    div().h(p2_ui::text::scaled(px(11.))).line_height(px(11.)).text_size(size).css_weight(800.).tracking(KICKER_TRACKING_EM).text_color(p.text_accent).child(text)
}

/// `.setup-shell > header`: logo (md, pop) + eyebrow + h2 (27px×0,85, 700, -.02em, mt 3); alt kenarlık, pb 25.
pub fn setup_header(media: AnyElement, kicker: impl Into<gpui_kit::SharedString>, title: impl Into<gpui_kit::SharedString>, p: &Palette) -> Div {
    let (kicker, title): (gpui_kit::SharedString, gpui_kit::SharedString) = (kicker.into(), title.into());
    let h2 = fs(27.);
    div()
        .relative()
        .flex()
        .items_center()
        .gap(px(18.))
        .pb(px(25.))
        .border_b_1()
        .border_color(p.border_subtle)
        .child(media)
        .child(
            div()
                .flex()
                .flex_col()
                .child(eyebrow(kicker, p))
                .child(div().mt(px(3.)).h(p2_ui::text::scaled(normal_line_height(h2))).line_height(normal_line_height(h2)).text_size(h2).font_weight(FontWeight::BOLD).tracking(-0.02).text_color(p.text_strong).child(title)),
        )
}

/// Logo.tsx `md pop`: 44px, `p2-logo-pop` (.55 sn ease-pop, .12 sn gecikme; ölçek/dönme GPUI'de yok → opaklık).
pub fn logo_pop(id: &'static str, p: &Palette) -> AnyElement {
    logo_image(44., p).p2_enter(id, Entrance::Fade, Duration::from_millis(120)).into_any_element()
}

/// `.mode-back`: h 38, p 0 16, kenarlık `border`, r 8, saydam, 12px×0,85 `text-2`; hover `bg-hover` + `text-strong`.
pub fn mode_back(id: &'static str, label: impl Into<gpui_kit::SharedString>, p: &Palette) -> gpui_kit::Stateful<Div> {
    let label: gpui_kit::SharedString = label.into();
    let (hover_bg, hover_fg) = (p.bg_hover, p.text_strong);
    div()
        .id(id)
        .h(px(38.))
        .px(px(16.))
        .flex()
        .items_center()
        .border_1()
        .border_color(p.border)
        .rounded(p2_ui::tokens::radius::SM)
        .text_size(fs(12.))
        .text_color(p.text_2)
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg).text_color(hover_fg))
        .child(label)
}

/// `.mode-actions .install-button`: h 38, p 0 16, gap 6, r 11, `accent-grad` + glow, 12,5px×0,85 700; "metin →".
pub fn install_button(id: &'static str, label: impl Into<gpui_kit::SharedString>, busy: bool, p: &Palette) -> gpui_kit::Stateful<Div> {
    let label: gpui_kit::SharedString = label.into();
    div()
        .id(id)
        .relative()
        .h(px(38.))
        .px(px(16.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(p2_ui::tokens::radius::MD)
        .bg(p.accent_grad.background())
        .shadow(p2_ui::tokens::shadows(&[p.accent_glow_strong, p.inset_top]))
        .text_color(p.text_on_accent)
        .text_size(fs(12.5))
        .font_weight(FontWeight::BOLD)
        .when(busy, |s| s.opacity(0.65))
        .when(!busy, |s| s.cursor_pointer().hover(|s| s.top(px(-1.))))
        .child(label)
        .child("→")
}
