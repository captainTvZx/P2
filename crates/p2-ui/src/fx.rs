//! CSS efektleri — yamalı gpui-pre (`third_party/gpui-pre`) ve DirectX çizicisi
//! (`third_party/gpui-pre-windows`) üzerinden:
//!
//! | CSS | Rust |
//! |---|---|
//! | `backdrop-filter: blur(Npx)` | `.backdrop_blur(N)` (`--p2-blur` → [`BLUR`], `--p2-blur-sm` → [`BLUR_SM`]) |
//! | `backdrop-filter: blur(18px) saturate(1.3)` | `.backdrop(18., 1.3)` |
//! | `transform: scale(s)` / `rotate(d)` / `translate(x,y)` | `.scale(s)`, `.rotate(d)`, `.translate(x, y)`, `.transform_origin(ox, oy)`, `.p2_transform(..)` |
//! | `filter: grayscale(g)` / `brightness(k)` | `.grayscale(g)`, `.brightness(k)` |
//! | `transition: … 140ms ease-out` | varsayılan: hover/active/grup-hover stili olan ve `.id()`'li her öğe ([`install_transitions`]); öğeye özel `.transition(süre, eğri)` / `.no_transition()` |
//! | `--app-font-scale` (Ayarlar > Görünüm > yazı boyutu) | [`set_font_scale_percent`] |
//!
//! Dönüşüm yalnız çizime uygulanır (CSS'teki gibi düzeni etkilemez). Döndürme yalnız SVG/glif
//! sprite'larında görünür; dörtgenler eksene hizalı ölçeklenir.

use std::time::Duration;

use gpui_kit::{InteractiveElement, P2Backdrop, P2Filter, P2Transform, Styled, point, px};

use crate::motion::{CubicBezier, EASE_OUT, M_FAST};

/// `--p2-blur` (12px)
pub const BLUR: f32 = 12.;
/// `--p2-blur-sm` (6px)
pub const BLUR_SM: f32 = 6.;

fn bezier(c: CubicBezier) -> [f32; 4] {
    [c.x1, c.y1, c.x2, c.y2]
}

/// Uygulama açılışında: Electron ui.css'teki genel etkileşim geçişi
/// (`background-color, border-color, color, box-shadow, opacity var(--m-fast) var(--ease-out)`).
/// `reduced` = "hareketi azalt" (CSS'te süreler 1ms olur).
pub fn install_transitions(reduced: bool) {
    let spec = if reduced { None } else { Some((M_FAST, bezier(EASE_OUT))) };
    gpui_kit::set_p2_default_transition(spec);
}

/// Ana istemcinin açılışında (`p2-client/src/main.rs`): genel geçiş + kaydırma çubuğu.
/// `P2_FX_OFF=1` tanı için ikisini de kapatır. Oyun içi overlay bunları KULLANMAZ (slice7: panel tıklamaları
/// overlay host'unda bozuluyordu — ajan J ile incelenecek).
pub fn install_client_defaults() {
    let off = std::env::var_os("P2_FX_OFF").is_some();
    install_transitions(off);
    install_scrollbars(!off);
}

/// Bütün taşan `overflow_*_scroll` alanlarında Chromium kaydırma çubuğu (ui.css `::-webkit-scrollbar`).
pub fn install_scrollbars(on: bool) {
    gpui_kit::set_p2_scrollbars(on);
}

/// Kaydırma çubuğu kalınlığı (ui.css `::-webkit-scrollbar { width/height: 10px }`).
pub const SCROLLBAR: f32 = 10.;
/// Yatay rayın alt dolgusu: çubuk kartların altında, üstlerine binmeden durur.
pub const RAIL_GUTTER: f32 = SCROLLBAR + 4.;

/// Electron `appearance.fontScale` (70–125, varsayılan 85). Ardından `cx.refresh_windows()`.
pub fn set_font_scale_percent(percent: f32) {
    let p = percent.clamp(70., 125.);
    gpui_kit::set_p2_font_scale(p / (crate::tokens::FONT_SCALE * 100.));
}

/// Geçerli yazı ölçeği (Electron yüzdesi).
pub fn font_scale_percent() -> f32 {
    gpui_kit::p2_font_scale() * crate::tokens::FONT_SCALE * 100.
}

