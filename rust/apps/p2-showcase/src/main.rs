//! Faz 0 / Spike 1 — GPUI arayüz vitrini.
//!
//! Kanıtlanacaklar (docs/rust-gecis-plani.md §4, Faz 0):
//! 10.000 mesajlık sanal liste, zengin metin + renkli emoji, TR/JA/ZH IME ile yazma,
//! elastik modal, toast kuyruğu, sayfa geçişi, açık/koyu tema; ve ölçüm: bellek,
//! simge durumunda CPU, ilk kareye kadar geçen süre.
//!
//! Çalıştırma: `cargo run -p p2-showcase --release`

mod data;

mod view;

use std::sync::OnceLock;
use std::time::Instant;

use gpui_kit::component::TitleBar;
use gpui_kit::{AppContext as _, Bounds, WindowBounds, WindowOptions, px, size};

/// Süreç başlangıcı; ilk kare süresi buna göre ölçülür.
pub static STARTED: OnceLock<Instant> = OnceLock::new();

fn main() {
    STARTED.get_or_init(Instant::now);
    // Lucide ikonları: başlık çubuğunun küçült/büyüt/kapat düğmeleri bunlarla çizilir.
    gpui_kit::application().with_assets(gpui_kit::assets::Assets).run(|cx| {
        gpui_kit::init(cx);
        p2_ui::theme::apply(p2_ui::Palette::p2_dark(), cx);

        let bounds = Bounds::centered(None, size(px(1280.), px(800.)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(900.), px(600.))),
            ..TitleBar::window_options()
        };
        gpui_kit::open_window(options, cx, |window, cx| cx.new(|cx| view::Showcase::new(window, cx)))
            .expect("pencere açılamadı");
        cx.activate(true);
    });
}
