//! Sohbet satırı ve açılır pencerelerin görünümü — ChannelContent.tsx (`ChannelMessage`,
//! `MessageBody`, `Avatar`, `DayDivider`), messageMenu.tsx (`MessageTools`, `ReactionBar`,
//! `ThreadLink`, `MessageMenuPopup`), MessageRichText.tsx, attachments.tsx `FileView`.
//! Ölçüler `channel-content.css`/`direct.css`'in Electron'da HESAPLANMIŞ değerleri (px aynen,
//! yazı `fs()` ile 0,85 ölçekli). Satır hem kanal akışında hem iş parçacığı panelinde kullanılır;
//! eylemler `RowCb` ile çağırana döner (görünüm sahibi kendi durumunu günceller).

use std::ops::Range;
use p2_ui::P2Fx as _; // backdrop_blur
use std::rc::Rc;

use chrono::{DateTime, Datelike, Local, Timelike};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, ClickEvent, FontStyle, FontWeight, HighlightStyle, ImageSource, InteractiveText, MouseButton, MouseDownEvent, ObjectFit, Pixels, Point, SharedString, StyledText, Window, div, img, linear_color_stop, linear_gradient, px,
};
use p2_client_core::Message;
use p2_ui::P2Text as _;
use p2_ui::P2Motion as _;
use p2_ui::components::{avatar_hue, user_initials};
use p2_ui::icons::icon;
use p2_ui::rich::{Inline, RichBlock, rich_blocks};
use p2_ui::tokens::{FONT_EMOJI, FONT_MONO, Palette, Shadow, fs, shadows};

use super::extras::{FileRef, ImageCache, Kind, Slot, format_size, preview_kind, split_content};

// ---------------------------------------------------------------- zaman

fn local(ts: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(ts).ok().map(|d| d.with_timezone(&Local))
}

/// `new Date(ts).toLocaleString('tr-TR')` → `13.11.2026 05:05:08`.
pub fn tr_datetime(ts: &str) -> String {
    local(ts).map(|d| d.format("%d.%m.%Y %H:%M:%S").to_string()).unwrap_or_default()
}

/// `toLocaleTimeString('tr-TR', {hour, minute})` → `05:05`.
pub fn tr_time(ts: &str) -> String {
    local(ts).map(|d| format!("{:02}:{:02}", d.hour(), d.minute())).unwrap_or_default()
}

/// `toDateString()` karşılaştırması için yerel gün.
pub fn day_key(ts: &str) -> Option<chrono::NaiveDate> {
    local(ts).map(|d| d.date_naive())
}

/// `DayDivider`: bugünse "Bugün", değilse `toLocaleDateString('tr-TR', {day, month:'long', weekday:'long'})`
/// ("13 Kasım Perşembe"); CSS `text-transform: uppercase` (tr).
pub fn day_label(ts: &str) -> String {
    let Some(d) = local(ts) else { return String::new() };
    let text = if d.date_naive() == Local::now().date_naive() {
        p2_i18n::t("chat.day.today")
    } else {
        p2_i18n::tv("chat.day.date", &[("day", &d.day().to_string()), ("month", &p2_i18n::month_name(d.month0() as usize)), ("weekday", &p2_i18n::weekday_name(d.weekday().num_days_from_monday() as usize))])
    };
    tr_upper(&text)
}

pub fn tr_upper(s: &str) -> String {
    p2_i18n::upper(s)
}

/// CSS `text-transform: capitalize` (tr): her kelimenin ilk harfi büyük.
pub fn tr_capitalize(s: &str) -> String {
    let mut out = String::new();
    let mut start = true;
    for c in s.chars() {
        if start && c.is_alphabetic() {
            out.push_str(&tr_upper(&c.to_string()));
        } else {
            out.push(c);
        }
        start = c.is_whitespace();
    }
    out
}

// ---------------------------------------------------------------- eylemler

/// Satırdan görünüm sahibine giden eylemler. Konumlar pencere koordinatıdır.
#[derive(Clone, Debug)]
pub enum RowAct {
    Profile,
    React(String),
    Thread,
    /// Sağ tık (`openAtPoint`).
    Menu(Point<Pixels>),
    /// Araç çubuğundaki ••• (`openFrom(box)`; sağ kenar, alt kenar, üst kenar).
    MenuFrom { right: Pixels, bottom: Pixels, top: Pixels },
    /// Araç çubuğundaki tepki düğmesi (`picker.openAt(box, true)`).
    PickerFrom { right: Pixels, bottom: Pixels, top: Pixels },
    Quote,
    Download(FileRef),
    View(FileRef),
    Mention(String),
    Url(String),
    CopyCode(String, String),
    Hover(bool),
    // L ajanı: medya oynatıcıları ve YouTube kartı.
    Play(FileRef),
    /// İlerleme çubuğu (0..1 oran).
    Seek(FileRef, f32),
    Volume(FileRef, f32),
    Mute(FileRef),
    Fullscreen(FileRef),
    Youtube(String),
}

pub type RowCb = Rc<dyn Fn(RowAct, &mut Window, &mut App)>;

fn emit(cb: &RowCb, act: RowAct) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let cb = cb.clone();
    move |_, window, cx| {
        cx.stop_propagation();
        cb(act.clone(), window, cx)
    }
}

// ---------------------------------------------------------------- satır seçenekleri

/// Satırın yerleşim türü (CSS sınıfları).
#[derive(Clone, Copy, PartialEq)]
pub enum RowKind {
    /// `.message.slack-message` (akış): sütun 38, avatar 36.
    Feed,
    /// `.thread-root`: sütun 34 (avatar 32), boşluk 9, gövde 12px, üst boşluk 12.
    ThreadRoot,
    /// `.thread-reply` (panel içi): sütun 34, avatar 32.
    ThreadReply,
}

