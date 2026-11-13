//! Hareket dili. Kaynak: `src/motion.css`.
//!
//! Süre ve easing yalnız buradaki token'lardan alınır. Kurallar:
//! - Giriş animasyonları bir kez oynar, sonra element durağandır (kare üretimi durur).
//! - Sonsuz animasyonlar ([`pulse`]) düşük kare hızıyla sınırlanır ve yalnız pencere
//!   etkin/görünürken eklenir; çağıran taraf bunu `Window::is_window_active` ile denetler.
//! - Sistemde "Animasyon efektleri" kapalıysa (gpui-base bunu okur) çağıran taraf
//!   [`Entrance::None`] kullanır.

use std::time::Duration;

use gpui_kit::{Animation, AnimationElement, AnimationExt, ElementId, IntoElement, Styled, px};

// ---- Süreler (--m-*) ----
pub const M_FAST: Duration = Duration::from_millis(140);
pub const M_BASE: Duration = Duration::from_millis(220);
pub const M_SLOW: Duration = Duration::from_millis(360);
pub const M_ELASTIC: Duration = Duration::from_millis(620);
pub const M_STAGGER: Duration = Duration::from_millis(28);
/// `.anim-list` gecikme tavanı.
pub const M_STAGGER_MAX: Duration = Duration::from_millis(360);

/// CSS `cubic-bezier(x1, y1, x2, y2)`. y değerleri 1'i aşabilir (elastik taşma).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicBezier {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

impl CubicBezier {
    pub const fn new(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        Self { x1, y1, x2, y2 }
    }

    fn sample_curve(a: f32, b: f32, t: f32) -> f32 {
        // B(t) = 3(1-t)^2 t a + 3(1-t) t^2 b + t^3
        let u = 1. - t;
        3. * u * u * t * a + 3. * u * t * t * b + t * t * t
    }

    fn sample_derivative(a: f32, b: f32, t: f32) -> f32 {
        let u = 1. - t;
        3. * u * u * a + 6. * u * t * (b - a) + 3. * t * t * (1. - b)
    }

    /// x ilerlemesi (zaman) için y değerini döndürür. Tarayıcılardaki gibi önce Newton,
    /// yakınsamazsa ikiye bölme.
    pub fn ease(&self, x: f32) -> f32 {
        let x = x.clamp(0., 1.);
        if x == 0. || x == 1. {
            return x;
        }
        let mut t = x;
        for _ in 0..8 {
            let err = Self::sample_curve(self.x1, self.x2, t) - x;
            if err.abs() < 1e-5 {
                return Self::sample_curve(self.y1, self.y2, t);
            }
            let d = Self::sample_derivative(self.x1, self.x2, t);
            if d.abs() < 1e-6 {
                break;
            }
            t -= err / d;
        }
        let (mut lo, mut hi) = (0f32, 1f32);
        t = x;
        for _ in 0..30 {
            let v = Self::sample_curve(self.x1, self.x2, t);
            if (v - x).abs() < 1e-5 {
                break;
            }
            if v < x {
                lo = t
            } else {
                hi = t
            }
            t = (lo + hi) / 2.;
        }
        Self::sample_curve(self.y1, self.y2, t)
    }
}

// ---- Easing (--ease-*) ----
pub const EASE_OUT: CubicBezier = CubicBezier::new(0.2, 0.8, 0.2, 1.);
pub const EASE_IN: CubicBezier = CubicBezier::new(0.5, 0., 0.8, 0.4);
pub const EASE_ELASTIC: CubicBezier = CubicBezier::new(0.2, 1.5, 0.35, 1.);
pub const EASE_POP: CubicBezier = CubicBezier::new(0.2, 1.4, 0.4, 1.);

/// Giriş animasyonu türleri (`@keyframes p2-*`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Entrance {
    /// Hareketi azalt açıkken: anında görünür.
    None,
    /// `p2-fade`
    Fade,
    /// `p2-rise`: 8px aşağıdan.
    Rise,
    /// `p2-page`: 7px aşağıdan (CSS'teki 2px bulanıklık ve %0.3 ölçek GPUI'de yok).
    Page,
    /// `p2-slide-left`: 10px soldan.
    SlideLeft,
    /// `p2-slide-right`: 10px sağdan.
    SlideRight,
    /// `p2-slide-right-elastic` (toast): kendi genişliği + 48px sağdan, elastik.
    SlideRightElastic { width: f32 },
    /// `ow-slide-left-elastic` (sol köşedeki overlay bildirimi): kendi genişliği + 48px soldan.
    SlideLeftElastic { width: f32 },
    /// `p2-pop-elastic` (modal): scale(.86) translateY(14px) → yok, elastik eğri (yamalı GPUI ölçeği).
    PopElastic,
    /// `p2-scale-in` (menü): scale(.92) → 1, pop eğrisi. Orijin için [`Entrance::ScaleInFrom`].
    ScaleIn,
    /// `p2-scale-in` + `transform-origin` (oransal; ör. sol üst = (0, 0)).
    ScaleInFrom { ox: f32, oy: f32 },
    /// `p2-logo-pop` (.55s ease-pop): scale(.4) rotate(-14deg) → 1.14 / 3deg (%55) → yok.
    LogoPop,
    /// `p2-success-pop` (260ms ease-out): scale(.72) → 1.08 (%65) → 1.
    SuccessPop,
}

