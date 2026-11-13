//! Electron'un satır içi SVG ikonları (24×24 viewBox).
//!
//! Veri: [`crate::icons_data::ICONS`] (`node rust/tools/parity/gen-icons.mjs` üretir; kaynak
//! `src/ComposerIcons.tsx`, `src/ChannelIcons.tsx`, `src/CallIcons.tsx`). Kontur kalınlığı CSS'ten
//! gelir (bileşen başına farklı): `icons::icon("bell").stroke(1.8).el().size(px(19.)).text_color(c)`.
//!
//! Çizim: GPUI `svg().data(..)` alfa maskesi — renk `text_color` ile verilir; `.hover(..)` /
//! `group_hover(..)` ile CSS'teki `currentColor` geçişleri aynen çalışır. Çok renkli SVG
//! (ör. kapak gradyanı; GPUI'de radyal gradyan yok) için [`svg_image`].
//!
//! Not (B ajanı): bu dosyanın ilk sürümü (A) yanlışlıkla üzerine yazıldı; API `icon(..).stroke(..)`
//! biçimine göre yeniden kuruldu. A kendi sürümünü geri koyarsa `Icon::raw`, `el`, `svg_image`
//! korunmalı (workspace_nav.rs kullanıyor).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use gpui_kit::{Hsla, ImageFormat, Img, IntoElement, Rgba, Svg, img, svg};

use crate::icons_data::ICONS;

/// Workspace.tsx proje rayındaki satır içi SVG'ler (ComposerIcon dışı).
pub mod rail {
    pub const DOCUMENTS: &str = r#"<path d="M7 3.5h7l4 4V20H7z"/><path d="M14 3.5V8h4M10 12h5M10 15.5h5"/>"#;
    pub const LIBRARY: &str = r#"<path d="M12 3v10M8 9l4 4 4-4M5 20h14M5 20v-4M19 20v-4"/>"#;
}

/// Bir ikon: gövde + kontur kalınlığı + dolgu.
#[derive(Clone, Copy)]
pub struct Icon {
    body: &'static str,
    stroke: f32,
    filled: bool,
}

/// `ad` ya da `takım:ad` (ör. `"bell"`, `"channel:voice"`, `"call:mic"`). Bulunamazsa boş ikon.
pub fn icon(name: &str) -> Icon {
    let (set, name) = name.split_once(':').map_or((None, name), |(s, n)| (Some(s), n));
    let body = ICONS.iter().find(|(s, n, _)| *n == name && set.is_none_or(|x| x == *s)).map_or("", |(_, _, b)| *b);
    Icon { body, stroke: 1.8, filled: false }
}

impl Icon {
    /// ICONS dışındaki bir SVG gövdesi (viewBox 0 0 24 24).
    pub const fn raw(body: &'static str) -> Self {
        Self { body, stroke: 1.8, filled: false }
    }
    /// CSS `stroke-width`.
    pub fn stroke(mut self, width: f32) -> Self {
        self.stroke = width;
        self
    }
    /// CSS `fill: currentColor` (ör. yıldızlı kanal).
    pub fn filled(mut self) -> Self {
        self.filled = true;
        self
    }
    /// GPUI öğesi: boyut ve renk çağıranda (`.size(px(18.)).text_color(c)`).
    pub fn el(self) -> Svg {
        static CACHE: OnceLock<Mutex<HashMap<(usize, u32, bool), Arc<[u8]>>>> = OnceLock::new();
        let key = (self.body.as_ptr() as usize, self.stroke.to_bits(), self.filled);
        let data = CACHE
            .get_or_init(Default::default)
            .lock()
            .unwrap()
            .entry(key)
            .or_insert_with(|| {
                let fill = if self.filled { "#fff" } else { "none" };
                // Üretilen veride currentColor → #fff; alfa maskesi olduğundan renk önemsiz.
                let markup = format!(
                    r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="{fill}" stroke="#fff" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round">{}</svg>"##,
                    self.stroke, self.body
                );
                Arc::from(markup.into_bytes().into_boxed_slice())
            })
            .clone();
        svg().data(&data).flex_none()
    }
}

impl IntoElement for Icon {
    type Element = Svg;
    fn into_element(self) -> Svg {
        self.el()
    }
}

use gpui_kit::Styled as _;

/// Hsla → SVG `rgb(..)` ve saydamlık.
pub fn svg_color(c: Hsla) -> (String, f32) {
    let rgba: Rgba = c.into();
    let ch = |v: f32| (v * 255.).round().clamp(0., 255.) as u8;
    (format!("rgb({},{},{})", ch(rgba.r), ch(rgba.g), ch(rgba.b)), rgba.a)
}

/// Renkleri gömülü tam bir SVG belgesini görsel olarak çizer. Aynı belge önbellekten aynı
/// `Image` ile gelir (GPUI görsel önbelleği kimliğe bakar).
pub fn svg_image(markup: String) -> Img {
    static IMAGES: OnceLock<Mutex<HashMap<String, Arc<gpui_kit::Image>>>> = OnceLock::new();
    let image = {
        let mut map = IMAGES.get_or_init(Default::default).lock().unwrap();
        if map.len() > 256 {
            map.clear();
        }
        map.entry(markup.clone()).or_insert_with(|| Arc::new(gpui_kit::Image::from_bytes(ImageFormat::Svg, markup.into_bytes()))).clone()
    };
    img(image)
}
