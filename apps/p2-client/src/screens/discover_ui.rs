//! Keşfet ekranının parçaları (saf fonksiyonlar): ikonlar, kategori çipleri, afiş + karartma katmanı,
//! proje yüzü, sayaç satırı, katıl hapı ve iskelet kutuları. Durum ve düzen `discover.rs`'te.
//!
//! Dil: P2 Mavi — cam yüzeyler, harf aralıklı küçük başlıklar, yumuşak parıltılar. Afiş üstündeki
//! yazılar temadan bağımsız açık renktir (alttaki karartma katmanı her temada koyudur).

use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::{AnimationExt as _, AnyElement, Div, ElementId, Hsla, ImageSource, ObjectFit, SharedString, Stateful, Svg, div, img, px, relative};
use p2_ui::components::{avatar_hue, initials_label, linear_background, user_initials};
use p2_ui::controls::lh;
use p2_ui::icons::Icon;
use p2_ui::motion::{EASE_OUT, EASE_POP, M_BASE, M_FAST};
use p2_ui::tokens::{KICKER_TRACKING_EM, Palette, css_rgba, fs, radius, shadows};
use p2_ui::{P2Fx as _, P2Text as _, P2Transition as _};

/// Satır içi ikon gövdeleri (viewBox 24, kontur).
pub mod glyph {
    pub const SEARCH: &str = r#"<circle cx="11" cy="11" r="6.5"/><path d="M16 16l4.5 4.5"/>"#;
    pub const COMPASS: &str = r#"<circle cx="12" cy="12" r="8.5"/><path d="M15.5 8.5l-2 5-5 2 2-5z"/>"#;
    pub const USERS: &str = r#"<circle cx="9" cy="8.5" r="3"/><path d="M3.5 19c.6-3 2.8-4.5 5.5-4.5s4.9 1.5 5.5 4.5"/><path d="M15.5 5.8a3 3 0 010 5.4M17.5 14.8c1.6.6 2.6 2 3 4.2"/>"#;
    pub const X: &str = r#"<path d="M6 6l12 12M18 6L6 18"/>"#;
    pub const BACK: &str = r#"<path d="M19 12H5M11 6l-6 6 6 6"/>"#;
    pub const ARROW: &str = r#"<path d="M5 12h14M13 6l6 6-6 6"/>"#;
    pub const CHECK: &str = r#"<path d="M5 12.5l4.5 4.5L19 7.5"/>"#;
    pub const PLUS: &str = r#"<path d="M12 5v14M5 12h14"/>"#;
    pub const KEY: &str = r#"<circle cx="8" cy="15" r="4"/><path d="M11 12l8.5-8.5M16 7l2.5 2.5M13.5 9.5l2 2"/>"#;
    pub const HASH: &str = r#"<path d="M9 4L7 20M17 4l-2 16M4.5 9h16M3.5 15h16"/>"#;
    pub const MEGAPHONE: &str = r#"<path d="M4 10v4h3l7 4V6l-7 4H4z"/><path d="M17.5 9.5a3.5 3.5 0 010 5"/>"#;
    pub const VOICE: &str = r#"<path d="M5 9.5v5h3l5 4v-13l-5 4H5z"/><path d="M16.5 8.5a5 5 0 010 7M19 6a8.5 8.5 0 010 12"/>"#;
    pub const FORUM: &str = r#"<path d="M4 6h16v10H10l-4 3.5V16H4z"/><path d="M8 10h8M8 13h5"/>"#;
    pub const NEWS: &str = r#"<path d="M4 5h13v14H6a2 2 0 01-2-2z"/><path d="M17 9h3v8a2 2 0 01-2 2M7.5 9h6M7.5 12.5h6M7.5 16h4"/>"#;
    pub const GLOBE: &str = r#"<circle cx="12" cy="12" r="8.5"/><path d="M3.5 12h17M12 3.5c2.6 2.4 3.8 5.2 3.8 8.5s-1.2 6.1-3.8 8.5c-2.6-2.4-3.8-5.2-3.8-8.5s1.2-6.1 3.8-8.5z"/>"#;
    pub const LOCK: &str = r#"<rect x="5" y="10.5" width="14" height="9.5" rx="2"/><path d="M8 10.5V8a4 4 0 018 0v2.5"/>"#;
    pub const GAME: &str = r#"<path d="M7 8h10a4 4 0 014 4v2.5a3 3 0 01-5.4 1.8L14.5 15h-5l-1.1 1.3A3 3 0 013 14.5V12a4 4 0 014-4z"/><path d="M7.5 10.5v3M6 12h3M15.5 11h.01M17.5 13h.01"/>"#;
    pub const CODE: &str = r#"<path d="M8.5 7.5L4 12l4.5 4.5M15.5 7.5L20 12l-4.5 4.5M13.5 5l-3 14"/>"#;
    pub const BOOK: &str = r#"<path d="M4 5.5h6a2.5 2.5 0 012.5 2.5v11a2 2 0 00-2-2H4zM20 5.5h-5A2.5 2.5 0 0012.5 8v11a2 2 0 012-2H20z"/>"#;
    pub const MUSIC: &str = r#"<path d="M9 17.5V6l10-2v11.5"/><circle cx="6.5" cy="17.5" r="2.5"/><circle cx="16.5" cy="15.5" r="2.5"/>"#;
    pub const SPARK: &str = r#"<path d="M12 3.5l1.8 5.2 5.2 1.8-5.2 1.8L12 17.5l-1.8-5.2L5 10.5l5.2-1.8z"/><path d="M18.5 16l.7 1.8 1.8.7-1.8.7-.7 1.8-.7-1.8-1.8-.7 1.8-.7z"/>"#;
    pub const GRID: &str = r#"<rect x="4" y="4" width="6.5" height="6.5" rx="1.5"/><rect x="13.5" y="4" width="6.5" height="6.5" rx="1.5"/><rect x="4" y="13.5" width="6.5" height="6.5" rx="1.5"/><rect x="13.5" y="13.5" width="6.5" height="6.5" rx="1.5"/>"#;
}

