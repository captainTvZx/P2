//! Yazma kutusunun zengin davranışları (L ajanı) — RichComposer.tsx `RichInput` ve attachments.tsx
//! `useFileDrop`/`DropZone`:
//! - canlı biçim: kalın/italik/bağlantı yazarken sonucun kendisi görünür. Kutu içinde biçimli parça,
//!   metni markdown (`**x**`, `*x*`, `[x](url)`) olan bir *satır içi simge* (InlineToken) olarak tutulur;
//!   değer (`value()`) Electron'daki gibi markdown verir, ekranda yalnız etiket biçimli çizilir.
//!   Fark: Electron'da biçimli metnin içine imleç girip harf değiştirilebilir; burada parça bir bütün
//!   (silinir/seçilir) — GPUI metin kutusu biçimli koşu desteklemiyor.
//! - B/I düğmeleri ve Ctrl+B/I: seçim varsa sarar; yoksa "biçim kipi" açılır, yazılan harfler biçimli
//!   parçaya eklenir (execCommand('bold') ile aynı his). Düğmeler etkin durumu gösterir.
//! - bağlantı düğmesi: `.composer-link-bar` (adres kutusu, Ekle, Vazgeç); seçim varsa seçim bağlantı
//!   olur, yoksa adresin kendisi eklenir (+ bölünmez boşluk).
//! - yapıştırma: dosya/resim → ek; kod gibi görünen çok satırlı metin → ``` çiti.
//! - sürükle-bırak: sohbetin tamamı bırakma alanı (`.drop-zone` katmanı).

use std::ops::Range;

use gpui_kit::component::input::{InlineToken, InlineTokenContext, InputContent, InputEvent, InputState};
use gpui_kit::prelude::*;
use gpui_kit::component::Sizable as _;
use gpui_kit::{AnyElement, App, ClipboardEntry, ClipboardItem, Context, ExternalPaths, Window, div, px};
use p2_ui::P2Text as _;
use p2_ui::P2Fx as _;
use p2_ui::tokens::{Palette, fs};

use super::ChatPane;

/// Biçim türü: simge kimliği.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Fmt {
    Bold,
    Italic,
}

impl Fmt {
    fn id(self) -> &'static str {
        match self {
            Fmt::Bold => "b",
            Fmt::Italic => "i",
        }
    }
    fn wrap(self, s: &str) -> String {
        match self {
            Fmt::Bold => format!("**{s}**"),
            Fmt::Italic => format!("*{s}*"),
        }
    }
}

/// `markdownToHtml` karşılığı: markdown → metin + biçim simgeleri (dışarıdan gelen değer: düzenleme, alıntı).
pub fn markdown_content(md: &str) -> InputContent {
    let mut spans: Vec<(Range<usize>, InlineToken)> = Vec::new();
    let bytes = md.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let rest = &md[i..];
        // [etiket](https://…)
        if rest.starts_with('[') {
            if let Some(close) = rest.find("](") {
                let label = &rest[1..close];
                let after = &rest[close + 2..];
                if !label.is_empty() && !label.contains(']') && !label.contains('\n') && (after.starts_with("http://") || after.starts_with("https://")) {
                    if let Some(end) = after.find(')') {
                        let url = &after[..end];
                        if !url.chars().any(char::is_whitespace) {
                            let len = close + 2 + end + 1;
                            spans.push((i..i + len, InlineToken::new("a", &rest[..len]).with_label(label)));
                            i += len;
                            continue;
                        }
                    }
                }
            }
        }
        if rest.starts_with("**") {
            if let Some(end) = rest[2..].find("**") {
                let inner = &rest[2..2 + end];
                if !inner.is_empty() && !inner.contains('*') && !inner.contains('\n') {
                    let len = end + 4;
                    spans.push((i..i + len, InlineToken::new("b", &rest[..len]).with_label(inner)));
                    i += len;
                    continue;
                }
            }
        } else if rest.starts_with('*') {
            if let Some(end) = rest[1..].find('*') {
                let inner = &rest[1..1 + end];
                if !inner.is_empty() && !inner.contains('\n') {
                    let len = end + 2;
                    spans.push((i..i + len, InlineToken::new("i", &rest[..len]).with_label(inner)));
                    i += len;
                    continue;
                }
            }
        }
        i += rest.chars().next().map_or(1, char::len_utf8);
    }
    let mut content = InputContent::new(md.to_string());
    for (range, token) in spans {
        match content.clone().with_token(range, token) {
            Ok(c) => content = c,
            Err(_) => {}
        }
    }
    content
}