pub trait P2Fx: Styled + Sized {
    /// `backdrop-filter: blur(Npx)`
    fn backdrop_blur(self, blur_px: f32) -> Self {
        self.backdrop(blur_px, 1.)
    }
    /// `backdrop-filter: blur(Npx) saturate(S)`
    fn backdrop(mut self, blur_px: f32, saturate: f32) -> Self {
        self.style().p2_backdrop = Some(P2Backdrop { blur: px(blur_px), saturate });
        self
    }
    /// Tam CSS `transform`.
    fn p2_transform(mut self, t: P2Transform) -> Self {
        self.style().p2_transform = Some(t);
        self
    }
    fn p2_transform_mut(&mut self) -> &mut P2Transform {
        self.style().p2_transform.get_or_insert_with(P2Transform::default)
    }
    /// `transform: scale(s)`
    fn scale(mut self, s: f32) -> Self {
        let t = self.p2_transform_mut();
        t.scale_x = s;
        t.scale_y = s;
        self
    }
    /// `transform: scaleX(s)`
    fn scale_x(mut self, s: f32) -> Self {
        self.p2_transform_mut().scale_x = s;
        self
    }
    /// `transform: rotate(deg)` (SVG/glif sprite'larında)
    fn rotate(mut self, deg: f32) -> Self {
        self.p2_transform_mut().rotate_deg = deg;
        self
    }
    /// `transform: translate(x, y)` (px)
    fn translate(mut self, x: f32, y: f32) -> Self {
        self.p2_transform_mut().translate = point(px(x), px(y));
        self
    }
    /// `transform-origin` (oransal: 0 = sol/üst, 1 = sağ/alt)
    fn transform_origin(mut self, ox: f32, oy: f32) -> Self {
        self.p2_transform_mut().origin = point(ox, oy);
        self
    }
    /// `filter: grayscale(g)`
    fn grayscale(mut self, g: f32) -> Self {
        self.style().p2_filter.get_or_insert_with(P2Filter::default).grayscale = g;
        self
    }
    /// `filter: brightness(k)`
    fn brightness(mut self, k: f32) -> Self {
        self.style().p2_filter.get_or_insert_with(P2Filter::default).brightness = k;
        self
    }
}

impl<T: Styled + Sized> P2Fx for T {}

pub trait P2Transition: InteractiveElement + Sized {
    /// Bu öğenin hover/active geçişi (`transition: … <süre> cubic-bezier(..)`).
    fn transition(mut self, duration: Duration, curve: CubicBezier) -> Self {
        self.interactivity().p2_transition = Some(Some((duration, bezier(curve))));
        self
    }
    /// Otomatik Chromium kaydırma çubuğunu kapat (kendi çubuğunu çizen kaydırma alanları).
    fn no_auto_scrollbar(mut self) -> Self {
        self.interactivity().p2_scrollbar = Some(false);
        self
    }
    /// Genel ayar kapalı olsa da (oyun içi overlay) bu öğede Chromium kaydırma çubuğunu çiz.
    fn auto_scrollbar(mut self) -> Self {
        self.interactivity().p2_scrollbar = Some(true);
        self
    }
    /// Geçiş yok (CSS'te `transition` tanımsız öğeler; durum anında değişir).
    fn no_transition(mut self) -> Self {
        self.interactivity().p2_transition = Some(None);
        self
    }
}

impl<T: InteractiveElement + Sized> P2Transition for T {}

// ------------------------------------------------------------------------------------------ durum geçişi
// CSS `transition` bir sınıf/durum değişince de oynar (ör. `.chevron.collapsed` döndürmesi). GPUI'de durum
// değişikliği temel stili anında değiştirir; [`tween`] anahtar başına son hedefi hatırlar ve hedef değişince
// eski değerden yeniye `süre` + eğriyle ara değer verir (ilk görüşte animasyon yok — CSS gibi).

