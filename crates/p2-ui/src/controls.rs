//! ui.css ilkelleri (primitives) — Electron'un `.p2-*` sınıflarının birebir GPUI karşılıkları.
//! Bütün boyutlar ui.css'ten; yazı boyları `font_size::*` (× 0,85 ölçek), ağırlıklar değişken eksenle
//! tam değer (650, 550: `P2Text::css_weight`). Hepsi saf fonksiyon; tıklamayı çağıran ekler.
//!
//! | ui.css | Rust |
//! |---|---|
//! | `.p2-btn` + `.primary/.secondary/.ghost/.danger` × `.sm/.lg/.icon/.pill/.block` | [`button`] (`BtnVariant`, `BtnSize`) |
//! | `.p2-menu`, `.p2-menu-item(.danger)`, `hr`, `.p2-menu-label` | [`menu`], [`menu_item`], [`menu_separator`], [`menu_label`] |
//! | `.p2-badge(.soft/.danger/.success/.warning/.muted)` | [`badge`] |
//! | `.p2-pill(.active)` | [`pill`] |
//! | `.p2-kicker` | [`kicker`] |
//! | `.p2-kbd` / `.p2-keys` | [`kbd`], [`keys`] |
//! | `.p2-avatar` + `.p2-avatar-status` | [`avatar_box`], [`status_dot`] |
//! | `.p2-tabs(.line/.segment)` + `.p2-tab(.active)` | [`tabs`], [`tab`] |
//! | `.p2-card(.glow/.pad/.interactive)` | [`card`] |
//! | `.p2-modal-layer` + `.p2-modal(.sm/.lg/.xl)` + head/body/foot | [`modal_layer`], [`modal`], [`modal_head`], [`modal_foot`] |
//! | `.p2-row(.active)` | [`row`] |
//! | `.p2-progress(.thin)` | [`progress`] |
//! | `.p2-divider` | [`divider`] |
//! | `.p2-empty` | [`empty`] |
//! | `.p2-toast` (+ `.ok/.error`, süre çubuğu, parlama) | [`toast`] |
//! | yerel `title` ipucu (Chromium → Windows ipucu) | [`tooltip`] |

use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::{AnimationExt as _, AnyElement, AnyView, App, Div, ElementId, FontWeight, Hsla, Pixels, SharedString, Stateful, Window, div, px};

use crate::P2Text as _;
use crate::fx::P2Fx as _;
use crate::motion::{EASE_POP, Entrance, P2Motion as _};
use crate::text::normal_line_height;
use crate::tokens::{Palette, Shadow, css_rgba, font_size, radius, shadows};

// ------------------------------------------------------------------------------------------ düğme

#[derive(Clone, Copy, PartialEq, Default)]
pub enum BtnVariant {
    /// `.p2-btn` (bg-3, kenarlık saydam)
    #[default]
    Default,
    Primary,
    Secondary,
    Ghost,
    Danger,
}

#[derive(Clone, Copy, PartialEq, Default)]
pub enum BtnSize {
    /// 34px, p 0 14, r 8, fs-md
    #[default]
    Md,
    /// `.sm`: 28px, p 0 10, r 6, fs-sm
    Sm,
    /// `.lg`: 40px, p 0 18, r 11, fs-lg
    Lg,
}

/// `.p2-btn`. `icon` = kare (`.icon`), `pill` = hap köşe. Hover/active durumları ui.css'teki gibi
/// (primary hover: brightness 1.08 + 1px yukarı + büyük gölge; active: hafif kararma).
pub fn button(id: impl Into<ElementId>, variant: BtnVariant, size: BtnSize, icon: bool, pill: bool, p: &Palette) -> Stateful<Div> {
    let (h, pad, r, text) = match size {
        BtnSize::Md => (34., 14., radius::SM, font_size::MD),
        BtnSize::Sm => (28., 10., radius::XS, font_size::SM),
        BtnSize::Lg => (40., 18., radius::MD, font_size::LG),
    };
    let base = div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .h(px(h))
        .map(|s| if icon { s.w(px(h)) } else { s.px(px(pad)) })
        .rounded(if pill { radius::PILL } else { r })
        .border_1()
        .border_color(gpui_kit::transparent_black())
        .text_size(text)
        .css_weight(650.)
        .line_height(text)
        .whitespace_nowrap()
        .cursor_pointer();
    let (hover_bg, strong) = (p.bg_hover, p.text_strong);
    match variant {
        BtnVariant::Default => base.bg(p.bg_3).text_color(p.text).hover(move |s| s.bg(hover_bg).text_color(strong)),
        BtnVariant::Primary => {
            let rest = shadows(&[Shadow::new(0., 6., 18., 0., css_rgba(20, 90, 220, 0.35)), p.inset_top]);
            let hot = shadows(&[Shadow::new(0., 10., 24., 0., css_rgba(20, 90, 220, 0.45)), p.inset_top]);
            let grad = p.accent_grad.clone();
            let hot_grad = crate::tokens::Gradient { angle: grad.angle, stops: grad.stops.iter().map(|(c, at)| (crate::form::brighten(*c, 1.08), *at)).collect() };
            base.bg(grad.background())
                .text_color(p.text_on_accent)
                .shadow(rest)
                .hover(move |s| s.bg(hot_grad.background()).shadow(hot.clone()).relative().top(px(-1.)))
                .active(|s| s.opacity(0.94).top(px(0.)))
        }
        BtnVariant::Secondary => {
            let bs = p.border_strong;
            base.bg(p.bg_3).border_color(p.border).text_color(p.text).hover(move |s| s.bg(hover_bg).border_color(bs).text_color(strong))
        }
        BtnVariant::Ghost => {
            let soft = p.accent_soft;
            base.text_color(p.text_2).hover(move |s| s.bg(soft).text_color(strong))
        }
        BtnVariant::Danger => {
            let d = p.danger;
            base.bg(p.danger_soft).border_color(p.danger_border).text_color(p.danger_text).hover(move |s| s.bg(d).border_color(d).text_color(gpui_kit::white()))
        }
    }
}