impl Entrance {
    fn timing(self) -> (Duration, CubicBezier) {
        match self {
            Entrance::None => (Duration::from_millis(1), EASE_OUT),
            Entrance::Fade | Entrance::Rise | Entrance::Page => (M_BASE, EASE_OUT),
            Entrance::SlideLeft | Entrance::SlideRight => (M_BASE, EASE_OUT),
            Entrance::SlideRightElastic { .. } | Entrance::SlideLeftElastic { .. } | Entrance::PopElastic => (M_ELASTIC, EASE_ELASTIC),
            Entrance::ScaleIn | Entrance::ScaleInFrom { .. } => (M_BASE, EASE_POP),
            Entrance::LogoPop => (Duration::from_millis(550), EASE_POP),
            Entrance::SuccessPop => (Duration::from_millis(260), EASE_OUT),
        }
    }

    /// Eğrilmiş ilerleme `p` için (opaklık, x kayması, y kayması).
    /// Opaklık taşmayan doğrusal ilerlemeden (`raw`), konum taşmalı eğriden alınır;
    /// CSS'te de opaklık 1'i aşamaz.
    pub fn frame(self, raw: f32, p: f32) -> (f32, f32, f32) {
        let rest = 1. - p;
        let opacity = match self {
            Entrance::None => 1.,
            _ => (raw * 1.6).min(1.),
        };
        match self {
            Entrance::None | Entrance::Fade => (opacity, 0., 0.),
            Entrance::Rise => (opacity, 0., 8. * rest),
            Entrance::Page => (opacity, 0., 7. * rest),
            Entrance::SlideLeft => (opacity, -10. * rest, 0.),
            Entrance::SlideRight => (opacity, 10. * rest, 0.),
            Entrance::SlideRightElastic { width } => (opacity, (width + 48.) * rest, 0.),
            Entrance::SlideLeftElastic { width } => (opacity, -(width + 48.) * rest, 0.),
            Entrance::PopElastic => (opacity, 0., 14. * rest),
            Entrance::ScaleIn | Entrance::ScaleInFrom { .. } | Entrance::LogoPop | Entrance::SuccessPop => (opacity, 0., 0.),
        }
    }

    /// CSS ölçeği / döndürmesi / orijini (`p` eğrilmiş ilerleme, `raw` doğrusal).
    pub fn scale_frame(self, raw: f32, p: f32) -> (f32, f32, (f32, f32)) {
        let rest = 1. - p;
        match self {
            Entrance::PopElastic => (1. - 0.14 * rest, 0., (0.5, 0.5)),
            // .p2-menu varsayılan transform-origin: top left
            Entrance::ScaleIn => (1. - 0.08 * rest, 0., (0., 0.)),
            Entrance::ScaleInFrom { ox, oy } => (1. - 0.08 * rest, 0., (ox, oy)),
            Entrance::Page => (1. - 0.003 * rest, 0., (0.5, 0.5)),
            Entrance::LogoPop => {
                // Anahtar kareler: 0 → .55 → 1 (her aralık kendi içinde ease-pop; CSS animasyon
                // easing'i anahtar kare aralığına uygulanır).
                let (s, r) = if raw < 0.55 {
                    let k = EASE_POP.ease(raw / 0.55);
                    (0.4 + (1.14 - 0.4) * k, -14. + 17. * k)
                } else {
                    let k = EASE_POP.ease((raw - 0.55) / 0.45);
                    (1.14 - 0.14 * k, 3. - 3. * k)
                };
                (s, r, (0.5, 0.5))
            }
            Entrance::SuccessPop => {
                let s = if raw < 0.65 {
                    0.72 + 0.36 * EASE_OUT.ease(raw / 0.65)
                } else {
                    1.08 - 0.08 * EASE_OUT.ease((raw - 0.65) / 0.35)
                };
                (s, 0., (0.5, 0.5))
            }
            _ => (1., 0., (0.5, 0.5)),
        }
    }
}

