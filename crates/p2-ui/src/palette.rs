//! Tema motoru — `src/theme.ts` `P2_THEMES` + `paletteFor()` birebir Rust karşılığı.
//! Renk yuvarlamaları JS ile aynıdır (`Math.round`), böylece her token Electron'un `<html>`
//! üzerine yazdığı değerle bayt bayt aynı RGB'yi verir (birim testi: `paletteFor` örnek çıktıları).

use gpui_kit::Hsla;

use crate::tokens::{Gradient, HUES, Hue, Palette, Shadow, css_hsl, css_rgba};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeMode {
    Dark,
    Light,
}

/// theme.ts `ThemeSpec`.
#[derive(Clone, Debug)]
pub struct ThemeSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub mode: ThemeMode,
    /// "#rrggbb"
    pub accent: String,
    pub surface: String,
    pub tint: f32,
    pub contrast: f32,
    pub saturation: f32,
}

const fn spec(id: &'static str, name: &'static str, light: bool, accent: &'static str, surface: &'static str, tint: f32, contrast: f32, saturation: f32) -> (&'static str, &'static str, bool, &'static str, &'static str, f32, f32, f32) {
    (id, name, light, accent, surface, tint, contrast, saturation)
}

/// theme.ts `P2_THEMES` (sıra ve adlar Ayarlar > Görünüm ızgarasındakiyle aynı).
pub const THEMES: [(&str, &str, bool, &str, &str, f32, f32, f32); 19] = [
    spec("p2-mavi", "P2 Mavi", false, "#2c7bff", "#070b14", 0.55, 1., 1.),
    spec("gece", "Gece", false, "#487eb0", "#1b1f26", 0.3, 1., 1.),
    spec("komur", "Kömür", false, "#7f8c99", "#17191c", 0.12, 1., 0.75),
    spec("okyanus", "Okyanus", false, "#1f9aa8", "#121d22", 0.5, 1., 1.),
    spec("orman", "Orman", false, "#3f9c62", "#141b17", 0.45, 1., 1.),
    spec("safak", "Şafak", false, "#d2704a", "#1f1a1a", 0.4, 1., 1.),
    spec("menekse", "Menekşe", false, "#8b62d4", "#1a1722", 0.45, 1., 1.),
    spec("gulkurusu", "Gül Kurusu", false, "#c05a7d", "#20181d", 0.4, 1., 1.),
    spec("gece-yarisi", "Gece Yarısı", false, "#4460c8", "#0f1220", 0.55, 1.05, 1.),
    spec("gece-mavi", "Gece Mavisi", false, "#3d7ea6", "#141a20", 0.42, 1., 1.),
    spec("kahve", "Kahve", false, "#a1784e", "#1c1815", 0.35, 1., 0.9),
    spec("zeytin", "Zeytin", false, "#8a9a4b", "#191b14", 0.35, 1., 1.),
    spec("kagit", "Kâğıt", true, "#2f6fb0", "#d9dde2", 0.16, 0.92, 1.),
    spec("sis", "Sis", true, "#526f88", "#d4d9de", 0.18, 0.9, 1.),
    spec("gun", "Gün", true, "#267c6b", "#d6e0dc", 0.2, 0.92, 1.),
    spec("lavanta", "Lavanta", true, "#6652ad", "#ddd9e7", 0.22, 0.92, 1.),
    spec("kum", "Kum", true, "#9a6237", "#e2d9cf", 0.18, 0.9, 1.),
    spec("nane", "Nane", true, "#287e62", "#d3dfda", 0.2, 0.9, 1.),
    spec("gul", "Gül", true, "#a94e6d", "#e2d5da", 0.2, 0.9, 1.),
];

pub fn themes() -> Vec<ThemeSpec> {
    THEMES
        .iter()
        .map(|&(id, name, light, accent, surface, tint, contrast, saturation)| ThemeSpec {
            id,
            name,
            mode: if light { ThemeMode::Light } else { ThemeMode::Dark },
            accent: accent.into(),
            surface: surface.into(),
            tint,
            contrast,
            saturation,
        })
        .collect()
}

