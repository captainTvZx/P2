//! Kütüphane (library.css) görsel ilkelleri ve biçimlendiriciler (library.ts): düğmeler, çipler,
//! rozetler, paneller, avatar; `formatBytes/formatDuration/formatEta/formatSpeed`, `timeAgo`,
//! `channelLabel`, `phaseLabel`. Ölçüler library.css'ten; düz `font-size: Npx` → `fs(N)`.

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Div, ElementId, FontWeight, Hsla, SharedString, Stateful, Svg, div, px, relative};
use p2_ui::P2Text as _;
use p2_ui::components::{avatar_hue, user_initials};
use p2_ui::icons::Icon;
use p2_ui::tokens::{Palette, Shadow, css_rgba, font_size, fs, radius, shadows};

// ------------------------------------------------------------------------------------------ ikonlar

pub const GEAR: &str = r#"<circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.7 1.7 0 00.3 1.8l.1.1a2 2 0 11-2.8 2.8l-.1-.1a1.7 1.7 0 00-1.8-.3 1.7 1.7 0 00-1 1.5V21a2 2 0 11-4 0v-.1a1.7 1.7 0 00-1.1-1.5 1.7 1.7 0 00-1.8.3l-.1.1a2 2 0 11-2.8-2.8l.1-.1a1.7 1.7 0 00.3-1.8 1.7 1.7 0 00-1.5-1H3a2 2 0 110-4h.1a1.7 1.7 0 001.5-1.1 1.7 1.7 0 00-.3-1.8l-.1-.1a2 2 0 112.8-2.8l.1.1a1.7 1.7 0 001.8.3h0a1.7 1.7 0 001-1.5V3a2 2 0 114 0v.1a1.7 1.7 0 001 1.5 1.7 1.7 0 001.8-.3l.1-.1a2 2 0 112.8 2.8l-.1.1a1.7 1.7 0 00-.3 1.8v0a1.7 1.7 0 001.5 1H21a2 2 0 110 4h-.1a1.7 1.7 0 00-1.5 1z" />"#;
pub const SEARCH: &str = r#"<circle cx="11" cy="11" r="7" /><path d="M20 20l-3.5-3.5" />"#;
pub const PENCIL: &str = r#"<path d="M12 20h9M16.5 3.5a2.1 2.1 0 013 3L7 19l-4 1 1-4z" />"#;
pub const DOWNLOAD: &str = r#"<path d="M12 4v11m0 0l-4.5-4.5M12 15l4.5-4.5M5 20h14" />"#;
pub const PLAY: &str = r#"<path d="M7 4.5v15l12-7.5z" />"#;
pub const STOP: &str = r#"<rect x="6" y="6" width="12" height="12" rx="2" />"#;
pub const PAUSE: &str = r#"<rect x="6" y="5" width="4" height="14" rx="1" /><rect x="14" y="5" width="4" height="14" rx="1" />"#;
pub const CARET: &str = r#"<path d="M6 9l6 6 6-6" />"#;
pub const PLUS: &str = r#"<path d="M12 5v14M5 12h14" />"#;
pub const GRIP: &str = r#"<path d="M22 2L2 22M22 12L12 22" />"#;
pub const DIFF_DOWN: &str = r#"<path d="M12 3v12m0 0l-4-4m4 4l4-4M5 21h14" />"#;

/// Satır içi SVG (viewBox 24): kontur kalınlığı ve boyut CSS'ten.
/// GPUI'de svg rengi miras alınmaz: `c` = CSS `currentColor`.
pub fn ico(body: &'static str, stroke: f32, size: f32, c: Hsla) -> Svg {
    Icon::raw(body).stroke(stroke).el().size(px(size)).text_color(c)
}

/// `fill: currentColor` SVG (oynat/durdur).
pub fn ico_fill(body: &'static str, size: f32, c: Hsla) -> Svg {
    Icon::raw(body).stroke(0.).filled().el().size(px(size)).text_color(c)
}

// ------------------------------------------------------------------------------------------ metin

/// `text-transform: uppercase` (`<html lang="tr">`: i → İ).
pub fn tr_upper(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'i' => vec!['İ'],
            'ı' => vec!['I'],
            c => c.to_uppercase().collect(),
        })
        .collect()
}

/// library.ts `channelLabel`: "alpha" → "Alpha".
pub fn channel_label(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => tr_upper(&first.to_string()) + chars.as_str(),
        None => String::new(),
    }
}

