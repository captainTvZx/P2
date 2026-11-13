//! Oyun içi overlay görünümü — ana uygulamayla aynı GPUI + p2-ui tasarım sistemi.
//! Kart konumu ve düğme koordinatları sabittir; girdi testi (PostMessage) bunları kullanır.

use gpui_kit::prelude::*;
use gpui_kit::{Context, FontWeight, Hsla, Window, div, px};
use p2_ui::theme::ActiveP2 as _;
use p2_ui::tokens::{font_size, radius};

pub const CARD_W: f32 = 380.;
pub const CARD_H: f32 = 296.;
pub const MARGIN: f32 = 24.;
pub const BUTTON_W: f32 = 132.;
pub const BUTTON_H: f32 = 34.;

/// Kartın sol üst köşesi (overlay yüzeyi genişliğine göre sağ üstte).
pub fn card_origin(surface_w: f32) -> (f32, f32) {
    (surface_w - CARD_W - MARGIN, MARGIN)
}
/// Düğmenin merkezi (girdi testinde tıklanacak nokta).
pub fn button_center(surface_w: f32) -> (i32, i32) {
    let (x, y) = card_origin(surface_w);
    ((x + CARD_W - 16. - BUTTON_W / 2.) as i32, (y + CARD_H - 16. - BUTTON_H / 2.) as i32)
}

pub struct OverlayView {
    pub surface_w: f32,
    pub frame: u64,
    pub game_fps: f32,
    pub clicks: u32,
}

impl OverlayView {
    pub fn new(surface_w: f32) -> Self {
        Self { surface_w, frame: 0, game_fps: 0., clicks: 0 }
    }
}

fn person(name: &str, initials: &str, hue: (Hsla, Hsla), dot: Hsla, ring: Hsla, speaking: bool) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .relative()
                .child(
                    div()
                        .size(px(26.))
                        .rounded_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(hue.0)
                        .text_color(hue.1)
                        .text_size(px(10.))
                        .font_weight(FontWeight::BOLD)
                        .when(speaking, |s| s.border_2().border_color(dot))
                        .child(initials.to_string()),
                )
                .child(div().absolute().right(px(-1.)).bottom(px(-1.)).size(px(9.)).rounded_full().border_2().border_color(ring).bg(dot)),
        )
        .child(div().text_size(font_size::MD).child(name.to_string()))
}

impl Render for OverlayView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.p2().clone();
        let (x, y) = card_origin(self.surface_w);
        // İndirme çubuğu: kare sayısına bağlı (deterministik saat; her karede ilerler).
        let progress = (self.frame % 240) as f32 / 240.;
        let clicked = self.clicks > 0;

        div().size_full().font_family(p2_ui::tokens::FONT).text_color(p.text).child(
            div()
                .absolute()
                .left(px(x))
                .top(px(y))
                .w(px(CARD_W))
                .h(px(CARD_H))
                .p_4()
                .flex()
                .flex_col()
                .gap_3()
                .rounded(radius::LG)
                .border_1()
                .border_color(p.border_strong)
                .bg(p.bg_2.opacity(0.94))
                .shadow_lg()
                // Başlık
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(div().px_1p5().rounded(radius::XS).bg(p.accent).text_color(p.text_on_accent).text_size(font_size::SM).font_weight(FontWeight::BOLD).child("P2"))
                        .child(div().text_color(p.text_strong).font_weight(FontWeight::SEMIBOLD).child("Oyun içi overlay"))
                        .child(div().flex_1())
                        .child(div().text_size(font_size::XS).text_color(p.text_3).child("Rust · GPUI")),
                )
                // Sesli oda
                .child(div().text_size(font_size::XS).font_weight(FontWeight::SEMIBOLD).text_color(p.text_4).child("SESLİ ODA · ekip-odası"))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1p5()
                        .child(person("Hasan Özoğlu", "HÖ", p.hue(0), p.success, p.bg_2, true))
                        .child(person("Cem Akkaya", "CA", p.hue(1), p.success, p.bg_2, false))
                        .child(person("Aslı Demir", "AD", p.hue(2), p.warning, p.bg_2, false)),
                )
                // İndirme
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .flex()
                                .text_size(font_size::SM)
                                .child(div().flex_1().text_color(p.text_2).child("1950s Tow Truck · 0.9.10 iniyor"))
                                .child(div().text_color(p.text_accent).child(format!("%{:.0}", progress * 100.))),
                        )
                        .child(
                            div()
                                .h(px(6.))
                                .w_full()
                                .rounded_full()
                                .bg(p.bg_3)
                                .child(div().h_full().w(px((CARD_W - 32.) * progress)).rounded_full().bg(p.accent_hover)),
                        ),
                )
                .child(div().flex_1())
                // Alt satır: sayaç + düğme
                .child(
                    div()
                        .flex()
                        .items_center()
                        .child(
                            div()
                                .flex_1()
                                .text_size(font_size::XS)
                                .text_color(p.text_3)
                                .child(format!("kare #{} · oyun {:.0} FPS", self.frame, self.game_fps)),
                        )
                        .child(
                            div()
                                .id("overlay-button")
                                .w(px(BUTTON_W))
                                .h(px(BUTTON_H))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(radius::SM)
                                .bg(if clicked { p.success } else { p.accent })
                                .text_color(if clicked { p.bg_0 } else { p.text_on_accent })
                                .text_size(font_size::SM)
                                .font_weight(FontWeight::SEMIBOLD)
                                .cursor_pointer()
                                .hover(|s| s.bg(p.accent_hover))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.clicks += 1;
                                    cx.notify();
                                }))
                                .child(if clicked { format!("Tıklandı ✓ ({})", self.clicks) } else { "Tıkla".to_string() }),
                        ),
                ),
        )
    }
}
