//! Görev panosu ortak görünüm parçaları — board.css'in birebir karşılığı (dom-styles.mjs ile ölçüldü).
//!
//! Sütun/öncelik tonları (`--p2-status-*`), `.board-button`, yerel `select` görünümü, `.board-avatar`,
//! `.board-kind` / `.board-labels` / `.board-row-priority` çipleri, tek kenarı renkli çerçeve (CSS'te
//! `border-top: 3px solid ton` + 1px yan kenarlar; GPUI'de tek kenarlık rengi olduğundan üst üste iki kutu)
//! ve tarih biçimleri (`toLocaleDateString('tr-TR', …)`).

use gpui_kit::prelude::*;
use p2_ui::P2Fx as _; // backdrop_blur
use gpui_kit::{AnyElement, Div, FontWeight, Hsla, SharedString, Stateful, TextRun, Window, div, font, px};
use p2_client_core::{Member, Task};
use p2_ui::P2Text as _;
use p2_ui::tokens::{FONT, FONT_MONO, Palette, Shadow, fs, radius, shadows};

/// BoardPage.tsx `COLUMNS` etiketleri.
pub fn column_label(status: &str) -> &'static str {
    match status {
        "TODO" | "BACKLOG" => "Yapılacak",
        "IN_PROGRESS" => "Devam ediyor",
        "REVIEW" => "İncelemede",
        "DONE" => "Bitti",
        _ => "—",
    }
}

/// `--p2-status-todo/progress/review/done`.
pub fn column_tone(status: &str, p: &Palette) -> Hsla {
    match status {
        "IN_PROGRESS" => p.accent,
        "REVIEW" => p.warning,
        "DONE" => p.success,
        _ => p.slate_300,
    }
}

/// `.board-card.priority-*` sol kenar rengi.
pub fn priority_edge(priority: &str, p: &Palette) -> Hsla {
    match priority {
        "LOW" => p.success,
        "HIGH" => p.warning,
        "URGENT" => p.danger,
        _ => p.accent,
    }
}

/// `.board-priority` noktası (orta öncelik `accent-hover`).
pub fn priority_dot(priority: &str, p: &Palette) -> Hsla {
    match priority {
        "LOW" => p.success,
        "HIGH" => p.warning,
        "URGENT" => p.danger,
        _ => p.accent_hover,
    }
}

pub fn priority_label(priority: &str) -> &'static str {
    p2_client_core::priority_label(priority)
}

/// api/tasks.ts `KIND_SHORT` (kart rozeti).
pub fn kind_short(kind: &str) -> &'static str {
    match kind {
        "BUG" => "Hata",
        "FEATURE" => "Özellik",
        "IMPROVEMENT" => "İyileştir",
        "DOCS" => "Belge",
        _ => "Görev",
    }
}

/// Türkçe büyük harf (`text-transform: uppercase`, tr yerel ayarı).
pub fn tr_upper(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'i' => vec!['İ'],
            'ı' => vec!['I'],
            c => c.to_uppercase().collect(),
        })
        .collect()
}

/// `.board-kind`: h 16, p 0 6, hap, `800 9.5px` .03em büyük harf, beyaz yazı türün renginde.
pub fn kind_badge(kind: &str, p: &Palette) -> Option<Div> {
    let bg = match kind {
        "BUG" => p.danger,
        "FEATURE" => p.accent,
        "IMPROVEMENT" => p.warning,
        "DOCS" => p.success,
        _ => return None,
    };
    Some(
        div()
            .flex_none()
            .h(px(16.))
            .px(px(6.))
            .flex()
            .items_center()
            .rounded(radius::PILL)
            .bg(bg)
            .text_color(p.text_on_accent)
            .text_size(fs(9.5))
            .css_weight(800.)
            .tracking(0.03)
            .child(tr_upper(kind_short(kind))),
    )
}

