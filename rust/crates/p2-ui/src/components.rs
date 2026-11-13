//! P2 Mavi ortak bileşenleri (`ui.css`, `setup.css`, `styles.css` karşılıkları).
//! Hepsi saf fonksiyon: çağıran tıklama dinleyicisini kendisi ekler (`.on_click(...)`).

use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::{AnyElement, Div, ElementId, FontWeight, Hsla, SharedString, Stateful, div, linear_color_stop, linear_gradient, px, rgb};

use crate::motion::{Entrance, P2Motion as _};
use crate::tokens::{Palette, font_size, radius};

fn c(hex: u32) -> Hsla {
    rgb(hex).into()
}

/// `--p2-accent-grad`: 135°, #4f9dff → #2c7bff.
pub fn accent_gradient() -> gpui_kit::Background {
    linear_gradient(135., linear_color_stop(c(0x4f9dff), 0.), linear_color_stop(c(0x2c7bff), 1.))
}

/// `--p2-glass-strong` (etkin temanın; GPUI iki durak çizer — TOKENS.md).
pub fn glass_strong(p: &Palette) -> gpui_kit::Background {
    p.glass_strong.background()
}

/// `--p2-glass` (menü, toast, güncelleme şeridi, overlay pencereleri).
pub fn glass(p: &Palette) -> gpui_kit::Background {
    p.glass.background()
}

/// Etkin temanın `--p2-accent-grad`ı (P2 Mavi dışındaki temalarda farklıdır).
pub fn accent_gradient_for(p: &Palette) -> gpui_kit::Background {
    p.accent_grad.background()
}

/// Sayfa zemini: `radial-gradient(accent-soft …), bg-0` yaklaşığı.
pub fn page_background(p: &Palette) -> gpui_kit::Background {
    linear_gradient(200., linear_color_stop(p.bg_1, 0.), linear_color_stop(p.bg_0, 0.7))
}

/// P2 logosu (`.p2-logo` yedeği): yuvarlatılmış mavi kare içinde "P2".
pub fn logo(size: f32) -> impl IntoElement {
    div()
        .size(px(size))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(size * 0.28))
        .bg(accent_gradient())
        .text_color(c(0xffffff))
        .text_size(px(size * 0.36))
        .font_weight(FontWeight::EXTRA_BOLD)
        .child("P2")
}

static P2_ICON_PNG: &[u8] = include_bytes!("../assets/p2-icon.png");

/// `src/assets/p2-icon.png` (512×512) — Logo.tsx'in gerçek simgesi.
pub fn p2_icon() -> std::sync::Arc<gpui_kit::Image> {
    static ICON: std::sync::OnceLock<std::sync::Arc<gpui_kit::Image>> = std::sync::OnceLock::new();
    ICON.get_or_init(|| std::sync::Arc::new(gpui_kit::Image::from_bytes(gpui_kit::ImageFormat::Png, P2_ICON_PNG.to_vec()))).clone()
}

/// Logo.tsx `.p2-logo` (ui.css 331): köşe %25, `accent-glow-strong` + `0 0 0 1px rgba(120,180,255,.25)`;
/// `xs` (20px) gölgesiz. Boylar: xs 20, sm 28, md 44, lg 64, xl 96.
pub fn logo_image(size: f32, p: &Palette) -> Div {
    let glow = size > 20.;
    div()
        .size(px(size))
        .flex_shrink_0()
        .rounded(px(size * 0.25))
        .when(glow, |d| d.shadow(crate::tokens::shadows(&[p.accent_glow_strong, crate::tokens::Shadow::new(0., 0., 0., 1., crate::tokens::css_rgba(120, 180, 255, 0.25))])))
        .child(gpui_kit::img(p2_icon()).size(px(size)).rounded(px(size * 0.25)).object_fit(gpui_kit::ObjectFit::Cover))
}

