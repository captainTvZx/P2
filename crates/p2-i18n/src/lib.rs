//! P2 dil desteği — `src/i18n.ts` + `src/locales/*` karşılığı.
//!
//! Sözlükler `locales/<dil>.json` olarak derlemeye gömülür (`tools/i18n-export.mts` üretir).
//! Bir anahtar seçili dilde yoksa İngilizceye, o da yoksa Türkçeye, o da yoksa anahtarın
//! kendisine düşülür; `{ad}` yer tutucuları değişkenlerle doldurulur.

use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Language {
    pub id: &'static str,
    pub native: &'static str,
    pub english: &'static str,
    pub locale: &'static str,
}

pub const LANGUAGES: &[Language] = &[
    Language { id: "tr", native: "Türkçe", english: "Turkish", locale: "tr-TR" },
    Language { id: "en", native: "English (US)", english: "English (US)", locale: "en-US" },
    Language { id: "en-GB", native: "English (UK)", english: "English (UK)", locale: "en-GB" },
    Language { id: "de", native: "Deutsch", english: "German", locale: "de-DE" },
    Language { id: "es", native: "Español (España)", english: "Spanish (Spain)", locale: "es-ES" },
    Language { id: "es-LA", native: "Español (Latinoamérica)", english: "Spanish (Latin America)", locale: "es-419" },
    Language { id: "fr", native: "Français", english: "French", locale: "fr-FR" },
    Language { id: "pt-BR", native: "Português (Brasil)", english: "Portuguese (Brazil)", locale: "pt-BR" },
    Language { id: "ja", native: "日本語", english: "Japanese", locale: "ja-JP" },
    Language { id: "zh-CN", native: "中文（简体）", english: "Chinese (Simplified)", locale: "zh-CN" },
    Language { id: "zh-TW", native: "中文（繁體）", english: "Chinese (Traditional)", locale: "zh-TW" },
];

const SOURCES: &[(&str, &str)] = &[
    ("tr", include_str!("../locales/tr.json")),
    ("en", include_str!("../locales/en.json")),
    ("en-GB", include_str!("../locales/en-GB.json")),
    ("de", include_str!("../locales/de.json")),
    ("es", include_str!("../locales/es.json")),
    ("es-LA", include_str!("../locales/es-LA.json")),
    ("fr", include_str!("../locales/fr.json")),
    ("pt-BR", include_str!("../locales/pt-BR.json")),
    ("ja", include_str!("../locales/ja.json")),
    ("zh-CN", include_str!("../locales/zh-CN.json")),
    ("zh-TW", include_str!("../locales/zh-TW.json")),
];

type Dict = HashMap<String, String>;

fn dictionaries() -> &'static HashMap<&'static str, Dict> {
    static DICTS: OnceLock<HashMap<&'static str, Dict>> = OnceLock::new();
    DICTS.get_or_init(|| {
        SOURCES.iter().map(|(id, json)| (*id, serde_json::from_str(json).expect("geçerli sözlük JSON'u"))).collect()
    })
}

fn current() -> &'static RwLock<&'static str> {
    static CURRENT: OnceLock<RwLock<&'static str>> = OnceLock::new();
    // Varsayılan dil İngilizce: yeni profilde ya da ayarlarda dil yoksa (kayıtlı seçim main.rs'de uygulanır).
    CURRENT.get_or_init(|| RwLock::new("en"))
}

pub fn language_of(id: &str) -> Language {
    *LANGUAGES.iter().find(|l| l.id == id).unwrap_or(&LANGUAGES[1])
}

/// Seçili dili değiştirir (bilinmeyen kimlik İngilizceye düşer).
pub fn set_language(id: &str) {
    *current().write().unwrap() = language_of(id).id;
}

pub fn language() -> Language {
    language_of(*current().read().unwrap())
}

/// `navigator.languages` karşılığı: sistemin tercih ettiği UI dilleri.
fn system_tags() -> Vec<String> {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Globalization::GetUserDefaultLocaleName;
        let mut buf = [0u16; 85]; // LOCALE_NAME_MAX_LENGTH
        let n = GetUserDefaultLocaleName(&mut buf);
        if n > 1 {
            return vec![String::from_utf16_lossy(&buf[..n as usize - 1])];
        }
    }
    std::env::var("LANG").ok().map(|l| l.split('.').next().unwrap_or("").replace('_', "-")).into_iter().collect()
}

/// i18n.ts `detectLanguage`: sistem dili uygulama dillerinden birine yakınsa onu seçer.
pub fn detect_language() -> &'static str {
    for tag in system_tags() {
        let lower = tag.to_lowercase();
        if let Some(exact) = LANGUAGES.iter().find(|l| l.locale.to_lowercase() == lower || l.id.to_lowercase() == lower) {
            return exact.id;
        }
        let base = lower.split('-').next().unwrap_or("");
        if let Some(loose) = LANGUAGES.iter().find(|l| l.id.split('-').next() == Some(base)) {
            return loose.id;
        }
    }
    "tr"
}

/// Belirli dilde çeviri (dil → en → tr → anahtar), `{ad}` yer tutucuları doldurulur.
pub fn translate(lang: &str, key: &str, vars: &[(&str, &str)]) -> String {
    let dicts = dictionaries();
    let text = [lang, "en", "tr"]
        .iter()
        .find_map(|l| dicts.get(l).and_then(|d| d.get(key)))
        .map(String::as_str)
        .unwrap_or(key);
    if vars.is_empty() || !text.contains('{') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) if after[..close].chars().all(|c| c.is_alphanumeric() || c == '_') => {
                let name = &after[..close];
                match vars.iter().find(|(k, _)| *k == name) {
                    Some((_, v)) => out.push_str(v),
                    None => out.push_str(&rest[open..open + close + 2]),
                }
                rest = &after[close + 1..];
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Seçili dilde çeviri.
pub fn t(key: &str) -> String {
    translate(language().id, key, &[])
}

/// Seçili dilde, değişkenli çeviri: `tv("set.version", &[("version", "1.0")])`.
pub fn tv(key: &str, vars: &[(&str, &str)]) -> String {
    translate(language().id, key, vars)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_dictionaries_parse_and_tr_is_complete() {
        let dicts = dictionaries();
        assert_eq!(dicts.len(), SOURCES.len());
        let tr = &dicts["tr"];
        assert!(tr.len() >= 170);
        for (id, d) in dicts {
            for key in d.keys() {
                assert!(tr.contains_key(key), "{id} sözlüğünde Türkçede olmayan anahtar: {key}");
            }
        }
    }

    #[test]
    fn default_language_is_english() {
        assert_eq!(language_of("yok").id, "en");
        assert_eq!(*current().read().unwrap(), "en");
    }

    #[test]
    fn fallback_chain_and_vars() {
        assert_eq!(translate("tr", "act.close", &[]), "Kapat");
        assert_eq!(translate("xx", "act.close", &[]), translate("en", "act.close", &[]));
        assert_eq!(translate("tr", "yok.boyle.anahtar", &[]), "yok.boyle.anahtar");
        assert_eq!(translate("tr", "set.version", &[("version", "1.2.3")]), "P2 · sürüm 1.2.3");
        assert_eq!(translate("tr", "set.version", &[]), "P2 · sürüm {version}");
    }
}