/// `.board-labels em`: h 17, p 0 7, 1px kenarlık, hap, bg-2, `600 10px` text-2. `big`: ayrıntı paneli (h 20, 11px).
pub fn label_chips(task: &Task, max: usize, big: bool, p: &Palette) -> Option<Div> {
    let labels = task.label_list();
    if labels.is_empty() {
        return None;
    }
    let chip = |text: String| {
        div()
            .flex_none()
            .max_w(px(120.))
            .h(px(if big { 20. } else { 17. }))
            .px(px(7.))
            .flex()
            .items_center()
            .overflow_hidden()
            .border_1()
            .border_color(p.border)
            .rounded(radius::PILL)
            .bg(p.bg_2)
            .text_color(p.text_2)
            .text_size(fs(if big { 11. } else { 10. }))
            .css_weight(600.)
            .whitespace_nowrap()
            .child(div().truncate().child(text))
    };
    let mut row = div().flex().flex_wrap().gap(px(4.));
    for l in labels.iter().take(max) {
        row = row.child(chip(l.clone()));
    }
    if labels.len() > max {
        row = row.child(chip(format!("+{}", labels.len() - max)));
    }
    Some(row)
}

/// `.board-row-priority`: p 2 8, hap, `750 9.5px`, öncelik tonunda yumuşak zemin.
pub fn priority_chip(priority: &str, p: &Palette) -> Div {
    let (bg, fg) = match priority {
        "LOW" => (p.success_soft, p.success_text),
        "HIGH" => (p.warning_soft, p.warning_text),
        "URGENT" => (p.danger_soft, p.danger_text),
        _ => (p.accent_soft, p.text_accent),
    };
    div().flex_none().px(px(8.)).py(px(2.)).rounded(radius::PILL).bg(bg).text_color(fg).text_size(fs(9.5)).css_weight(750.).child(priority_label(priority))
}

/// `font: 700 10px var(--p2-font-mono)` (kısa yazımlı `font` ölçeklenmez).
pub fn mono(text: impl Into<SharedString>, size: f32, color: Hsla) -> Div {
    div().flex_none().font_family(FONT_MONO).text_size(px(size)).font_weight(FontWeight::BOLD).text_color(color).child(text.into())
}

#[derive(Clone, Copy, PartialEq)]
pub enum Btn {
    Plain,
    Primary,
    Danger,
    Subtle,
}

/// `.board-button(.primary/.danger/.subtle)`: h 30, p 0 12, r 8, `650 11.5px`.
pub fn board_button(id: impl Into<gpui_kit::ElementId>, label: impl Into<SharedString>, kind: Btn, disabled: bool, p: &Palette) -> Stateful<Div> {
    sized_button(id, label, kind, disabled, 30., 12., fs(11.5), p)
}

#[allow(clippy::too_many_arguments)]
pub fn sized_button(id: impl Into<gpui_kit::ElementId>, label: impl Into<SharedString>, kind: Btn, disabled: bool, h: f32, pad: f32, size: gpui_kit::Pixels, p: &Palette) -> Stateful<Div> {
    let base = div()
        .id(id)
        .flex_none()
        .h(px(h))
        .px(px(pad))
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .rounded(radius::SM)
        .text_size(size)
        .css_weight(650.)
        .whitespace_nowrap()
        .when(disabled, |s| s.opacity(0.45))
        .when(!disabled, |s| s.cursor_pointer())
        .child(label.into());
    let (hover, strong, bs) = (p.bg_hover, p.text_strong, p.border_strong);
    match kind {
        Btn::Plain => base.border_color(p.border).bg(p.bg_3).text_color(p.text).when(!disabled, |s| s.hover(move |s| s.bg(hover).border_color(bs).text_color(strong))),
        Btn::Primary => {
            let grad = p.accent_grad.clone();
            let hot = p2_ui::tokens::Gradient { angle: grad.angle, stops: grad.stops.iter().map(|(c, at)| (p2_ui::form::brighten(*c, 1.1), *at)).collect() };
            base.border_color(gpui_kit::transparent_black())
                .bg(grad.background())
                .text_color(p.text_on_accent)
                .shadow(shadows(&[p.accent_glow_strong, p.inset_top]))
                .when(!disabled, |s| s.hover(move |s| s.bg(hot.background()).text_color(gpui_kit::white())))
        }
        Btn::Danger => {
            let d = p.danger;
            base.border_color(p.danger_border).bg(p.danger_soft).text_color(p.danger_text).when(!disabled, |s| s.hover(move |s| s.bg(d).border_color(d).text_color(gpui_kit::white())))
        }
        Btn::Subtle => base.border_color(gpui_kit::transparent_black()).text_color(p.text_3).when(!disabled, |s| s.hover(move |s| s.bg(hover).text_color(strong))),
    }
}