/// CSS `radial-gradient(circle at X% Y%, c0 0, c1 s1, c2 s2)` (farthest-corner) — GPUI'de radyal gradyan
/// yok; SVG olarak resvg ile çizilir (`icons::svg_image`, belge başına önbellek). `base` altta kalan düz
/// zemin (CSS'te gradyanın altındaki öğe rengi). Boyut değişince yeni görüntü üretilir.
pub fn radial_background(w: f32, h: f32, at: (f32, f32), stops: &[(Hsla, f32)], base: Hsla) -> gpui_kit::Img {
    let (cx, cy) = (w * at.0, h * at.1);
    let r = [(0., 0.), (w, 0.), (0., h), (w, h)].iter().map(|(x, y): &(f32, f32)| ((x - cx).powi(2) + (y - cy).powi(2)).sqrt()).fold(0., f32::max);
    let color = |c: Hsla| crate::icons::svg_color(c);
    let (bc, ba) = color(base);
    let mut stops_svg = String::new();
    // Chromium premultiplied uzayda karıştırır ve sonucu opak zemine bindirir: durakları önce
    // zeminle birleştirip opak vermek aynı sonucu verir (SVG/resvg düz alfa karıştırır).
    for (c, at) in stops {
        let (rgb, a) = color(over(*c, base));
        stops_svg.push_str(&format!(r#"<stop offset="{at}" stop-color="{rgb}" stop-opacity="{a}"/>"#));
    }
    let markup = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><defs><radialGradient id="g" gradientUnits="userSpaceOnUse" cx="{cx}" cy="{cy}" r="{r}">{stops_svg}</radialGradient></defs><rect width="{w}" height="{h}" fill="{bc}" fill-opacity="{ba}"/><rect width="{w}" height="{h}" fill="url(#g)"/></svg>"#
    );
    crate::icons::svg_image(markup).w(px(w)).h(px(h))
}

/// `top` rengini (alfalı) opak `base` üstüne bindirir.
pub fn over(top: Hsla, base: Hsla) -> Hsla {
    let (t, b): (gpui_kit::Rgba, gpui_kit::Rgba) = (top.into(), base.into());
    let a = t.a;
    gpui_kit::Rgba { r: t.r * a + b.r * (1. - a), g: t.g * a + b.g * (1. - a), b: t.b * a + b.b * (1. - a), a: 1. }.into()
}

/// `.eyebrow` / `.p2-kicker`: küçük, aralıklı, vurgu renkli başlık.
pub fn eyebrow(text: impl Into<SharedString>, p: &Palette) -> impl IntoElement {
    div().text_size(font_size::XS).font_weight(FontWeight::BOLD).text_color(p.text_accent).child(text.into())
}

/// `.install-button` / `.auth-card form > button`: birincil eylem, sağda ok.
pub fn primary_button(id: impl Into<ElementId>, label: impl Into<SharedString>, busy: bool, p: &Palette) -> Stateful<Div> {
    let label = label.into();
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_between()
        .px(px(18.))
        .py(px(13.))
        .rounded(radius::MD)
        .bg(accent_gradient())
        .text_color(p.text_on_accent)
        .font_weight(FontWeight::EXTRA_BOLD)
        .text_size(font_size::MD)
        .shadow_md()
        .when(busy, |s| s.opacity(0.65).cursor_not_allowed())
        .when(!busy, |s| s.cursor_pointer().hover(|s| s.opacity(0.92)).active(|s| s.opacity(0.85)))
        .child(label)
        .child("→")
}

/// `.mode-back`: ikincil, kenarlıklı düğme.
pub fn secondary_button(id: impl Into<ElementId>, label: impl Into<SharedString>, p: &Palette) -> Stateful<Div> {
    let border = p.border;
    let hover_bg = p.bg_hover;
    div()
        .id(id)
        .px(px(16.))
        .py(px(12.))
        .rounded(radius::MD)
        .border_1()
        .border_color(border)
        .bg(p.bg_3)
        .text_color(p.text_2)
        .text_size(font_size::MD)
        .font_weight(FontWeight::SEMIBOLD)
        .cursor_pointer()
        .hover(move |s| s.bg(hover_bg))
        .child(label.into())
}

/// Uygulama içi küçük düğme (ayarlar, paneller, pencereler): `primary` vurgu renginde,
/// değilse kenarlıklı. Giriş ekranlarındaki büyük `primary_button`'ın sade karşılığı.
pub fn compact_button(id: impl Into<ElementId>, label: impl Into<SharedString>, primary: bool, busy: bool, p: &Palette) -> Stateful<Div> {
    let (hover_bg, accent_hover) = (p.bg_hover, p.accent_hover);
    div()
        .id(id)
        .flex()
        .items_center()
        .justify_center()
        .px(px(14.))
        .py(px(7.))
        .rounded(radius::SM)
        .text_size(font_size::MD)
        .font_weight(FontWeight::SEMIBOLD)
        .when(primary, |s| s.bg(p.accent).text_color(p.text_on_accent))
        .when(!primary, |s| s.border_1().border_color(p.border).bg(p.bg_3).text_color(p.text_2))
        .when(busy, |s| s.opacity(0.65).cursor_not_allowed())
        .when(!busy, move |s| s.cursor_pointer().hover(move |s| if primary { s.bg(accent_hover) } else { s.bg(hover_bg) }))
        .child(label.into())
}

/// Metin bağlantısı gibi görünen düğme (`.switch button`).
pub fn link_button(id: impl Into<ElementId>, label: impl Into<SharedString>, p: &Palette) -> Stateful<Div> {
    div()
        .id(id)
        .text_color(p.text_accent)
        .font_weight(FontWeight::BOLD)
        .cursor_pointer()
        .hover(|s| s.opacity(0.8))
        .child(label.into())
}

/// `.auth-card label`: etiket + alan (+ isteğe bağlı not).
pub fn field(label: impl Into<SharedString>, input: impl IntoElement, note: Option<SharedString>, p: &Palette) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(7.))
        .child(div().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(p.text_2).child(label.into()))
        .child(input)
        .when_some(note, |s, n| s.child(div().text_size(font_size::XS).text_color(p.text_4).child(n)))
}

