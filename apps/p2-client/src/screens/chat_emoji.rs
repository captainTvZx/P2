//! Emoji paneli — EmojiPanel.tsx (emoji-picker-react, koyu tema, yerel emoji, ten rengi kapalı,
//! önizleme kapalı) ve P2 sarmalayıcısı (başlık, hızlı satır, alt bilgi). Veri ve sıra
//! `emoji_data.rs`'te (kütüphanenin kendi JSON'undan üretildi; bayraklar yok).
//! Ölçüler (330 genişlik, 352 yükseklik): başlık 56, hızlı satır 46, arama 68, kategori 40,
//! gövde 132 (40px hücreler, 7 sütun), alt bilgi 52.

use std::rc::Rc;

use gpui_kit::component::Sizable as _;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, Context, Entity, ScrollStrategy, SharedString, UniformListScrollHandle, Window, div, px, uniform_list};
use p2_ui::P2Text as _;
use p2_ui::icons::svg_image;
use p2_ui::tokens::{FONT_EMOJI, FONT_MONO, Palette, fs};

#[path = "emoji_data.rs"]
mod data;

/// Kategori anahtarı (sprite sütunu), adın i18n anahtarı.
const CATEGORIES: [(usize, &str); 7] = [(0, "chat.emoji.cat.smileys"), (1, "chat.emoji.cat.animals"), (2, "chat.emoji.cat.food"), (3, "chat.emoji.cat.travel"), (5, "chat.emoji.cat.activities"), (6, "chat.emoji.cat.objects"), (7, "chat.emoji.cat.symbols")];
/// Son kullanılanlar sprite sütunu (epr-icn-suggested: −8 × boyut).
const SUGGESTED_COL: usize = 8;
/// emoji-picker-react kategori sütunları: smileys 0, animals 1, food 2, travel 3, activities 4, objects 5, symbols 6.
const SPRITE_COL: [usize; 7] = [0, 1, 2, 3, 4, 5, 6];
const QUICK: [&str; 8] = ["😀", "😂", "😍", "👍", "🔥", "🎉", "❤️", "👀"];
const COLS: usize = 7;

#[derive(Clone)]
enum Row {
    /// Kategori başlığı (-1: son kullanılanlar)
    Label(i32),
    Cells(Vec<(String, String)>),
}

pub type Pick = Rc<dyn Fn(String, &mut Window, &mut App)>;

pub struct EmojiPanel {
    pub search: Entity<InputState>,
    scroll: UniformListScrollHandle,
    /// Son seçilen (alt bilgide görünür): emoji, ad
    selected: Option<(String, String)>,
    query: String,
}