// ------------------------------------------------------------------------------------------ menü

/// `.p2-menu`: min-w 200, p 6, kenarlık `border-strong`, r 11, `glass`, `shadow-lg`,
/// `p2-scale-in` 220ms ease-pop (GPUI'de ölçek yok: opaklık + 6px kayma). `epoch` her açılışta farklı olmalı.
/// Konumlandırma çağıranda (`anchored()`/`deferred()`); menü ekran kenarına göre kaydırılmalıdır.
/// Öğeleri ekledikten sonra `.p2_enter(("p2-menu", epoch), Entrance::ScaleIn, Duration::ZERO)` ile canlandırın.
pub fn menu(id: impl Into<ElementId>, p: &Palette) -> Stateful<Div> {
    div()
        .id(id)
        .min_w(px(200.))
        .p(px(6.))
        .flex()
        .flex_col()
        .border_1()
        .border_color(p.border_strong)
        .rounded(radius::MD)
        .bg(p.glass.background())
        .backdrop_blur(crate::fx::BLUR)
        .shadow(p.shadow_lg())
        .text_color(p.text)
        .occlude()
}

/// `.p2-menu-item`: 32px satır, `auto 1fr auto` ızgara, gap 10, p 0 10, r 6, `550 fs-md`;
/// hover `accent-soft` + `text-strong`; `danger` kırmızı. `icon` 16px (opaklık .85), `trail` kısayol/not (`fs-xs`, text-4).
pub fn menu_item(id: impl Into<ElementId>, label: impl Into<SharedString>, icon: Option<AnyElement>, trail: Option<SharedString>, danger: bool, p: &Palette) -> Stateful<Div> {
    let (hover_bg, hover_fg) = if danger { (p.danger_soft, p.danger_text) } else { (p.accent_soft, p.text_strong) };
    div()
        .id(id)
        .w_full()
        .h(px(32.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .gap(px(10.))
        .px(px(10.))
        .rounded(radius::XS)
        .text_color(if danger { p.danger_text } else { p.text })
        .text_size(font_size::MD)
        .css_weight(550.)
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg).text_color(hover_fg))
        .when_some(icon, |s, i| s.child(div().size(px(16.)).flex_shrink_0().opacity(0.85).child(i)))
        .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis().child(label.into()))
        .when_some(trail, |s, t| s.child(div().text_color(p.text_4).text_size(font_size::XS).child(t)))
}

/// `.p2-menu hr`: 1px, kenar boşluğu 5px 4px, `border-subtle`.
pub fn menu_separator(p: &Palette) -> Div {
    div().h(px(1.)).flex_shrink_0().mx(px(4.)).my(px(5.)).bg(p.border_subtle)
}

/// `.p2-menu-label`: p 6 10 4 + `.p2-kicker`.
pub fn menu_label(text: impl Into<SharedString>, p: &Palette) -> Div {
    div().pt(px(6.)).px(px(10.)).pb(px(4.)).child(kicker(text, p))
}

/// Alt menü göstergesi (menü öğesinin sağında `›`).
pub fn submenu_chevron(p: &Palette) -> AnyElement {
    crate::glyphs::ui_stroke("chevron-right", 14., 2.).text_color(p.text_4).into_any_element()
}

// ------------------------------------------------------------------------------------------ etiketler

/// `.p2-kicker`: `800 fs-2xs`, aralık .18em, büyük harf, `text-accent`. Metni büyük harfle verin.
pub fn kicker(text: impl Into<SharedString>, p: &Palette) -> Div {
    div().text_size(font_size::XXS).css_weight(800.).tracking(crate::tokens::KICKER_TRACKING_EM).text_color(p.text_accent).child(text.into())
}

