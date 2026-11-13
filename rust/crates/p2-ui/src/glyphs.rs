//! [`crate::icons`]'u tamamlayan ikonlar (A ajanı):
//! - `call` takımı: CallIcons.tsx'in DOLGULU ikonları (18 adet; `icons_data.rs`, gen-icons.mjs üretir).
//! - Ada göre erişim: `named("send", 16., 1.7)` — ComposerIcons/ChannelIcons çizgi ikonları dizgi adıyla.
//! - Elle çizilen arayüz ikonları ([`ui`]): başlık çubuğu pencere düğmeleri (titlebar.css kutularının
//!   birebir SVG'si), `check`, `chevron-down/right`, `arrow-right`, `eye`, `eye-off`.
//!
//! Hepsi alfa maskesi: renk `.text_color(..)` (CSS `currentColor`), boyut `size` (CSS width/height px).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use gpui_kit::{Styled, Svg, px, svg};

use crate::icons_data::ICONS;

/// (ad, viewBox, iç işaretleme, dolgu mu)
static UI_ICONS: &[(&str, &str, &str, bool)] = &[
    // `.minimize-icon::after`: 10×10 kutuda sol/sağ 1px, alttan 1px, 1px yükseklik
    ("win-min", "0 0 10 10", r#"<rect x="1" y="8" width="8" height="1"/>"#, true),
    // `.maximize-icon`: 10×10, 1px kenarlık
    ("win-max", "0 0 10 10", r#"<path fill-rule="evenodd" d="M0 0h10v10H0Z M1 1v8h8V1Z"/>"#, true),
    // `.restore-icon`: iki 7×7 kutu (sağ üst + sol alt; alttaki önde)
    ("win-restore", "0 0 10 10", r#"<path fill-rule="evenodd" d="M3 0h7v7H7V6h2V1H4v2H3Z M0 3h7v7H0Z M1 4v5h5V4Z"/>"#, true),
    // `.close-icon`: sol 4px, üst -1px, 1×12 çizgi ±45° (merkez 4.5,5)
    ("win-close", "0 0 10 10", r#"<path d="M4 -1h1v12h-1Z" transform="rotate(45 4.5 5)"/><path d="M4 -1h1v12h-1Z" transform="rotate(-45 4.5 5)"/>"#, true),
    ("check", "0 0 24 24", r#"<path d="m5 12.5 4.5 4.5L19 7.5"/>"#, false),
    ("chevron-down", "0 0 24 24", r#"<path d="m6 9 6 6 6-6"/>"#, false),
    ("chevron-right", "0 0 24 24", r#"<path d="m9 6 6 6-6 6"/>"#, false),
    ("arrow-right", "0 0 24 24", r#"<path d="M5 12h14M13 6l6 6-6 6"/>"#, false),
    ("eye", "0 0 24 24", r#"<path d="M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12Z"/><circle cx="12" cy="12" r="3"/>"#, false),
    ("eye-off", "0 0 24 24", r#"<path d="M10.6 5.6A9.6 9.6 0 0 1 12 5.5C18 5.5 21.5 12 21.5 12a17 17 0 0 1-2.6 3.4M6.6 6.6A16.6 16.6 0 0 0 2.5 12S6 18.5 12 18.5a9 9 0 0 0 5.4-1.7M9.9 9.9a3 3 0 0 0 4.2 4.2M3.5 3.5l17 17"/>"#, false),
];

fn markup(view_box: &str, body: &str, fill: bool, stroke: f32) -> String {
    let paint = if fill { r#"fill="black""#.to_string() } else { format!(r#"fill="none" stroke="black" stroke-width="{stroke}" stroke-linecap="round" stroke-linejoin="round""#) };
    format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{view_box}" {paint}>{body}</svg>"#)
}

fn cached(key: String, build: impl FnOnce() -> Option<String>) -> Option<Arc<[u8]>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<[u8]>>>> = OnceLock::new();
    let mut map = CACHE.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap();
    if let Some(data) = map.get(&key) {
        return Some(data.clone());
    }
    let data: Arc<[u8]> = Arc::from(build()?.into_bytes().into_boxed_slice());
    map.insert(key, data.clone());
    Some(data)
}

fn from(data: Option<Arc<[u8]>>, size: f32) -> Svg {
    let s = svg().size(px(size)).flex_shrink_0();
    match data {
        Some(d) => s.data(&d),
        None => s,
    }
}

/// Dolgulu arama ikonu (CallIcons.tsx): `mic`, `micOff`, `headphones`, `headphonesOff`, `volume`,
/// `volumeOff`, `camera`, `cameraOff`, `screen`, `screenOff`, `phone`, `hangup`, `pen`, `expand`,
/// `shrink`, `play`, `pause`, `personAdd`.
pub fn filled(name: &str, size: f32) -> Svg {
    from(cached(format!("call/{name}"), || ICONS.iter().find(|(s, n, _)| *s == "call" && *n == name).map(|(_, _, b)| markup("0 0 24 24", b, true, 0.))), size)
}

/// ComposerIcons/ChannelIcons çizgi ikonu ada göre (`send`, `close`, `voice`, `announcement` …).
pub fn named(name: &str, size: f32, stroke: f32) -> Svg {
    from(
        cached(format!("line/{name}/{stroke}"), || {
            ICONS.iter().find(|(s, n, _)| *s != "call" && *n == name).map(|(_, _, b)| markup("0 0 24 24", b, false, stroke))
        }),
        size,
    )
}

/// Arayüz ikonu (çizgili olanlarda kontur 2).
pub fn ui(name: &str, size: f32) -> Svg {
    ui_stroke(name, size, 2.)
}

pub fn ui_stroke(name: &str, size: f32, stroke: f32) -> Svg {
    from(cached(format!("ui/{name}/{stroke}"), || UI_ICONS.iter().find(|(n, ..)| *n == name).map(|(_, vb, b, f)| markup(vb, b, *f, stroke))), size)
}

/// Bütün adlar (takım, ad) — vitrin ve test için.
pub fn all() -> Vec<(&'static str, &'static str)> {
    ICONS.iter().map(|(s, n, _)| (*s, *n)).chain(UI_ICONS.iter().map(|(n, ..)| ("ui", *n))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn butun_ikonlar_bulunur() {
        for (set, name) in all() {
            let found = match set {
                "call" => cached(format!("t/call/{name}"), || ICONS.iter().find(|(s, n, _)| *s == "call" && *n == name).map(|(_, _, b)| markup("0 0 24 24", b, true, 0.))),
                "ui" => cached(format!("t/ui/{name}"), || UI_ICONS.iter().find(|(n, ..)| *n == name).map(|(_, vb, b, f)| markup(vb, b, *f, 2.))),
                _ => cached(format!("t/line/{name}"), || ICONS.iter().find(|(_, n, _)| *n == name).map(|(_, _, b)| markup("0 0 24 24", b, false, 1.8))),
            };
            assert!(found.is_some(), "{set}/{name}");
        }
    }
}
