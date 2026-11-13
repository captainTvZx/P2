//! P2 token'ları — Electron'un birebir karşılığı.
//!
//! Kaynaklar (Electron referanstır; birini değiştirirsen ötekini de güncelle):
//! - `src/p2-theme.css` `:root` — P2 Mavi (varsayılan tema) sabitleri → [`Palette::p2_dark`]
//! - `src/theme.ts` `P2_THEMES` + `paletteFor()` — diğer 18 hazır tema ve özel tema → [`crate::palette`]
//! - `src/p2-theme.css` geometri/tipografi → [`radius`], [`space`], [`font_size`], [`FONT_SCALE`]
//!
//! Eşleme tablosu: `rust/tools/parity/TOKENS.md`.

use gpui_kit::{BoxShadow, Hsla, Pixels, point, px, rgb, rgba};

fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}
fn ca(hex_rgba: u32) -> Hsla {
    rgba(hex_rgba).into()
}
/// CSS `rgba(r,g,b,a)` birebir (a 0..1).
pub fn css_rgba(r: u8, g: u8, b: u8, a: f32) -> Hsla {
    gpui_kit::Rgba { r: r as f32 / 255., g: g as f32 / 255., b: b as f32 / 255., a }.into()
}
/// CSS `hsl(h s% l%)` birebir.
pub fn css_hsl(h: f32, s: f32, l: f32) -> Hsla {
    gpui_kit::hsla(h / 360., s / 100., l / 100., 1.)
}

/// CSS `box-shadow` katmanı (GPUI içe gölgeyi de çizer: `--p2-inset-top`).
#[derive(Clone, Copy, Debug)]
pub struct Shadow {
    pub x: f32,
    pub y: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: Hsla,
    pub inset: bool,
}

impl Shadow {
    pub const fn new(x: f32, y: f32, blur: f32, spread: f32, color: Hsla) -> Self {
        Self { x, y, blur, spread, color, inset: false }
    }
}

/// Birden çok gölge katmanını GPUI `box_shadow` listesine çevirir (CSS sırası korunur).
pub fn shadows(layers: &[Shadow]) -> Vec<BoxShadow> {
    layers
        .iter()
        .map(|s| BoxShadow { color: s.color, offset: point(px(s.x), px(s.y)), blur_radius: px(s.blur), spread_radius: px(s.spread), inset: s.inset })
        .collect()
}

/// CSS `linear-gradient(<açı>deg, ...)`. GPUI iki durak çizer: 3+ duraklı gradyanlarda (ör.
/// `--p2-glass-strong`) [`Gradient::background`] ilk ve son durağı kullanır.
#[derive(Clone, Debug)]
pub struct Gradient {
    pub angle: f32,
    pub stops: Vec<(Hsla, f32)>,
}

impl Gradient {
    pub fn background(&self) -> gpui_kit::Background {
        let first = self.stops.first().copied().unwrap_or((gpui_kit::transparent_black(), 0.));
        let last = self.stops.last().copied().unwrap_or(first);
        gpui_kit::linear_gradient(self.angle, gpui_kit::linear_color_stop(first.0, first.1), gpui_kit::linear_color_stop(last.0, last.1))
    }
}

/// Kimlik tonu (`--p2-hue-N-bg`, `-bg2`, `-fg`).
#[derive(Clone, Copy, Debug)]
pub struct Hue {
    pub bg: Hsla,
    pub bg2: Hsla,
    pub fg: Hsla,
}