pub struct RowOpts<'a> {
    pub message: &'a Message,
    pub kind: RowKind,
    pub hovered: bool,
    /// `.latest` (yeni gelen mesajın kısa vurgusu)
    pub latest: bool,
    /// `.flash` (aramadan gidilen mesaj)
    pub flash: bool,
    /// Araç çubuğu düğmeleri (Electron `actions`).
    pub quote: bool,
    pub thread: bool,
    pub images: &'a ImageCache,
    /// L ajanı: oynatıcılar (yoksa video/ses durağan çizilir).
    pub players: Option<&'a super::media::Players>,
    pub copied: Option<&'a str>,
    pub cb: RowCb,
}

/// Mesaj yazarı: mesajla gelen fotoğraf adresi önbelleğe kaydedilir (proje dışı kişiler, DM'ler).
fn author_tile(a: &p2_client_core::models::Author, size: f32, radius: f32, font: Pixels, p: &Palette) -> gpui_kit::Div {
    if a.avatar_url.as_deref().is_some_and(|u| !u.trim().is_empty()) {
        crate::avatars::remember(&a.id, a.avatar_url.as_deref());
    }
    avatar_tile(&a.id, &a.display_name, size, radius, font, p)
}

/// `--avatar-bg` → `--avatar-bg2` 145° gradyan, baş harfler 800.
pub fn avatar_tile(key: &str, name: &str, size: f32, radius: f32, font: Pixels, p: &Palette) -> gpui_kit::Div {
    let hue = p.hues[avatar_hue(key)];
    let base = div().flex_none().size(px(size)).flex().items_center().justify_center().rounded(px(radius)).overflow_hidden();
    if let Some(image) = crate::avatars::image(key) {
        return base.child(img(ImageSource::Render(image)).size_full().rounded(px(radius)).object_fit(ObjectFit::Cover));
    }
    // Satır kutusu = kutu yüksekliği: baş harfler esnek ortalamanın piksel yuvarlaması olmadan tam ortada.
    base.bg(linear_gradient(145., linear_color_stop(hue.bg, 0.), linear_color_stop(hue.bg2, 1.))).text_color(hue.fg).text_size(font).line_height(px(size)).css_weight(800.).child(p2_ui::components::initials_label(user_initials(name)))
}

// ---------------------------------------------------------------- zengin metin

/// `MessageRichText`: `<p>` satırları (satır yüksekliği 1.45) ve kod blokları.
pub fn rich_text(id: &str, visible: &str, size: Pixels, p: &Palette, cb: &RowCb, copied: Option<&str>) -> Option<AnyElement> {
    if visible.is_empty() {
        return None;
    }
    let line_h = px((f32::from(size) * 1.45 * 100.).round() / 100.);
    let mut col = div().w_full().flex().flex_col();
    for (bi, block) in rich_blocks(visible).into_iter().enumerate() {
        match block {
            RichBlock::Para(lines) => {
                let mut para = div().mt(px(2.)).w_full().flex().flex_col().text_size(size).line_height(line_h).text_color(p.text);
                for (li, line) in lines.into_iter().enumerate() {
                    para = para.child(rich_line(&format!("{id}-{bi}-{li}"), line, line_h, p, cb));
                }
                col = col.child(para);
            }
            RichBlock::Code(code) => {
                let key = format!("{id}-{bi}");
                let done = copied == Some(key.as_str());
                let (hover, strong) = (p.bg_hover, p.text_strong);
                col = col.child(
                    div()
                        .relative()
                        .w_full()
                        .max_w(px(720.))
                        .my(px(7.))
                        .child(
                            div()
                                .id(SharedString::from(format!("code-{key}")))
                                .w_full()
                                .pt(px(11.))
                                .pb(px(11.))
                                .pl(px(13.))
                                .pr(px(70.))
                                // Yatay kaydırma: kaydırılabilir genişlik yalnız doğrudan çocuktan ölçülür;
                                // metin `flex_none` sarmalayıcıda tam satır genişliğini alır (yoksa kutuya
                                // gerilip uzun satırlar kırpılıyordu). Dikey tekerlek mesaj listesinde
                                // kalır; blok yalnız yatay girişle (shift+tekerlek, dokunmatik yüzey) kayar.
                                .flex()
                                .overflow_x_scroll()
                                .restrict_scroll_to_axis()
                                .border_1()
                                .border_color(p.border)
                                .rounded(px(8.))
                                .bg(p.bg_0)
                                .font_family(FONT_MONO)
                                .text_size(px(11.))
                                .line_height(px(17.05))
                                .text_color(p.text)
                                .whitespace_nowrap()
                                .child(div().flex_none().child(code.clone())),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!("copy-{key}")))
                                .absolute()
                                .top(px(12.))
                                .right(px(7.))
                                .px(px(7.))
                                .py(px(4.))
                                .border_1()
                                .border_color(p.border)
                                .rounded(px(5.))
                                .bg(p.bg_3)
                                .text_size(fs(9.))
                                .line_height(px(9.))
                                .text_color(p.text_2)
                                .cursor_pointer()
                                .hover(move |s| s.bg(hover).text_color(strong))
                                .child(p2_i18n::t(if done { "common.copied" } else { "act.copy" }))
                                .on_click(emit(cb, RowAct::CopyCode(key.clone(), code))),
                        ),
                );
            }
        }
    }
    Some(col.into_any_element())
}