pub fn theme_by_id(id: &str) -> Option<ThemeSpec> {
    themes().into_iter().find(|t| t.id == id)
}

// ---------------------------------------------------------------- renk yardımcıları (theme.ts)

#[derive(Clone, Copy, Debug)]
struct Hsl {
    h: f64,
    s: f64,
    l: f64,
}

fn clamp(v: f64, min: f64, max: f64) -> f64 {
    v.max(min).min(max)
}
fn clamp01(v: f64) -> f64 {
    clamp(v, 0., 1.)
}
/// JS `Math.round` (yarımlar yukarı).
fn js_round(v: f64) -> f64 {
    (v + 0.5).floor()
}

fn parse(color: &str) -> Hsl {
    let value = color.trim_start_matches('#');
    let full: String = if value.len() == 3 || value.len() == 4 { value.chars().flat_map(|c| [c, c]).collect() } else { value.to_string() };
    let ch = |i: usize| full.get(i..i + 2).and_then(|s| u8::from_str_radix(s, 16).ok()).map(|v| v as f64 / 255.);
    let (Some(r), Some(g), Some(b)) = (ch(0), ch(2), ch(4)) else { return Hsl { h: 222., s: 0.48, l: 0.05 } };
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let mut h = 0.;
    if delta != 0. {
        h = if max == r { ((g - b) / delta) % 6. } else if max == g { (b - r) / delta + 2. } else { (r - g) / delta + 4. };
        h *= 60.;
        if h < 0. {
            h += 360.;
        }
    }
    let l = (max + min) / 2.;
    let s = if delta == 0. { 0. } else { delta / (1. - (2. * l - 1.).abs()) };
    Hsl { h, s, l }
}

fn to_rgb(c: Hsl) -> [u8; 3] {
    let Hsl { h, s, l } = c;
    let cc = (1. - (2. * l - 1.).abs()) * s;
    let x = cc * (1. - ((h / 60.) % 2. - 1.).abs());
    let m = l - cc / 2.;
    let (r, g, b) = if h < 60. {
        (cc, x, 0.)
    } else if h < 120. {
        (x, cc, 0.)
    } else if h < 180. {
        (0., cc, x)
    } else if h < 240. {
        (0., x, cc)
    } else if h < 300. {
        (x, 0., cc)
    } else {
        (cc, 0., x)
    };
    [r, g, b].map(|v| js_round(clamp01(v + m) * 255.) as u8)
}

fn hsl(h: f64, s: f64, l: f64) -> Hsl {
    Hsl { h: (h + 360.) % 360., s: clamp01(s), l: clamp01(l) }
}
fn hex(c: Hsl) -> Hsla {
    let [r, g, b] = to_rgb(c);
    css_rgba(r, g, b, 1.)
}
fn rgba_of(c: Hsl, a: f32) -> Hsla {
    let [r, g, b] = to_rgb(c);
    css_rgba(r, g, b, a)
}
/// `parse(hex(x))` — JS bazı türevleri hex dizgisinden yeniden ayrıştırır (yuvarlama dahil).
fn reparse(c: Hsl) -> Hsl {
    let [r, g, b] = to_rgb(c);
    parse(&format!("#{r:02x}{g:02x}{b:02x}"))
}