#[derive(Clone, Copy, PartialEq, Default)]
pub enum BadgeTone {
    #[default]
    Accent,
    Soft,
    Danger,
    Success,
    Warning,
    Muted,
}

/// `.p2-badge`: h 18, min-w 18, p 0 6, hap, `700 fs-xs`.
pub fn badge(text: impl Into<SharedString>, tone: BadgeTone, p: &Palette) -> Div {
    let (bg, fg) = match tone {
        BadgeTone::Accent => (p.accent, p.text_on_accent),
        BadgeTone::Soft => (p.accent_soft, p.text_accent),
        BadgeTone::Danger => (p.danger, gpui_kit::white()),
        BadgeTone::Success => (p.success_soft, p.success_text),
        BadgeTone::Warning => (p.warning_soft, p.warning_text),
        BadgeTone::Muted => (p.bg_hover, p.text_3),
    };
    div()
        .h(px(18.))
        .min_w(px(18.))
        .px(px(6.))
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(radius::PILL)
        .bg(bg)
        .text_color(fg)
        .text_size(font_size::XS)
        .font_weight(FontWeight::BOLD)
        .line_height(font_size::XS)
        .child(text.into())
}

/// `.p2-pill(.active)`: h 26, p 0 10, gap 6, kenarlık, hap, `600 fs-sm`.
pub fn pill(id: impl Into<ElementId>, active: bool, p: &Palette) -> Stateful<Div> {
    let (bs, strong) = (p.border_strong, p.text_strong);
    div()
        .id(id)
        .h(px(26.))
        .px(px(10.))
        .flex()
        .items_center()
        .gap(px(6.))
        .border_1()
        .rounded(radius::PILL)
        .text_size(font_size::SM)
        .font_weight(FontWeight::SEMIBOLD)
        .cursor_pointer()
        .map(|s| {
            if active {
                s.bg(p.accent_soft).border_color(p.border_accent).text_color(p.text_accent)
            } else {
                s.bg(p.bg_3).border_color(p.border).text_color(p.text_2).hover(move |s| s.border_color(bs).text_color(strong))
            }
        })
}

/// `.p2-kbd`: min-w 20, p 2 7, kenarlık `rgba(140,190,255,.35)` (alt 2px .5), r 6, beyaz gradyan, `650 fs-xs`.
pub fn kbd(key: impl Into<SharedString>, p: &Palette) -> Div {
    div()
        .min_w(px(20.))
        .px(px(7.))
        .pt(px(2.))
        .pb(px(1.))
        .flex()
        .justify_center()
        .border_1()
        .border_b_2()
        .border_color(css_rgba(140, 190, 255, 0.4))
        .rounded(radius::XS)
        .bg(gpui_kit::linear_gradient(180., gpui_kit::linear_color_stop(css_rgba(255, 255, 255, 0.14), 0.), gpui_kit::linear_color_stop(css_rgba(255, 255, 255, 0.06), 1.)))
        .shadow(shadows(&[Shadow::new(0., 1., 0., 0., css_rgba(0, 0, 0, 0.4))]))
        .text_color(p.white)
        .text_size(font_size::XS)
        .css_weight(650.)
        .child(key.into())
}

/// `.p2-keys`: tuşlar arasında soluk "+" (`rgba(255,255,255,.4)`), gap 3.
pub fn keys(parts: &[&str], p: &Palette) -> Div {
    let mut d = div().flex().items_center().gap(px(3.));
    for (i, k) in parts.iter().enumerate() {
        if i > 0 {
            d = d.child(div().mx(px(1.)).text_size(font_size::XS).text_color(css_rgba(255, 255, 255, 0.4)).child("+"));
        }
        d = d.child(kbd(k.to_string(), p));
    }
    d
}

// ------------------------------------------------------------------------------------------ avatar

/// `.p2-avatar` kutusu: köşe %25 (yuvarlak için `round`), 145° kimlik tonu gradyanı, `800` yazı
/// (boy × .36), aralık .02em. İçerik (baş harf ya da görsel) çağırandan.
pub fn avatar_box(size: f32, hue: usize, round: bool, p: &Palette) -> Div {
    let h = p.hues[hue % 10];
    div()
        .relative()
        .size(px(size))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .overflow_hidden()
        .rounded(if round { px(size / 2.) } else { px(size * 0.25) })
        .bg(gpui_kit::linear_gradient(145., gpui_kit::linear_color_stop(h.bg, 0.), gpui_kit::linear_color_stop(h.bg2, 1.)))
        .text_color(h.fg)
        .text_size(px(size * 0.36))
        // Satır kutusu = kutu yüksekliği: baş harfler esnek ortalamanın piksel yuvarlamasına uğramadan
        // kutunun tam ortasında (Inter'de büyük harf yüksekliği satır kutusunda ortalı).
        .line_height(px(size))
        // Harfler optik olarak ortada (GPUI P2 yaması: mürekkep genişliği + büyük harf yüksekliği, alt piksel).
        .p2_cap_center()
        .css_weight(800.)
        .tracking(0.02)
}