/// Yazı genişliği (Chromium yerel `select` kutusunun içeriğe göre genişliği için).
pub fn text_width(window: &Window, text: &str, size: gpui_kit::Pixels, weight: f32) -> f32 {
    let mut f = font(FONT);
    f.weight = FontWeight(weight);
    let text = if text.is_empty() { " " } else { text };
    let run = TextRun { len: text.len(), font: f, color: gpui_kit::black(), background_color: None, underline: None, strikethrough: None };
    f32::from(window.text_system().shape_line(SharedString::from(text.to_string()), size, &[run], None).width)
}

const CHEVRON: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" width="12" height="12" viewBox="0 0 12 12"><path d="M2 4.2 6 8.2l4-4" fill="none" stroke="black" stroke-width="1.9" stroke-linecap="round" stroke-linejoin="round"/></svg>"#;

/// Chromium'un koyu şemalı yerel `select`'i (`.board-controls select`): h 30, p 0 8, r 8, bg-3,
/// `650 11.5px`; genişlik en uzun seçenek + ok alanı; ok sağda 4px içeride.
#[allow(clippy::too_many_arguments)]
pub fn native_select(id: impl Into<gpui_kit::ElementId>, label: impl Into<SharedString>, width: f32, h: f32, pad: f32, size: gpui_kit::Pixels, weight: f32, open: bool, p: &Palette) -> Stateful<Div> {
    div()
        .id(id)
        .relative()
        .flex_none()
        .w(px(width))
        .h(px(h))
        .pl(px(pad))
        .pr(px(28.))
        .flex()
        .items_center()
        .border_1()
        .rounded(radius::SM)
        .bg(p.bg_3)
        .cursor_pointer()
        .map(|s| if open { s.border_color(p.border_accent).shadow(shadows(&[p.ring])) } else { s.border_color(p.border) })
        .child(div().flex_1().min_w_0().truncate().text_size(size).css_weight(weight).text_color(p.text).child(label.into()))
        .child(div().absolute().right(px(3.)).top(px((h - 2. - 12.) / 2.)).child(gpui_kit::svg().data(CHEVRON).size(px(12.)).text_color(p.text)))
}

/// Yerel `select`in genişliği: en uzun seçenek + iki yan boşluk + kenarlık + ok.
pub fn select_width(window: &Window, options: &[String], size: gpui_kit::Pixels, weight: f32, pad: f32, max: f32) -> f32 {
    let w = options.iter().map(|o| text_width(window, o, size, weight)).fold(0., f32::max);
    (w + pad + 28. + 2.).ceil().min(max)
}

/// Açık `select` listesi (Chromium'un yerel açılır listesi yerine cam liste; seçili satır vurgulu).
pub fn select_list(id: &str, options: &[String], selected: usize, width: f32, p: &Palette) -> (Stateful<Div>, Vec<Stateful<Div>>) {
    let list = div()
        .id(SharedString::from(format!("{id}-list")))
        .occlude()
        .min_w(px(width))
        .max_h(px(300.))
        .overflow_y_scroll()
        .p(px(4.))
        .flex()
        .flex_col()
        .rounded(radius::SM)
        .border_1()
        .border_color(p.border_strong)
        .bg(p.glass.background()).backdrop_blur(p2_ui::fx::BLUR)
        .shadow(p.shadow_lg());
    let (hover, strong) = (p.accent_soft, p.text_strong);
    let rows = options
        .iter()
        .enumerate()
        .map(|(i, label)| {
            div()
                .id(SharedString::from(format!("{id}-opt-{i}")))
                .h(px(26.))
                .px(px(8.))
                .flex()
                .items_center()
                .rounded(px(6.))
                .text_size(fs(11.5))
                .whitespace_nowrap()
                .cursor_pointer()
                .map(|s| if i == selected { s.bg(p.accent_soft_2).text_color(p.text_strong) } else { s.text_color(p.text).hover(move |s| s.bg(hover).text_color(strong)) })
                .child(label.clone())
        })
        .collect();
    (list, rows)
}