/// Bir `<p>` satırı: biçimli parçalar tek metinde (vurgu aralıkları), bağlantı ve bahsetmeler tıklanır.
/// `button.mention` 1px 3px iç boşluklu satır içi blok: satır kutusu 18px'e çıkar; yatay boşluk
/// ince boşlukla (U+2009 ≈ 3px) taklit edilir.
fn rich_line(id: &str, line: Vec<Inline>, line_h: Pixels, p: &Palette, cb: &RowCb) -> AnyElement {
    if line.is_empty() {
        return div().h(line_h).into_any_element();
    }
    let mut text = String::new();
    let mut highlights: Vec<(Range<usize>, HighlightStyle)> = Vec::new();
    let mut clicks: Vec<(Range<usize>, RowAct)> = Vec::new();
    let mut has_mention = false;
    for part in line {
        let start = text.len();
        match part {
            Inline::Text(t) => text.push_str(&t),
            Inline::Bold(t) => {
                text.push_str(&t);
                highlights.push((start..text.len(), HighlightStyle { font_weight: Some(FontWeight(750.)), color: Some(p.text_strong), ..Default::default() }));
            }
            Inline::Italic(t) => {
                text.push_str(&t);
                highlights.push((start..text.len(), HighlightStyle { font_style: Some(FontStyle::Italic), color: Some(p.text_2), ..Default::default() }));
            }
            Inline::Link { label, url } => {
                text.push_str(&label);
                highlights.push((start..text.len(), HighlightStyle { color: Some(p.link), ..Default::default() }));
                clicks.push((start..text.len(), RowAct::Url(url)));
            }
            Inline::Mention(m) => {
                has_mention = true;
                text.push('\u{2009}');
                text.push_str(&m);
                text.push('\u{2009}');
                highlights.push((start..text.len(), HighlightStyle { font_weight: Some(FontWeight(650.)), color: Some(p.text_accent), background_color: Some(p.accent_soft), ..Default::default() }));
                clicks.push((start..text.len(), RowAct::Mention(m.trim_start_matches('@').to_string())));
            }
        }
    }
    let styled = StyledText::new(text).with_highlights(highlights);
    let el: AnyElement = if clicks.is_empty() {
        styled.into_any_element()
    } else {
        let ranges: Vec<Range<usize>> = clicks.iter().map(|c| c.0.clone()).collect();
        let acts: Vec<RowAct> = clicks.into_iter().map(|c| c.1).collect();
        let cb = cb.clone();
        InteractiveText::new(SharedString::from(format!("rt-{id}")), styled)
            .on_click(ranges, move |ix, window, cx| {
                if let Some(a) = acts.get(ix) {
                    cb(a.clone(), window, cx)
                }
            })
            .into_any_element()
    };
    div().w_full().when(has_mention, |s| s.py(px(1.))).child(el).into_any_element()
}

// ---------------------------------------------------------------- ekler (FileView)

fn download_button(id: String, file: &FileRef, h: f32, pad: (f32, f32), p: &Palette, cb: &RowCb) -> impl IntoElement {
    let soft = p.accent_soft;
    div()
        .id(SharedString::from(id))
        .flex_none()
        .h(px(h))
        .px(px(pad.1))
        .flex()
        .items_center()
        .rounded(px(5.))
        .text_size(fs(9.))
        .text_color(p.link)
        .cursor_pointer()
        .hover(move |s| s.bg(soft))
        .child(p2_i18n::t("chat.download"))
        .on_click(emit(cb, RowAct::Download(file.clone())))
}

/// `.message-attachments`: genişlik min(520px, %100), aralık 8.
pub fn attachments(id: &str, files: &[FileRef], images: &ImageCache, p: &Palette, cb: &RowCb) -> AnyElement {
    attachments_with(id, files, images, None, p, cb)
}