/// Uygulamanın bütün renk token'ları. Alan adları CSS değişkeninin `--p2-` önekisiz,
/// `-` → `_` biçimidir (`--p2-text-2` → `text_2`). İki Rust'a özgü alan: `mention_bg`, `code_bg`.
#[derive(Clone, Debug)]
pub struct Palette {
    pub dark: bool,
    /// Etkin tema kimliği (`p2-mavi`, `kagit`, … ya da `custom`).
    pub theme_id: String,
    // ---- Yaprak merdivenler
    pub navy_950: Hsla,
    pub navy_900: Hsla,
    pub navy_850: Hsla,
    pub navy_800: Hsla,
    pub navy_700: Hsla,
    pub navy_600: Hsla,
    pub navy_500: Hsla,
    pub slate_400: Hsla,
    pub slate_300: Hsla,
    pub slate_200: Hsla,
    pub slate_100: Hsla,
    pub white: Hsla,
    pub blue_900: Hsla,
    pub blue_800: Hsla,
    pub blue_700: Hsla,
    pub blue_600: Hsla,
    pub blue_500: Hsla,
    pub blue_400: Hsla,
    pub blue_300: Hsla,
    pub blue_200: Hsla,
    pub blue_100: Hsla,
    // ---- Zemin katmanları
    pub bg_0: Hsla,
    pub bg_1: Hsla,
    pub bg_2: Hsla,
    pub bg_3: Hsla,
    pub bg_hover: Hsla,
    pub hover_soft: Hsla,
    pub bg_selected: Hsla,
    pub glass: Gradient,
    pub glass_strong: Gradient,
    pub overlay: Hsla,
    // ---- Kenarlık
    pub border_subtle: Hsla,
    pub border: Hsla,
    pub border_strong: Hsla,
    pub border_accent: Hsla,
    pub line: Hsla,
    // ---- Metin
    pub text: Hsla,
    pub text_strong: Hsla,
    pub text_2: Hsla,
    pub text_3: Hsla,
    pub text_4: Hsla,
    pub text_accent: Hsla,
    pub text_on_accent: Hsla,
    pub neutral_fallback: Hsla,
    pub link: Hsla,
    // ---- Vurgu
    pub accent: Hsla,
    pub accent_hover: Hsla,
    pub accent_active: Hsla,
    pub accent_soft: Hsla,
    pub accent_soft_2: Hsla,
    pub accent_grad: Gradient,
    pub accent_bar: Gradient,
    pub accent_glow: Shadow,
    pub accent_glow_strong: Shadow,
    /// `--p2-ring`: `0 0 0 3px <renk>` (odak halkası)
    pub ring: Shadow,
    // ---- Durum
    pub success: Hsla,
    pub success_soft: Hsla,
    pub success_border: Hsla,
    pub success_text: Hsla,
    pub success_bg: Hsla,
    pub success_deep: Hsla,
    pub success_mid: Hsla,
    pub success_light: Hsla,
    pub text_on_success: Hsla,
    pub danger: Hsla,
    pub danger_deep: Hsla,
    pub danger_soft: Hsla,
    pub danger_border: Hsla,
    pub danger_text: Hsla,
    pub danger_bg: Hsla,
    pub danger_mid: Hsla,
    pub danger_light: Hsla,
    pub warning: Hsla,
    pub warning_soft: Hsla,
    pub warning_text: Hsla,
    pub warning_bg: Hsla,
    pub warning_deep: Hsla,
    // ---- Gölge
    pub inset_top: Shadow,
    pub shadow_sm: Vec<Shadow>,
    pub shadow_md: Vec<Shadow>,
    /// `--p2-shadow-lg` = iki gölge + `--p2-accent-glow` + `--p2-inset-top`
    pub shadow_lg: Vec<Shadow>,
    // ---- Kimlik tonları
    pub hues: [Hue; 10],
    // ---- Rust'a özgü (CSS'te ayrı token yok; kullanıldığı yerde CSS'ten doğrulanmalı)
    pub mention_bg: Hsla,
    pub code_bg: Hsla,
}

/// `--p2-hue-N` tonlarının açısı (theme.ts `HUES`).
pub const HUES: [f32; 10] = [223., 252., 168., 328., 41., 203., 22., 135., 270., 354.];

