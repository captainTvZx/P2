//! NotesEditor.tsx karşılığı: sürüm notları / aktivite gönderisi / proje notu editörü.
//! `.message-composer.slack-composer.library-notes-editor`: üstte `+` (dosya ekle) + yazı alanı +
//! boyut tutamacı, altta biçim çubuğu (Kalın, İtalik, Bağlantı | Emoji, Ekle) ve isteğe bağlı
//! gönder düğmesi. Çıktı mesaj biçimi: markdown metin + satır başına bir ek işareti.

use std::sync::Arc;

use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, App, Context, Entity, MouseButton, PathPromptOptions, SharedString, Window, div, px, relative};
use p2_client_core::Api;
use p2_ui::glyphs;
use p2_ui::tokens::{Palette, css_rgba, font_size, fs, radius, shadows};

use super::ui::{GRIP, ico, sh};

/// NotesEditor emoji paneli (kısa liste; EmojiPanel'in sık kullanılanları).
pub const EMOJI: [&str; 40] = [
    "😀", "😂", "🙂", "😉", "😍", "🤔", "😅", "😎", "😢", "😡", "👍", "👎", "👏", "🙏", "💪", "🤝", "👀", "🔥", "✨", "🎉", "❤️", "💯", "✅", "❌",
    "⚠️", "🐛", "🚀", "🎮", "🕹️", "🏆", "⏰", "📌", "📎", "📷", "💡", "🛠️", "📦", "🧪", "☕", "🍕",
];

pub struct NoteBox {
    pub text: Entity<TextareaState>,
    /// Ek işaretleri (`[[p2-file|…]]`), metnin altına satır satır eklenir.
    pub files: Vec<String>,
    pub emoji: bool,
    pub uploading: bool,
    /// Tutamaçla ayarlanan yükseklik (px); `None` = CSS varsayılanı.
    pub height: Option<f32>,
    pub compact: bool,
}

fn is_marker(line: &str) -> bool {
    let t = line.trim();
    t.starts_with("[[p2-file|") && t.ends_with("]]")
}

impl NoteBox {
    pub fn new<V: 'static>(placeholder: &str, compact: bool, window: &mut Window, cx: &mut Context<V>) -> Self {
        let placeholder = placeholder.to_string();
        let (min, max) = if compact { (1, 12) } else { (5, 20) };
        let text = cx.new(|cx| TextareaState::new(window, cx).auto_grow(min, max).placeholder(placeholder));
        Self { text, files: Vec::new(), emoji: false, uploading: false, height: None, compact }
    }

    /// Mesaj biçimi: metin + ek işaretleri.
    pub fn value(&self, cx: &App) -> String {
        let mut out = self.text.read(cx).value().to_string().trim_end().to_string();
        for f in &self.files {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(f);
        }
        out.chars().take(4000 + self.files.iter().map(|f| f.len() + 1).sum::<usize>()).collect()
    }

    pub fn is_empty(&self, cx: &App) -> bool {
        self.text.read(cx).value().trim().is_empty() && self.files.is_empty()
    }

    pub fn set(&mut self, content: &str, window: &mut Window, cx: &mut App) {
        let (markers, text): (Vec<&str>, Vec<&str>) = content.lines().partition(|l| is_marker(l));
        self.files = markers.into_iter().map(str::to_string).collect();
        let text = text.join("\n");
        self.text.update(cx, |s, cx| s.set_value(text, window, cx));
    }

    pub fn clear(&mut self, window: &mut Window, cx: &mut App) {
        self.files.clear();
        self.emoji = false;
        self.text.update(cx, |s, cx| s.set_value("", window, cx));
    }

    fn append(&mut self, piece: &str, window: &mut Window, cx: &mut App) {
        let mut v = self.text.read(cx).value().to_string();
        v.push_str(piece);
        self.text.update(cx, |s, cx| {
            s.set_value(v, window, cx);
            s.focus(window, cx);
        });
    }
}

/// Erişimci: görünümün içindeki NoteBox (aynı görünümde birden çok editör olabilir).
pub type Pick<V> = fn(&mut V) -> Option<&mut NoteBox>;