/// Elementlere P2 giriş animasyonu ekler. Akıştaki element `relative` konumlanır ve kayma
/// `top/left` ile verilir. `.absolute()` çağrısı `p2_enter`'dan ÖNCE yapılmış öğede konum
/// korunur, kayma `translate` ile boyanır (bkz. gövde). `absolute` sonradan eklenirse
/// (`AnimationElement` üzerinde) bu algılanamaz — o durumda sarmalayıcı kullanın.
pub trait P2Motion: Styled + IntoElement + Sized + 'static {
    /// `delay`: `.anim-list` gibi sıralı girişler için; tavanı [`M_STAGGER_MAX`].
    fn p2_enter(
        self,
        id: impl Into<ElementId>,
        kind: Entrance,
        delay: Duration,
    ) -> AnimationElement<Self> {
        let (duration, curve) = kind.timing();
        let delay = delay.min(M_STAGGER_MAX);
        let total = duration + delay;
        let lead = delay.as_secs_f32() / total.as_secs_f32();
        self.with_animation(
            id,
            // Gecikme, ilk `lead` kesitinde ilerlemeyi 0'da tutarak uygulanır.
            Animation::new(total),
            move |el, t| {
                let raw = if lead >= 1. { 1. } else { ((t - lead) / (1. - lead)).clamp(0., 1.) };
                let p = curve.ease(raw);
                let (mut opacity, dx, dy) = kind.frame(raw, p);
                let (scale, rotate, origin) = kind.scale_frame(raw, p);
                if matches!(kind, Entrance::LogoPop) {
                    opacity = (raw / 0.55).min(1.);
                }
                use crate::fx::P2Fx as _;
                let mut el = el.opacity(opacity);
                // Zaten `absolute` olan öğe (açılır menü, modal, popover) `relative` yapılırsa akışa
                // düşüp ekran dışına kayar ve tıklamalar ıskalar. Onun konumuna (`left/top/right/
                // bottom`) dokunmadan kaymayı yalnız boyamada `translate` ile ver (CSS translateY gibi).
                let absolute = matches!(el.style().position, Some(gpui_kit::Position::Absolute));
                if absolute {
                    if dx != 0. || dy != 0. {
                        el = el.translate(dx, dy);
                    }
                } else {
                    el = el.relative().left(px(dx)).top(px(dy));
                }
                if scale != 1. || rotate != 0. {
                    el.scale(scale).rotate(rotate).transform_origin(origin.0, origin.1)
                } else {
                    el
                }
            },
        )
    }
}

impl<E: Styled + IntoElement + Sized + 'static> P2Motion for E {}

/// `p2-pulse` (opaklık 1 → .55 → 1, 2.6 sn). Pencere arka plandayken hiç eklenmemeli.
///
/// `Animation::with_max_fps` BİLEREK kullanılmaz: GPUI onu ekranın kare döngüsünden
/// bağımsız bir arka plan zamanlayıcısıyla sürüyor ve pencere küçültülmüşken de
/// görünümü yeniden kurduruyor (Faz 0 ölçümü: simge durumunda saniyede ~11 çizim).
/// Sınırsız animasyon ise kare döngüsüne bağlıdır; pencere gizlenince kendiliğinden durur.
pub fn pulse() -> Animation {
    Animation::new(Duration::from_millis(2600)).repeat()
}

/// `pulse()` için opaklık.
pub fn pulse_opacity(t: f32) -> f32 {
    let wave = (t * std::f32::consts::TAU).cos() * 0.5 + 0.5; // 1 → 0 → 1
    0.55 + 0.45 * wave
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bezier_endpoints() {
        for curve in [EASE_OUT, EASE_IN, EASE_ELASTIC, EASE_POP] {
            assert_eq!(curve.ease(0.), 0.);
            assert_eq!(curve.ease(1.), 1.);
        }
    }

    #[test]
    fn linear_bezier_is_identity() {
        let linear = CubicBezier::new(0., 0., 1., 1.);
        for i in 0..=10 {
            let x = i as f32 / 10.;
            assert!((linear.ease(x) - x).abs() < 1e-3, "{x}");
        }
    }

    #[test]
    fn elastic_overshoots() {
        let peak = (1..100).map(|i| EASE_ELASTIC.ease(i as f32 / 100.)).fold(0f32, f32::max);
        assert!(peak > 1.05, "elastik eğri taşmalı, tepe {peak}");
    }

    #[test]
    fn ease_out_matches_browser_midpoint() {
        // cubic-bezier(.2,.8,.2,1), x=.5: x(t)=.6t(1-t)+t³=.5 → t≈.7245 → y≈.946
        assert!((EASE_OUT.ease(0.5) - 0.946).abs() < 0.005, "{}", EASE_OUT.ease(0.5));
    }

    #[test]
    fn pulse_range() {
        assert!((pulse_opacity(0.) - 1.).abs() < 1e-6);
        assert!((pulse_opacity(0.5) - 0.55).abs() < 1e-6);
    }
}