pub fn t(key: &str) -> String {
    p2_i18n::t(key)
}

pub fn tv(key: &str, vars: &[(&str, &str)]) -> String {
    p2_i18n::tv(key, vars)
}

/// GPUI'de svg rengi miras alınmaz: `c` = CSS `currentColor`.
pub fn ico(body: &'static str, size: f32, c: Hsla) -> Svg {
    Icon::raw(body).stroke(1.9).el().size(px(size)).text_color(c)
}

/// Afiş üstü yazı rengi (her temada açık).
pub fn on_img(alpha: f32) -> Hsla {
    css_rgba(255, 255, 255, alpha)
}

pub fn category_glyph(id: &str) -> &'static str {
    match id {
        "GAME" => glyph::GAME,
        "DEV" => glyph::CODE,
        "COMMUNITY" => glyph::USERS,
        "EDUCATION" => glyph::BOOK,
        "MUSIC" => glyph::MUSIC,
        "ALL" => glyph::GRID,
        _ => glyph::SPARK,
    }
}

pub fn category_label(id: &str) -> String {
    t(&format!("disc.cat.{id}"))
}

pub fn channel_glyph(kind: &str) -> &'static str {
    match kind {
        "ANNOUNCEMENT" => glyph::MEGAPHONE,
        "VOICE" => glyph::VOICE,
        "FORUM" => glyph::FORUM,
        _ => glyph::HASH,
    }
}

/// 1240 → "1,2 B" / "1.2K" (dile göre); 999 → "999".
pub fn count_text(n: u64) -> String {
    if n < 1000 {
        return n.to_string();
    }
    let k = n as f64 / 1000.;
    let tr = p2_i18n::language().id == "tr";
    let num = if k >= 10. { format!("{}", k.round() as u64) } else { format!("{k:.1}") };
    if tr { format!("{} B", num.replace('.', ",")) } else { format!("{num}K") }
}