/// `.library-notes-editor`. `submit`: (etiket, gönderilebilir mi, tıklayınca).
#[allow(clippy::too_many_arguments)]
pub fn note_box<V: 'static>(id: &str, nb: &NoteBox, api: Arc<Api>, pick: Pick<V>, submit: Option<(&str, bool, fn(&mut V, &mut Window, &mut Context<V>))>, busy: bool, p: &Palette, cx: &mut Context<V>) -> AnyElement {
    let id = SharedString::from(id.to_string());
    let (soft, strong, soft2) = (p.accent_soft, p.text_strong, p.accent_soft_2);
    let tool = |key: &str, glyph: &str, stroke: f32| {
        div()
            .id(SharedString::from(format!("{id}-{key}")))
            .group(SharedString::from(format!("{id}-{key}")))
            .w(px(27.))
            .h(px(26.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(radius::XS)
            .text_color(p.text_3)
            .cursor_pointer()
            .hover(move |s| s.bg(soft).text_color(strong))
            .child(glyphs::named(glyph, 15., stroke).text_color(p.text_3).group_hover(SharedString::from(format!("{id}-{key}")), move |s| s.text_color(strong)))
    };
    let attach_api = api.clone();
    let attach = cx.listener(move |this: &mut V, _: &gpui_kit::ClickEvent, window: &mut Window, cx: &mut Context<V>| {
        let rx = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: true, prompt: Some("Görsel veya dosya ekle".into()) });
        let api = attach_api.clone();
        if let Some(nb) = pick(this) {
            nb.uploading = true;
        }
        cx.spawn_in(window, async move |view, cx| {
            let paths = match rx.await {
                Ok(Ok(Some(paths))) => paths,
                _ => Vec::new(),
            };
            let markers = cx
                .background_executor()
                .spawn(async move {
                    paths
                        .iter()
                        .filter_map(|path| {
                            let data = std::fs::read(path).ok()?;
                            let name = path.file_name()?.to_string_lossy().to_string();
                            api.upload_file(&name, &data).ok().map(|f| p2_client_core::attachment_marker(&f))
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = view.update(cx, |this, cx| {
                if let Some(nb) = pick(this) {
                    nb.uploading = false;
                    nb.files.extend(markers);
                }
                cx.notify();
            });
        })
        .detach();
    });
    let toggle_emoji = cx.listener(move |this: &mut V, _: &gpui_kit::ClickEvent, _: &mut Window, cx: &mut Context<V>| {
        if let Some(nb) = pick(this) {
            nb.emoji = !nb.emoji;
        }
        cx.notify();
    });
    let mut emoji_grid = None;
    if nb.emoji {
        let mut grid = div()
            .id(SharedString::from(format!("{id}-emoji")))
            .absolute()
            .left(px(8.))
            .top_full()
            .mt(px(6.))
            .w(px(330.))
            .p(px(8.))
            .flex()
            .flex_wrap()
            .gap(px(2.))
            .rounded(radius::LG)
            .border_1()
            .border_color(p.border_strong)
            .bg(p.glass_strong.background())
            .shadow(p.shadow_lg())
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        for e in EMOJI {
            let hover = p.bg_hover;
            grid = grid.child(
                div()
                    .id(SharedString::from(format!("{id}-emo-{e}")))
                    .size(px(36.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(radius::SM)
                    .text_size(fs(20.))
                    .font_family(p2_ui::tokens::FONT_EMOJI)
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover))
                    .child(e)
                    .on_click(cx.listener(move |this: &mut V, _, window, cx| {
                        if let Some(nb) = pick(this) {
                            nb.emoji = false;
                            nb.append(e, window, cx);
                        }
                        cx.notify();
                    })),
            );
        }
        emoji_grid = Some(gpui_kit::deferred(grid).with_priority(2));
    }
    let min_h = if nb.compact { 38. } else { 150. };
    let files: Vec<AnyElement> = nb
        .files
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let name = m.trim_start_matches("[[p2-file|").split('|').next().unwrap_or("dosya").to_string();
            div()
                .id(SharedString::from(format!("{id}-file-{i}")))
                .flex()
                .items_center()
                .gap(px(6.))
                .h(px(26.))
                .px(px(8.))
                .rounded(radius::SM)
                .bg(p.bg_2)
                .border_1()
                .border_color(p.border_subtle)
                .text_size(fs(11.))
                .text_color(p.text_2)
                .child(name)
                .child(div().id(SharedString::from(format!("{id}-file-x-{i}"))).cursor_pointer().text_color(p.text_3).child("×").on_click(cx.listener(move |this: &mut V, _, _, cx| {
                    if let Some(nb) = pick(this) {
                        if i < nb.files.len() {
                            nb.files.remove(i);
                        }
                    }
                    cx.notify();
                })))
                .into_any_element()
        })
        .collect();
    let send = submit.map(|(label, enabled, run)| {
        let enabled = enabled && !busy && !nb.uploading;
        let (glow, inset) = (p.accent_glow_strong, p.inset_top);
        div()
            .id(SharedString::from(format!("{id}-send")))
            .h(px(28.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(radius::SM)
            .map(|s| if enabled { s.bg(p.accent_grad.background()).text_color(p.text_on_accent).shadow(shadows(&[glow, inset])).cursor_pointer().hover(|s| s.relative().top(px(-1.))) } else { s.bg(p.bg_3).text_color(p.text_4) })
            .child(div().text_size(fs(11.5)).font_weight(gpui_kit::FontWeight::BOLD).child(label.to_string()))
            .child(glyphs::named("send", 15., 1.8).text_color(if enabled { p.text_on_accent } else { p.text_4 }))
            .when(enabled, |s| s.on_click(cx.listener(move |this: &mut V, _, window, cx| run(this, window, cx))))
    });
    div()
        .relative()
        .flex()
        .flex_col()
        .w_full()
        .border_1()
        .border_color(p.border)
        .rounded(radius::LG)
        .bg(p.glass.background())
        // `box-shadow: var(--p2-shadow-sm), var(--p2-inset-top)` — açık temalarda gölge farklı (palet).
        .shadow(shadows(&p.shadow_sm.iter().copied().chain([p.inset_top]).collect::<Vec<_>>()))
        .when(!files.is_empty(), |s| s.child(div().flex().flex_wrap().gap(px(6.)).pt(px(8.)).px(px(8.)).children(files)))
        .child(
            div()
                .relative()
                .flex()
                .items_start()
                .gap(px(4.))
                .pt(px(2.))
                // Ölçüm: `.composer-input-row` 43px = 2 + 38 (düzenleyici) + 3 (satır içi kutunun alt payı).
                .pb(px(3.))
                .pr(px(8.))
                .pl(px(6.))
                .child(tool("plus", "plus", 1.8).h(px(30.)).w(px(26.)).mt(px(3.)).on_click(attach))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .min_h(px(min_h))
                        .when_some(nb.height, |s, h| s.h(px(h)))
                        .pt(px(9.))
                        .pb(px(9.))
                        .pr(px(22.))
                        .text_size(font_size::MD)
                        .line_height(relative(1.45))
                        .text_color(p.text)
                        // `.composer-editor` 11,05px / 1.45 (16,02px), p 9 22 9 4: XSmall boyutu 4px yatay iç boşluk verir
                        // (chat.rs ile aynı); yazı boyu Textarea'ya kalıtılmaz, kendisine verilir.
                        .child(gpui_kit::component::Sizable::with_size(Textarea::new(&nb.text).appearance(false), gpui_kit::component::Size::XSmall).text_size(font_size::MD).line_height(px(16.0225))),
                )
                .child(div().absolute().right(px(6.)).bottom(px(5.)).size(px(16.)).flex().items_center().justify_center().rounded(px(4.)).opacity(0.7).text_color(p.text_3).child(ico(GRIP, 1.7 * 2., 10., p.text_3))),
        )
        .child(
            div()
                .h(px(34.))
                .flex()
                .items_center()
                .justify_between()
                .px(px(7.))
                .border_t_1()
                .border_color(p.border_subtle)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(2.))
                        .child(tool("bold", "bold", 2.2).on_click(fmt_listener(pick, "**kalın metin**", cx)))
                        .child(tool("italic", "italic", 1.8).on_click(fmt_listener(pick, "*italik metin*", cx)))
                        .child(tool("link", "link", 1.8).on_click(fmt_listener(pick, "[bağlantı](https://)", cx)))
                        .child(div().w(px(1.)).h(px(17.)).mx(px(4.)).bg(p.border_subtle))
                        .child(tool("emoji", "emoji", 1.8).when(nb.emoji, |s| s.bg(soft2).text_color(strong)).on_click(toggle_emoji))
                        .child(tool("attach", "plus", 1.8).on_click({
                            let api = api.clone();
                            cx.listener(move |this: &mut V, _: &gpui_kit::ClickEvent, window: &mut Window, cx: &mut Context<V>| {
                                let rx = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: true, prompt: Some("Görsel veya dosya ekle".into()) });
                                let api = api.clone();
                                cx.spawn_in(window, async move |view, cx| {
                                    let paths = match rx.await {
                                        Ok(Ok(Some(paths))) => paths,
                                        _ => Vec::new(),
                                    };
                                    let markers = cx
                                        .background_executor()
                                        .spawn(async move {
                                            paths
                                                .iter()
                                                .filter_map(|path| {
                                                    let data = std::fs::read(path).ok()?;
                                                    let name = path.file_name()?.to_string_lossy().to_string();
                                                    api.upload_file(&name, &data).ok().map(|f| p2_client_core::attachment_marker(&f))
                                                })
                                                .collect::<Vec<_>>()
                                        })
                                        .await;
                                    let _ = view.update(cx, |this, cx| {
                                        if let Some(nb) = pick(this) {
                                            nb.files.extend(markers);
                                        }
                                        cx.notify();
                                    });
                                })
                                .detach();
                                let _ = this;
                            })
                        })),
                )
                .children(send),
        )
        .children(emoji_grid)
        .into_any_element()
}

/// Biçim düğmesi: işaretli metni sona ekler (Kalın/İtalik/Bağlantı).
fn fmt_listener<V: 'static>(pick: Pick<V>, piece: &'static str, cx: &mut Context<V>) -> impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut App) + 'static {
    cx.listener(move |this: &mut V, _: &gpui_kit::ClickEvent, window: &mut Window, cx: &mut Context<V>| {
        if let Some(nb) = pick(this) {
            nb.append(piece, window, cx);
        }
        cx.notify();
    })
}