/// theme.ts `paletteFor(theme)` — `--p2-*` yaprak token'ları; semantik token'lar p2-theme.css'teki
/// `var()` bağlarıyla (bg-0 = navy-950 …) türetilir.
pub fn palette_for(theme: &ThemeSpec) -> Palette {
    let light = theme.mode == ThemeMode::Light;
    let c = theme.contrast as f64;
    let surface = parse(&theme.surface);
    let base = parse(&theme.accent);
    let accent = hsl(base.h, clamp(base.s * theme.saturation as f64, 0.04, 0.98), base.l);
    let navy_sat = clamp(surface.s * (0.5 + theme.tint as f64), 0., 0.6);
    let l0 = surface.l;
    let navy = |o: f64| hsl(surface.h, navy_sat, l0 + o * c);
    let slate_sat = clamp(navy_sat * 0.7 + 0.1, 0., 0.5);
    let slate = |l: f64| hsl(surface.h, slate_sat, l);
    let a = |dl: f64, s_mul: f64| hsl(accent.h, accent.s * s_mul, accent.l + dl);

    let mut p = Palette::p2_dark();
    p.theme_id = theme.id.to_string();
    p.dark = !light;
    let (n950, n900, n850, n800, n700, n600, n500, s400, s300, s200, s100, wh, b900, b800, b500, b400, b300, b200, b100, text_accent);
    if !light {
        n950 = navy(0.);
        n900 = navy(0.03);
        n850 = navy(0.05);
        n800 = navy(0.08);
        n700 = navy(0.12);
        n600 = navy(0.17);
        n500 = navy(0.23);
        s400 = slate(clamp(l0 + 0.38 * c, 0.3, 0.6));
        s300 = slate(clamp(l0 + 0.55 * c, 0.5, 0.75));
        s200 = slate(clamp(l0 + 0.70 * c, 0.65, 0.86));
        s100 = slate(clamp(l0 + 0.86 * c, 0.82, 0.95));
        wh = slate(0.97);
        b900 = hsl(accent.h, accent.s * 0.6, l0 + 0.10);
        b800 = hsl(accent.h, accent.s * 0.6, l0 + 0.16);
        b500 = a(0.09, 1.);
        b400 = a(0.18, 0.9);
        b300 = a(0.25, 0.85);
        b200 = a(0.35, 0.8);
        b100 = a(0.44, 0.75);
        text_accent = hsl(accent.h, accent.s.max(0.35), 0.74);
        p.text_2 = rgba_of(reparse(s100), 0.78);
        p.hover_soft = rgba_of(a(0.25, 1.), 0.07);
        p.glass = Gradient { angle: 135., stops: vec![(rgba_of(navy(0.05), 0.96), 0.), (rgba_of(navy(0.), 0.94), 1.)] };
        p.glass_strong = Gradient { angle: 160., stops: vec![(hex(navy(0.09)), 0.), (hex(navy(0.05)), 0.48), (hex(navy(0.03)), 1.)] };
        p.overlay = rgba_of(navy(-0.01), 0.62);
        let edge = a(0.25, 0.85);
        p.border_subtle = rgba_of(edge, 0.10);
        p.border = rgba_of(edge, 0.18);
        p.border_strong = rgba_of(a(0.15, 1.), 0.38);
        p.inset_top = Shadow { inset: true, ..Shadow::new(0., 1., 0., 0., css_rgba(255, 255, 255, 0.08)) };
        p.shadow_sm = vec![Shadow::new(0., 4., 12., 0., css_rgba(0, 0, 0, 0.35))];
        p.shadow_md = vec![Shadow::new(0., 12., 30., 0., css_rgba(0, 0, 0, 0.45)), Shadow::new(0., 0., 0., 1., css_rgba(0, 0, 0, 0.3))];
        p.neutral_fallback = gpui_kit::rgb(0x79818c).into();
        p.hues = HUES.map(|h| Hue { bg: css_hsl(h, 45., 24.), bg2: css_hsl(h, 45., 32.), fg: css_hsl(h, 70., 90.) });
        p.success_text = gpui_kit::rgb(0x8fe8b4).into();
        p.success_bg = gpui_kit::rgb(0x122a1d).into();
        p.danger_text = gpui_kit::rgb(0xff9aa4).into();
        p.danger_bg = gpui_kit::rgb(0x2a1420).into();
        p.warning_text = gpui_kit::rgb(0xffd08a).into();
        p.warning_bg = gpui_kit::rgb(0x2a2210).into();
    } else {
        n950 = navy(0.);
        n900 = navy(0.045);
        n850 = navy(0.075);
        n800 = hsl(surface.h, navy_sat * 0.6, clamp(l0 + 0.105 * c, 0., 0.985));
        n700 = navy(-0.05);
        n600 = navy(-0.09);
        n500 = navy(-0.17);
        s400 = slate(clamp(l0 - 0.32 * c, 0.35, 0.6));
        s300 = slate(clamp(l0 - 0.44 * c, 0.3, 0.5));
        s200 = slate(clamp(l0 - 0.56 * c, 0.2, 0.4));
        s100 = slate(clamp(l0 - 0.70 * c, 0.1, 0.25));
        wh = slate(0.07);
        b900 = hsl(accent.h, accent.s * 0.5, 0.93);
        b800 = hsl(accent.h, accent.s * 0.5, 0.87);
        b500 = a(-0.08, 1.);
        b400 = a(-0.14, 0.9);
        b300 = a(-0.18, 0.85);
        b200 = hsl(accent.h, accent.s * 0.7, 0.78);
        b100 = hsl(accent.h, accent.s * 0.6, 0.88);
        text_accent = hsl(accent.h, accent.s.max(0.35), 0.32);
        p.text_2 = rgba_of(reparse(s100), 0.80);
        p.hover_soft = rgba_of(a(-0.1, 1.), 0.07);
        p.glass = Gradient { angle: 135., stops: vec![(rgba_of(navy(0.09), 0.97), 0.), (rgba_of(navy(0.04), 0.95), 1.)] };
        p.glass_strong = Gradient { angle: 160., stops: vec![(hex(navy(0.11)), 0.), (hex(navy(0.07)), 0.48), (hex(navy(0.03)), 1.)] };
        p.overlay = rgba_of(slate(0.12), 0.45);
        let edge = a(-0.14, 1.);
        p.border_subtle = rgba_of(edge, 0.12);
        p.border = rgba_of(edge, 0.22);
        p.border_strong = rgba_of(edge, 0.42);
        p.inset_top = Shadow { inset: true, ..Shadow::new(0., 1., 0., 0., css_rgba(255, 255, 255, 0.6)) };
        p.shadow_sm = vec![Shadow::new(0., 4., 12., 0., css_rgba(20, 30, 50, 0.12))];
        p.shadow_md = vec![Shadow::new(0., 12., 30., 0., css_rgba(20, 30, 50, 0.16)), Shadow::new(0., 0., 0., 1., css_rgba(20, 30, 50, 0.06))];
        p.neutral_fallback = gpui_kit::rgb(0x8a929c).into();
        p.hues = HUES.map(|h| Hue { bg: css_hsl(h, 50., 88.), bg2: css_hsl(h, 45., 80.), fg: css_hsl(h, 55., 26.) });
        p.success_text = gpui_kit::rgb(0x15723f).into();
        p.success_bg = gpui_kit::rgb(0xdff7e8).into();
        p.danger_text = gpui_kit::rgb(0xb1283a).into();
        p.danger_bg = gpui_kit::rgb(0xffe1e5).into();
        p.warning_text = gpui_kit::rgb(0x8a5a00).into();
        p.warning_bg = gpui_kit::rgb(0xfff0d6).into();
    }
    let b700 = a(-0.14, 1.);
    let b600 = accent;
    // Mod bağımsız vurgu türevleri (JS: hover/bar hex dizgisinden yeniden ayrıştırılır)
    let hover = reparse(b500);
    let bar = reparse(b300);
    p.accent_soft = rgba_of(accent, if light { 0.14 } else { 0.16 });
    p.accent_soft_2 = rgba_of(accent, if light { 0.24 } else { 0.28 });
    p.accent_grad = Gradient { angle: 135., stops: vec![(hex(hover), 0.), (hex(accent), 1.)] };
    p.accent_bar = Gradient { angle: 90., stops: vec![(hex(hover), 0.), (hex(bar), 1.)] };
    p.accent_glow = Shadow::new(0., 0., 34., 0., rgba_of(accent, 0.18));
    p.accent_glow_strong = Shadow::new(0., 6., 18., 0., rgba_of(a(-0.1, 1.), if light { 0.3 } else { 0.45 }));
    p.ring = Shadow::new(0., 0., 0., 3., rgba_of(hover, 0.35));
    p.text_on_accent = if accent.l > 0.62 { hex(hsl(accent.h, 0.5, 0.08)) } else { gpui_kit::rgb(0xffffff).into() };
    p.shadow_lg = {
        let mut v = vec![p.shadow_md[0], p.shadow_md[1], p.accent_glow, p.inset_top];
        v[0] = if light { Shadow::new(0., 18., 44., 0., css_rgba(20, 30, 50, 0.22)) } else { Shadow::new(0., 18., 44., 0., css_rgba(0, 0, 0, 0.55)) };
        v[1] = if light { Shadow::new(0., 0., 0., 1., css_rgba(20, 30, 50, 0.08)) } else { Shadow::new(0., 0., 0., 1., css_rgba(0, 0, 0, 0.35)) };
        v
    };

    // Yapraklar
    (p.navy_950, p.navy_900, p.navy_850, p.navy_800, p.navy_700, p.navy_600, p.navy_500) = (hex(n950), hex(n900), hex(n850), hex(n800), hex(n700), hex(n600), hex(n500));
    (p.slate_400, p.slate_300, p.slate_200, p.slate_100, p.white) = (hex(s400), hex(s300), hex(s200), hex(s100), hex(wh));
    (p.blue_900, p.blue_800, p.blue_700, p.blue_600, p.blue_500) = (hex(b900), hex(b800), hex(b700), hex(b600), hex(b500));
    (p.blue_400, p.blue_300, p.blue_200, p.blue_100) = (hex(b400), hex(b300), hex(b200), hex(b100));
    p.text_accent = hex(text_accent);
    // Semantik bağlar (p2-theme.css var() zinciri)
    p.bg_0 = p.navy_950;
    p.bg_1 = p.navy_900;
    p.bg_2 = p.navy_850;
    p.bg_3 = p.navy_800;
    p.bg_hover = p.navy_700;
    p.bg_selected = p.blue_800;
    p.border_accent = p.blue_500;
    p.line = p.navy_600;
    p.text = p.slate_100;
    p.text_strong = p.white;
    p.text_3 = p.slate_300;
    p.text_4 = p.slate_400;
    p.link = p.blue_400;
    p.accent = p.blue_600;
    p.accent_hover = p.blue_500;
    p.accent_active = p.blue_700;
    p.mention_bg = rgba_of(accent, if light { 0.15 } else { 0.22 });
    p.code_bg = p.navy_900;
    p
}