thread_local! {
    /// Son kullanılanlar (emoji-picker-react `epr_suggested`); süreç boyunca.
    static RECENT: std::cell::RefCell<Vec<(String, String)>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn name_of(emoji: &str) -> String {
    data::EMOJIS.iter().find(|e| e.1 == emoji).map(|e| e.2.rsplit('|').next().unwrap_or_default().to_string()).unwrap_or_default()
}

impl EmojiPanel {
    pub fn new<T: 'static>(window: &mut Window, cx: &mut Context<T>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder(p2_i18n::t("chat.emoji.search")));
        search.update(cx, |s, cx| s.focus(window, cx));
        Self { search, scroll: UniformListScrollHandle::new(), selected: None, query: String::new() }
    }

    /// Seçimi kaydeder (alt bilgi + son kullanılanlar).
    pub fn chose(&mut self, emoji: &str) {
        let name = name_of(emoji);
        self.selected = Some((emoji.to_string(), name.clone()));
        RECENT.with(|r| {
            let mut r = r.borrow_mut();
            r.retain(|x| x.0 != emoji);
            r.insert(0, (emoji.to_string(), name));
            r.truncate(COLS * 4);
        });
    }

    fn rows(&self) -> Vec<Row> {
        let q = self.query.trim().to_lowercase();
        let mut rows = Vec::new();
        let push = |rows: &mut Vec<Row>, label: i32, cells: Vec<(String, String)>| {
            if cells.is_empty() {
                return;
            }
            rows.push(Row::Label(label));
            for chunk in cells.chunks(COLS) {
                rows.push(Row::Cells(chunk.to_vec()));
            }
        };
        if q.is_empty() {
            let recent = RECENT.with(|r| r.borrow().clone());
            push(&mut rows, -1, recent);
        }
        for (ci, _) in CATEGORIES.iter().enumerate() {
            let cells: Vec<(String, String)> = data::EMOJIS
                .iter()
                .filter(|e| e.0 as usize == ci && (q.is_empty() || e.2.split('|').any(|n| n.to_lowercase().contains(&q))))
                .map(|e| (e.1.to_string(), e.2.rsplit('|').next().unwrap_or_default().to_string()))
                .collect();
            push(&mut rows, ci as i32, cells);
        }
        rows
    }

    /// Arama kutusu değişince (çağıranın InputEvent::Change aboneliğinden).
    pub fn set_query(&mut self, q: String) {
        if q != self.query {
            self.query = q;
            self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        }
    }

    /// Paneli çizer. `pick` emojiyi alır (çağıran `chose` + ekleme yapar).
    pub fn render(&self, p: &Palette, pick: Pick) -> AnyElement {
        let rows = Rc::new(self.rows());
        // Etkin kategori: görünen ilk satırın kategorisi.
        let top = (-f32::from(self.scroll.0.borrow().base_handle.offset().y) / 40.).floor().max(0.) as usize;
        let mut active = -2;
        for r in rows.iter().take(top + 1) {
            if let Row::Label(c) = r {
                active = *c;
            }
        }
        if active == -2 {
            active = rows.iter().find_map(|r| if let Row::Label(c) = r { Some(*c) } else { None }).unwrap_or(0);
        }
        let (hover, accent_fg) = (p.bg_hover, p.text_accent);
        let label_bg = p.bg_2;
        let list_rows = rows.clone();
        let list_pick = pick.clone();
        let grid = uniform_list("emoji-grid", rows.len(), move |range, _, _| {
            range
                .map(|ix| match &list_rows[ix] {
                    Row::Label(c) => div()
                        .h(px(40.))
                        .px(px(12.))
                        .flex()
                        .items_center()
                        .bg(label_bg)
                        .text_size(fs(10.))
                        .css_weight(750.)
                        .text_color(accent_fg)
                        .child(super::view::tr_capitalize(&p2_i18n::t(if *c < 0 { "chat.emoji.cat.recent" } else { CATEGORIES[*c as usize].1 })))
                        .into_any_element(),
                    Row::Cells(cells) => div()
                        .h(px(40.))
                        .px(px(10.))
                        .flex()
                        .children(cells.iter().map(|(e, _)| {
                            let (e2, pick) = (e.clone(), list_pick.clone());
                            div()
                                .id(SharedString::from(format!("ep-{e}")))
                                .size(px(40.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(8.))
                                .font_family(FONT_EMOJI)
                                .text_size(px(30.))
                                .line_height(px(30.))
                                .cursor_pointer()
                                .hover(move |s| s.bg(hover))
                                .child(e.clone())
                                .on_click(move |_, window, cx| pick(e2.clone(), window, cx))
                        }))
                        .into_any_element(),
                })
                .collect()
        })
        .track_scroll(&self.scroll)
        .h(px(132.))
        .w_full();

        // Kategori düğmeleri (sprite: satır 0 pasif, satır 1 etkin/üzerinde).
        let sprite = |col: usize, row: usize| {
            let markup = data::NAV_SPRITE.replacen("viewBox=\"0 0 200 80\"", &format!("viewBox=\"{} {} 20 20\"", col * 20, row * 20), 1).replacen("width=\"200px\" height=\"80px\"", "width=\"30px\" height=\"30px\"", 1);
            svg_image(markup).size(px(30.))
        };
        let mut nav = div().h(px(40.)).pt(px(3.)).pb(px(7.)).px(px(8.)).flex().justify_around();
        let mut nav_items: Vec<(i32, usize)> = vec![(-1, SUGGESTED_COL)];
        nav_items.extend((0..CATEGORIES.len()).map(|i| (i as i32, SPRITE_COL[i])));
        for (cat, col) in nav_items {
            let is_active = cat == active;
            let target = rows.iter().position(|r| matches!(r, Row::Label(c) if *c == cat));
            let scroll = self.scroll.clone();
            let gid = SharedString::from(format!("ecat-{cat}"));
            nav = nav.child(
                div()
                    .id(gid.clone())
                    .group(gid.clone())
                    .relative()
                    .size(px(30.))
                    .rounded(px(8.))
                    .when(target.is_some(), |s| s.cursor_pointer())
                    .when(target.is_none() && self.query.is_empty() && cat >= 0, |s| s.opacity(0.3))
                    .child(div().absolute().inset_0().child(sprite(col, if is_active { 1 } else { 0 })))
                    .when(!is_active, |s| s.child(div().absolute().inset_0().invisible().group_hover(gid.clone(), |s| s.visible()).child(sprite(col, 1))))
                    .on_click(move |_, _, cx| {
                        if let Some(ix) = target {
                            scroll.scroll_to_item(ix, ScrollStrategy::Top);
                            cx.refresh_windows();
                        }
                    }),
            );
        }
        let magnifier = data::MAGNIFIER.replacen("viewBox=\"0 0 20 40\"", "viewBox=\"0 20 20 20\"", 1).replacen("width=\"20px\" height=\"40px\"", "width=\"20px\" height=\"20px\"", 1);
        let focused_border = p.accent;
        let search = div()
            .h(px(68.))
            .px(px(10.))
            .pt(px(15.))
            .child(
                div()
                    .relative()
                    .h(px(38.))
                    .border_1()
                    .border_color(focused_border)
                    .rounded(px(8.))
                    .bg(gpui_kit::black())
                    .pl(px(28.))
                    .pr(px(30.))
                    .flex()
                    .items_center()
                    .text_size(fs(12.))
                    .child(div().absolute().left(px(10.)).top(px(9.)).child(svg_image(magnifier).size(px(20.))))
                    .child(div().flex_1().child(Input::new(&self.search).appearance(false).with_size(gpui_kit::component::Size::XSmall).text_size(fs(12.)))),
            );
        let (sel_emoji, sel_name, sel_code) = match &self.selected {
            Some((e, n)) => (e.clone(), n.clone(), format!(":{}:", n.split_whitespace().collect::<Vec<_>>().join("_"))),
            None => ("✨".to_string(), p2_i18n::t("chat.emoji.pickOne"), p2_i18n::t("chat.emoji.clickToAdd")),
        };
        let mono = self.selected.is_some();
        let quick_hover = p.bg_hover;
        div()
            .w_full()
            .flex()
            .flex_col()
            .bg(p.bg_2)
            .child(
                div()
                    .h(px(56.))
                    .px(px(14.))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(p.border_subtle)
                    .bg(p.bg_1)
                    .child(div().flex().flex_col().gap(px(3.)).child(div().text_size(fs(13.)).css_weight(700.).text_color(p.text_strong).child(p2_i18n::t("chat.fmt.emoji"))).child(div().text_size(fs(9.)).text_color(p.text_3).child(p2_i18n::t("chat.emoji.subtitle"))))
                    .child(div().font_family(FONT_EMOJI).text_size(fs(27.)).child(self.selected.as_ref().map_or("😊".to_string(), |s| s.0.clone()))),
            )
            .child(div().h(px(46.)).px(px(10.)).py(px(5.)).flex().items_center().gap(px(3.)).border_b_1().border_color(p.border_subtle).children(QUICK.iter().map(|e| {
                let (e2, pick) = (e.to_string(), pick.clone());
                div()
                    .id(SharedString::from(format!("eq-{e}")))
                    .flex_1()
                    .h(px(34.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(7.))
                    .font_family(FONT_EMOJI)
                    .text_size(fs(20.))
                    .cursor_pointer()
                    .hover(move |s| s.bg(quick_hover))
                    .child(*e)
                    .on_click(move |_, window, cx| pick(e2.clone(), window, cx))
            })))
            .child(search)
            .child(nav)
            .child(grid)
            .child(
                div()
                    .h(px(52.))
                    .px(px(13.))
                    .flex()
                    .items_center()
                    .gap(px(9.))
                    .border_t_1()
                    .border_color(p.border_subtle)
                    .bg(p.bg_1)
                    .child(div().w(px(38.)).font_family(FONT_EMOJI).text_size(fs(26.)).child(sel_emoji))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(3.))
                            .child(div().truncate().text_size(fs(11.)).css_weight(700.).text_color(p.text_strong).child(sel_name))
                            .child(div().font_family(FONT_MONO).text_size(px(9.)).text_color(p.text_3).when(!mono, |s| s.font_family(FONT_MONO)).child(sel_code)),
                    ),
            )
            .into_any_element()
    }
}