/// library.ts `phaseLabel`.
pub fn phase_label(phase: &str) -> &'static str {
    match phase {
        "preparing" => "Hazırlanıyor",
        "downloading" => "İndiriliyor",
        "waiting-for-exit" => "Oyun kapanınca güncellenecek",
        "installing" => "Kuruluyor",
        "verifying" => "Doğrulanıyor",
        "paused" => "Duraklatıldı",
        "done" => "Tamam",
        "failed" => "Hata",
        "canceled" => "İptal edildi",
        _ => "",
    }
}

/// library.ts `formatBytes` (ondalık nokta, `toFixed(1)`).
pub fn fmt_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".into();
    }
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let index = (((bytes as f64).ln() / 1024f64.ln()).floor() as usize).min(UNITS.len() - 1);
    let v = bytes as f64 / 1024f64.powi(index as i32);
    if index == 0 { format!("{v:.0} {}", UNITS[0]) } else { format!("{v:.1} {}", UNITS[index]) }
}

pub fn fmt_speed(bps: u64) -> String {
    format!("{}/s", fmt_bytes(bps))
}

/// library.ts `formatDuration`: "1 sa 20 dk", "35 dk", "12 sn"; sıfır "—".
pub fn fmt_duration(seconds: f64) -> String {
    if seconds <= 0. {
        return "—".into();
    }
    if seconds < 60. {
        return format!("{} sn", seconds.round().max(1.) as u64);
    }
    let hours = (seconds / 3600.).floor() as u64;
    let minutes = ((seconds % 3600.) / 60.).round() as u64;
    if hours >= 100 {
        return format!("{hours} sa");
    }
    if hours > 0 {
        return if minutes > 0 { format!("{hours} sa {minutes} dk") } else { format!("{hours} sa") };
    }
    format!("{minutes} dk")
}

/// library.ts `formatEta`: "~3 dk", "~45 sn".
pub fn fmt_eta(seconds: u64) -> String {
    match seconds {
        0 => String::new(),
        s if s < 60 => format!("~{} sn", s.max(1)),
        s if s < 3600 => format!("~{} dk", (s as f64 / 60.).round() as u64),
        s => format!("~{} sa {} dk", s / 3600, ((s % 3600) as f64 / 60.).round() as u64),
    }
}

const MONTHS: [&str; 12] = ["Ocak", "Şubat", "Mart", "Nisan", "Mayıs", "Haziran", "Temmuz", "Ağustos", "Eylül", "Ekim", "Kasım", "Aralık"];
const MONTHS_SHORT: [&str; 12] = ["Oca", "Şub", "Mar", "Nis", "May", "Haz", "Tem", "Ağu", "Eyl", "Eki", "Kas", "Ara"];