/// Renk → "#rrggbb" (testler ve tanılama için).
pub fn to_hex(c: Hsla) -> String {
    let r = gpui_kit::Rgba::from(c);
    format!("#{:02x}{:02x}{:02x}", js_round(r.r as f64 * 255.) as u8, js_round(r.g as f64 * 255.) as u8, js_round(r.b as f64 * 255.) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// "#rrggbb" ya da "rgba(r,g,b,a)" → (r,g,b,a×1000)
    fn css(v: &str) -> (u8, u8, u8, u32) {
        let v = v.trim();
        if let Some(h) = v.strip_prefix('#') {
            let ch = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap();
            return (ch(0), ch(2), ch(4), 1000);
        }
        let inner = v.trim_start_matches("rgba(").trim_end_matches(')');
        let n: Vec<f32> = inner.split(',').map(|x| x.trim().parse().unwrap()).collect();
        (n[0] as u8, n[1] as u8, n[2] as u8, (n[3] * 1000.).round() as u32)
    }
    fn of(c: Hsla) -> (u8, u8, u8, u32) {
        let r = gpui_kit::Rgba::from(c);
        let q = |x: f32| js_round(x as f64 * 255.) as u8;
        (q(r.r), q(r.g), q(r.b), (r.a * 1000.).round() as u32)
    }

    /// Electron'un theme.ts paletteFor() çıktısı (geçişte export-palettes.mjs ile üretildi; Electron artık depoda yok).
    #[test]
    fn butun_temalar_electron_ile_ayni() {
        let json: serde_json::Value = serde_json::from_str(include_str!("palette_electron.json")).unwrap();
        let mut checked = 0;
        for t in themes() {
            let want = &json[t.id];
            let p = palette_for(&t);
            let pairs: Vec<(&str, Hsla)> = vec![
                ("--p2-navy-950", p.navy_950), ("--p2-navy-900", p.navy_900), ("--p2-navy-850", p.navy_850), ("--p2-navy-800", p.navy_800),
                ("--p2-navy-700", p.navy_700), ("--p2-navy-600", p.navy_600), ("--p2-navy-500", p.navy_500),
                ("--p2-slate-400", p.slate_400), ("--p2-slate-300", p.slate_300), ("--p2-slate-200", p.slate_200), ("--p2-slate-100", p.slate_100), ("--p2-white", p.white),
                ("--p2-blue-900", p.blue_900), ("--p2-blue-800", p.blue_800), ("--p2-blue-700", p.blue_700), ("--p2-blue-600", p.blue_600),
                ("--p2-blue-500", p.blue_500), ("--p2-blue-400", p.blue_400), ("--p2-blue-300", p.blue_300), ("--p2-blue-200", p.blue_200), ("--p2-blue-100", p.blue_100),
                ("--p2-text-accent", p.text_accent), ("--p2-text-2", p.text_2), ("--p2-hover-soft", p.hover_soft), ("--p2-overlay", p.overlay),
                ("--p2-border-subtle", p.border_subtle), ("--p2-border", p.border), ("--p2-border-strong", p.border_strong),
                ("--p2-neutral-fallback", p.neutral_fallback), ("--p2-accent-soft", p.accent_soft), ("--p2-accent-soft-2", p.accent_soft_2),
                ("--p2-text-on-accent", p.text_on_accent), ("--p2-success-text", p.success_text), ("--p2-success-bg", p.success_bg),
                ("--p2-danger-text", p.danger_text), ("--p2-danger-bg", p.danger_bg), ("--p2-warning-text", p.warning_text), ("--p2-warning-bg", p.warning_bg),
            ];
            for (key, got) in pairs {
                let w = want[key].as_str().unwrap_or_else(|| panic!("{} {key} yok", t.id));
                // GPUI rengi f32 HSLA saklar: RGB gidiş-dönüşünde ±1 yuvarlama payı kalır (çizimde de aynı).
                let (g, e) = (of(got), css(w));
                let close = g.0.abs_diff(e.0) <= 1 && g.1.abs_diff(e.1) <= 1 && g.2.abs_diff(e.2) <= 1 && g.3.abs_diff(e.3) <= 2;
                assert!(close, "{} {key}: Rust {g:?} Electron {w}", t.id);
                checked += 1;
            }
            // Gradyan uçları
            let grad = want["--p2-accent-grad"].as_str().unwrap();
            assert!(grad.contains(&to_hex(p.accent_grad.stops[0].0)) && grad.contains(&to_hex(p.accent_grad.stops[1].0)), "{} accent-grad {grad}", t.id);
            let bar = want["--p2-accent-bar"].as_str().unwrap();
            assert!(bar.contains(&to_hex(p.accent_bar.stops[1].0)), "{} accent-bar {bar}", t.id);
        }
        assert!(checked > 600);
    }

    #[test]
    fn p2_mavi_sabitleri() {
        let p = Palette::for_theme("p2-mavi");
        assert_eq!(to_hex(p.bg_0), "#070b14");
        assert_eq!(to_hex(p.accent), "#2c7bff");
        assert_eq!(to_hex(Palette::for_theme("dark").text), "#d6e4ff");
        assert_eq!(Palette::for_theme("light").theme_id, "kagit");
    }
}