/// `.error`: kırmızı kutu, `p2-rise` ile girer. `epoch` her yeni hatada değişmeli.
pub fn error_box(message: impl Into<SharedString>, epoch: usize, p: &Palette) -> AnyElement {
    div()
        .px(px(14.))
        .py(px(11.))
        .rounded(radius::MD)
        .border_1()
        .border_color(p.danger.opacity(0.42))
        .bg(p.danger_soft)
        .text_color(if p.dark { c(0xff9aa4) } else { p.danger })
        .text_size(font_size::SM)
        .child(message.into())
        .p2_enter(("error", epoch), Entrance::Rise, Duration::ZERO)
        .into_any_element()
}

/// `.setup-shell` / `.auth-card`: cam kart.
pub fn glass_card(p: &Palette) -> Div {
    div()
        .relative()
        .overflow_hidden()
        .rounded(radius::XL)
        .border_1()
        .border_color(p.border_strong)
        .bg(glass_strong(p))
        .shadow(p.shadow_lg())
}

/// Parola gücü (App.tsx `passwordStrength`): sunucunun en az kuralı (8+, büyük, küçük, rakam)
/// sağlanmadan "kabul edilebilir" olmaz.
pub struct Strength {
    pub score: u8,
    pub label: &'static str,
    pub ok: bool,
}

pub fn password_strength(value: &str) -> Strength {
    if value.is_empty() {
        return Strength { score: 0, label: "", ok: false };
    }
    let rules = [
        value.chars().count() >= 8,
        value.chars().any(char::is_uppercase),
        value.chars().any(char::is_lowercase),
        value.chars().any(|c| c.is_ascii_digit()),
    ];
    let ok = rules.iter().all(|r| *r);
    let mut score = rules.iter().filter(|r| **r).count() as u8;
    if value.chars().any(|c| !c.is_alphanumeric()) {
        score += 1;
    }
    let len = value.chars().count();
    if len >= 12 {
        score += 1;
    }
    if len >= 16 {
        score += 1;
    }
    let label = if !ok {
        "Zayıf — 8+ karakter, büyük/küçük harf ve rakam gerekli"
    } else if score <= 4 {
        "Orta"
    } else if score == 5 {
        "Güçlü"
    } else {
        "Çok güçlü"
    };
    Strength { score: score.min(6), label, ok }
}

/// `.password-meter`: altı dilimli çubuk + etiket.
pub fn password_meter(value: &str, confirm: Option<&str>, p: &Palette) -> impl IntoElement {
    let s = password_strength(value);
    let tone = if !s.ok { p.danger } else if s.score <= 4 { p.warning } else { p.success };
    div()
        .flex()
        .flex_col()
        .gap_1()
        .child(div().h(px(4.)).w_full().rounded_full().bg(p.bg_3).child(div().h_full().w(gpui_kit::relative(s.score as f32 / 6.)).rounded_full().bg(tone)))
        .child(
            div()
                .flex()
                .gap_1()
                .text_size(font_size::XS)
                .text_color(p.text_3)
                .child(s.label)
                .when_some(confirm.filter(|c| !c.is_empty()), |d, c| {
                    d.child(" · ").child(if c == value {
                        div().text_color(p.success).child("Parolalar eşleşiyor ✓")
                    } else {
                        div().text_color(p.danger).child("Parolalar eşleşmiyor")
                    })
                }),
        )
}