#[derive(Clone, Copy, PartialEq)]
pub enum Presence {
    Online,
    Away,
    Busy,
    Offline,
}

/// `.p2-avatar-status`: 11×11 nokta, 2px `bg-1` halka; avatarın sağ-alt köşesine −2px taşar
/// (avatarı `relative` bir sarmalayıcıya koyup bunu `child` olarak ekleyin). `ring` = halka rengi (zemine göre).
pub fn status_dot(presence: Presence, ring: Option<Hsla>, p: &Palette) -> Div {
    let fill = match presence {
        Presence::Online => p.success,
        Presence::Away => p.warning,
        Presence::Busy => p.danger,
        Presence::Offline => p.text_4,
    };
    div().absolute().right(px(-2.)).bottom(px(-2.)).size(px(11.)).child(dot_paint(ring.unwrap_or(p.bg_1), fill, None))
}

impl Presence {
    /// Sunucu durum adı (`ONLINE`/`AWAY`/`BUSY`, diğerleri çevrimdışı).
    pub fn from_status(status: &str) -> Self {
        match status {
            "ONLINE" => Presence::Online,
            "AWAY" => Presence::Away,
            "BUSY" => Presence::Busy,
            _ => Presence::Offline,
        }
    }
}

/// direct.css `.presence` (+ `.online/.away/.busy`): 11×11 (border-box), 2px `ring` halka avatarı keser,
/// iç 7px nokta: çevrimiçi `success` + `0 0 0 1px` / `0 0 7px` yeşil parıltı, uzakta `bg-2` zemin +
/// `inset 0 0 0 2.5px warning` (sarı halka, ortada 2px delik), meşgul `danger` + `0 0 0 1px` kırmızı hale,
/// çevrimdışı `text-4`. Konum çağıranda (`.absolute().right(..).bottom(..)`).
///
/// Daireler doğrudan boyanır; delik kesirli merkezli bir yol olarak (quad sınırları tam piksele yaslanır):
/// uzakta deliği tam ortada (5,5px) kalır — eskiden 2,5px iç boşluk yuvarlanıp delik yarım piksel sola/yukarı
/// kayıyor, sarı halka bir yanda 2 bir yanda 3px görünüyordu. Halka da kenarlık değil dolu daire
/// (GPUI'de yuvarlak kenarlığın dış kenarında açık bir hale kalıyor).
pub fn presence_ring(presence: Presence, ring: Hsla, p: &Palette) -> Div {
    let glow = |spread: f32, blur: f32, c: Hsla| gpui_kit::BoxShadow { color: c, offset: gpui_kit::point(px(0.), px(0.)), blur_radius: px(blur), spread_radius: px(spread), inset: false };
    let dot = div().flex_none().size(px(11.)).rounded_full();
    match presence {
        Presence::Online => dot.shadow(vec![glow(1., 0., css_rgba(61, 220, 132, 0.35)), glow(0., 7., css_rgba(61, 220, 132, 0.45))]).child(dot_paint(ring, p.success, None)),
        Presence::Away => dot.child(dot_paint(ring, p.warning, Some(p.bg_2))),
        Presence::Busy => dot.shadow(vec![glow(1., 0., css_rgba(255, 92, 108, 0.35))]).child(dot_paint(ring, p.danger, None)),
        Presence::Offline => dot.child(dot_paint(ring, p.text_4, None)),
    }
}

/// Durum noktasını kesirli koordinatlarla boyar: `ring` (dış daire), 2px içeride `fill` (7px),
/// `hole` varsa merkezde 2px delik (`inset 2.5px` halkanın içi).
fn dot_paint(ring: Hsla, fill: Hsla, hole: Option<Hsla>) -> impl IntoElement {
    gpui_kit::canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let circle = |inset: f32| {
                let d = (f32::from(bounds.size.width).min(f32::from(bounds.size.height)) - inset * 2.).max(0.);
                let b = gpui_kit::Bounds::new(bounds.center() - gpui_kit::point(px(d / 2.), px(d / 2.)), gpui_kit::size(px(d), px(d)));
                (b, px(d / 2.))
            };
            let (outer, r) = circle(0.);
            window.paint_quad(gpui_kit::fill(outer, ring).corner_radii(r));
            let (inner, r) = circle(2.);
            window.paint_quad(gpui_kit::fill(inner, fill).corner_radii(r));
            // Delik yol olarak: `paint_quad` sınırları tam piksele yaslıyor (2px delik 4,5px'ten 4px'e
            // kayıp halkayı bir yanda 2, bir yanda 3px yapıyordu); yol kesirli merkezde kalır.
            if let Some(hole) = hole {
                let c = bounds.center();
                let r = 1.;
                let mut b = gpui_kit::PathBuilder::fill();
                for i in 0..=24 {
                    let a = i as f32 / 24. * std::f32::consts::TAU;
                    let pt = gpui_kit::point(c.x + px(r * a.cos()), c.y + px(r * a.sin()));
                    if i == 0 { b.move_to(pt) } else { b.line_to(pt) }
                }
                b.close();
                if let Ok(path) = b.build() {
                    window.paint_path(path, hole);
                }
            }
        },
    )
    .size_full()
}