/// Simgenin çizimi (`.composer-editor b/i/a`): kalın 750 + `text-strong`, italik `text-2`,
/// bağlantı `link` + alt çizgi. Seçiliyken seçim zemini.
pub fn token_element(ctx: &InlineTokenContext, p: &Palette) -> AnyElement {
    let label = ctx.token().label().clone();
    let el = div().flex_none().h(ctx.line_height()).flex().items_center().text_size(fs(13.)).when(ctx.is_selected(), |s| s.bg(p.accent_soft_2));
    match ctx.token().id().as_ref() {
        "b" => el.css_weight(750.).text_color(p.text_strong).child(label),
        "i" => el.italic().text_color(p.text_2).child(label),
        _ => el.text_color(p.link).child(div().underline().child(label)),
    }
    .into_any_element()
}

/// RichComposer yapıştırma: kod gibi görünen çok satırlı metin çitlenir.
pub fn fence_if_code(text: &str) -> Option<String> {
    let already = text.trim_start().starts_with("```") || text.trim_start().starts_with("'''");
    if already || !text.contains('\n') {
        return None;
    }
    let trimmed = text.trim();
    let tail = trimmed.ends_with(['{', '}', ';']);
    let starts = ["const", "let", "var", "function", "class", "import", "export", "def", "public", "private", "SELECT", "INSERT"];
    let looks = text.lines().any(|l| {
        let l = l.trim_start();
        starts.iter().any(|s| l.starts_with(s) && l[s.len()..].starts_with(char::is_whitespace)) || (l.starts_with('<') && l[1..].trim_start_matches('/').starts_with(|c: char| c.is_ascii_lowercase()))
    });
    (tail || looks).then(|| format!("```\n{trimmed}\n```"))
}

