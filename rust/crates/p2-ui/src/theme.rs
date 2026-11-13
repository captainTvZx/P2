//! P2 paletini global durum olarak tutar ve gpui-component temasına aktarır;
//! böylece kütüphanenin girdi, kaydırma çubuğu gibi bileşenleri de P2 renklerinde çizilir.

use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::{App, Global};

use crate::tokens::{FONT, Palette, font_size, radius};

pub struct P2Theme(pub Palette);

struct FontsInstalled;
impl Global for FontsInstalled {}

impl Global for P2Theme {}

pub trait ActiveP2 {
    fn p2(&self) -> &Palette;
}

impl ActiveP2 for App {
    fn p2(&self) -> &Palette {
        &self.global::<P2Theme>().0
    }
}

/// Paleti kurar (ilk açılışta ve tema değişince).
pub fn apply(palette: Palette, cx: &mut App) {
    // Gömülü Inter yalnız bir kez eklenir (her uygulama — istemci, overlay, konsol — aynı yazıyı kullanır).
    if !cx.has_global::<FontsInstalled>() {
        crate::fonts::install(cx);
        cx.set_global(FontsInstalled);
    }
    let mode = if palette.dark { ThemeMode::Dark } else { ThemeMode::Light };
    Theme::change(mode, None, cx);

    let theme = Theme::global_mut(cx);
    theme.font_family = FONT.into();
    // Electron: body 16px × yazı ölçeği 0,85 = 13,6px; girdiler/düğmeler "font: inherit".
    theme.font_size = font_size::ROOT;
    theme.radius = radius::SM;
    theme.radius_lg = radius::LG;

    let p = &palette;
    let colors = &mut theme.colors;
    colors.background = p.bg_0;
    colors.foreground = p.text;
    colors.border = p.border;
    colors.input = p.border;
    colors.ring = p.accent_hover;
    colors.caret = p.accent_hover;
    colors.selection = p.accent_soft_2;
    colors.primary = p.accent;
    colors.primary_hover = p.accent_hover;
    colors.primary_active = p.accent_active;
    colors.primary_foreground = p.text_on_accent;
    colors.muted = p.bg_3;
    // gpui-component girdileri yer tutucuyu bununla çizer: ui.css `.p2-input::placeholder`, `.channel-search`,
    // `.pn-text` → `--p2-text-4` (Chromium varsayılanı #757575 yalnız kural olmayan auth alanlarında).
    colors.muted_foreground = p.text_4;
    colors.popover = p.bg_2;
    colors.popover_foreground = p.text;
    colors.accent = p.bg_hover;
    colors.accent_foreground = p.text_strong;
    colors.scrollbar = gpui_kit::transparent_black();
    // ui.css: *::-webkit-scrollbar-thumb rgba(120,170,255,.18), hover .32 (her temada aynı sabit), iz saydam, 10px.
    colors.scrollbar_thumb = crate::tokens::css_rgba(120, 170, 255, 0.18);
    colors.scrollbar_thumb_hover = crate::tokens::css_rgba(120, 170, 255, 0.32);
    colors.link = p.link;
    colors.danger = p.danger;
    colors.success = p.success;
    colors.warning = p.warning;
    colors.title_bar = p.bg_0;
    colors.title_bar_border = p.border_subtle;
    colors.overlay = p.overlay;

    cx.set_global(P2Theme(palette));
    cx.refresh_windows();
}