// ------------------------------------------------------------------------------------------ sekmeler

#[derive(Clone, Copy, PartialEq, Default)]
pub enum TabsStyle {
    /// `.p2-tabs`: gap 4
    #[default]
    Plain,
    /// `.line`: alt çizgi + etkin sekmede 2px `accent-bar` (220ms ease-pop açılır)
    Line,
    /// `.segment`: p 3, r 11, `bg-2`, kenarlık `border-subtle`
    Segment,
}

pub fn tabs(style: TabsStyle, p: &Palette) -> Div {
    let d = div().flex();
    match style {
        TabsStyle::Plain => d.gap(px(4.)),
        TabsStyle::Line => d.border_b_1().border_color(p.border_subtle),
        TabsStyle::Segment => d.gap(px(4.)).p(px(3.)).rounded(radius::MD).bg(p.bg_2).border_1().border_color(p.border_subtle),
    }
}

/// `.p2-tab`: h 30, p 0 12, gap 6, r 8, `650 fs-md`, `text-3`; hover `bg-hover`; etkin `bg-3` + inset-top
/// (`line` stilinde zemin yok, alt çizgi). `epoch` etkin sekme değişince artarsa çizgi canlanır.
pub fn tab(id: impl Into<ElementId>, label: impl Into<SharedString>, active: bool, style: TabsStyle, epoch: usize, p: &Palette) -> Stateful<Div> {
    let (hover_bg, text) = (p.bg_hover, p.text);
    let line = style == TabsStyle::Line;
    let bar = p.accent_bar.clone();
    div()
        .id(id)
        .relative()
        .h(px(30.))
        .px(px(12.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(if line { px(0.) } else { radius::SM })
        .text_size(font_size::MD)
        .css_weight(650.)
        .cursor_pointer()
        .map(|s| {
            if active {
                let s = s.text_color(p.text_strong);
                if line { s } else { s.bg(p.bg_3).shadow(shadows(&[p.inset_top])) }
            } else {
                s.text_color(p.text_3).when(!line, |s| s.hover(move |s| s.bg(hover_bg).text_color(text))).when(line, |s| s.hover(move |s| s.text_color(text)))
            }
        })
        .child(label.into())
        .when(line && active, move |s| {
            s.child(
                div().absolute().left(px(10.)).right(px(10.)).bottom(px(-1.)).h(px(2.)).rounded(px(2.)).bg(bar.background()).with_animation(("p2-tab-bar", epoch), gpui_kit::Animation::new(crate::motion::M_BASE), |el, t| {
                    let k = EASE_POP.ease(t).max(0.);
                    // scaleX(0 → 1) (yamalı GPUI ölçeği; ease-pop taşması dahil)
                    el.scale_x(k)
                }),
            )
        })
}

// ------------------------------------------------------------------------------------------ kart / modal

/// `.p2-card` (`glow`: `border-strong` + `shadow-lg`; `pad`: 14 16).
pub fn card(glow: bool, pad: bool, p: &Palette) -> Div {
    let mut layers = if glow { p.shadow_lg.clone() } else { p.shadow_md.clone() };
    if !glow {
        layers.push(p.inset_top);
    }
    div()
        .relative()
        .border_1()
        .border_color(if glow { p.border_strong } else { p.border })
        .rounded(radius::LG)
        .bg(p.glass.background())
        .shadow(shadows(&layers))
        .text_color(p.text)
        .when(pad, |s| s.py(px(14.)).px(px(16.)))
}

/// `.p2-modal-layer`: tam ekran, `overlay` zemini, p 38 24 24 (başlık çubuğu payı), ortalı; `p2-fade` 220ms.
/// `dim` = `.dim` (rgba(2,4,10,.82)). `backdrop-filter: blur(6px)` yamalı çizicide gerçek.
/// Çocukları ekledikten sonra `.p2_enter("p2-modal-layer", Entrance::Fade, Duration::ZERO)`. DİKKAT: `p2_enter`
/// öğeyi `relative` yapar; canlandırılacaksa `div().absolute().inset_0().child(modal_layer(..).size_full()…p2_enter(..))`.
pub fn modal_layer(id: impl Into<ElementId>, dim: bool, p: &Palette) -> Stateful<Div> {
    div()
        .id(id)
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .pt(px(38.))
        .px(px(24.))
        .pb(px(24.))
        .bg(if dim { css_rgba(2, 4, 10, 0.82) } else { p.overlay })
        .backdrop_blur(crate::fx::BLUR_SM)
        .occlude()
}

#[derive(Clone, Copy, PartialEq, Default)]
pub enum ModalSize {
    Sm,
    #[default]
    Md,
    Lg,
    Xl,
}

/// `.p2-modal`: genişlik min(590, 100vw−48) (sm 400, lg 720, xl 880), r 18, kenarlık `border-strong`,
/// `glass-strong`, `shadow-lg`, taşma gizli; `p2-pop-elastic` 620ms.
/// Çocukları ekledikten sonra `.p2_enter("p2-modal", Entrance::PopElastic, Duration::ZERO)`.
pub fn modal(size: ModalSize, window: &Window, p: &Palette) -> Div {
    let max: f32 = match size {
        ModalSize::Sm => 400.,
        ModalSize::Md => 590.,
        ModalSize::Lg => 720.,
        ModalSize::Xl => 880.,
    };
    let vw = f32::from(window.viewport_size().width);
    let vh = f32::from(window.viewport_size().height);
    div()
        .relative()
        .overflow_hidden()
        .flex()
        .flex_col()
        .w(px(max.min(vw - 48.)))
        .max_h(px(vh - 90.))
        .border_1()
        .border_color(p.border_strong)
        .rounded(radius::XL)
        .bg(p.glass_strong.background())
        .shadow(p.shadow_lg())
        .text_color(p.text)
}

/// `.p2-modal-head`: p 18 16 12 20; başlık `700 fs-xl` (satır 1.25), alt yazı `fs-sm` text-3 (1.4); sağda kapat.
pub fn modal_head(title: impl Into<SharedString>, subtitle: Option<SharedString>, close: Option<AnyElement>, p: &Palette) -> Div {
    div()
        .flex()
        .items_start()
        .gap(px(12.))
        .pt(px(18.))
        .pr(px(16.))
        .pb(px(12.))
        .pl(px(20.))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(div().text_size(font_size::XL).font_weight(FontWeight::BOLD).text_color(p.text_strong).line_height(gpui_kit::relative(1.25)).child(title.into()))
                .when_some(subtitle, |s, t| s.child(div().mt(px(3.)).text_size(font_size::SM).text_color(p.text_3).line_height(gpui_kit::relative(1.4)).child(t))),
        )
        .children(close)
}

/// `.p2-modal-body`: p 4 20 18.
pub fn modal_body() -> Div {
    div().pt(px(4.)).px(px(20.)).pb(px(18.))
}

/// `.p2-modal-foot`: sağa yaslı, gap 10, p 12 16 16, üst kenarlık `border-subtle`.
pub fn modal_foot(p: &Palette) -> Div {
    div().flex().justify_end().gap(px(10.)).pt(px(12.)).px(px(16.)).pb(px(16.)).border_t_1().border_color(p.border_subtle)
}

// ------------------------------------------------------------------------------------------ diğer

/// `.p2-row(.active)`: min-h 36, p 0 10, gap 10, r 8, `550 fs-md`, `text-2`; hover `bg-hover` + 2px sağa.
pub fn row(id: impl Into<ElementId>, active: bool, p: &Palette) -> Stateful<Div> {
    let (hover_bg, strong) = (p.bg_hover, p.text_strong);
    div()
        .id(id)
        .relative()
        .w_full()
        .min_h(px(36.))
        .px(px(10.))
        .flex()
        .items_center()
        .gap(px(10.))
        .rounded(radius::SM)
        .text_size(font_size::MD)
        .css_weight(550.)
        .cursor_pointer()
        .map(|s| if active { s.bg(p.bg_selected).text_color(strong) } else { s.text_color(p.text_2).hover(move |s| s.bg(hover_bg).text_color(strong).left(px(2.))) })
}

/// `.p2-progress`: 4px (thin 2px) hap, `navy-600` iz, `accent-bar` dolgu; `value` 0..1, `None` = belirsiz (1,3 sn gidip gelen %38).
pub fn progress(value: Option<f32>, thin: bool, p: &Palette) -> AnyElement {
    let bar = p.accent_bar.clone();
    let track = div().relative().w_full().h(px(if thin { 2. } else { 4. })).overflow_hidden().rounded(radius::PILL).bg(p.navy_600);
    match value {
        Some(v) => track.child(div().absolute().top_0().bottom_0().left_0().w(gpui_kit::relative(v.clamp(0., 1.))).rounded(radius::PILL).bg(bar.background())).into_any_element(),
        None => track
            .child(div().absolute().top_0().bottom_0().w(gpui_kit::relative(0.38)).rounded(radius::PILL).bg(bar.background()).with_animation("p2-progress-run", gpui_kit::Animation::new(Duration::from_millis(1300)).repeat(), |el, t| {
                // ease-in-out; sol -40% → 105%
                let k = crate::motion::CubicBezier::new(0.42, 0., 0.58, 1.).ease(t);
                el.left(gpui_kit::relative(-0.4 + 1.45 * k))
            }))
            .into_any_element(),
    }
}

/// `.p2-divider`: 1px, dikey 8px boşluk.
pub fn divider(p: &Palette) -> Div {
    div().h(px(1.)).my(px(8.)).bg(p.border_subtle)
}

/// `.p2-empty`: ortalı, gap 6, p 34 18; başlık `fs-lg` text, açıklama `fs-sm` text-3.
pub fn empty(title: impl Into<SharedString>, detail: Option<SharedString>, p: &Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(6.))
        .py(px(34.))
        .px(px(18.))
        .text_color(p.text_3)
        .child(div().text_size(font_size::LG).font_weight(FontWeight::BOLD).text_color(p.text).child(title.into()))
        .when_some(detail, |s, d| s.child(div().text_size(font_size::SM).child(d)))
}