pub fn attachments_with(id: &str, files: &[FileRef], images: &ImageCache, players: Option<&super::media::Players>, p: &Palette, cb: &RowCb) -> AnyElement {
    let mut col = div().mt(px(8.)).w_full().max_w(px(520.)).flex().flex_col().gap(px(8.));
    for (i, f) in files.iter().enumerate() {
        let key = format!("{id}-{i}");
        let label = format!("{} · {}", f.name, format_size(f.size));
        let kind = preview_kind(&f.name, &f.mime);
        let (border_strong, sm) = (p.border_strong, p.shadow_sm());
        match kind {
            Kind::Image | Kind::Svg => {
                let slot = images.get(&f.path());
                let (pic, width): (AnyElement, f32) = match slot {
                    Some(Slot::Ready(image, w, h)) => {
                        let scale = (518. / *w as f32).min(360. / *h as f32).min(1.);
                        let (dw, dh) = ((*w as f32 * scale).round(), (*h as f32 * scale).round());
                        (div().w_full().flex().justify_center().child(img(ImageSource::Render(image.clone())).w(px(dw)).h(px(dh)).object_fit(ObjectFit::Contain)).into_any_element(), (dw + 2.).max(240.))
                    }
                    _ => (div().into_any_element(), 240.),
                };
                let view = f.clone();
                col = col.child(
                    div()
                        .id(SharedString::from(format!("img-{key}")))
                        .w(px(width))
                        .max_w_full()
                        .flex()
                        .flex_col()
                        .overflow_hidden()
                        .border_1()
                        .border_color(p.border)
                        .rounded(px(11.))
                        .bg(p.bg_2)
                        .cursor_pointer()
                        .hover(move |s| s.border_color(border_strong).shadow(sm.clone()))
                        .on_click(emit(cb, RowAct::View(view)))
                        .child(pic)
                        .child(
                            div()
                                .w_full()
                                .pt(px(5.))
                                .pb(px(5.))
                                .pl(px(9.))
                                .pr(px(7.))
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap(px(8.))
                                .child(div().min_w_0().truncate().text_size(fs(9.)).text_color(p.text_3).child(label))
                                .child(download_button(format!("dl-{key}"), f, 23., (0., 5.), p, cb)),
                        ),
                );
            }
            Kind::Text => {
                let body: AnyElement = match images.get(&f.path()) {
                    Some(Slot::Failed) => div().p(px(12.)).text_size(fs(10.)).text_color(p.text_3).child(p2_i18n::t("chat.textPreviewFailed")).into_any_element(),
                    other => {
                        let text = match other {
                            // <pre>: sondaki tek satır sonu yeni satır açmaz.
                            Some(Slot::Text(t)) if !t.is_empty() => t.strip_suffix("\r\n").or_else(|| t.strip_suffix('\n')).unwrap_or(t).to_string(),
                            _ => p2_i18n::t("chat.textLoading"),
                        };
                        div()
                            .id(SharedString::from(format!("txt-{key}")))
                            .max_h(px(320.))
                            .overflow_y_scroll()
                            .p(px(12.))
                            .bg(p.bg_0)
                            .font_family(FONT_MONO)
                            .text_size(px(11.))
                            .line_height(px(17.05))
                            .text_color(p.text)
                            .child(text)
                            .into_any_element()
                    }
                };
                let view = f.clone();
                col = col.child(
                    div()
                        .id(SharedString::from(format!("tp-{key}")))
                        .w_full()
                        .flex()
                        .flex_col()
                        .overflow_hidden()
                        .border_1()
                        .border_color(p.border)
                        .rounded(px(11.))
                        .bg(p.bg_2)
                        .cursor_pointer()
                        .on_click(emit(cb, RowAct::View(view)))
                        .child(
                            div()
                                .h(px(18.))
                                .px(px(10.))
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap(px(12.))
                                .text_size(fs(9.))
                                .text_color(p.text_3)
                                .child(div().min_w_0().truncate().child(label))
                                .child(download_button(format!("dl-{key}"), f, 17., (4., 7.), p, cb)),
                        )
                        .child(body),
                );
            }
            Kind::Pdf => col = col.child(super::media_view::pdf_preview(&key, f, images, p, cb)),
            Kind::Video => col = col.child(super::media_view::video_preview(&key, f, players.and_then(|m| m.get(&f.path())), p, cb)),
            Kind::Audio => col = col.child(super::media_view::audio_preview(&key, f, players.and_then(|m| m.get(&f.path())), p, cb)),
            kind => {
                // `.message-file` (belge, tablo, arşiv…); PDF/ses/video önizlemesi yok → aynı kart.
                let tag = match kind {
                    Kind::Docx => p2_i18n::t("chat.tag.doc"),
                    Kind::Sheet => p2_i18n::t("chat.tag.sheet"),
                    Kind::Slides => p2_i18n::t("chat.tag.slides"),
                    Kind::Archive => p2_i18n::t("chat.tag.archive"),
                    Kind::Epub => "EPUB".to_string(),
                    Kind::Font => "FONT".to_string(),
                    Kind::Pdf => "PDF".to_string(),
                    Kind::Audio => p2_i18n::t("chat.tag.audio"),
                    Kind::Video => p2_i18n::t("chat.tag.video"),
                    _ => p2_i18n::t("chat.tag.file"),
                };
                let small_tag = !matches!(kind, Kind::Binary | Kind::Pdf | Kind::Audio | Kind::Video);
                let (tag_bg, tag_fg) = if kind == Kind::Pdf { (p.danger_bg, p.danger_text) } else { (p.bg_selected, p.text_accent) };
                let view = f.clone();
                col = col.child(
                    div()
                        .id(SharedString::from(format!("file-{key}")))
                        .w_full()
                        .min_h(px(54.))
                        .px(px(11.))
                        .py(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .border_1()
                        .border_color(p.border)
                        .rounded(px(11.))
                        .bg(p.bg_3)
                        .cursor_pointer()
                        .hover(move |s| s.border_color(border_strong))
                        .on_click(emit(cb, RowAct::View(view)))
                        .child(div().flex_none().w(px(42.)).child(div().w(px(40.)).h(px(38.)).flex().items_center().justify_center().rounded(px(8.)).bg(tag_bg).text_color(tag_fg).text_size(fs(if small_tag { 7. } else { 8. })).css_weight(850.).child(tag)))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap(px(3.))
                                .child(div().truncate().text_size(fs(11.)).css_weight(700.).text_color(p.text).child(f.name.clone()))
                                .child(div().text_size(fs(9.)).text_color(p.text_3).child(p2_i18n::tv("chat.file.openToPreview", &[("size", &format_size(f.size))]))),
                        )
                        .child(div().text_size(fs(9.)).text_color(p.link).child(p2_i18n::t("chat.preview"))),
                );
            }
        }
    }
    col.into_any_element()
}

// ---------------------------------------------------------------- tepkiler, iş parçacığı bağlantısı

pub fn reactions(id: &str, m: &Message, p: &Palette, cb: &RowCb) -> Option<AnyElement> {
    if m.reactions.is_empty() {
        return None;
    }
    let mut row = div().mt(px(7.)).flex().flex_wrap().gap(px(5.));
    for r in &m.reactions {
        let (border, bg, fg) = if r.reacted { (p.border_accent, p.accent_soft, p.text_accent) } else { (p.border, p.bg_3, p.text_2) };
        let (hb, hbg, hfg) = if r.reacted { (p.accent_hover, p.accent_soft_2, p.text_accent) } else { (p.border_strong, p.bg_hover, p.text_strong) };
        row = row.child(
            div()
                .id(SharedString::from(format!("rx-{id}-{}", r.emoji)))
                .h(px(25.))
                .px(px(9.))
                .flex()
                .items_center()
                .gap(px(5.))
                .border_1()
                .border_color(border)
                .rounded(px(13.))
                .bg(bg)
                .text_color(fg)
                .cursor_pointer()
                .hover(move |s| s.border_color(hb).bg(hbg).text_color(hfg))
                .child(div().font_family(FONT_EMOJI).text_size(fs(13.)).line_height(fs(13.)).child(r.emoji.clone()))
                .child(div().text_size(fs(10.)).css_weight(750.).child(r.count.to_string()))
                .tooltip(super::chat_header::tip_owned(p2_i18n::t(if r.reacted { "chat.react.remove" } else { "chat.react.give" })))
                .on_click(emit(cb, RowAct::React(r.emoji.clone()))),
        );
    }
    Some(row.into_any_element())
}