/// "az önce" / "5 dk" / "3 sa" / "2 g" / "13.11.2026".
pub fn ago(iso: &str) -> String {
    let Some(at) = chrono::DateTime::parse_from_rfc3339(iso).ok() else { return String::new() };
    let secs = (chrono::Utc::now().timestamp() - at.timestamp()).max(0);
    let n = |v: i64| v.to_string();
    match secs {
        s if s < 60 => t("disc.time.now"),
        s if s < 3600 => tv("disc.time.min", &[("n", &n(s / 60))]),
        s if s < 86400 => tv("disc.time.hour", &[("n", &n(s / 3600))]),
        s if s < 86400 * 30 => tv("disc.time.day", &[("n", &n(s / 86400))]),
        _ => at.with_timezone(&chrono::Local).format("%d.%m.%Y").to_string(),
    }
}

/// Küçük harf aralıklı başlık (`.p2-kicker`).
pub fn kicker(text: impl Into<SharedString>, color: Hsla) -> Div {
    div().text_size(fs(9.5)).line_height(lh(fs(9.5))).css_weight(800.).tracking(KICKER_TRACKING_EM).text_color(color).child(text.into())
}

/// Proje yüzü: ikon görseli ya da ton gradyanı üstünde baş harfler.
pub fn project_face(id: &str, name: &str, icon_url: Option<&str>, size: f32, r: f32, p: &Palette) -> Div {
    let key = format!("discover-icon:{id}:{}", icon_url.unwrap_or_default());
    crate::avatars::remember(&key, icon_url);
    let hue = p.hues[avatar_hue(id) % 10];
    let base = div()
        .size(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .overflow_hidden()
        .rounded(px(r))
        .bg(gpui_kit::linear_gradient(145., gpui_kit::linear_color_stop(hue.bg, 0.), gpui_kit::linear_color_stop(hue.bg2, 1.)))
        .text_color(hue.fg)
        .text_size(px(size * 0.34))
        .line_height(px(size))
        .css_weight(800.);
    match icon_url.and_then(|_| crate::avatars::image(&key)) {
        Some(image) => base.child(img(ImageSource::Render(image)).size_full().rounded(px(r)).object_fit(ObjectFit::Cover)),
        None => base.child(initials_label(user_initials(name))),
    }
}

/// Afiş: kapak görseli; yoksa projenin tonundan gradyan + ince halkalar. `r`: (üst, alt) köşe yarıçapı.
pub fn banner(id: &str, cover_url: Option<&str>, w: f32, h: f32, r: (f32, f32), p: &Palette) -> Div {
    let hue = p.hues[avatar_hue(id) % 10];
    let base = div().relative().w(px(w)).h(px(h)).flex_none().overflow_hidden().rounded_t(px(r.0)).rounded_b(px(r.1));
    match super::discover_images::cover(cover_url) {
        Some(image) => base.child(img(ImageSource::Render(image)).absolute().top_0().left_0().w(px(w)).h(px(h)).rounded_t(px(r.0)).rounded_b(px(r.1)).object_fit(ObjectFit::Cover)),
        None => {
            let ring = |size: f32, right: f32, top: f32, a: f32| div().absolute().size(px(size)).right(px(right)).top(px(top)).rounded_full().border_1().border_color(Hsla { a, ..hue.fg });
            base.bg(gpui_kit::linear_gradient(128., gpui_kit::linear_color_stop(hue.bg2, 0.), gpui_kit::linear_color_stop(p.accent_soft_2, 1.)))
                .child(ring(h * 1.5, -h * 0.5, -h * 0.55, 0.22))
                .child(ring(h * 0.9, -h * 0.2, -h * 0.25, 0.16))
                .child(ring(h * 0.42, h * 0.04, -h * 0.02, 0.12))
        }
    }
}

/// Afişin altını koyulaştıran katman (yazılar okunur kalsın): saydam → lacivert, aşağı doğru.
pub fn scrim(w: f32, h: f32, strength: f32) -> gpui_kit::Img {
    let c = |a: f32| css_rgba(5, 9, 20, a * strength);
    linear_background(w, h, 180., &[(c(0.04), 0.), (c(0.12), 0.34), (c(0.62), 0.68), (c(0.94), 1.)])
}

/// Çevrimiçi / üye sayacı satırı.
pub fn counts(online: u64, members: u64, on_image: bool, p: &Palette) -> Div {
    let (fg, dim) = if on_image { (on_img(0.92), on_img(0.6)) } else { (p.text_2, p.text_3) };
    let online_text = tv("disc.online", &[("n", &count_text(online))]);
    let member_text = tv(if members == 1 { "disc.member" } else { "disc.members" }, &[("n", &count_text(members))]);
    div()
        .flex()
        .items_center()
        .gap(px(6.))
        .text_size(fs(10.5))
        .line_height(lh(fs(10.5)))
        .css_weight(650.)
        .child(div().size(px(7.)).flex_none().rounded_full().bg(if online > 0 { p.success } else { dim }).when(online > 0, |s| s.shadow(shadows(&[p2_ui::tokens::Shadow::new(0., 0., 6., 0., Hsla { a: 0.55, ..p.success })]))))
        .child(div().text_color(fg).child(online_text))
        .child(div().text_color(dim).child("·"))
        .child(div().text_color(if on_image { on_img(0.78) } else { p.text_3 }).child(member_text))
}

#[derive(Clone, Copy, PartialEq)]
pub enum Pill {
    Join,
    Busy,
    Joined,
}

/// Kart üstündeki katıl hapı: `Katıl` (vurgu gradyanı) / `Katılınıyor…` / `Üyesin` (yeşil).
pub fn join_pill(id: impl Into<ElementId>, state: Pill, large: bool, p: &Palette) -> Stateful<Div> {
    let (h, pad, size) = if large { (34., 18., 12.) } else { (28., 13., 11.) };
    let base = div().id(id).h(px(h)).px(px(pad)).flex_none().flex().items_center().gap(px(6.)).rounded(radius::PILL).text_size(fs(size)).line_height(lh(fs(size))).css_weight(750.);
    match state {
        Pill::Joined => base.bg(p.success_soft).border_1().border_color(p.success_border).text_color(p.success_text).cursor_pointer().child(ico(glyph::CHECK, size + 1., p.success_text)).child(t("disc.joined")).hover(|s| s.brightness(1.12)),
        Pill::Busy => base.bg(p.accent_grad.background()).text_color(p.text_on_accent).opacity(0.65).child(t("disc.joining")),
        Pill::Join => base
            .bg(p.accent_grad.background())
            .shadow(shadows(&[p.accent_glow, p.inset_top]))
            .text_color(p.text_on_accent)
            .cursor_pointer()
            .transition(M_FAST, EASE_OUT)
            .hover(|s| s.brightness(1.1).translate(0., -1.))
            .child(t("disc.join")),
    }
}

/// Cam çip (afiş üstü rozetleri): bulanık arka plan + ince kenarlık.
pub fn glass_chip(icon: Option<&'static str>, text: impl Into<SharedString>, accent: bool, p: &Palette) -> Div {
    div()
        .h(px(22.))
        .px(px(9.))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(5.))
        .rounded(radius::PILL)
        .border_1()
        .map(|s| {
            if accent {
                s.bg(p.accent_grad.background()).border_color(on_img(0.22)).shadow(shadows(&[p.accent_glow]))
            } else {
                s.bg(css_rgba(8, 14, 28, 0.46)).border_color(on_img(0.16)).backdrop_blur(p2_ui::fx::BLUR_SM)
            }
        })
        .text_color(on_img(0.94))
        .text_size(fs(9.))
        .line_height(lh(fs(9.)))
        .css_weight(800.)
        .tracking(0.08)
        .when_some(icon, |s, body| s.child(ico(body, 11., on_img(0.94))))
        .child(text.into())
}