#[derive(Clone, Copy, PartialEq, Default)]
pub enum ToastTone {
    #[default]
    Info,
    Ok,
    Error,
}

/// `.p2-toast` (352px): `44px 1fr auto` ızgara, gap 13, p 12 12 14 14, kenarlık `border-strong`
/// (hata: `danger-border`), r 14, `glass`, `shadow-lg`; sağdan elastik giriş (620ms), 0,25 sn gecikmeli
/// parlama, altta 2px süre çubuğu (`accent-bar`, `duration` boyunca soldan sağa kısalır, opaklık .85).
/// `media` = logo ya da avatar (44px), `close` = sağ üst kapat düğmesi.
pub fn toast(id: impl Into<ElementId>, media: AnyElement, kicker_text: Option<SharedString>, title: impl Into<SharedString>, body: Option<SharedString>, close: Option<AnyElement>, tone: ToastTone, duration: Option<Duration>, p: &Palette) -> AnyElement {
    let id: ElementId = id.into();
    let (bar_a, bar_b, kicker_color) = match tone {
        ToastTone::Info => (p.accent_bar.stops[0].0, p.accent_bar.stops.last().map_or(p.accent, |s| s.0), p.text_accent),
        ToastTone::Ok => (p.success, p.success_text, p.success_text),
        ToastTone::Error => (p.danger, p.danger_text, p.danger_text),
    };
    let width = 352.;
    let band = width * 0.38;
    let shine = div()
        .absolute()
        .top_0()
        .bottom_0()
        .left(px(-band * 1.2))
        .w(px(band))
        .flex()
        .child(div().w(px(band / 2.)).h_full().bg(gpui_kit::linear_gradient(90., gpui_kit::linear_color_stop(css_rgba(120, 180, 255, 0.), 0.), gpui_kit::linear_color_stop(css_rgba(120, 180, 255, 0.14), 1.))))
        .child(div().w(px(band / 2.)).h_full().bg(gpui_kit::linear_gradient(90., gpui_kit::linear_color_stop(css_rgba(120, 180, 255, 0.14), 0.), gpui_kit::linear_color_stop(css_rgba(120, 180, 255, 0.), 1.))))
        .with_animation("p2-toast-shine", gpui_kit::Animation::new(Duration::from_millis(1350)), move |el, t| {
            let raw = ((t * 1.35 - 0.25) / 1.1).clamp(0., 1.);
            let k = crate::motion::CubicBezier::new(0., 0., 0.58, 1.).ease(raw);
            el.left(px(band * (-1.2 + 3.8 * k)))
        });
    // Electron'da çubuk, kartın `overflow: hidden` + `border-radius` kırpmasıyla yuvarlak alt köşelerin
    // içinde kalır. GPUI'nin içerik maskesi dikdörtgen olduğundan çubuk, kartın iç (kenarlık içi) köşe
    // yayıyla kesilmiş bir yol olarak çizilir: üst kenar düz, alt kenar köşelerde yayı izler.
    let inner_w = width - 2.;
    let inner_r = f32::from(radius::LG) - 1.;
    let (stop_a, stop_b) = (bar_a.opacity(0.85), bar_b.opacity(0.85));
    let bar = div().absolute().left_0().bottom_0().h(px(inner_r)).w_full().child(
        gpui_kit::canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let end = f32::from(bounds.size.width).min(inner_w);
                if let Some(path) = toast_bar_path(bounds.origin, f32::from(bounds.size.height), end, inner_w, inner_r) {
                    window.paint_path(path, gpui_kit::linear_gradient(90., gpui_kit::linear_color_stop(stop_a, 0.), gpui_kit::linear_color_stop(stop_b, 1.)));
                }
            },
        )
        .size_full(),
    );
    let bar: AnyElement = match duration {
        Some(d) => bar.with_animation("p2-toast-bar", gpui_kit::Animation::new(d), |el, t| el.w(gpui_kit::relative(1. - t))).into_any_element(),
        None => bar.into_any_element(),
    };
    div()
        .id(id)
        .relative()
        .overflow_hidden()
        .w(px(width))
        .flex()
        .items_center()
        .gap(px(13.))
        .pt(px(12.))
        .pr(px(12.))
        .pb(px(14.))
        .pl(px(14.))
        .border_1()
        .border_color(if tone == ToastTone::Error { p.danger_border } else { p.border_strong })
        .rounded(radius::LG)
        .bg(p.glass.background())
        .shadow(p.shadow_lg())
        .text_color(gpui_kit::white())
        .child(shine)
        .child(div().size(px(44.)).flex_shrink_0().child(media))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .when_some(kicker_text, |s, k| s.child(kicker(k, p).text_color(kicker_color)))
                .child(div().text_size(font_size::LG).font_weight(FontWeight::BOLD).line_height(gpui_kit::relative(1.2)).text_color(p.text_strong).overflow_hidden().whitespace_nowrap().text_ellipsis().child(title.into()))
                .when_some(body, |s, b| s.child(div().mt(px(2.)).text_color(p.text_2).text_size(font_size::SM).font_weight(FontWeight::MEDIUM).line_height(gpui_kit::relative(1.3)).child(b))),
        )
        .when_some(close, |s, c| s.child(div().self_start().mt(px(-4.)).mr(px(-4.)).child(c)))
        .child(bar)
        .p2_enter("p2-toast", Entrance::SlideRightElastic { width }, Duration::ZERO)
        .into_any_element()
}