impl ChatPane {
    /// B/I düğmesi ya da Ctrl+B/I.
    pub(super) fn format(&mut self, fmt: Fmt, window: &mut Window, cx: &mut Context<Self>) {
        let composer = self.composer.clone();
        let (value, range, tokens) = {
            let s = composer.read(cx);
            (s.value().to_string(), s.selected_range(), s.tokens().to_vec())
        };
        if range.start < range.end {
            // Seçim zaten bu biçimde tek parçaysa biçimi kaldır (execCommand gibi aç/kapa).
            if let Some(t) = tokens.iter().find(|t| t.range() == range && t.token().id().as_ref() == fmt.id()) {
                let label = t.token().label().to_string();
                composer.update(cx, |s, cx| {
                    s.set_selected_range(range.clone(), cx);
                    s.replace(label, window, cx);
                });
            } else {
                let plain = plain_text(&value, &tokens, range.clone());
                if !plain.trim().is_empty() {
                    let token = InlineToken::new(fmt.id(), fmt.wrap(&plain)).with_label(plain);
                    self.fmt_guard = true;
                    composer.update(cx, |s, cx| {
                        let _ = s.replace_range_with_token(range.clone(), token, window, cx);
                    });
                    self.fmt_guard = false;
                }
            }
            self.fmt_pending = None;
        } else {
            // Biçim kipi: aç/kapa.
            self.fmt_pending = match self.fmt_pending {
                Some((f, _)) if f == fmt => None,
                _ => Some((fmt, range.start)),
            };
        }
        composer.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    /// Değişiklikten sonra: biçim kipindeyken yeni yazılan düz metni biçimli parçaya katar.
    pub(super) fn after_change(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.fmt_guard {
            return;
        }
        let Some((fmt, start)) = self.fmt_pending else { return };
        let composer = self.composer.clone();
        let (value, cursor, tokens) = {
            let s = composer.read(cx);
            (s.value().to_string(), s.selected_range().end, s.tokens().to_vec())
        };
        if cursor <= start || cursor > value.len() || !value.is_char_boundary(start) {
            self.fmt_pending = if cursor < start { None } else { self.fmt_pending };
            return;
        }
        let typed = &value[start..cursor];
        if typed.is_empty() || typed.contains('\n') || tokens.iter().any(|t| t.range().start >= start && t.range().end <= cursor && t.range().start < cursor && t.token().id().as_ref() != fmt.id()) {
            return;
        }
        // Hemen öncesinde aynı biçimde parça varsa birleştir.
        let (from, label) = match tokens.iter().find(|t| t.range().end == start && t.token().id().as_ref() == fmt.id()) {
            Some(prev) => (prev.range().start, format!("{}{typed}", prev.token().label())),
            None => match tokens.iter().find(|t| t.range().start == start && t.range().end == cursor) {
                Some(_) => return,
                None => (start, typed.to_string()),
            },
        };
        let token = InlineToken::new(fmt.id(), fmt.wrap(&label)).with_label(label);
        self.fmt_guard = true;
        composer.update(cx, |s, cx| {
            let _ = s.replace_range_with_token(from..cursor, token, window, cx);
        });
        self.fmt_guard = false;
        let end = composer.read(cx).selected_range().end;
        // Kip sürer: bir sonraki harf bu parçanın sonuna eklenir.
        self.fmt_pending = Some((fmt, end));
    }

    /// Düğmelerin etkin durumu (`queryCommandState`): kip ya da imleç biçimli parçanın içinde/sonunda.
    pub(super) fn format_state(&self, cx: &App) -> (bool, bool, bool) {
        let s = self.composer.read(cx);
        let range = s.selected_range();
        let at = |id: &str| s.tokens().iter().any(|t| t.token().id().as_ref() == id && t.range().start < range.end.max(range.start + 1) && t.range().end >= range.end && t.range().start <= range.start);
        let pending = |f: Fmt| self.fmt_pending.is_some_and(|(p, _)| p == f);
        (pending(Fmt::Bold) || at("b"), pending(Fmt::Italic) || at("i"), at("a") || self.link_bar.is_some())
    }

    /// Bağlantı düğmesi: seçimi saklar, `.composer-link-bar` açılır.
    pub(super) fn open_link_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let range = self.composer.read(cx).selected_range();
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("https://ornek.com"));
        input.update(cx, |s, cx| s.focus(window, cx));
        let sub = cx.subscribe_in(&input, window, |this, _, event: &InputEvent, window, cx| match event {
            InputEvent::PressEnter { .. } => this.apply_link(window, cx),
            _ => cx.notify(),
        });
        self._subs.push(sub);
        self.link_bar = Some((input, range));
        cx.notify();
    }

    pub(super) fn close_link_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.link_bar = None;
        self.composer.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    /// `applyLink`: `https://` yoksa eklenir; seçim → `[seçim](adres)`, yoksa adres + bölünmez boşluk.
    pub(super) fn apply_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((input, range)) = self.link_bar.take() else { return };
        let target = input.read(cx).value().trim().to_string();
        if target.is_empty() {
            self.close_link_bar(window, cx);
            return;
        }
        let href = if target.to_ascii_lowercase().starts_with("http://") || target.to_ascii_lowercase().starts_with("https://") { target } else { format!("https://{target}") };
        let composer = self.composer.clone();
        let (value, tokens) = {
            let s = composer.read(cx);
            (s.value().to_string(), s.tokens().to_vec())
        };
        let r = range.start.min(value.len())..range.end.min(value.len());
        self.fmt_guard = true;
        if r.start < r.end {
            let label = plain_text(&value, &tokens, r.clone());
            let token = InlineToken::new("a", format!("[{label}]({href})")).with_label(label);
            composer.update(cx, |s, cx| {
                let _ = s.replace_range_with_token(r, token, window, cx);
            });
        } else {
            let token = InlineToken::new("a", format!("[{href}]({href})")).with_label(href.clone());
            composer.update(cx, |s, cx| {
                s.set_selected_range(r.clone(), cx);
                let _ = s.replace_with_token(token, window, cx);
                s.insert("\u{a0}", window, cx);
            });
        }
        self.fmt_guard = false;
        composer.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    /// Yapıştırma: dosya/resim → ek (`onFiles`); kod gibi görünen metin çitlenir. `true`: işlendi.
    pub(super) fn paste(&mut self, item: &ClipboardItem, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let mut paths = Vec::new();
        for entry in item.entries() {
            match entry {
                ClipboardEntry::ExternalPaths(p) => paths.extend(p.paths().iter().cloned()),
                ClipboardEntry::Image(img) => {
                    // Electron: panodan gelen resim dosyası "image.png".
                    let dir = std::env::temp_dir().join("p2-pano");
                    let _ = std::fs::create_dir_all(&dir);
                    let ext = match img.format {
                        gpui_kit::ImageFormat::Jpeg => "jpg",
                        gpui_kit::ImageFormat::Gif => "gif",
                        gpui_kit::ImageFormat::Webp => "webp",
                        _ => "png",
                    };
                    let sub = dir.join(format!("{}-{}", std::process::id(), img.id));
                    let _ = std::fs::create_dir_all(&sub);
                    let path = sub.join(format!("image.{ext}"));
                    if std::fs::write(&path, &img.bytes).is_ok() {
                        paths.push(path);
                    }
                }
                ClipboardEntry::String(_) => {}
            }
        }
        if !paths.is_empty() {
            crate::trace(&format!("sohbet: yapıştırılan {} dosya", paths.len()));
            self.upload_paths(paths, window, cx);
            return true;
        }
        let Some(text) = item.text() else { return false };
        match fence_if_code(&text) {
            Some(fenced) => {
                self.composer.update(cx, |s, cx| s.insert(fenced, window, cx));
                true
            }
            None => false,
        }
    }

    /// Sohbete sürüklenen dosyalar (`useFileDrop`).
    pub(super) fn drop_files(&mut self, paths: &ExternalPaths, window: &mut Window, cx: &mut Context<Self>) {
        self.drag_over = false;
        let list: Vec<_> = paths.paths().to_vec();
        if !list.is_empty() {
            crate::trace(&format!("sohbet: bırakılan {} dosya", list.len()));
            self.upload_paths(list, window, cx);
        }
        cx.notify();
    }

    /// `DropZone` etiketi.
    pub(super) fn drop_label(&self) -> String {
        match &self.subject {
            super::ChatSubject::Channel(c) => format!("#{} kanalına yüklenecek", c.name),
            super::ChatSubject::Direct(m) => format!("{} kişisine gönderilecek", m.display_name),
            super::ChatSubject::Group(g) => format!("{} grubuna gönderilecek", g.name),
        }
    }

    /// `.drop-zone`: sohbet alanının 8px içinde kesikli çerçeveli katman (yalnız dosya sürüklenirken).
    pub(super) fn drop_zone(&self, p: &Palette, window: &Window, cx: &App) -> Option<AnyElement> {
        if !self.drag_over || !(cx.has_active_drag() || std::env::var_os("P2_AUTOPILOT_DROP_ZONE").is_some()) {
            return None;
        }
        let b = self.bounds.get();
        let vh = f32::from(window.viewport_size().height);
        let (x, y, w) = (f32::from(b.origin.x), f32::from(b.origin.y) + if self.embedded { 0. } else { 58. }, f32::from(b.size.width));
        let h = vh - y;
        Some(
            div()
                .absolute()
                .left(px(x + 8.))
                .top(px(y + 8. - super::view::ROOT_TOP))
                .w(px(w - 16.))
                .h(px(h - 16.))
                .flex()
                .items_center()
                .justify_center()
                .border_2()
                .border_dashed()
                .border_color(p.accent_hover)
                .rounded(px(18.))
                .bg(p2_ui::tokens::css_rgba(7, 11, 20, 0.78))
                .shadow(p2_ui::tokens::shadows(&[p2_ui::tokens::Shadow { x: 0., y: 0., blur: 60., spread: 0., color: p.accent_soft, inset: true }]))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(4.))
                        .child(div().size(px(46.)).flex().items_center().justify_center().rounded_full().bg(p.accent_grad.background()).shadow(p2_ui::tokens::shadows(&[p.accent_glow_strong])).text_color(p.text_on_accent).text_size(fs(20.)).child("⇪"))
                        .child(div().text_size(fs(14.)).css_weight(700.).text_color(p.text_strong).child("Bırak ve gönder"))
                        .child(div().text_size(fs(11.5)).text_color(p.text_3).child(self.drop_label())),
                )
                .into_any_element(),
        )
    }

    /// `.composer-link-bar`: kutunun 8px üstünde, sol/sağ kenara yaslı; 7px iç boşluk, cam, gölge.
    pub(super) fn link_bar_view(&self, p: &Palette, me: gpui_kit::WeakEntity<ChatPane>) -> Option<AnyElement> {
        let (input, _) = self.link_bar.as_ref()?;
        let (add, cancel) = (me.clone(), me);
        let (hover, strong) = (p.bg_hover, p.text_strong);
        Some(
            div()
                .absolute()
                .left(px(42.))
                .right(px(7.))
                .bottom_full()
                .mb(px(8.))
                .child(
                    div()
                        .id("composer-link-bar")
                        .p(px(7.))
                        .flex()
                        .gap(px(6.))
                        .border_1()
                        .border_color(p.border_strong)
                        .rounded(px(11.))
                        .bg(p.glass.background())
                        .backdrop_blur(p2_ui::fx::BLUR)
                        .shadow(p.shadow_lg())
                        .occlude()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .h(px(30.))
                                .px(px(9.))
                                .flex()
                                .items_center()
                                .border_1()
                                .border_color(p.border_accent)
                                .rounded(px(6.))
                                .bg(p.bg_3)
                                .text_size(fs(12.))
                                .text_color(p.text)
                                .child(gpui_kit::component::input::Input::new(input).appearance(false).with_size(gpui_kit::component::Size::XSmall)),
                        )
                        .child(
                            div()
                                .id("link-add")
                                .h(px(30.))
                                .px(px(12.))
                                .flex()
                                .items_center()
                                .border_1()
                                .border_color(gpui_kit::transparent_black())
                                .rounded(px(6.))
                                .bg(p.accent_grad.background())
                                .text_color(p.text_on_accent)
                                .text_size(fs(11.))
                                .css_weight(650.)
                                .cursor_pointer()
                                .child("Ekle")
                                .on_click(move |_, window, cx| {
                                    let _ = add.update(cx, |this, cx| this.apply_link(window, cx));
                                }),
                        )
                        .child(
                            div()
                                .id("link-cancel")
                                .h(px(30.))
                                .px(px(12.))
                                .flex()
                                .items_center()
                                .border_1()
                                .border_color(p.border)
                                .rounded(px(6.))
                                .text_color(p.text_2)
                                .text_size(fs(11.))
                                .css_weight(650.)
                                .cursor_pointer()
                                .hover(move |s| s.bg(hover).text_color(strong))
                                .child("Vazgeç")
                                .on_click(move |_, window, cx| {
                                    let _ = cancel.update(cx, |this, cx| this.close_link_bar(window, cx));
                                }),
                        )
                        .p2_enter_fade(),
                )
                .into_any_element(),
        )
    }
}