/// messageMenu.tsx `ThreadLink`: soldan kıvrılan bağlantı çizgisi, yanıtlayan avatarları, "N yanıt".
fn thread_link(id: &str, m: &Message, p: &Palette, cb: &RowCb) -> Option<AnyElement> {
    let count = m.count.replies;
    if count == 0 {
        return None;
    }
    let mut seen = std::collections::HashSet::new();
    let people: Vec<&str> = m.replies.iter().map(|r| r.author.display_name.as_str()).filter(|n| seen.insert(*n)).take(3).collect();
    let (border_accent, soft, hover_line) = (p.border_accent, p.accent_soft, p.accent_hover);
    let gid = SharedString::from(format!("tl-{id}"));
    let mut faces = div().flex().pr(px(2.));
    for (i, name) in people.iter().enumerate() {
        let hue = p.hues[avatar_hue(name)];
        faces = faces.child(
            // 2px `bg-0` halka (box-shadow) → dış zemin + iç daire.
            div().flex_none().size(px(24.)).m(px(-2.)).when(i > 0, |s| s.ml(px(-9.))).p(px(2.)).rounded_full().bg(p.bg_0).child(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(linear_gradient(145., linear_color_stop(hue.bg, 0.), linear_color_stop(hue.bg2, 1.)))
                    .text_color(hue.fg)
                    .text_size(fs(8.))
                    .line_height(px(20.))
                    .css_weight(750.)
                    .tracking(-0.02)
                    .child(p2_ui::components::initials_label(user_initials(name))),
            ),
        );
    }
    let last = m.replies.first().map(|r| tr_time(&r.created_at));
    Some(
        div()
            .id(gid.clone())
            .group(gid.clone())
            .relative()
            .mt(px(7.))
            .mb(px(1.))
            .pt(px(3.))
            .pb(px(3.))
            .pl(px(7.))
            .pr(px(9.))
            .flex()
            .items_center()
            .gap(px(8.))
            .border_1()
            .border_color(gpui_kit::transparent_black())
            .rounded(px(99.))
            .text_size(fs(11.))
            .text_color(p.text_accent)
            .cursor_pointer()
            .hover(move |s| s.border_color(border_accent).bg(soft))
            .child(div().absolute().left(px(-14.)).top(px(-8.)).w(px(11.)).h(px(15.)).border_l_2().border_b_2().border_color(p.navy_500).rounded_bl(px(9.)).group_hover(gid.clone(), move |s| s.border_color(hover_line)))
            .when(!people.is_empty(), |s| s.child(faces))
            .child(div().css_weight(650.).child(p2_i18n::tv("chat.replies", &[("n", &count.to_string())])))
            .when_some(last, |s, t| s.child(div().text_size(px(7.79)).text_color(p.text_3).child(p2_i18n::tv("chat.lastReply", &[("time", &t)]))))
            .child(div().opacity(0.65).group_hover(gid, |s| s.opacity(1.)).child("›"))
            .tooltip(super::chat_header::tip_owned(p2_i18n::t("chat.openThread")))
            .on_click(emit(cb, RowAct::Thread))
            .into_any_element(),
    )
}

// ---------------------------------------------------------------- satır