/// `.board-avatar`: yuvarlak, 140° kimlik tonu gradyanı, `750` yazı (boy × .36, en az 8px; satır içi
/// stil olduğundan ölçeklenmez). Kişi yoksa kesikli `+` kutusu.
pub fn board_avatar(member: Option<&Member>, size: f32, p: &Palette) -> AnyElement {
    let font = (size * 0.36).round().max(8.);
    let Some(m) = member else {
        return div()
            .flex_none()
            .size(px(size))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .border_1()
            .border_dashed()
            .border_color(p.border_strong)
            .text_color(p.text_3)
            .text_size(px(font))
            .css_weight(750.)
            .child("+")
            .into_any_element();
    };
    if m.avatar_url.as_deref().is_some_and(|u| !u.is_empty()) {
        return p2_ui::components::avatar(&m.id, &m.display_name, size, p).into_any_element();
    }
    let h = p.hues[p2_ui::components::avatar_hue(&m.id) % 10];
    div()
        .flex_none()
        .size(px(size))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(gpui_kit::linear_gradient(140., gpui_kit::linear_color_stop(h.bg, 0.), gpui_kit::linear_color_stop(h.bg2, 1.)))
        .text_color(h.fg)
        .text_size(px(font))
        .css_weight(750.)
        .child(p2_ui::components::user_initials(&m.display_name))
        .into_any_element()
}

/// Tek kenarı kalın ve renkli çerçeve. CSS: `border: 1px solid <kenar>; border-<taraf>: 3px solid <ton>`.
/// GPUI kenarlığı tek renk ve tek kalınlık çizer: zeminlerle kurulur — dış kutu kenar renginde, köşe
/// kavsini de kapsayan ton şeridi (`strip`), üstte içerik kutusu (`inner`, opak dolgu çağıranda).
/// Kullanım: `outer.child(strip).child(inner.child(..))`.
pub fn top_toned(tone: Hsla, edge: Hsla, r: gpui_kit::Pixels) -> (Div, Div, Div) {
    let rr = f32::from(r);
    let outer = div().relative().flex().flex_col().rounded(r).bg(edge).pt(px(3.)).px(px(1.)).pb(px(1.));
    let strip = div().absolute().top_0().left_0().right_0().h(px(rr + 3.)).rounded_t(r).bg(tone);
    let inner = div().relative().flex_1().min_h_0().flex().flex_col().rounded_t(px((rr - 2.).max(0.))).rounded_b(px(rr - 1.)).overflow_hidden();
    (outer, strip, inner)
}

pub fn left_toned(tone: Hsla, edge: Hsla, r: gpui_kit::Pixels) -> (Div, Div, Div) {
    let rr = f32::from(r);
    let outer = div().relative().flex().rounded(r).bg(edge).pl(px(3.)).pr(px(1.)).py(px(1.));
    let strip = div().absolute().top_0().left_0().bottom_0().w(px(rr + 3.)).rounded_l(r).bg(tone);
    let inner = div().relative().flex_1().min_w_0().flex().flex_col().rounded_l(px((rr - 2.).max(0.))).rounded_r(px(rr - 1.)).overflow_hidden();
    (outer, strip, inner)
}

/// Yarı saydam zemin token'ının `bg-0` üstündeki opak karşılığı (iç içe zeminli çerçevelerde alttaki
/// kenar rengi görünmesin diye).
pub fn opaque(c: Hsla, base: Hsla) -> Hsla {
    p2_ui::components::over(c, base)
}

pub fn opaque_gradient(g: &p2_ui::tokens::Gradient, base: Hsla) -> gpui_kit::Background {
    p2_ui::tokens::Gradient { angle: g.angle, stops: g.stops.iter().map(|(c, at)| (opaque(*c, base), *at)).collect() }.background()
}

/// `--p2-accent-glow` gibi tek katman gölge.
pub fn glow(s: Shadow) -> Vec<gpui_kit::BoxShadow> {
    shadows(&[s])
}

// ---------------------------------------------------------------- tarih biçimleri