/// initials.ts `avatarHue`: kimlikten türeyen sabit ton (0..10). Aynı kişi her yerde aynı renkte.
pub fn avatar_hue(key: &str) -> usize {
    let mut hash: u32 = 0;
    for unit in key.encode_utf16() {
        hash = hash.wrapping_mul(31).wrapping_add(unit as u32);
    }
    (hash % 10) as usize
}

fn tr_upper(s: &str) -> String {
    s.chars()
        .flat_map(|c| match c {
            'i' => vec!['İ'],
            'ı' => vec!['I'],
            c => c.to_uppercase().collect(),
        })
        .collect()
}

/// initials.ts `userInitials`: "Hasan Özoğlu" → HÖ, "Hasan Özoğlu - 2" → HÖ2, "captainTvZ" → CA.
pub fn user_initials(name: &str) -> String {
    let parts: Vec<&str> = name.trim().split(|c: char| c.is_whitespace() || "._-".contains(c)).filter(|p| !p.is_empty()).collect();
    let words: Vec<&str> = parts.iter().copied().filter(|p| p.chars().any(char::is_alphabetic)).collect();
    let digits: String = parts.iter().filter(|p| p.chars().all(|c| c.is_ascii_digit())).copied().collect::<String>().chars().take(2).collect();
    let letters = if words.len() == 1 && digits.is_empty() {
        words[0].chars().take(2).collect::<String>()
    } else {
        words.iter().take(2).filter_map(|w| w.chars().next()).collect()
    };
    let out = tr_upper(&letters) + &digits;
    if !out.is_empty() {
        return out;
    }
    let fallback = tr_upper(&name.trim().chars().take(2).collect::<String>());
    if fallback.is_empty() { "P2".into() } else { fallback }
}

/// Kimlik tonlu yuvarlak avatar (baş harfler).
/// Uygulamanın verdiği profil fotoğrafı kaynağı (kullanıcı kimliği → hazır görsel). Yoksa
/// ya da görsel henüz inmediyse baş harfler çizilir.
pub type AvatarSource = fn(&str) -> Option<std::sync::Arc<gpui_kit::RenderImage>>;
static AVATAR_SOURCE: std::sync::OnceLock<AvatarSource> = std::sync::OnceLock::new();

pub fn set_avatar_source(source: AvatarSource) {
    let _ = AVATAR_SOURCE.set(source);
}

pub fn avatar(key: &str, name: &str, size: f32, p: &Palette) -> gpui_kit::Div {
    let (bg, fg) = p.hue(avatar_hue(key));
    if let Some(image) = AVATAR_SOURCE.get().and_then(|f| f(key)) {
        return div()
            .size(px(size))
            .flex_shrink_0()
            .rounded_full()
            .overflow_hidden()
            .bg(bg)
            .child(gpui_kit::img(gpui_kit::ImageSource::Render(image)).size_full().rounded_full().object_fit(gpui_kit::ObjectFit::Cover));
    }
    div()
        .size(px(size))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(bg)
        .text_color(fg)
        .text_size(px(size * 0.36))
        .font_weight(FontWeight::BOLD)
        .child(user_initials(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_matches_initials_ts() {
        assert_eq!(user_initials("Hasan Özoğlu"), "HÖ");
        assert_eq!(user_initials("Hasan Özoğlu - 2"), "HÖ2");
        assert_eq!(user_initials("captainTvZ"), "CA");
        assert_eq!(user_initials("ilker"), "İL");
        // JS: hash=(hash*31+code)>>>0 ; "ab" → (0*31+97)*31+98 = 3105 → %10 = 5
        assert_eq!(avatar_hue("ab"), 5);
    }

    #[test]
    fn strength_matches_app_tsx() {
        assert!(!password_strength("kisa").ok);
        assert!(!password_strength("kucukharf1").ok);
        let s = password_strength("Parola123");
        assert!(s.ok);
        assert_eq!(s.label, "Orta");
        assert_eq!(password_strength("Parola123!Uzun-Parola").label, "Çok güçlü");
    }
}
