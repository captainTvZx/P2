//! Ekran paylaşımı penceresi — ScreenShareModal.tsx + call.css `.share-layer/.share-modal` birebir:
//! başlık + ×, Ekranlar/Pencereler sekmeleri, küçük resimli kaynak ızgarası (minmax 190px),
//! Çözünürlük / Kare hızı seçimleri, "Bilgisayar sesini paylaş", ipucu, Vazgeç / Paylaşımı başlat.
//! Kaynaklar libwebrtc DesktopCapturer'dan arka planda okunur (küçük resim yok → `.share-noshot`).
//! Test/parite: `P2_TEST_SOURCES=1` gerçek ekran ve pencereleri hiç listelemez; parite aracının
//! Electron tarafındaki sahte `desktopCapturer` ile aynı üç sentetik kaynak (test deseni) gelir.

use std::sync::Mutex;
use p2_ui::P2Fx as _; // backdrop_blur

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Context, EventEmitter, Hsla, MouseButton, SharedString, Window, div, px};
use p2_media::{ScreenInfo, ScreenSource};
use p2_ui::P2Text as _;
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::{Palette, css_rgba, fs, shadows};
use p2_ui::{Entrance, P2Motion as _};

use crate::task;

pub enum PickerEvent {
    /// Kaynak, kare hızı, kalite ön ayarı (`1080p`…), bilgisayar sesi.
    Picked(ScreenSource, u32, String, bool),
    Cancelled,
}

/// screenShare.ts `QUALITIES` (kimlik, etiket) ve `FRAME_RATES`.
const QUALITIES: [(&str, &str); 6] = [("2160p", "4K · 2160p"), ("1440p", "2K · 1440p"), ("1080p", "Full HD · 1080p"), ("720p", "HD · 720p"), ("480p", "Düşük · 480p"), ("360p", "Çok düşük · 360p")];
const FRAME_RATES: [u32; 4] = [5, 15, 30, 60];

/// settings.ts `shareQuality`/`shareFps` (varsayılan 1080p, 30): oturum boyunca hatırlanır.
static REMEMBERED: Mutex<(&str, u32)> = Mutex::new(("1080p", 30));

/// Kaynak: tür (ekran/pencere), ad, küçük resim rengi (sentetik kaynaklarda).
struct Source {
    source: ScreenSource,
    name: String,
    screen: bool,
    thumb: Option<[u8; 3]>,
}

pub struct ScreenPicker {
    sources: Option<Vec<Source>>,
    selected: Option<usize>,
    kind_screen: bool,
    fps: u32,
    quality: &'static str,
    /// "Bilgisayar sesini paylaş".
    audio: bool,
    /// Açık seçim listesi: 0 çözünürlük, 1 kare hızı.
    open: Option<u8>,
}

impl EventEmitter<PickerEvent> for ScreenPicker {}

/// Parite: Electron `electron-main.cjs` FAKE_SOURCES ile aynı.
fn synthetic_sources() -> Vec<Source> {
    [("Ekran 1", true, [0x1d, 0x3b, 0x66]), ("P2 — parite", false, [0x2b, 0x4c, 0x3a]), ("Not Defteri", false, [0x4a, 0x2f, 0x55])]
        .into_iter()
        .map(|(name, screen, rgb)| Source { source: ScreenSource::TestPattern, name: name.into(), screen, thumb: Some(rgb) })
        .collect()
}