const MONTHS_SHORT: [&str; 12] = ["Oca", "Şub", "Mar", "Nis", "May", "Haz", "Tem", "Ağu", "Eyl", "Eki", "Kas", "Ara"];
const MONTHS: [&str; 12] = ["Ocak", "Şubat", "Mart", "Nisan", "Mayıs", "Haziran", "Temmuz", "Ağustos", "Eylül", "Ekim", "Kasım", "Aralık"];

pub fn local(iso: &str) -> Option<chrono::DateTime<chrono::Local>> {
    chrono::DateTime::parse_from_rfc3339(iso).ok().map(|d| d.with_timezone(&chrono::Local)).or_else(|| {
        chrono::NaiveDate::parse_from_str(iso.get(..10)?, "%Y-%m-%d").ok().and_then(|d| d.and_hms_opt(12, 0, 0)).and_then(|d| d.and_local_timezone(chrono::Local).single())
    })
}

/// `toLocaleDateString('tr-TR', {day:'numeric', month:'short'})` → "13 Kas".
pub fn day_text(iso: &str) -> String {
    use chrono::Datelike as _;
    local(iso).map(|d| format!("{} {}", d.day(), MONTHS_SHORT[d.month0() as usize])).unwrap_or_default()
}

/// `{day:'numeric', month:'long', year:'numeric'}` → "13 Kasım 2026".
pub fn long_day(iso: &str) -> String {
    use chrono::Datelike as _;
    local(iso).map(|d| format!("{} {} {}", d.day(), MONTHS[d.month0() as usize], d.year())).unwrap_or_default()
}

/// `{day:'numeric', month:'long'}` (sprint takvimi alt yazısı) → "13 Kasım".
pub fn day_month(date: chrono::NaiveDate) -> String {
    use chrono::Datelike as _;
    format!("{} {}", date.day(), MONTHS[date.month0() as usize])
}

pub fn month_year(date: chrono::NaiveDate) -> String {
    use chrono::Datelike as _;
    format!("{} {}", MONTHS[date.month0() as usize], date.year())
}

/// Yorum zamanı: `{day:'numeric', month:'short', hour:'2-digit', minute:'2-digit'}` → "13 Kas 14:20".
pub fn comment_time(iso: &str) -> String {
    use chrono::{Datelike as _, Timelike as _};
    local(iso).map(|d| format!("{} {} {:02}:{:02}", d.day(), MONTHS_SHORT[d.month0() as usize], d.hour(), d.minute())).unwrap_or_default()
}

/// Bitiş geçmiş mi (bitmiş görev geç sayılmaz).
pub fn is_late(task: &Task) -> bool {
    task.status != "DONE" && task.due_at.as_deref().and_then(local).is_some_and(|d| d < chrono::Local::now())
}

/// Sprint ilerlemesi: (yüzde, kalan gün).
pub fn sprint_progress(starts: Option<&str>, ends: Option<&str>) -> Option<(f32, i64)> {
    let (a, b) = (local(starts?)?, local(ends?)?);
    let (start, end, now) = (a.timestamp_millis() as f64, b.timestamp_millis() as f64, chrono::Local::now().timestamp_millis() as f64);
    if end <= start {
        return None;
    }
    let percent = (((now - start) / (end - start)) * 100.).round().clamp(0., 100.) as f32;
    let left = ((end - now) / 86_400_000.).ceil() as i64;
    Some((percent, left))
}

/// Pencereyi başlık çubuğunun altından örten katman (Electron'daki `position: fixed; inset: 0` pencereler;
/// başlık çubuğu onların üstünde kalır). Katmanın üst boşluğu 0 olmalı (CSS'teki 38px bu kayma). `child` kendi `absolute().inset_0()` katmanını taşır.
pub fn window_cover(child: impl IntoElement, window: &Window) -> AnyElement {
    let size = window.viewport_size();
    gpui_kit::deferred(gpui_kit::anchored().position_mode(gpui_kit::AnchoredPositionMode::Window).position(gpui_kit::point(px(0.), px(38.))).child(div().relative().w(size.width).h(size.height - px(38.)).child(child)))
        .with_priority(3)
        .into_any_element()
}