/// Toast süre çubuğu (2px): `[0, end]` aralığının, genişliği `full` ve alt köşe yarıçapı `r` olan yuvarlak
/// dikdörtgenle kesişimi. `h` = çizim alanı yüksekliği (alt kenarı kartın iç alt kenarı).
fn toast_bar_path(origin: gpui_kit::Point<Pixels>, h: f32, end: f32, full: f32, r: f32) -> Option<gpui_kit::Path<Pixels>> {
    const BAR: f32 = 2.;
    const STEP: f32 = 0.25;
    // Yay derinliği: x'te alt kenarın tabandan ne kadar yukarı çekildiği.
    let depth = |x: f32| -> f32 {
        let d = if x < r { r - x } else if x > full - r { x - (full - r) } else { 0. };
        let d = d.clamp(0., r);
        r - (r * r - d * d).max(0.).sqrt()
    };
    // Yayın çubuğun üst kenarını kestiği x (sol); sağda simetriği.
    let x0 = r - (r * r - (r - BAR) * (r - BAR)).max(0.).sqrt();
    let x1 = end.min(full - x0);
    if x1 <= x0 + 0.01 {
        return None;
    }
    let at = |x: f32, y: f32| gpui_kit::point(origin.x + px(x), origin.y + px(y));
    let mut b = gpui_kit::PathBuilder::fill();
    b.move_to(at(x0, h - BAR));
    b.line_to(at(x1, h - BAR));
    // Alt kenar sağdan sola: sağ yay, düz bölge (tek çizgi), sol yay.
    let mut xs = vec![x1];
    let mut x = x1;
    while x > full - r && x > x0 {
        x = (x - STEP).max(full - r).max(x0);
        xs.push(x);
    }
    if x > r {
        x = r.max(x0);
        xs.push(x);
    }
    while x > x0 {
        x = (x - STEP).max(x0);
        xs.push(x);
    }
    for x in xs {
        b.line_to(at(x, h - depth(x).min(BAR)));
    }
    b.close();
    b.build().ok()
}