trait FadeIn {
    fn p2_enter_fade(self) -> AnyElement;
}
impl FadeIn for gpui_kit::Stateful<gpui_kit::Div> {
    fn p2_enter_fade(self) -> AnyElement {
        use p2_ui::P2Motion as _;
        self.p2_enter("link-bar", p2_ui::Entrance::Rise, std::time::Duration::ZERO).into_any_element()
    }
}

/// Aralığın düz metni: simgeler etiketleriyle (markdown işaretleri olmadan).
fn plain_text(value: &str, tokens: &[gpui_kit::component::input::InlineTokenSpan], range: Range<usize>) -> String {
    let mut out = String::new();
    let mut at = range.start;
    for t in tokens.iter().filter(|t| t.range().end > range.start && t.range().start < range.end) {
        if t.range().start > at {
            out.push_str(&value[at..t.range().start]);
        }
        out.push_str(t.token().label());
        at = t.range().end;
    }
    if at < range.end {
        out.push_str(&value[at..range.end]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_tokens() {
        let c = markdown_content("a **kalın** ve *eğik* [site](https://p2.app) son");
        let ids: Vec<&str> = c.tokens().iter().map(|t| t.token().id().as_ref()).collect();
        assert_eq!(ids, vec!["b", "i", "a"]);
        assert_eq!(c.tokens()[0].token().label().as_ref(), "kalın");
        assert_eq!(c.text().as_ref(), "a **kalın** ve *eğik* [site](https://p2.app) son");
    }

    #[test]
    fn code_fence() {
        assert_eq!(fence_if_code("const a = 1;\nlet b = 2;").as_deref(), Some("```\nconst a = 1;\nlet b = 2;\n```"));
        assert_eq!(fence_if_code("merhaba\ndünya"), None);
        assert_eq!(fence_if_code("tek satır;"), None);
    }
}
