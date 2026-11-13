//! CSS yazı özellikleri — GPUI'de doğrudan olmayanlar.
//!
//! - **`letter-spacing`**: `third_party/gpui-pre-windows` yaması (`P2ls` sahte özellik etiketi →
//!   DirectWrite `SetCharacterSpacing`, karakter SONUNA eklenir; Chromium ile aynı). Kullanım:
//!   `div().tracking(-0.06)` (em; `letter-spacing: -.06em`) ya da `.tracking_px(0.24, font_px)`.
//!   Chromium harf aralığı varken isteğe bağlı bitişik harfleri (liga/clig) kapatır; burada da kapanır.
//! - **Ara ağırlıklar** (`font-weight: 650`): aynı yama, değişken Inter'in `wght` eksenini tam değere
//!   ayarlar → `.font_weight(FontWeight(650.))` ya da `.css_weight(650.)` doğrudan çalışır.
//! - **`font-optical-sizing: auto`**: yama `opsz` eksenini yazı boyuna (CSS px) ayarlar (Inter 14–32);
//!   büyük başlıklar Chromium'daki gibi "Display" kesimiyle çizilir. Ek bir şey yazmak gerekmez.
//!
//! Not: harf aralığı FontFeatures'a gömüldüğü için kalıtılır (alt öğelere de geçer) — CSS'te de
//! `letter-spacing` kalıtılır.

use std::sync::Arc;

use gpui_kit::{FontFeatures, FontWeight, Pixels, Styled};

/// `letter-spacing: <em>em` için FontFeatures (yamanın okuduğu `P2ls` = em × 1000).
pub fn tracking_features(em: f32) -> FontFeatures {
    let milli = (em * 1000.).round() as i32;
    if milli == 0 {
        return FontFeatures::default();
    }
    FontFeatures(Arc::new(vec![("P2ls".into(), milli as u32), ("liga".into(), 0), ("clig".into(), 0)]))
}

/// CSS `line-height: normal` satır kutusu (Chromium: yuvarlanmış çıkış + yuvarlanmış iniş; Inter hhea
/// 1984/2048 ve 496/2048). `font_px` çizilen boy. Ör. 10,2px → 12, 11,05px → 14, 25,5px → 31.
pub fn normal_line_height(font_px: Pixels) -> Pixels {
    let size = f32::from(font_px);
    gpui_kit::px((size * 0.96875).round() + (size * 0.2421875).round())
}

pub trait P2Text: Styled + Sized {
    /// CSS `letter-spacing: <em>em` (ör. `-.06em` → `-0.06`).
    fn tracking(self, em: f32) -> Self {
        self.font_features(tracking_features(em))
    }
    /// CSS `letter-spacing: <px>px`; `font_px` çizilen yazı boyu (ölçekli).
    fn tracking_px(self, ls_px: f32, font_px: Pixels) -> Self {
        let size = f32::from(font_px);
        if size <= 0. {
            return self;
        }
        self.tracking(ls_px / size)
    }
    /// CSS `font-weight: N` (değişken eksen; 650/750 dahil).
    fn css_weight(self, weight: f32) -> Self {
        self.font_weight(FontWeight(weight))
    }
}

impl<T: Styled + Sized> P2Text for T {}