/// `.message-actions` düğmesi (32×30, ikon 16).
fn action_button(id: String, name: &'static str, stroke: f32, tip: String, p: &Palette) -> gpui_kit::Stateful<gpui_kit::Div> {
    let (soft, strong) = (p.accent_soft, p.text_strong);
    let gid = SharedString::from(id.clone());
    div()
        .id(gid.clone())
        .group(gid.clone())
        .w(px(32.))
        .h(px(30.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .hover(move |s| s.bg(soft))
        .child(icon(name).stroke(stroke).el().size(px(16.)).text_color(p.text_3).group_hover(gid, move |s| s.text_color(strong)))
        .tooltip(super::chat_header::tip_owned(tip))
}

pub fn message_row(o: RowOpts, p: &Palette) -> AnyElement {
    let m = o.message;
    let id = m.id.clone();
    let cb = o.cb.clone();
    let (col, av, gap, body_size, radius) = match o.kind {
        RowKind::Feed => (38., 36., 10., fs(13.), 12.),
        RowKind::ThreadRoot => (34., 32., 9., fs(12.), 12.),
        RowKind::ThreadReply => (34., 32., 10., fs(13.), 12.),
    };
    let (visible, files) = split_content(&m.content);
    let mut body = div().flex_1().min_w_0().flex().flex_col();
    if let Some(q) = &m.quoted_message {
        let quoted = q.content.split_whitespace().collect::<Vec<_>>().join(" ");
        body = body.child(
            div()
                .relative()
                .mt(px(1.))
                .mb(px(6.))
                .pl(px(10.))
                .flex()
                .flex_col()
                .gap(px(2.))
                .text_size(fs(10.))
                .text_color(p.text_3)
                .child(div().absolute().left_0().top(px(2.)).bottom(px(2.)).w(px(3.)).rounded(px(3.)).bg(p.accent_hover))
                .child(div().css_weight(700.).text_color(p.text_2).child(q.author.display_name.clone()))
                .child(div().truncate().child(quoted)),
        );
    }
    if m.forwarded_from_id.is_some() {
        body = body.child(div().mb(px(3.)).text_size(fs(9.)).css_weight(700.).text_color(p.text_accent).child(format!("↗ {}", p2_i18n::t("chat.forwarded"))));
    }
    let name_gid = SharedString::from(format!("an-{id}"));
    body = body.child(
        div()
            .h(px(18.))
            .flex()
            .items_start()
            .gap(px(if o.kind == RowKind::ThreadRoot { 7. } else { 8. }))
            .child(
                div()
                    .id(name_gid)
                    .text_size(fs(13.))
                    .line_height(px(14.))
                    .css_weight(700.)
                    .text_color(p.text_strong)
                    .cursor_pointer()
                    .hover(|s| s.underline())
                    .child(m.author.display_name.clone())
                    .tooltip(super::chat_header::tip_owned(p2_i18n::tv("chat.openProfile", &[("name", &m.author.display_name)])))
                    .on_click(emit(&cb, RowAct::Profile)),
            )
            .child(div().mt(px(4.)).text_size(fs(9.)).line_height(px(9.)).text_color(p.text_4).child(format!("{}{}", tr_datetime(&m.created_at), if m.edited_at.is_some() { format!(" · {}", p2_i18n::t("chat.edited")) } else { String::new() }))),
    );
    if let Some(rt) = rich_text(&id, &visible, body_size, p, &cb, o.copied) {
        body = body.child(rt);
    }
    if !files.is_empty() {
        body = body.child(attachments_with(&id, &files, o.images, o.players, p, &cb));
    }
    // MessageRichText: YouTube kartları (uzak resim önizlemesi Electron'da hiç yüklenmiyor — bkz. media.rs).
    for yt in super::media::youtube_ids(&visible) {
        body = body.child(super::media_view::youtube_card(&id, &yt, o.images, p, &cb));
    }
    let mut content = div().flex_1().min_w_0().flex().flex_col().child(body).children(reactions(&id, m, p, &cb));
    if o.kind == RowKind::Feed {
        content = content.children(thread_link(&id, m, p, &cb).map(|t| div().flex().child(t)));
    }

    let toolbar = o.hovered.then(|| {
        let picker = cb.clone();
        let more = cb.clone();
        let mut bar = div()
            .absolute()
            .top(px(if o.kind == RowKind::Feed { -14. } else { -12. }))
            .right(px(if o.kind == RowKind::Feed { 9. } else { 6. }))
            .flex()
            .overflow_hidden()
            .border_1()
            .border_color(p.border_strong)
            .rounded(px(8.))
            .bg(p.glass.background())
            .shadow(p.shadow_md())
            .child(action_button(format!("ma-r-{id}"), "composer:reaction", 1.7, p2_i18n::t("chat.addReaction"), p).on_mouse_down(MouseButton::Left, move |e: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                // Düğme kutusu: araç çubuğu satırın sağ üstünde; tıklama noktasından kenarlar yaklaşık.
                picker(RowAct::PickerFrom { right: e.position.x + px(16.), bottom: e.position.y + px(15.), top: e.position.y - px(15.) }, window, cx)
            }));
        if o.quote {
            bar = bar.child(action_button(format!("ma-q-{id}"), "composer:quote", 1.7, p2_i18n::t("chat.quote"), p).on_click(emit(&cb, RowAct::Quote)));
        }
        if o.thread {
            bar = bar.child(action_button(format!("ma-t-{id}"), "composer:thread", 1.7, p2_i18n::t("chat.replyInThread"), p).on_click(emit(&cb, RowAct::Thread)));
        }
        bar.child(action_button(format!("ma-m-{id}"), "composer:more", 2.6, p2_i18n::t("chat.moreActions"), p).on_mouse_down(MouseButton::Left, move |e: &MouseDownEvent, window, cx| {
            cx.stop_propagation();
            more(RowAct::MenuFrom { right: e.position.x + px(16.), bottom: e.position.y + px(15.), top: e.position.y - px(15.) }, window, cx)
        }))
    });

    let hover_cb = cb.clone();
    let menu_cb = cb.clone();
    let (latest_a, latest_b) = (p.accent_soft, p.accent_hover);
    let top_margin = if o.kind == RowKind::ThreadRoot { 12. } else { 0. };
    div()
        .id(SharedString::from(format!("m-{id}")))
        .relative()
        .w_full()
        .mt(px(top_margin))
        .flex()
        .gap(px(gap))
        .pt(px(8.))
        .pr(px(8.))
        .pb(px(8.))
        .pl(px(4.))
        .rounded(px(8.))
        .when(o.latest, |s| s.bg(linear_gradient(90., linear_color_stop(latest_a, 0.), linear_color_stop(gpui_kit::transparent_black(), 1.))).shadow(shadows(&[Shadow { x: 2., y: 0., blur: 0., spread: 0., color: latest_b, inset: true }])))
        .when(o.flash, |s| s.bg(p.warning_soft).shadow(shadows(&[Shadow { x: 2., y: 0., blur: 0., spread: 0., color: p.warning, inset: true }])))
        .when(o.hovered && !o.latest && !o.flash, |s| s.bg(p.hover_soft))
        .on_hover(move |over: &bool, window, cx| hover_cb(RowAct::Hover(*over), window, cx))
        .on_mouse_down(MouseButton::Right, move |e: &MouseDownEvent, window, cx| {
            cx.stop_propagation();
            menu_cb(RowAct::Menu(e.position), window, cx)
        })
        .child(
            div().flex_none().w(px(col)).child(
                div()
                    .id(SharedString::from(format!("av-{id}")))
                    .cursor_pointer()
                    .child(author_tile(&m.author, av, radius, fs(11.), p))
                    .tooltip(super::chat_header::tip_owned(p2_i18n::tv("chat.openProfile", &[("name", &m.author.display_name)])))
                    .on_click(emit(&cb, RowAct::Profile)),
            ),
        )
        .child(content)
        .children(toolbar)
        .into_any_element()
}

/// Forum konu kartı (ChannelContent.tsx `ForumTopicCard`, `.forum-feed article`): 38|1fr|auto ızgara,
/// 12px iç boşluk, 14px yarıçap, cam zemin; başlık, "yazar · tarih · N ek", tepkiler, sağda "N yanıt".
pub fn forum_card(m: &Message, hovered: bool, p: &Palette, cb: RowCb) -> AnyElement {
    let (visible, files) = split_content(&m.content);
    let title = if !visible.trim().is_empty() {
        visible.trim().to_string()
    } else {
        files.first().map(|f| f.name.rsplit_once('.').map_or(f.name.clone(), |(s, _)| s.trim().to_string())).unwrap_or_else(|| p2_i18n::t("chat.fileShare"))
    };
    let meta = format!("{} · {}{}", m.author.display_name, tr_datetime(&m.created_at), if files.is_empty() { String::new() } else { format!(" · {}", p2_i18n::tv("chat.attachCount", &[("n", &files.len().to_string())])) });
    let id = m.id.clone();
    let (hover_cb, menu_cb) = (cb.clone(), cb.clone());
    div()
        .id(SharedString::from(format!("topic-{id}")))
        .relative()
        .mb(px(7.))
        .p(px(12.))
        .flex()
        .items_center()
        .gap(px(10.))
        .border_1()
        .border_color(if hovered { p.border_strong } else { p.border })
        .rounded(px(14.))
        .bg(p.glass.background())
        .shadow(if hovered { p.shadow_md() } else { shadows(&[p.inset_top]) })
        .cursor_pointer()
        .on_hover(move |over: &bool, window, cx| hover_cb(RowAct::Hover(*over), window, cx))
        .on_mouse_down(MouseButton::Right, move |e: &MouseDownEvent, window, cx| {
            cx.stop_propagation();
            menu_cb(RowAct::Menu(e.position), window, cx)
        })
        .on_click(emit(&cb, RowAct::Thread))
        .child(div().flex_none().w(px(38.)).child(author_tile(&m.author, 34., 11., fs(11.), p)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .child(div().mb(px(4.)).truncate().text_size(fs(13.)).css_weight(700.).text_color(p.text_strong).child(title))
                .child(div().text_size(fs(10.)).css_weight(500.).text_color(p.text_3).child(meta))
                .children(reactions(&id, m, p, &cb)),
        )
        .child(div().flex_none().text_size(fs(10.)).css_weight(500.).text_color(p.text_3).child(p2_i18n::tv("chat.replies", &[("n", &m.count.replies.to_string())])))
        .into_any_element()
}

/// `.day-divider`: iki çizgi arasında hap (15px üst, 9px alt boşluk).
pub fn day_divider(ts: &str, p: &Palette) -> AnyElement {
    let size = fs(9.5);
    div()
        .mt(px(15.))
        .mb(px(9.))
        .h(px(20.))
        .flex()
        .items_center()
        .gap(px(10.))
        .child(div().flex_1().h(px(1.)).bg(p.border_subtle))
        .child(
            div()
                .h(px(20.))
                .px(px(10.))
                .flex()
                .items_center()
                .border_1()
                .border_color(p.border)
                .rounded(px(12.))
                .bg(p.bg_2)
                .text_size(size)
                .css_weight(800.)
                .text_color(p.text_accent)
                .tracking(0.14)
                .child(day_label(ts)),
        )
        .child(div().flex_1().h(px(1.)).bg(p.border_subtle))
        .into_any_element()
}

// ---------------------------------------------------------------- menüler (cam, 11px)

/// `.message-context-menu` kabı (genişlik CSS'ten; 6px iç boşluk).
pub fn menu_panel(id: &'static str, width: f32, pad: f32, p: &Palette) -> gpui_kit::Stateful<gpui_kit::Div> {
    div()
        .id(id)
        .w(px(width))
        .p(px(pad))
        .flex()
        .flex_col()
        .border_1()
        .border_color(p.border_strong)
        .rounded(px(11.))
        .bg(p.glass.background()).backdrop_blur(p2_ui::fx::BLUR)
        .shadow(p.shadow_lg())
        .occlude()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tone {
    Normal,
    Danger,
    /// `.menu-coming` (soluk + "yakında")
    Coming,
    /// `:disabled` (opaklık .42)
    Disabled,
}

/// `.message-context-menu > button`: 24px glif sütunu, 11px/650 metin, sağda isteğe bağlı iz.
pub fn menu_item(id: impl Into<SharedString>, glyph: &str, label: impl Into<SharedString>, tone: Tone, trail: Option<AnyElement>, height: f32, p: &Palette) -> gpui_kit::Stateful<gpui_kit::Div> {
    let (fg, glyph_fg, hover_bg, hover_fg) = match tone {
        Tone::Danger => (p.danger_text, p.danger_text, p.danger_soft, p.danger_text),
        Tone::Coming => (p.text_4, p.text_accent, p.accent_soft, p.text_strong),
        _ => (p.text, p.text_accent, p.accent_soft, p.text_strong),
    };
    let disabled = tone == Tone::Disabled;
    div()
        .id(id.into())
        .w_full()
        .flex_none()
        .min_h(px(height))
        .px(px(9.))
        .flex()
        .items_center()
        .rounded(px(6.))
        .text_size(fs(11.))
        .css_weight(650.)
        .text_color(fg)
        .when(disabled, |s| s.opacity(0.42))
        .when(!disabled, |s| s.cursor_pointer().hover(move |s| s.bg(hover_bg).text_color(hover_fg)))
        // Glifler Electron yığınındaki gibi Segoe UI (yoksa emoji yedeği) ile çizilir.
        .child(div().flex_none().w(px(24.)).flex().justify_center().font_family(p2_ui::tokens::FONT_FALLBACK).text_size(fs(15.)).text_color(glyph_fg).child(glyph.to_string()))
        .child(div().flex_1().min_w_0().truncate().child(label.into()))
        .children(trail)
}

pub fn menu_hr(p: &Palette) -> gpui_kit::Div {
    div().flex_none().h(px(1.)).mx(px(3.)).my(px(5.)).bg(p.border_subtle)
}

pub fn coming_tag(p: &Palette) -> AnyElement {
    div().flex_none().text_size(fs(8.)).css_weight(600.).text_color(p.text_4).child(p2_i18n::t("chat.comingSoon")).into_any_element()
}

/// `useMenuPlacement`: menü açılırken `p2-scale-in` (.92) ölçeğiyle ölçülür; Electron yerleşimi
/// o ölçekli kutuya göre hesaplar — aynısı. `x`/`y` pencere koordinatı, `up` üst kenar.
pub fn place_menu(x: f32, y: f32, up: f32, right: bool, w: f32, h: f32, window: &Window) -> (f32, f32) {
    let vp = window.viewport_size();
    let (vw, vh) = (f32::from(vp.width), f32::from(vp.height));
    let (bw, bh) = (w * 0.92, h * 0.92);
    let gap = 12.;
    let left = (if right { x - bw } else { x }).min(vw - bw - gap).max(gap);
    let fits = y + bh <= vh - gap;
    let top = (if fits { y } else { up - bh }).min(vh - bh - gap).max(gap);
    (left, top)
}

/// Çalışma alanı kökü pencerede başlık çubuğunun (38px) altında.
pub const ROOT_TOP: f32 = 38.;



// ---------------------------------------------------------------- mesaj bağlam menüsü (ortak)

/// messageMenu.tsx `MessageTools` menüsünün eylemleri.
#[derive(Clone)]
pub enum MenuAct {
    React(String),
    Picker,
    Quote,
    Edit,
    Thread,
    Forward,
    Copy,
    Save(FileRef),
    History,
    Link,
    Delete,
}

/// Menüde hangi maddeler var (`MessageActions`).
pub struct MenuSpec<'a> {
    pub message: &'a Message,
    pub editable: bool,
    pub deletable: bool,
    pub thread: bool,
    pub forward: bool,
    pub history: bool,
    pub link: bool,
}

pub type MenuCb = Rc<dyn Fn(MenuAct, &mut Window, &mut App)>;

/// `.message-context-menu` (230px, 34px satırlar); `x/y/up/right` `useMenuPlacement` girdileri.
pub fn message_menu_view(spec: MenuSpec, x: f32, y: f32, up: f32, right: bool, p: &Palette, window: &Window, cb: MenuCb) -> AnyElement {
    let m = spec.message;
    let act = |a: MenuAct| {
        let cb = cb.clone();
        move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
            cx.stop_propagation();
            cb(a.clone(), window, cx)
        }
    };
    let mut items: Vec<AnyElement> = Vec::new();
    let mut height = 12.;
    let mut strip = div().mb(px(6.)).flex().gap(px(5.));
    for e in ["💯", "⭐", "❤️", "🌷"] {
        let soft2 = p.accent_soft_2;
        strip = strip.child(
            div()
                .id(SharedString::from(format!("rs-{e}")))
                .flex_1()
                .h(px(41.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(7.))
                .bg(p.bg_3)
                .font_family(FONT_EMOJI)
                .text_size(fs(18.))
                .cursor_pointer()
                .hover(move |s| s.bg(soft2))
                .child(e)
                .tooltip(super::chat_header::tip_owned(p2_i18n::tv("chat.reactWith", &[("emoji", e)])))
                .on_click(act(MenuAct::React(e.to_string()))),
        );
    }
    items.push(strip.into_any_element());
    height += 47.;
    items.push(menu_item("mm-react", "☺", p2_i18n::t("chat.addReaction"), Tone::Normal, Some(div().text_size(fs(18.)).text_color(p.text_3).child("›").into_any_element()), 34., p).on_click(act(MenuAct::Picker)).into_any_element());
    items.push(menu_hr(p).into_any_element());
    height += 45.;
    let mut row = |id: &'static str, glyph: &str, label: String, tone: Tone, a: Option<MenuAct>| {
        let it = menu_item(id, glyph, label, tone, None, 34., p);
        items.push(match a {
            Some(a) => it.on_click(act(a)).into_any_element(),
            None => it.into_any_element(),
        });
        height += 34.;
    };
    row("mm-quote", "❝", p2_i18n::t("chat.quote"), Tone::Normal, Some(MenuAct::Quote));
    if spec.editable {
        row("mm-edit", "✎", p2_i18n::t("chat.edit"), Tone::Normal, Some(MenuAct::Edit));
    }
    if spec.thread {
        row("mm-thread", "↩", p2_i18n::t("chat.replyInThread"), Tone::Normal, Some(MenuAct::Thread));
    }
    if spec.forward {
        row("mm-forward", "↗", p2_i18n::t("chat.forward"), Tone::Normal, Some(MenuAct::Forward));
    }
    items.push(menu_hr(p).into_any_element());
    height += 11.;
    let mut row = |id: SharedString, glyph: &str, label: String, tone: Tone, trail: Option<AnyElement>, a: Option<MenuAct>| {
        let it = menu_item(id, glyph, label, tone, trail, 34., p);
        items.push(match a {
            Some(a) => it.on_click(act(a)).into_any_element(),
            None => it.into_any_element(),
        });
        height += 34.;
    };
    row("mm-copy".into(), "▤", p2_i18n::t("chat.copyText"), Tone::Normal, None, Some(MenuAct::Copy));
    let files = split_content(&m.content).1;
    let many = files.len() > 1;
    for (i, f) in files.into_iter().enumerate() {
        let label = if many { p2_i18n::tv("chat.saveAsNamed", &[("name", &f.name)]) } else { p2_i18n::t("chat.saveAs") };
        row(SharedString::from(format!("mm-save-{i}")), "↓", label, Tone::Normal, None, Some(MenuAct::Save(f)));
    }
    row("mm-pin".into(), "⚑", p2_i18n::t("chat.pin"), Tone::Coming, Some(coming_tag(p)), None);
    if spec.history && m.edited_at.is_some() {
        row("mm-history".into(), "🕘", p2_i18n::t("chat.editHistory"), Tone::Normal, None, Some(MenuAct::History));
    }
    if spec.link {
        row("mm-link".into(), "🔗", p2_i18n::t("chat.copyLink"), Tone::Normal, None, Some(MenuAct::Link));
    }
    if spec.deletable {
        items.push(menu_hr(p).into_any_element());
        items.push(menu_item("mm-delete", "🗑", p2_i18n::t("chat.delete"), Tone::Danger, None, 34., p).on_click(act(MenuAct::Delete)).into_any_element());
        height += 45.;
    }
    let (left, top) = place_menu(x, y, up, right, 230., height + 2., window);
    div()
        .absolute()
        .left(px(left))
        .top(px(top))
        .child(menu_panel("message-menu", 230., 6., p).children(items).p2_enter("message-menu", p2_ui::Entrance::ScaleIn, std::time::Duration::ZERO))
        .into_any_element()
}

/// Yerel `title` ipucu (iş parçacığı paneli gibi kardeş modüller için).
pub fn tip_text(text: &'static str) -> impl Fn(&mut Window, &mut App) -> gpui_kit::AnyView + 'static {
    super::chat_header::tip(text)
}