/// Yerel `title` ipucu: Chromium Windows'ta sistem ipucunu çizer (Windows 11: 12px Segoe UI, ince kenarlık,
/// r 4, p 5 8). Koyu sistem temasında koyu. Kullanım: `.tooltip(|w, cx| p2_ui::controls::tooltip("Menü", w, cx))`.
pub fn tooltip(text: impl Into<SharedString>, _window: &mut Window, cx: &mut App) -> AnyView {
    let text = text.into();
    cx.new(|_| Tooltip(text)).into()
}

struct Tooltip(SharedString);

impl Render for Tooltip {
    fn render(&mut self, _: &mut Window, _: &mut gpui_kit::Context<Self>) -> impl IntoElement {
        let dark = crate::tokens::css_rgba(43, 43, 43, 1.);
        div()
            .font_family(crate::tokens::FONT_FALLBACK)
            .text_size(px(12.))
            .line_height(normal_line_height(px(12.)))
            .px(px(8.))
            .py(px(5.))
            .rounded(px(4.))
            .border_1()
            .border_color(crate::tokens::css_rgba(255, 255, 255, 0.08))
            .bg(dark)
            .text_color(gpui_kit::white())
            .shadow(shadows(&[Shadow::new(0., 4., 8., 0., css_rgba(0, 0, 0, 0.26))]))
            .child(self.0.clone())
    }
}

/// Satırlık yazı yüksekliği kısayolu (CSS `line-height: normal`).
pub fn lh(font_px: Pixels) -> Pixels {
    normal_line_height(font_px)
}

#[allow(dead_code)]
fn _assert_hsla(_: Hsla) {}