impl Palette {
    /// P2 Mavi (koyu, varsayılan) — `src/p2-theme.css` `:root` sabitleri birebir.
    pub fn p2_dark() -> Self {
        let navy_950 = c(0x070b14);
        let navy_900 = c(0x0a1020);
        let navy_850 = c(0x0e1626);
        let navy_800 = c(0x121c30);
        let navy_700 = c(0x182440);
        let navy_600 = c(0x1f2f52);
        let navy_500 = c(0x2b3e66);
        let slate_400 = c(0x4d6690);
        let slate_300 = c(0x7d93b8);
        let slate_200 = c(0xa9bcdc);
        let slate_100 = c(0xd6e4ff);
        let white = c(0xeef5ff);
        let (blue_900, blue_800, blue_700, blue_600, blue_500) = (c(0x0f1f44), c(0x173064), c(0x1f4aa8), c(0x2c7bff), c(0x4f9dff));
        let (blue_400, blue_300, blue_200, blue_100) = (c(0x7cc4ff), c(0x8fd0ff), c(0xb9dcff), c(0xdcedff));
        let accent_glow = Shadow::new(0., 0., 34., 0., css_rgba(40, 120, 255, 0.18));
        let inset_top = Shadow { inset: true, ..Shadow::new(0., 1., 0., 0., css_rgba(255, 255, 255, 0.08)) };
        let hue = |h: f32, s: f32, l1: f32, l2: f32, fs: f32, fl: f32| Hue { bg: css_hsl(h, s, l1), bg2: css_hsl(h, s, l2), fg: css_hsl(h, fs, fl) };
        Self {
            dark: true,
            theme_id: "p2-mavi".into(),
            navy_950, navy_900, navy_850, navy_800, navy_700, navy_600, navy_500,
            slate_400, slate_300, slate_200, slate_100, white,
            blue_900, blue_800, blue_700, blue_600, blue_500, blue_400, blue_300, blue_200, blue_100,
            bg_0: navy_950,
            bg_1: navy_900,
            bg_2: navy_850,
            bg_3: navy_800,
            bg_hover: navy_700,
            hover_soft: css_rgba(120, 170, 255, 0.07),
            bg_selected: blue_800,
            glass: Gradient { angle: 135., stops: vec![(css_rgba(14, 22, 38, 0.96), 0.), (css_rgba(8, 12, 22, 0.94), 1.)] },
            glass_strong: Gradient { angle: 160., stops: vec![(c(0x131d31), 0.), (c(0x0e1626), 0.48), (c(0x0a1020), 1.)] },
            overlay: css_rgba(4, 8, 18, 0.62),
            border_subtle: css_rgba(120, 170, 255, 0.10),
            border: css_rgba(120, 170, 255, 0.18),
            border_strong: css_rgba(96, 170, 255, 0.38),
            border_accent: blue_500,
            line: navy_600,
            text: slate_100,
            text_strong: white,
            text_2: css_rgba(214, 228, 255, 0.78),
            text_3: slate_300,
            text_4: slate_400,
            text_accent: c(0x7fb6ff),
            text_on_accent: c(0xffffff),
            neutral_fallback: c(0x79818c),
            link: blue_400,
            accent: blue_600,
            accent_hover: blue_500,
            accent_active: blue_700,
            accent_soft: css_rgba(44, 123, 255, 0.16),
            accent_soft_2: css_rgba(44, 123, 255, 0.28),
            accent_grad: Gradient { angle: 135., stops: vec![(c(0x4f9dff), 0.), (c(0x2c7bff), 1.)] },
            accent_bar: Gradient { angle: 90., stops: vec![(c(0x4f9dff), 0.), (c(0x8fd0ff), 1.)] },
            accent_glow,
            accent_glow_strong: Shadow::new(0., 6., 18., 0., css_rgba(20, 90, 220, 0.45)),
            ring: Shadow::new(0., 0., 0., 3., css_rgba(79, 157, 255, 0.35)),
            success: c(0x3ddc84),
            success_soft: css_rgba(61, 220, 132, 0.16),
            success_border: css_rgba(61, 220, 132, 0.4),
            success_text: c(0x8fe8b4),
            success_bg: c(0x122a1d),
            success_deep: c(0x179c58),
            success_mid: c(0x1fb86a),
            success_light: c(0x5eeaa0),
            text_on_success: c(0x06281a),
            danger: c(0xff5c6c),
            danger_deep: c(0x8f2f38),
            danger_soft: css_rgba(255, 92, 108, 0.16),
            danger_border: css_rgba(255, 92, 108, 0.42),
            danger_text: c(0xff9aa4),
            danger_bg: c(0x2a1420),
            danger_mid: c(0xd9404f),
            danger_light: c(0xff7a87),
            warning: c(0xffb545),
            warning_soft: css_rgba(255, 181, 69, 0.16),
            warning_text: c(0xffd08a),
            warning_bg: c(0x2a2210),
            warning_deep: c(0xb8842e),
            inset_top,
            shadow_sm: vec![Shadow::new(0., 4., 12., 0., css_rgba(0, 0, 0, 0.35))],
            shadow_md: vec![Shadow::new(0., 12., 30., 0., css_rgba(0, 0, 0, 0.45)), Shadow::new(0., 0., 0., 1., css_rgba(0, 0, 0, 0.3))],
            shadow_lg: vec![Shadow::new(0., 18., 44., 0., css_rgba(0, 0, 0, 0.55)), Shadow::new(0., 0., 0., 1., css_rgba(0, 0, 0, 0.35)), accent_glow, inset_top],
            hues: [
                hue(223., 45., 24., 32., 70., 90.),
                hue(252., 45., 24., 32., 70., 90.),
                hue(168., 45., 22., 30., 70., 88.),
                hue(328., 42., 26., 34., 70., 90.),
                hue(41., 45., 24., 32., 75., 88.),
                hue(203., 48., 24., 32., 70., 90.),
                hue(22., 45., 24., 32., 70., 90.),
                hue(135., 38., 22., 30., 65., 88.),
                hue(270., 42., 26., 34., 70., 90.),
                hue(354., 42., 26., 34., 70., 90.),
            ],
            mention_bg: ca(0x2c7bff38),
            code_bg: navy_900,
        }
    }