impl ScreenPicker {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let synthetic = std::env::var_os("P2_TEST_SOURCES").is_some();
        let list = move || if synthetic { Vec::new() } else { p2_media::screen_sources() };
        task::run(window, cx, list, move |this, sources: Vec<ScreenInfo>, _, cx| {
            let mut sources: Vec<Source> = if synthetic {
                synthetic_sources()
            } else {
                sources.into_iter().map(|s| Source { screen: matches!(s.source, ScreenSource::Screen(_)), name: if s.title.is_empty() { "(adsız pencere)".into() } else { s.title }, source: s.source, thumb: None }).collect()
            };
            // Geliştirme/test: gerçek ekran yerine hareketli desen.
            if !synthetic && std::env::var_os("P2_TEST_PATTERN").is_some() {
                sources.insert(0, Source { source: ScreenSource::TestPattern, name: "Test deseni".into(), screen: true, thumb: Some([0x1d, 0x3b, 0x66]) });
            }
            // ScreenShareModal: ilk ekran (yoksa ilk kaynak) seçili gelir.
            this.selected = sources.iter().position(|s| s.screen).or((!sources.is_empty()).then_some(0));
            this.sources = Some(sources);
            crate::trace("ekran paylaşımı: kaynaklar okundu");
            cx.notify();
        });
        // settings.ts `shareAudio` (varsayılan açık) hatırlanır.
        let audio = p2_client_core::store::load_settings().media.share_audio;
        let (quality, fps) = *REMEMBERED.lock().unwrap();
        Self { sources: None, selected: None, kind_screen: true, fps, quality, audio, open: None }
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.selected.and_then(|ix| self.sources.as_ref()?.get(ix).map(|s| s.source.clone())) else { return };
        *REMEMBERED.lock().unwrap() = (self.quality, self.fps);
        cx.emit(PickerEvent::Picked(source, self.fps, self.quality.to_string(), self.audio));
    }
}

/// `.share-source img` / `.share-noshot`: 16/10, 7px köşe; sentetik küçük resim 320×180 `cover`.
fn thumb(rgb: Option<[u8; 3]>, w: f32, p: &Palette) -> AnyElement {
    let h = w * 10. / 16.;
    match rgb {
        Some([r, g, b]) => {
            let markup = format!(
                r##"<svg xmlns="http://www.w3.org/2000/svg" width="320" height="200" viewBox="0 0 320 180" preserveAspectRatio="xMidYMid slice"><rect width="320" height="180" fill="#{r:02x}{g:02x}{b:02x}"/><rect x="33" y="22" width="255" height="32" fill="#eeeeee"/></svg>"##
            );
            // Sabit 320×200 (16/10) belge, kutuya esnetilir: boyut değişince yeni görsel üretilmesin (titreme).
            p2_ui::icons::svg_image(markup).object_fit(gpui_kit::ObjectFit::Fill).w(px(w)).h(px(h)).rounded(px(7.)).into_any_element()
        }
        None => div().w(px(w)).h(px(h)).rounded(px(7.)).bg(p.bg_0).into_any_element(),
    }
}

/// Yerel `<select>` görünümü (h32, 8px köşe, sağda ok) — açılır liste uygulamanın menü stilinde.
fn select_box(id: &'static str, label: String, width: f32, p: &Palette) -> gpui_kit::Stateful<gpui_kit::Div> {
    div()
        .id(id)
        .relative()
        .w(px(width))
        .h(px(32.))
        .pl(px(8.))
        .pr(px(6.))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(4.))
        .rounded(px(8.))
        .border_1()
        .border_color(p.border)
        .bg(p.bg_3)
        .text_color(p.text)
        .text_size(fs(11.5))
        .cursor_pointer()
        .child(div().whitespace_nowrap().child(label))
        .child(p2_ui::glyphs::ui_stroke("chevron-down", 12., 2.2).text_color(p.text))
}

impl Render for ScreenPicker {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        let vp = window.viewport_size();
        let width = (f32::from(vp.width) - 48.).min(720.);
        // Izgara: iç genişlik (kenarlık 1 + iç boşluk 18 + ızgara 2) → auto-fill minmax(190px,1fr), aralık 9.
        let inner = width - 2. - 36. - 4.;
        let cols = (((inner + 9.) / (190. + 9.)).floor() as usize).max(1);
        let col_w = (inner - 9. * (cols as f32 - 1.)) / cols as f32;