/// Kategori çipi (hero): ikon + ad + sayı; seçiliyken vurgulu.
pub fn category_chip(id: impl Into<ElementId>, icon: &'static str, label: String, count: Option<u64>, active: bool, p: &Palette) -> Stateful<Div> {
    let (border_strong, strong, hover_bg) = (p.border_strong, p.text_strong, p.bg_hover);
    let fg = if active { p.text_on_accent } else { p.text_2 };
    div()
        .id(id)
        .h(px(30.))
        .pl(px(10.))
        .pr(px(if count.is_some() { 6. } else { 12. }))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(radius::PILL)
        .border_1()
        .text_size(fs(11.))
        .line_height(lh(fs(11.)))
        .css_weight(700.)
        .cursor_pointer()
        .transition(M_FAST, EASE_OUT)
        .map(|s| {
            if active {
                s.bg(p.accent_grad.background()).border_color(p.border_accent).text_color(fg).shadow(shadows(&[p.accent_glow]))
            } else {
                s.bg(p.glass.background()).border_color(p.border).text_color(fg).hover(move |s| s.border_color(border_strong).bg(hover_bg).text_color(strong).translate(0., -1.))
            }
        })
        .child(ico(icon, 13., if active { p.text_on_accent } else { p.text_accent }))
        .child(label)
        .when_some(count, |s, n| {
            s.child(
                div()
                    .min_w(px(18.))
                    .h(px(18.))
                    .px(px(5.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(radius::PILL)
                    .bg(if active { css_rgba(255, 255, 255, 0.2) } else { p.accent_soft })
                    .text_color(if active { p.text_on_accent } else { p.text_accent })
                    .text_size(fs(9.))
                    .line_height(lh(fs(9.)))
                    .css_weight(800.)
                    .child(count_text(n)),
            )
        })
}

/// Bölüm başlığı: küçük başlık + başlık, sağda isteğe bağlı öğe.
pub fn section_head(kick: String, title: String, trailing: Option<AnyElement>, p: &Palette) -> Div {
    div()
        .w_full()
        .flex()
        .items_end()
        .gap(px(12.))
        .child(div().flex_1().min_w_0().flex().flex_col().gap(px(5.)).child(kicker(kick, p.text_accent)).child(div().text_size(fs(17.)).line_height(lh(fs(17.))).css_weight(750.).tracking(-0.02).text_color(p.text_strong).child(title)))
        .children(trailing)
}

/// Yüklenirken gösterilen iskelet kutusu (nabız animasyonlu).
pub fn skeleton(id: impl Into<ElementId>, w: f32, h: f32, r: f32, p: &Palette) -> AnyElement {
    div()
        .w(px(w))
        .h(px(h))
        .flex_none()
        .rounded(px(r))
        .border_1()
        .border_color(p.border_subtle)
        .bg(p.glass.background())
        .with_animation(id, p2_ui::motion::pulse(), |el, t| el.opacity(p2_ui::motion::pulse_opacity(t)))
        .into_any_element()
}

/// Boş durum: halkalı işaret + başlık + açıklama (+ eylem).
pub fn empty_state(icon: &'static str, title: String, detail: String, action: Option<AnyElement>, p: &Palette) -> Div {
    div()
        .w_full()
        .py(px(46.))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(8.))
        .rounded(radius::LG)
        .border_1()
        .border_color(p.border_subtle)
        .bg(p.glass.background())
        .child(
            div()
                .relative()
                .size(px(76.))
                .mb(px(6.))
                .flex()
                .items_center()
                .justify_center()
                .child(div().absolute().inset_0().rounded_full().border_1().border_color(p.border))
                .child(div().absolute().inset(px(10.)).rounded_full().border_1().border_color(p.border_strong).bg(p.accent_soft))
                .child(ico(icon, 24., p.text_accent)),
        )
        .child(div().text_size(fs(15.)).line_height(lh(fs(15.))).css_weight(750.).text_color(p.text_strong).child(title))
        .child(div().max_w(px(380.)).text_center().text_size(fs(11.5)).line_height(relative(1.55)).text_color(p.text_3).child(detail))
        .children(action.map(|a| div().mt(px(8.)).child(a)))
}

/// İkincil düğme (cam): ikon + metin.
pub fn ghost_button(id: impl Into<ElementId>, icon: Option<&'static str>, label: impl Into<SharedString>, p: &Palette) -> Stateful<Div> {
    let (hover_bg, strong, border_strong) = (p.bg_hover, p.text_strong, p.border_strong);
    div()
        .id(id)
        .h(px(32.))
        .px(px(12.))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(7.))
        .rounded(radius::MD)
        .border_1()
        .border_color(p.border)
        .bg(p.glass.background())
        .text_color(p.text_2)
        .text_size(fs(11.5))
        .line_height(lh(fs(11.5)))
        .css_weight(700.)
        .cursor_pointer()
        .transition(M_FAST, EASE_OUT)
        .hover(move |s| s.bg(hover_bg).border_color(border_strong).text_color(strong))
        .when_some(icon, |s, body| s.child(ico(body, 14., p.text_accent)))
        .child(label.into())
}