    /// "Açık" tercihi: Electron'daki ilk açık hazır tema (Kâğıt, `kagit`). Electron'da P2 Mavi'nin
    /// açık bir eşi yoktur; açık görünüm her zaman `paletteFor()` ile üretilen bir temadır.
    pub fn p2_light() -> Self {
        Self::for_theme("kagit")
    }

    /// Tema kimliğinden palet (`p2-mavi` CSS sabitleri, diğerleri `paletteFor()`).
    /// Eski Rust ayarları: "dark" → p2-mavi, "light" → kagit.
    pub fn for_theme(id: &str) -> Self {
        match id {
            "" | "dark" | "p2-mavi" => Self::p2_dark(),
            "light" => Self::for_theme("kagit"),
            other => match crate::palette::theme_by_id(other) {
                Some(spec) => crate::palette::palette_for(&spec),
                None => Self::p2_dark(),
            },
        }
    }

    /// Avatar/proje kimlik tonu: (zemin, yazı) — `--p2-hue-N-bg` / `-fg`.
    pub fn hue(&self, index: usize) -> (Hsla, Hsla) {
        let h = self.hues[index % 10];
        (h.bg, h.fg)
    }

    pub fn shadow_sm(&self) -> Vec<BoxShadow> {
        shadows(&self.shadow_sm)
    }
    pub fn shadow_md(&self) -> Vec<BoxShadow> {
        shadows(&self.shadow_md)
    }
    pub fn shadow_lg(&self) -> Vec<BoxShadow> {
        shadows(&self.shadow_lg)
    }
}

/// Geometri (`--p2-r-*`).
pub mod radius {
    use super::*;
    pub const XS: Pixels = px(6.);
    pub const SM: Pixels = px(8.);
    pub const MD: Pixels = px(11.);
    pub const LG: Pixels = px(14.);
    pub const XL: Pixels = px(18.);
    /// `--p2-r-pill`
    pub const PILL: Pixels = px(999.);
}

/// Boşluk (`--p2-s-*`).
pub mod space {
    use super::*;
    pub const S1: Pixels = px(4.);
    pub const S2: Pixels = px(8.);
    pub const S3: Pixels = px(12.);
    pub const S4: Pixels = px(16.);
    pub const S5: Pixels = px(20.);
    pub const S6: Pixels = px(24.);
    pub const S8: Pixels = px(32.);
}