        let tab = |id: &'static str, label: &'static str, active: bool, p: &Palette| {
            let (hover_bg, text) = (p.bg_hover, p.text);
            div()
                .id(id)
                .h(px(30.))
                .px(px(13.))
                .flex()
                .items_center()
                .rounded(px(8.))
                .text_size(fs(11.))
                .css_weight(650.)
                .cursor_pointer()
                .map(|s| if active { s.bg(p.bg_3).text_color(p.text_strong).shadow(shadows(&[p.inset_top])) } else { s.text_color(p.text_3).hover(move |s| s.bg(hover_bg).text_color(text)) })
                .child(label)
        };
        let tabs = div()
            .flex()
            .gap(px(4.))
            .p(px(3.))
            .rounded(px(11.))
            .bg(p.bg_2)
            .border_1()
            .border_color(p.border_subtle)
            .child(tab("share-tab-screen", "Ekranlar", self.kind_screen, &p).on_click(cx.listener(|this, _, _, cx| {
                this.kind_screen = true;
                cx.notify();
            })))
            .child(tab("share-tab-window", "Pencereler", !self.kind_screen, &p).on_click(cx.listener(|this, _, _, cx| {
                this.kind_screen = false;
                cx.notify();
            })));

        let mut grid = div().id("share-sources").min_h(px(150.)).max_h(px(330.)).overflow_y_scroll().p(px(2.)).flex().flex_col().gap(px(9.));
        let empty = |text: &'static str, p: &Palette| div().w_full().py(px(36.)).flex().justify_center().text_size(fs(11.)).text_color(p.text_3).child(text);
        match &self.sources {
            None => grid = grid.child(empty("Kaynaklar okunuyor…", &p)),
            Some(sources) => {
                let visible: Vec<(usize, &Source)> = sources.iter().enumerate().filter(|(_, s)| s.screen == self.kind_screen).collect();
                if visible.is_empty() {
                    grid = grid.child(empty(if self.kind_screen { "Ekran bulunamadı." } else { "Paylaşılabilir pencere yok." }, &p));
                }
                for row in visible.chunks(cols) {
                    let mut r = div().flex().gap(px(9.));
                    for (ix, s) in row {
                        let ix = *ix;
                        let active = self.selected == Some(ix);
                        let (hover_bg, shadow) = (p.bg_hover, p.shadow_sm());
                        r = r.child(
                            div()
                                .id(("share-source", ix))
                                .w(px(col_w))
                                .flex_none()
                                .flex()
                                .flex_col()
                                .gap(px(6.))
                                .p(px(6.))
                                .rounded(px(11.))
                                .border_2()
                                .border_color(if active { p.border_accent } else { gpui_kit::transparent_black() })
                                .bg(if active { p.accent_soft } else { p.bg_3 })
                                .text_color(if active { p.text_strong } else { p.text })
                                .when(active, |s| s.shadow(shadows(&[p.accent_glow])))
                                .when(!active, move |s| s.hover(move |s| s.bg(hover_bg).top(px(-2.)).shadow(shadow.clone())))
                                .cursor_pointer()
                                .child(thumb(s.thumb, col_w - 4. - 12., &p))
                                .child(div().flex().items_center().gap(px(6.)).overflow_hidden().text_size(fs(10.5)).child(div().truncate().child(s.name.clone())))
                                .tooltip({
                                    let name = s.name.clone();
                                    move |window, cx| gpui_kit::component::tooltip::Tooltip::new(name.clone()).build(window, cx)
                                })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.selected = Some(ix);
                                    cx.notify();
                                }))
                                .p2_enter(("share-source-in", ix), Entrance::Rise, std::time::Duration::ZERO),
                        );
                    }
                    grid = grid.child(r);
                }
            }
        }

        // Seçenekler: Çözünürlük, Kare hızı (açılır listeler), bilgisayar sesi.
        let quality_label = QUALITIES.iter().find(|q| q.0 == self.quality).map(|q| q.1).unwrap_or("Full HD · 1080p");
        let opt_label = |text: &'static str, p: &Palette| div().text_size(fs(10.5)).text_color(p.text_3).child(text);
        let on = self.audio;
        let accent = p.accent;
        let check = div()
            .id("share-audio")
            .mt(px(14.))
            .flex()
            .items_center()
            .gap(px(7.))
            .cursor_pointer()
            .text_size(fs(11.5))
            .text_color(p.text_3)
            .on_click(cx.listener(|this, _, _, cx| {
                this.audio = !this.audio;
                cx.notify();
            }))
            .child(
                div()
                    .size(px(15.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(3.))
                    .border_1()
                    .border_color(if on { accent } else { css_rgba(133, 133, 133, 1.) })
                    .bg(if on { accent } else { css_rgba(59, 59, 59, 1.) })
                    .when(on, |s| s.child(p2_ui::glyphs::ui_stroke("check", 12., 3.).text_color(gpui_kit::white()))),
            )
            .child("Bilgisayar sesini paylaş");
        let options = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(12.))
            .pt(px(10.))
            .border_t_1()
            .border_color(p.border_subtle)
            .child(div().flex().flex_col().gap(px(5.)).child(opt_label("Çözünürlük", &p)).child(select_box("share-quality", quality_label.into(), 114., &p).on_click(cx.listener(|this, _, _, cx| {
                this.open = if this.open == Some(0) { None } else { Some(0) };
                cx.notify();
            }))))
            .child(div().flex().flex_col().gap(px(5.)).child(opt_label("Kare hızı", &p)).child(select_box("share-fps", format!("{} FPS", self.fps), 70., &p).on_click(cx.listener(|this, _, _, cx| {
                this.open = if this.open == Some(1) { None } else { Some(1) };
                cx.notify();
            }))))
            .child(check);
        let hint = format!(
            "Yüksek çözünürlük ve kare hızı daha fazla bant genişliği ister. İzleyenler kendi tarafında kaliteyi düşürebilir.{}",
            if on { " Bilgisayar sesi doğrudan hoparlör çıkışından alınır; aramadaki kişilerin sesi yayına karışmaz." } else { "" }
        );
        let can_start = self.selected.is_some();
        let (hover_bg, strong) = (p.bg_hover, p.text_strong);
        let footer = div()
            .flex()
            .justify_end()
            .gap(px(8.))
            .child(
                div()
                    .id("share-cancel")
                    .h(px(36.))
                    .px(px(16.))
                    .flex()
                    .items_center()
                    .rounded(px(8.))
                    .border_1()
                    .border_color(p.border)
                    .text_color(p.text_2)
                    .text_size(fs(12.))
                    .css_weight(650.)
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover_bg).text_color(strong))
                    .child("Vazgeç")
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(PickerEvent::Cancelled))),
            )
            .child(
                div()
                    .id("share-start")
                    .h(px(36.))
                    .px(px(18.))
                    .flex()
                    .items_center()
                    .rounded(px(8.))
                    .bg(p.accent_grad.background())
                    .shadow(shadows(&[p.accent_glow_strong, p.inset_top]))
                    .text_color(p.text_on_accent)
                    .text_size(fs(12.))
                    .css_weight(700.)
                    .when(!can_start, |s| s.opacity(0.55))
                    .when(can_start, |s| s.cursor_pointer().hover(|s| s.top(px(-1.))))
                    .child("Paylaşımı başlat")
                    .on_click(cx.listener(|this, _, _, cx| this.start(cx))),
            );
        let (close_hover, close_strong) = (p.accent_soft, p.text_strong);
        let mut modal = div()
            .relative()
            .w(px(width))
            .max_h(px(f32::from(vp.height) - 90.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .pt(px(16.))
            .px(px(18.))
            .pb(px(14.))
            .rounded(px(18.))
            .border_1()
            .border_color(p.border_strong)
            .bg(p.glass_strong.background())
            .shadow(p.shadow_lg())
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(10.))
                    .child(div().flex().flex_col().child(div().text_size(fs(14.)).css_weight(700.).text_color(p.text_strong).child("Ekran paylaş")).child(div().mt(px(2.)).text_size(fs(10.5)).text_color(p.text_3).child("Paylaşmak istediğin ekranı ya da pencereyi seç")))
                    .child(
                        div()
                            .id("share-close")
                            .size(px(30.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .text_size(fs(19.))
                            .text_color(p.text_3)
                            .cursor_pointer()
                            .hover(move |s| s.bg(close_hover).text_color(close_strong))
                            .child("×")
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(PickerEvent::Cancelled))),
                    ),
            )
            .child(div().flex().child(tabs))
            .child(grid)
            .child(options)
            .child(div().text_size(fs(10.)).line_height(fs(15.)).text_color(p.text_3).child(hint))
            .child(footer);
        // Açılır liste (yerel <select> açılır penceresi yerine uygulama menüsü görünümü).
        if let Some(which) = self.open {
            let items: Vec<(String, bool, u8, usize)> = if which == 0 {
                QUALITIES.iter().enumerate().map(|(i, q)| (q.1.to_string(), q.0 == self.quality, 0, i)).collect()
            } else {
                FRAME_RATES.iter().enumerate().map(|(i, f)| (format!("{f} FPS"), *f == self.fps, 1, i)).collect()
            };
            let (list_left, list_bottom) = (18. + if which == 0 { 0. } else { 127. }, 14. + 36. + 8. + 45. + 12. + 2.);
            let mut list = div().min_w(px(if which == 0 { 160. } else { 90. })).p(px(5.)).flex().flex_col().gap(px(1.)).rounded(px(11.)).border_1().border_color(p.border_strong).bg(p.glass.background()).backdrop_blur(p2_ui::fx::BLUR).shadow(p.shadow_lg());
            for (label, active, kind, i) in items {
                let (soft, strong) = (p.accent_soft, p.text_strong);
                list = list.child(
                    div()
                        .id(SharedString::from(format!("share-opt-{kind}-{i}")))
                        .h(px(28.))
                        .px(px(9.))
                        .flex()
                        .items_center()
                        .rounded(px(6.))
                        .text_size(fs(11.))
                        .text_color(p.text)
                        .when(active, |s| s.bg(p.bg_selected).text_color(p.text_strong))
                        .hover(move |s| s.bg(soft).text_color(strong))
                        .cursor_pointer()
                        .child(label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if kind == 0 {
                                this.quality = QUALITIES[i].0;
                            } else {
                                this.fps = FRAME_RATES[i];
                            }
                            this.open = None;
                            cx.notify();
                        })),
                );
            }
            modal = modal.child(div().absolute().left(px(list_left)).bottom(px(list_bottom)).child(list.p2_enter("share-select", Entrance::ScaleIn, std::time::Duration::ZERO)));
        }
        let _: Option<Hsla> = None;
        // p2_enter öğeyi relative yapar: tam kaplayan katman dış mutlak sarmalayıcıda.
        // .share-layer (fixed, inset 0, z-index modal 120) pencerenin ortasına hizalanır ama başlık çubuğu
        // (.titlebar z-index 1000) üstte kalır: karartma/bulanıklık yalnız 38px altında (Electron görüntüsü).
        div().absolute().top(px(-38.)).left_0().right_0().bottom_0().child(div()
            .id("picker-scrim")
            .size_full()
            .pt(px(38.))
            .px(px(24.))
            .pb(px(24.))
            .flex()
            .items_center()
            .justify_center()
            .relative()
            .on_mouse_down(MouseButton::Left, cx.listener(|_, _, _, cx| cx.emit(PickerEvent::Cancelled)))
            .child(div().absolute().top(px(38.)).left_0().right_0().bottom_0().bg(p.overlay).backdrop_blur(p2_ui::fx::BLUR_SM))
            .on_key_down(cx.listener(|this, ev: &gpui_kit::KeyDownEvent, _, cx| {
                if ev.keystroke.key == "escape" {
                    if this.open.take().is_none() {
                        cx.emit(PickerEvent::Cancelled);
                    }
                    cx.notify();
                }
            }))
            .child(modal.p2_enter("screen-picker", Entrance::PopElastic, std::time::Duration::ZERO))
            .p2_enter("share-layer", Entrance::Fade, std::time::Duration::ZERO))
    }
}