/// Kart kabuğu: cam zemin, kenarlık; üzerine gelince 3px yükselir, vurgu kenarlığı ve derin gölge.
pub fn card_shell(id: impl Into<ElementId>, group: SharedString, w: f32, p: &Palette) -> Stateful<Div> {
    let (accent, lg) = (p.border_accent, p.shadow_lg());
    div()
        .id(id)
        .group(group)
        .relative()
        .w(px(w))
        .flex_none()
        .flex()
        .flex_col()
        .rounded(radius::LG)
        .border_1()
        .border_color(p.border)
        .bg(p.bg_2)
        .shadow(shadows(&[p.inset_top]))
        .cursor_pointer()
        .transition(M_BASE, EASE_POP)
        .hover(move |s| s.translate(0., -3.).border_color(accent).shadow(lg.clone()))
}

/// Seçilebilir karo (görünürlük: Özel / Herkese açık): ikon + başlık + açıklama + radyo işareti.
pub fn choice_tile(id: impl Into<ElementId>, icon: &'static str, title: String, hint: String, on: bool, p: &Palette) -> Stateful<Div> {
    let (hover, border, strong) = (p.bg_hover, p.border_strong, p.text_strong);
    div()
        .id(id)
        .flex_1()
        .min_w_0()
        .p(px(12.))
        .flex()
        .items_start()
        .gap(px(10.))
        .rounded(radius::MD)
        .border_1()
        .cursor_pointer()
        .transition(M_FAST, EASE_OUT)
        .map(|s| if on { s.border_color(p.border_accent).bg(p.accent_soft).shadow(shadows(&[p.ring])) } else { s.border_color(p.border).bg(p.bg_3).hover(move |s| s.bg(hover).border_color(border).text_color(strong)) })
        .child(div().size(px(30.)).flex_none().flex().items_center().justify_center().rounded(px(9.)).bg(if on { p.accent_soft_2 } else { p.bg_2 }).child(ico(icon, 15., if on { p.text_accent } else { p.text_3 })))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(3.))
                .child(div().text_size(fs(12.)).css_weight(750.).text_color(if on { p.text_strong } else { p.text }).child(title))
                .child(div().text_size(fs(10.)).line_height(relative(1.45)).text_color(p.text_3).child(hint)),
        )
        .child(div().mt(px(2.)).size(px(14.)).flex_none().rounded_full().flex().items_center().justify_center().map(|s| if on { s.bg(p.accent).child(div().size(px(5.)).rounded_full().bg(p.text_on_accent)) } else { s.border_1().border_color(p.border_strong) }))
}

/// Giriş gecikmesi (kart sırası; tavanı motion'da).
pub fn stagger(i: usize) -> Duration {
    Duration::from_millis(40 * i.min(8) as u64)
}