// ---------------------------------------------------------------- :kısa_kod: (emojiShortcodes.ts)

fn normalize(name: &str) -> String {
    let lower = name.to_lowercase();
    let lower = lower.trim().replace('&', "and").replace(['\'', '’', '.'], "");
    let mut out = String::new();
    let mut under = false;
    for c in lower.chars() {
        if c.is_ascii_alphanumeric() || c == '+' || c == '-' {
            out.push(c);
            under = false;
        } else if !under {
            out.push('_');
            under = true;
        }
    }
    out.trim_matches('_').to_string()
}

fn shortcodes() -> &'static Vec<(String, &'static str)> {
    static MAP: std::sync::OnceLock<Vec<(String, &'static str)>> = std::sync::OnceLock::new();
    MAP.get_or_init(|| {
        let mut map: Vec<(String, &'static str)> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let common: [(&str, &str); 29] = [("+1", "👍"), ("thumbsup", "👍"), ("-1", "👎"), ("thumbsdown", "👎"), ("heart", "❤️"), ("red_heart", "❤️"), ("smile", "😄"), ("grin", "😀"), ("joy", "😂"), ("rofl", "🤣"), ("wink", "😉"), ("blush", "😊"), ("thinking", "🤔"), ("sob", "😭"), ("cry", "😢"), ("fire", "🔥"), ("tada", "🎉"), ("rocket", "🚀"), ("eyes", "👀"), ("ok_hand", "👌"), ("clap", "👏"), ("pray", "🙏"), ("muscle", "💪"), ("wave", "👋"), ("100", "💯"), ("star", "⭐"), ("warning", "⚠️"), ("white_check_mark", "✅"), ("x", "❌")];
        for e in data::EMOJIS {
            for name in e.2.split('|').collect::<Vec<_>>().into_iter().rev() {
                let key = normalize(name);
                if !key.is_empty() && seen.insert(key.clone()) {
                    map.push((key, e.1));
                }
            }
        }
        for (k, v) in common {
            match map.iter_mut().find(|x| x.0 == k) {
                Some(x) => x.1 = v,
                None => map.push((k.to_string(), v)),
            }
        }
        map
    })
}

fn lookup(key: &str) -> Option<&'static str> {
    shortcodes().iter().find(|x| x.0 == key).map(|x| x.1)
}

/// expandEmojiShortcodes: :ad: → emoji (bilinmeyen olduğu gibi kalır).
pub fn expand_shortcodes(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find(':') {
        let after = &rest[start + 1..];
        let len = after.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '+' || c == '-')).unwrap_or(after.len());
        if len > 0 && after[len..].starts_with(':') {
            if let Some(e) = lookup(&after[..len].to_lowercase()) {
                out.push_str(&rest[..start]);
                out.push_str(e);
                rest = &after[len + 1..];
                continue;
            }
        }
        out.push_str(&rest[..start + 1]);
        rest = after;
    }
    out.push_str(rest);
    out
}

/// Sondaki :sorgu (boşluk ya da satır başından sonra).
pub fn shortcode_query(text: &str) -> Option<String> {
    let at = text.rfind(':')?;
    if text[..at].chars().last().is_some_and(|c| !c.is_whitespace()) {
        return None;
    }
    let q = &text[at + 1..];
    q.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '+' || c == '-').then(|| q.to_string())
}

/// suggestEmojiShortcodes (en çok 8; boş sorguda popüler liste).
pub fn suggest_shortcodes(query: &str) -> Vec<(String, &'static str)> {
    let needle = query.to_lowercase();
    let mut keys: Vec<String> = if needle.is_empty() {
        ["smile", "joy", "heart", "+1", "fire", "tada", "rocket", "eyes"].iter().map(|s| s.to_string()).collect()
    } else {
        let mut k: Vec<String> = shortcodes().iter().filter(|x| x.0.starts_with(&needle)).map(|x| x.0.clone()).collect();
        k.sort_by_key(|x| x.len());
        k
    };
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for key in keys.drain(..) {
        if let Some(e) = lookup(&key) {
            if seen.insert(e) {
                out.push((key, e));
                if out.len() >= 8 {
                    break;
                }
            }
        }
    }
    out
}