/// ISO-8601 → yerel saat (ms). Ayrıştırılamazsa `None`.
pub fn parse_ms(iso: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(iso).ok().map(|d| d.timestamp_millis())
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn local(ms: i64) -> chrono::DateTime<chrono::Local> {
    use chrono::TimeZone as _;
    chrono::Local.timestamp_millis_opt(ms).single().unwrap_or_else(chrono::Local::now)
}

/// LibraryPage.tsx `timeAgo` (ms): "az önce", "5 dk önce", "3 sa önce", "dün", "13 Kasım 2026".
pub fn time_ago_ms(ms: i64) -> String {
    use chrono::Datelike as _;
    let diff = (now_ms() - ms) as f64 / 1000.;
    if diff < 60. {
        return "az önce".into();
    }
    if diff < 3600. {
        return format!("{} dk önce", ((diff / 60.).floor() as i64).max(1));
    }
    if diff < 86400. {
        return format!("{} sa önce", (diff / 3600.).floor() as i64);
    }
    if diff < 172800. {
        return "dün".into();
    }
    let d = local(ms);
    format!("{} {} {}", d.day(), MONTHS[d.month0() as usize], d.year())
}

pub fn time_ago(iso: &str) -> String {
    parse_ms(iso).map(time_ago_ms).unwrap_or_default()
}

/// ProjectNotesPanel.tsx `noteTimeAgo` (kısa ay, yılsız).
pub fn note_time_ago(iso: &str) -> String {
    use chrono::Datelike as _;
    let Some(ms) = parse_ms(iso) else { return String::new() };
    let diff = (now_ms() - ms) as f64 / 1000.;
    if diff < 172800. {
        return time_ago_ms(ms);
    }
    let d = local(ms);
    format!("{} {}", d.day(), MONTHS_SHORT[d.month0() as usize])
}

/// `toLocaleDateString('tr-TR')` → "13.11.2026".
pub fn short_date(iso: &str) -> String {
    use chrono::Datelike as _;
    parse_ms(iso).map(|ms| local(ms)).map(|d| format!("{:02}.{:02}.{}", d.day(), d.month(), d.year())).unwrap_or_default()
}

// ------------------------------------------------------------------------------------------ yüzeyler

/// `.library-panel`: p 16 18, kenarlık `border`, r 14, `glass`, `inset-top`.
pub fn panel(p: &Palette) -> Div {
    div().flex().flex_col().pt(px(16.)).pb(px(16.)).px(px(18.)).border_1().border_color(p.border).rounded(radius::LG).bg(p.glass.background()).shadow(shadows(&[p.inset_top]))
}

/// `.library-section h3`: `800 fs-2xs`, aralık .18em, büyük harf, `text-accent`, mb 12, gap 8.
pub fn h3(text: &str, p: &Palette) -> Div {
    div().flex().items_center().gap(px(8.)).mb(px(12.)).min_h(px(10.)).child(kicker_text(text, p))
}

pub fn kicker_text(text: &str, p: &Palette) -> Div {
    div().text_size(font_size::XXS).css_weight(800.).tracking(0.18).text_color(p.text_accent).line_height(relative(1.2109)).child(tr_upper(text))
}

/// `.library-hint`: `text-3`, 12.5px, satır 1.5.
pub fn hint(text: impl Into<SharedString>, p: &Palette) -> Div {
    div().text_color(p.text_3).text_size(fs(12.5)).line_height(relative(1.5)).child(text.into())
}

/// `.library-count(.good/.mixed/.bad)`.
pub fn count(text: impl Into<SharedString>, tone: Option<&str>, p: &Palette) -> Div {
    let (bg, fg) = match tone {
        Some("good") => (p.success_soft, p.success_text),
        Some("mixed") => (p.warning_soft, p.warning_text),
        Some("bad") => (p.danger_soft, p.danger_text),
        _ => (p.bg_hover, p.text_2),
    };
    div().px(px(7.)).py(px(2.)).rounded(radius::PILL).bg(bg).text_color(fg).text_size(fs(10.)).css_weight(750.).line_height(relative(1.2109)).child(text.into())
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tone {
    Plain,
    Live,
    Draft,
    Installed,
}

/// `.library-badge(.live/.draft/.installed)`: p 3 8, hap, `700 9.5px`, büyük harf, aralık .04em.
pub fn badge(text: &str, tone: Tone, p: &Palette) -> Div {
    let (bg, fg) = match tone {
        Tone::Plain => (p.bg_hover, p.text_3),
        Tone::Live => (p.success_soft, p.success_text),
        Tone::Draft => (p.warning_soft, p.warning_text),
        Tone::Installed => (p.accent_soft, p.text_accent),
    };
    div().flex().flex_shrink_0().items_center().px(px(8.)).py(px(3.)).rounded(radius::PILL).bg(bg).text_color(fg).text_size(fs(9.5)).font_weight(FontWeight::BOLD).tracking(0.04).line_height(relative(1.2109)).child(tr_upper(text))
}

/// Kategori rengi (avatar ton paleti; `'channel:' + ad`).
pub fn channel_hue(name: &str, p: &Palette) -> p2_ui::tokens::Hue {
    p.hues[avatar_hue(&format!("channel:{name}")) % 10]
}

fn alpha(c: Hsla, a: f32) -> Hsla {
    Hsla { a: c.a * a, ..c }
}

/// `.library-chip-badge`: p 3 9 3 7, hap, ton zemini %70, `700 10px` aralık .03em, 7px nokta.
pub fn chip_badge(name: &str, p: &Palette) -> Div {
    let hue = channel_hue(name, p);
    div()
        .flex()
        .flex_shrink_0()
        .items_center()
        .gap(px(6.))
        .pl(px(7.))
        .pr(px(9.))
        .py(px(3.))
        .rounded(radius::PILL)
        .bg(alpha(hue.bg, 0.7))
        .text_color(hue.fg)
        .text_size(fs(10.))
        .font_weight(FontWeight::BOLD)
        .tracking(0.03)
        .line_height(relative(1.2109))
        .child(div().size(px(7.)).rounded_full().bg(hue.fg))
        .child(channel_label(name))
}

/// `.library-chip(.active)`: seçilebilir kategori çipi (yükleyici/düzenleme).
pub fn chip(id: impl Into<ElementId>, name: &str, active: bool, p: &Palette) -> Stateful<Div> {
    let hue = channel_hue(name, p);
    let hover = p.bg_hover;
    div()
        .id(id)
        .flex()
        .items_center()
        .gap(px(6.))
        .min_h(px(28.))
        .pl(px(9.))
        .pr(px(11.))
        .border_1()
        .rounded(radius::PILL)
        .text_size(fs(11.5))
        .css_weight(650.)
        .cursor_pointer()
        .map(|s| if active { s.border_color(hue.fg).bg(alpha(hue.bg, 0.7)).text_color(hue.fg) } else { s.border_color(p.border).bg(p.bg_3).text_color(p.text_2).hover(move |s| s.bg(hover)) })
        .child(div().size(px(8.)).rounded_full().bg(hue.fg))
        .child(channel_label(name))
}

/// `.library-avatar(.hue-N)`: yuvarlak, 145° kimlik gradyanı, `800` baş harf; fotoğraf varsa o.
pub fn lib_avatar(id: &str, name: &str, url: Option<&str>, size: f32, font_css: f32, p: &Palette) -> Div {
    if url.is_some() {
        crate::avatars::remember(id, url);
    }
    let h = p.hues[avatar_hue(id) % 10];
    let base = div()
        .size(px(size))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .overflow_hidden()
        .bg(gpui_kit::linear_gradient(145., gpui_kit::linear_color_stop(h.bg, 0.), gpui_kit::linear_color_stop(h.bg2, 1.)))
        .text_color(h.fg)
        .text_size(fs(font_css))
        .css_weight(800.)
        .line_height(relative(1.2109));
    match crate::avatars::image(id) {
        Some(image) => base.child(gpui_kit::img(gpui_kit::ImageSource::Render(image)).size_full().object_fit(gpui_kit::ObjectFit::Cover)),
        None => base.child(user_initials(name)),
    }
}

/// Proje ikonu (`.library-game-icon` / `.library-hero-icon`): ton sınıfı YOK → yedek renkler
/// (`bg-3 → bg-hover` / `bg-selected → blue-700`). Görsel varsa kapatır.
pub fn project_icon(project: &p2_client_core::Project, size: f32, r: f32, font_css: f32, hero: bool, p: &Palette) -> Div {
    let key = format!("project-icon:{}", project.id);
    crate::avatars::remember(&key, project.icon_url.as_deref());
    let (a, b, fg) = if hero { (p.bg_selected, p.blue_700, p.text_strong) } else { (p.bg_3, p.bg_hover, p.text) };
    let base = div()
        .size(px(size))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(r))
        .overflow_hidden()
        .bg(gpui_kit::linear_gradient(145., gpui_kit::linear_color_stop(a, 0.), gpui_kit::linear_color_stop(b, 1.)))
        .text_color(fg)
        .text_size(fs(font_css))
        .css_weight(800.)
        .line_height(relative(1.2109));
    match project.icon_url.as_ref().and(crate::avatars::image(&key)) {
        Some(image) => base.child(gpui_kit::img(gpui_kit::ImageSource::Render(image)).size_full().object_fit(gpui_kit::ObjectFit::Cover)),
        None => base.child(user_initials(&project.name)),
    }
}

// ------------------------------------------------------------------------------------------ düğmeler

/// `.library-primary`: min-h 36, p 0 16, r 8, `accent-grad`, `650 12.5px`, `accent-glow-strong` + inset.
pub fn primary(id: impl Into<ElementId>, label: impl Into<SharedString>, disabled: bool, p: &Palette) -> Stateful<Div> {
    let grad = p.accent_grad.clone();
    let hot = p2_ui::tokens::Gradient { angle: grad.angle, stops: grad.stops.iter().map(|(c, at)| (p2_ui::form::brighten(*c, 1.1), *at)).collect() };
    div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .gap(px(8.))
        .min_h(px(36.))
        .px(px(16.))
        .border_1()
        .border_color(gpui_kit::transparent_black())
        .rounded(radius::SM)
        .bg(grad.background())
        .text_color(p.text_on_accent)
        .text_size(fs(12.5))
        .css_weight(650.)
        .whitespace_nowrap()
        .shadow(shadows(&[p.accent_glow_strong, p.inset_top]))
        .map(|s| if disabled { s.opacity(0.5) } else { s.cursor_pointer().hover(move |s| s.bg(hot.background()).relative().top(px(-1.))) })
        .child(label.into())
}

#[derive(Clone, Copy, PartialEq)]
pub enum Sec {
    Plain,
    Accent,
    Danger,
}

/// `.library-secondary(.accent/.danger)`: min-h 38, p 0 15, kenarlık, r 8, `bg-3`, `650 12.5px`.
pub fn secondary(id: impl Into<ElementId>, label: impl Into<SharedString>, tone: Sec, disabled: bool, p: &Palette) -> Stateful<Div> {
    let (hover_bg, bs, strong, ds, dsoft) = (p.bg_hover, p.border_strong, p.text_strong, p.danger_border, p.danger_soft);
    let accent_hot = css_rgba(61, 220, 132, 0.28);
    let base = div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .gap(px(7.))
        .min_h(px(38.))
        .px(px(15.))
        .border_1()
        .rounded(radius::SM)
        .text_size(fs(12.5))
        .css_weight(650.)
        .whitespace_nowrap()
        .child(label.into());
    let base = match tone {
        Sec::Plain => base.border_color(p.border).bg(p.bg_3).text_color(p.text),
        Sec::Accent => base.border_color(p.success_border).bg(p.success_soft).text_color(p.success_text),
        Sec::Danger => base.border_color(p.border).bg(p.bg_3).text_color(p.danger_text),
    };
    if disabled {
        return base.opacity(0.5);
    }
    base.cursor_pointer().hover(move |s| match tone {
        Sec::Plain => s.bg(hover_bg).border_color(bs).text_color(strong),
        Sec::Accent => s.bg(accent_hot),
        Sec::Danger => s.border_color(ds).bg(dsoft),
    })
}

/// `.library-h3-action`: başlık sağında hap düğme (h 24, `650 10.5px`).
pub fn h3_action(id: impl Into<ElementId>, label: &str, p: &Palette) -> Stateful<Div> {
    let (hover, strong) = (p.bg_hover, p.text_strong);
    div()
        .id(id)
        .ml_auto()
        .h(px(24.))
        .px(px(10.))
        .flex()
        .items_center()
        .border_1()
        .border_color(p.border)
        .rounded(radius::PILL)
        .bg(p.bg_3)
        .text_color(p.text_2)
        .text_size(fs(10.5))
        .css_weight(650.)
        .cursor_pointer()
        .hover(move |s| s.bg(hover).text_color(strong))
        .child(label.to_string())
}

/// `.library-icon-btn`: 34×34, r 8, `text-3`; hover `accent-soft` + 1px yukarı.
pub fn icon_btn(id: impl Into<ElementId>, body: &'static str, stroke: f32, p: &Palette) -> Stateful<Div> {
    let (soft, strong) = (p.accent_soft, p.text_strong);
    let id: ElementId = id.into();
    let group = SharedString::from(format!("{id:?}"));
    div()
        .id(id)
        .group(group.clone())
        .size(px(34.))
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(radius::SM)
        .text_color(p.text_3)
        .cursor_pointer()
        .hover(move |s| s.bg(soft).text_color(strong).relative().top(px(-1.)))
        .child(ico(body, stroke, 18., p.text_3).group_hover(group, move |s| s.text_color(strong)))
}

/// Modal başlığı kapat düğmesi (30×30, `×` 19px).
pub fn close_x(id: impl Into<ElementId>, p: &Palette) -> Stateful<Div> {
    let (soft, strong) = (p.accent_soft, p.text_strong);
    div()
        .id(id)
        .size(px(30.))
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(radius::XS)
        .text_color(p.text_3)
        .text_size(fs(19.))
        .line_height(relative(1.))
        .cursor_pointer()
        .hover(move |s| s.bg(soft).text_color(strong))
        .child("×")
}

/// Modal alt düğmesi (`footer button`): h 32, p 0 14, `650 11px`; `primary` vurgu gradyanı.
pub fn foot_btn(id: impl Into<ElementId>, label: impl Into<SharedString>, primary: bool, disabled: bool, p: &Palette) -> Stateful<Div> {
    let (hover, strong) = (p.bg_hover, p.text_strong);
    div()
        .id(id)
        .h(px(32.))
        .px(px(14.))
        .flex()
        .items_center()
        .border_1()
        .rounded(radius::SM)
        .text_size(fs(11.))
        .css_weight(650.)
        .whitespace_nowrap()
        .map(|s| if primary { s.border_color(gpui_kit::transparent_black()).bg(p.accent_grad.background()).text_color(p.text_on_accent).shadow(shadows(&[p.accent_glow_strong, p.inset_top])) } else { s.border_color(p.border).bg(p.bg_3).text_color(p.text) })
        .map(|s| if disabled { s.opacity(0.5) } else if primary { s.cursor_pointer() } else { s.cursor_pointer().hover(move |s| s.bg(hover).text_color(strong)) })
        .child(label.into())
}

/// Küçük kart eylemi (`.library-card-actions button`): min-h 30, p 0 12, `bg-2`, `650 11.5px`.
pub fn card_btn(id: impl Into<ElementId>, label: impl Into<SharedString>, kind: CardBtn, p: &Palette) -> Stateful<Div> {
    let (hover, strong, dang, white, okhot) = (p.bg_hover, p.text_strong, p.danger, gpui_kit::white(), css_rgba(61, 220, 132, 0.3));
    let base = div().id(id).min_h(px(30.)).px(px(12.)).flex().items_center().border_1().rounded(radius::SM).text_size(fs(11.5)).css_weight(650.).whitespace_nowrap().cursor_pointer().child(label.into());
    match kind {
        CardBtn::Plain => base.border_color(p.border).bg(p.bg_2).text_color(p.text).hover(move |s| s.bg(hover).text_color(strong)),
        CardBtn::Active => base.border_color(p.border_accent).bg(p.accent_soft).text_color(p.text_strong),
        CardBtn::Ok => base.border_color(p.success_border).bg(p.success_soft).text_color(p.success_text).hover(move |s| s.bg(okhot)),
        CardBtn::Danger => base.border_color(p.danger_border).bg(p.danger_soft).text_color(p.danger_text).hover(move |s| s.bg(dang).text_color(white)),
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum CardBtn {
    Plain,
    Active,
    Ok,
    Danger,
}

/// `.eyebrow` + `h2` (sürüm/başarım pencereleri).
pub fn modal_title(eyebrow: &str, title: impl Into<SharedString>, p: &Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .min_w_0()
        .child(div().text_size(font_size::XXS).css_weight(800.).tracking(0.18).text_color(p.text_accent).child(tr_upper(eyebrow)))
        .child(div().mt(px(3.)).text_size(fs(16.)).css_weight(750.).text_color(p.text_strong).child(title.into()))
}

/// `.library-upload-modal` (ve kardeşleri): genişlik `w`, p 20, kenarlık `border-strong`, r 18,
/// `glass-strong`, `shadow-lg`.
pub fn modal_card(w: f32, window: &gpui_kit::Window, p: &Palette) -> Div {
    let vw = f32::from(window.viewport_size().width);
    let vh = f32::from(window.viewport_size().height);
    div()
        .relative()
        .flex()
        .flex_col()
        .w(px(w.min(vw - 48.)))
        .max_h(px(vh - 64.))
        .p(px(20.))
        .border_1()
        .border_color(p.border_strong)
        .rounded(radius::XL)
        .bg(p.glass_strong.background())
        .shadow(p.shadow_lg())
        .text_color(p.text)
}

/// `.library-field > span` etiketi (`650 10px`, `text-3`).
pub fn field_label(text: &str, p: &Palette) -> Div {
    div().text_size(fs(10.)).css_weight(650.).text_color(p.text_3).child(text.to_string())
}

/// `.library-error`.
pub fn error_bar(text: impl Into<SharedString>, p: &Palette) -> Div {
    div().flex().items_center().gap(px(10.)).py(px(9.)).px(px(12.)).border_1().border_color(p.danger_border).rounded(radius::MD).bg(p.danger_soft).text_color(p.danger_text).text_size(fs(12.)).line_height(relative(1.5)).child(text.into())
}

/// Gölge kısayolu.
pub fn sh(x: f32, y: f32, blur: f32, spread: f32, c: Hsla) -> Shadow {
    Shadow::new(x, y, blur, spread, c)
}

/// Bir öğe listesini `AnyElement`'e çevirir (kolaylık).
pub fn any(e: impl IntoElement) -> AnyElement {
    e.into_any_element()
}