thread_local! {
    static EPOCH_BASE: std::time::Instant = std::time::Instant::now();
    static TWEENS: std::cell::RefCell<std::collections::HashMap<String, (f32, f32, std::time::Instant, Duration, CubicBezier)>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// `key` için `target`a geçen değer. Animasyon sürerken bir sonraki kareyi ister.
pub fn tween(window: &gpui_kit::Window, key: &str, target: f32, duration: Duration, curve: CubicBezier) -> f32 {
    let (value, running) = tween_raw(key, target, duration, curve);
    if running.is_some() {
        window.request_animation_frame();
    }
    value
}

/// [`tween`] penceresiz: (değer, sürüyor mu). Sürüyorsa çağıran kare istemeli (ör. öğeyi
/// `with_animation` ile sarmak, bkz. [`keep_animating`]).
/// Sürerken ikinci değer geçişin kimliğidir (başlangıç anı).
pub fn tween_raw(key: &str, target: f32, duration: Duration, curve: CubicBezier) -> (f32, Option<u64>) {
    let now = std::time::Instant::now();
    let (value, running) = TWEENS.with(|t| {
        let mut map = t.borrow_mut();
        let entry = map.entry(key.to_string()).or_insert((target, target, now - duration, duration, curve));
        let current = {
            let (from, to, start, dur, c) = *entry;
            let k = (now.saturating_duration_since(start).as_secs_f32() / dur.as_secs_f32().max(1e-3)).clamp(0., 1.);
            from + (to - from) * c.ease(k)
        };
        if entry.1 != target {
            *entry = (current, target, now, duration, curve);
        }
        let (from, to, start, dur, c) = *entry;
        let k = (now.saturating_duration_since(start).as_secs_f32() / dur.as_secs_f32().max(1e-3)).clamp(0., 1.);
        let epoch = start.saturating_duration_since(EPOCH_BASE.with(|b| *b)).as_nanos() as u64;
        (from + (to - from) * c.ease(k), (k < 1.).then_some(epoch))
    });
    (value, running)
}

/// Geçiş sürerken kare üretimini sürdürür (öğeyi kısa bir boş animasyonla sarar).
pub fn keep_animating<E: gpui_kit::IntoElement + 'static>(el: E, key: &str, running: Option<u64>) -> gpui_kit::AnyElement {
    use gpui_kit::{AnimationExt as _, IntoElement as _};
    if let Some(epoch) = running {
        el.with_animation(gpui_kit::SharedString::from(format!("p2-tween-{key}-{epoch}")), gpui_kit::Animation::new(Duration::from_millis(400)), |el, _| el).into_any_element()
    } else {
        el.into_any_element()
    }
}

// ------------------------------------------------------------------------------------------ text-shadow
/// CSS `text-shadow: <dx> <dy> <blur> <color>` yaklaşığı: aynı metin gölge renginde, Gauss ağırlıklı
/// kaydırılmış kopyalarla (σ = blur / 2) arkaya çizilir. `make(None)` metnin kendisini, `make(Some(c))`
/// aynı düzende `c` renkli kopyasını döndürmeli.
pub fn text_shadow(make: impl Fn(Option<gpui_kit::Hsla>) -> gpui_kit::AnyElement, dx: f32, dy: f32, blur: f32, color: gpui_kit::Hsla) -> gpui_kit::Div {
    use gpui_kit::{ParentElement as _, Styled as _};
    let sigma = (blur / 2.).max(0.01);
    let mut taps: Vec<(f32, f32, f32)> = Vec::new();
    if blur <= 0.5 {
        taps.push((0., 0., 1.));
    } else {
        // Merkez + 3 halka × 8 yön (r = .7σ, 1.4σ, 2.1σ).
        taps.push((0., 0., 1.));
        for ring in 1..=3 {
            let r = sigma * 0.7 * ring as f32;
            let w = (-(r * r) / (2. * sigma * sigma)).exp();
            for k in 0..8 {
                let a = std::f32::consts::TAU * (k as f32 + 0.5 * (ring % 2) as f32) / 8.;
                taps.push((r * a.cos(), r * a.sin(), w));
            }
        }
    }
    let total: f32 = taps.iter().map(|t| t.2).sum();
    let mut el = gpui_kit::div().relative();
    for (x, y, w) in taps {
        // "over" birleşiminde n kopya ≈ 1-(1-a)^n; ağırlık payı küçük olduğundan doğrusal pay yeterli
        // (kenar yoğunluğu Chromium'daki bulanık gölgeye yakın kalsın diye ×1,6).
        let a = (color.a * w / total * 1.6).min(1.);
        el = el.child(gpui_kit::div().absolute().left(px(dx + x)).top(px(dy + y)).size_full().child(make(Some(gpui_kit::Hsla { a, ..color }))));
    }
    el.child(gpui_kit::div().relative().child(make(None)))
}