/// Electron'un varsayılan yazı ölçeği (Ayarlar > Görünüm, `defaultAppearance.fontScale` = 85).
/// `applyAppearance()` CSS'teki BÜTÜN düz `Npx` yazı boyutlarını ve `--p2-fs-*` token'larını bununla
/// çarpar; `:root` yazı boyutu 16px × 0,85 = 13,6px olur. Satır yüksekliği, boşluk, kutu boyutları
/// ÖLÇEKLENMEZ. Yani CSS'te `font-size: 12px` gördüğün yerde Rust'ta [`fs`]`(12.)` yaz.
pub const FONT_SCALE: f32 = 0.85;

/// CSS'teki düz `font-size: Npx` değerinin ekranda çizilen boyu (× [`FONT_SCALE`]).
pub const fn fs(css_px: f32) -> Pixels {
    px(css_px * FONT_SCALE)
}

/// CSS `em` cinsinden harf aralığı (`letter-spacing: .08em`) → px; `font_px` ölçeklenmiş yazı boyu.
pub fn tracking(em: f32, font_px: Pixels) -> Pixels {
    px(em * f32::from(font_px))
}

/// Yazı boyutları (`--p2-fs-*`), ölçek uygulanmış — Electron'da hesaplanan değerin aynısı.
pub mod font_size {
    use super::*;
    /// `--p2-fs-2xs` 9.5px
    pub const XXS: Pixels = fs(9.5);
    /// `--p2-fs-xs` 10.5px
    pub const XS: Pixels = fs(10.5);
    /// `--p2-fs-sm` 11.5px
    pub const SM: Pixels = fs(11.5);
    /// `--p2-fs-md` 13px
    pub const MD: Pixels = fs(13.);
    /// `--p2-fs-lg` 13.5px
    pub const LG: Pixels = fs(13.5);
    /// `--p2-fs-xl` 15px
    pub const XL: Pixels = fs(15.);
    /// `--p2-fs-2xl` 18px
    pub const XXL: Pixels = fs(18.);
    /// `--p2-fs-3xl` 24px
    pub const XXXL: Pixels = fs(24.);
    /// `:root` / `body` yazı boyu: 16px × ölçek = 13.6px (rem tabanı; `1em` buna göre).
    pub const ROOT: Pixels = fs(16.);
}

/// `--p2-kicker-tracking` (.18em) — üst başlık (eyebrow) harf aralığı.
pub const KICKER_TRACKING_EM: f32 = 0.18;

/// `--p2-font` yığınının ilk ailesi: Inter (değişken). `crate::fonts::install` gömülü
/// `InterVariable.ttf`'i yükler.
/// DirectWrite aileyi STAT tablosundan "Inter Variable Text" (opsz 14 örnekleri) olarak kaydeder;
/// "Inter Variable" adı çözülmez ve sessizce Segoe UI'ye düşer (examples/fonts.rs ile doğrulandı).
pub const FONT: &str = "Inter Variable Text";
/// Electron yığınındaki yedek (Inter yoksa).
pub const FONT_FALLBACK: &str = "Segoe UI";
/// `--p2-font-mono`: 'Cascadia Mono', Consolas, 'Courier New', monospace.
pub const FONT_MONO: &str = "Cascadia Mono";
/// Emoji yığını (`"Segoe UI Emoji", "Apple Color Emoji", "Noto Color Emoji"`).
pub const FONT_EMOJI: &str = "Segoe UI Emoji";
/// CSS `line-height: normal`, Inter için (hhea ascender 0.96875 + |descender| 0.2421875 = 1.2109).
/// GPUI'nin varsayılanı φ≈1.618'dir; Electron'da `line-height` verilmeyen her metin bu oranla çizilir.
pub const LINE_HEIGHT_NORMAL: f32 = 1.2109;

/// Katman sırası (`--p2-z-*`); GPUI'de `deferred().with_priority()` sırası için başvuru.
pub mod z {
    pub const PANEL: usize = 40;
    pub const DIALOG: usize = 90;
    pub const MODAL: usize = 120;
    pub const MENU: usize = 140;
    pub const VIEWER: usize = 160;
    pub const UPDATE: usize = 4000;
    pub const TOAST: usize = 4200;
}
