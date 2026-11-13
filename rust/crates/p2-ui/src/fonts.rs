//! Gömülü yazı tipleri. Electron `@fontsource-variable/inter` (Inter 4.x, wght 100–900) kullanır;
//! aynı tasarımın değişken TTF'i (`assets/fonts/InterVariable.ttf`, Inter 4.001, SIL OFL —
//! `Inter-OFL.txt`) ikili dosyaya gömülür ve uygulama açılışında GPUI metin sistemine eklenir.
//! Aile adı: [`crate::tokens::FONT`] ("Inter Variable Text" — DirectWrite'ın STAT'tan türettiği ad). DirectWrite, değişken yazı tipinin adlandırılmış
//! örneklerini (100, 200 … 900) ayrı ağırlıklar olarak kaydeder; ara ağırlıklar (ör. 650) ve `opsz`
//! gpui-pre-windows yamasıyla eksen değeri olarak verilir (TOKENS.md §6).

use std::borrow::Cow;

static INTER_VARIABLE: &[u8] = include_bytes!("../assets/fonts/InterVariable.ttf");

/// Uygulama başında bir kez çağır (`theme::apply`'dan önce ya da sonra fark etmez).
pub fn install(cx: &mut gpui_kit::App) {
    if let Err(e) = cx.text_system().add_fonts(vec![Cow::Borrowed(INTER_VARIABLE)]) {
        eprintln!("yazı tipi yüklenemedi (Inter): {e}");
    }
}
