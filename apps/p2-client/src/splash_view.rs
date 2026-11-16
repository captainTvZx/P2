//! Yükleniyor ekranı — App.tsx `<main className="splash"><Logo size="md" pop/><p>…</p></main>` + styles.css `.splash`:
//! başlık çubuğu altındaki alan (ölçülen 762px), ızgara ortası, gap 14,
//! `radial-gradient(circle at 50% 40%, accent-soft 0, transparent 42%), bg-0`; yazı `fs-sm` text-3,
//! `p2-rise` 220ms (.2 sn gecikme); logo `p2-logo-pop`; sayfa `p2-page`.

use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, SharedString, Window, div, px};
use p2_ui::components::{logo_image, radial_background};
use p2_ui::text::normal_line_height;
use p2_ui::tokens::{Palette, font_size};
use p2_ui::{Entrance, P2Motion as _};

pub fn loading(text: SharedString, p: &Palette, window: &Window) -> AnyElement {
    let vp = window.viewport_size();
    // `.splash`: içerik kutunun ortasında.
    // Ölçüm (dom-styles, 1280×800): `main.splash` [0,38,1280,762] — kutu başlık çubuğunun altındaki alan kadar.
    let (w, h) = (f32::from(vp.width), f32::from(vp.height) - 38.);
    let size = font_size::SM;
    div()
        .relative()
        .w(px(w))
        .h(px(h))
        .child(div().absolute().top_0().left_0().child(radial_background(w, h, (0.5, 0.4), &[(p.accent_soft, 0.), (p.bg_0, 0.42)], p.bg_0)))
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(14.))
                // `.p2-logo.pop`: `p2-logo-pop .55s var(--ease-pop) .12s both` (ölçek .4 → 1.14 → 1, −14° → 3° → 0).
                .child(logo_image(44., p).p2_enter("splash-logo", Entrance::LogoPop, Duration::from_millis(120)))
                .child(
                    div()
                        .my(size)
                        .h(p2_ui::text::scaled(normal_line_height(size)))
                        .line_height(normal_line_height(size))
                        .text_size(size)
                        .text_color(p.text_3)
                        .child(text)
                        .p2_enter("splash-text", Entrance::Rise, Duration::from_millis(200)),
                ),
        )
        .p2_enter("splash-page", Entrance::Page, Duration::ZERO)
        .into_any_element()
}
