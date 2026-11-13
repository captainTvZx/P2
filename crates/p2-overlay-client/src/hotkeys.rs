//! Oyun içi overlay kısayolları (src/hotkeys.ts): kanonik biçim `[Ctrl+][Alt+][Shift+]<Tuş>`
//! ("Shift+2", "Ctrl+Alt+F12", "Space"); boş = kapalı. Tuş adı → Windows sanal tuş kodu (VK);
//! DLL tablosu (`HotkeySpec`) buradan üretilir. Bas-konuş oyuna da iletilir (bit7).

use crate::HotkeySpec;

pub const MOD_SHIFT: u8 = 1;
pub const MOD_CTRL: u8 = 2;
pub const MOD_ALT: u8 = 4;
pub const MOD_PASS_THROUGH: u8 = 0x80;

/// overlayTypes.ts `HOTKEY_ID`.
pub const ID_PANEL: u8 = 1;
pub const ID_SCREENSHOT: u8 = 2;
pub const ID_MUTE: u8 = 3;
pub const ID_PUSH_TO_TALK: u8 = 4;
pub const ID_DEAFEN: u8 = 5;

pub const LABELS: [(&str, &str); 5] = [("panel", "Paneli aç/kapat"), ("screenshot", "Ekran görüntüsü"), ("mute", "Mikrofonu sustur"), ("deafen", "Sesi kapat (sağırlaştır)"), ("pushToTalk", "Bas-konuş")];

fn vk_of(name: &str) -> Option<u16> {
    let fixed = match name {
        "Space" => 0x20,
        "Tab" => 0x09,
        "Enter" => 0x0D,
        "Backspace" => 0x08,
        "ArrowLeft" => 0x25,
        "ArrowUp" => 0x26,
        "ArrowRight" => 0x27,
        "ArrowDown" => 0x28,
        "Insert" => 0x2D,
        "Delete" => 0x2E,
        "Home" => 0x24,
        "End" => 0x23,
        "PageUp" => 0x21,
        "PageDown" => 0x22,
        "NumpadMultiply" => 0x6A,
        "NumpadAdd" => 0x6B,
        "NumpadSubtract" => 0x6D,
        "NumpadDecimal" => 0x6E,
        "NumpadDivide" => 0x6F,
        "Semicolon" => 0xBA,
        "Equal" => 0xBB,
        "Comma" => 0xBC,
        "Minus" => 0xBD,
        "Period" => 0xBE,
        "Slash" => 0xBF,
        "Backquote" => 0xC0,
        "BracketLeft" => 0xDB,
        "Backslash" => 0xDC,
        "BracketRight" => 0xDD,
        "Quote" => 0xDE,
        "Pause" => 0x13,
        "CapsLock" => 0x14,
        "ScrollLock" => 0x91,
        _ => 0,
    };
    if fixed != 0 {
        return Some(fixed);
    }
    let b = name.as_bytes();
    if b.len() == 1 && (b[0].is_ascii_digit() || b[0].is_ascii_uppercase()) {
        return Some(b[0] as u16);
    }
    if let Some(n) = name.strip_prefix('F').and_then(|n| n.parse::<u16>().ok()).filter(|n| (1..=24).contains(n)) {
        return Some(0x70 + n - 1);
    }
    if let Some(n) = name.strip_prefix("Numpad").and_then(|n| n.parse::<u16>().ok()).filter(|n| *n <= 9) {
        return Some(0x60 + n);
    }
    None
}

/// "m" → "M", "f12" → "F12", "space" → "Space", "pageup" → "PageUp".
fn normalise(part: &str) -> Option<String> {
    if vk_of(part).is_some() {
        return Some(part.to_string());
    }
    let lower = part.to_lowercase();
    if lower.len() == 1 {
        let up = lower.to_uppercase();
        return vk_of(&up).map(|_| up);
    }
    if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u16>().ok()) {
        let name = format!("F{n}");
        return vk_of(&name).map(|_| name);
    }
    const NAMES: [&str; 33] = [
        "Space", "Tab", "Enter", "Backspace", "ArrowLeft", "ArrowUp", "ArrowRight", "ArrowDown", "Insert", "Delete", "Home", "End", "PageUp", "PageDown", "NumpadMultiply", "NumpadAdd", "NumpadSubtract", "NumpadDecimal",
        "NumpadDivide", "Semicolon", "Equal", "Comma", "Minus", "Period", "Slash", "Backquote", "BracketLeft", "Backslash", "BracketRight", "Quote", "Pause", "CapsLock", "ScrollLock",
    ];
    NAMES.iter().find(|n| n.to_lowercase() == lower).map(|n| n.to_string())
}

/// "Ctrl+Shift+2" → (vk, mods); boş ya da geçersiz → None.
pub fn parse(value: &str) -> Option<(u16, u8, String)> {
    let mut mods = 0u8;
    let mut key: Option<String> = None;
    for part in value.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        match part.to_lowercase().as_str() {
            "ctrl" | "control" if mods & MOD_CTRL == 0 => mods |= MOD_CTRL,
            "alt" if mods & MOD_ALT == 0 => mods |= MOD_ALT,
            "shift" if mods & MOD_SHIFT == 0 => mods |= MOD_SHIFT,
            "ctrl" | "control" | "alt" | "shift" => return None,
            _ => {
                if key.is_some() {
                    return None;
                }
                key = Some(normalise(part)?);
            }
        }
    }
    let key = key?;
    Some((vk_of(&key)?, mods, key))
}

pub fn build(mods: u8, key: &str) -> String {
    let mut parts = Vec::new();
    if mods & MOD_CTRL != 0 {
        parts.push("Ctrl".to_string());
    }
    if mods & MOD_ALT != 0 {
        parts.push("Alt".to_string());
    }
    if mods & MOD_SHIFT != 0 {
        parts.push("Shift".to_string());
    }
    parts.push(key.to_string());
    parts.join("+")
}

/// Kanonik dize; geçersizse boş.
pub fn canonical(value: &str) -> String {
    parse(value).map(|(_, mods, key)| build(mods, &key)).unwrap_or_default()
}

/// Çipler: "Shift+2" → ["Shift", "2"]; "Space" → ["Boşluk"].
pub fn format_keys(value: &str) -> Vec<String> {
    let Some((_, mods, key)) = parse(value) else { return Vec::new() };
    let mut keys = Vec::new();
    if mods & MOD_CTRL != 0 {
        keys.push("Ctrl".to_string());
    }
    if mods & MOD_ALT != 0 {
        keys.push("Alt".to_string());
    }
    if mods & MOD_SHIFT != 0 {
        keys.push("Shift".to_string());
    }
    keys.push(match key.as_str() {
        "Space" => "Boşluk".into(),
        "Backspace" => "⌫".into(),
        "ArrowLeft" => "←".into(),
        "ArrowRight" => "→".into(),
        "ArrowUp" => "↑".into(),
        "ArrowDown" => "↓".into(),
        "PageUp" => "Page Up".into(),
        "PageDown" => "Page Down".into(),
        other => other.to_string(),
    });
    keys
}

/// GPUI tuş adından ("a", "f12", "space", "2") kanonik kısayol; Esc ve yalnız değiştirici → None.
pub fn from_gpui(key: &str, ctrl: bool, alt: bool, shift: bool) -> Option<String> {
    let name = match key {
        "escape" | "shift" | "control" | "alt" | "platform" | "function" => return None,
        "space" => "Space".to_string(),
        "enter" => "Enter".to_string(),
        "tab" => "Tab".to_string(),
        "backspace" => "Backspace".to_string(),
        "left" => "ArrowLeft".to_string(),
        "right" => "ArrowRight".to_string(),
        "up" => "ArrowUp".to_string(),
        "down" => "ArrowDown".to_string(),
        "pageup" => "PageUp".to_string(),
        "pagedown" => "PageDown".to_string(),
        "home" => "Home".to_string(),
        "end" => "End".to_string(),
        "insert" => "Insert".to_string(),
        "delete" => "Delete".to_string(),
        other => normalise(other)?,
    };
    vk_of(&name)?;
    let mods = (if ctrl { MOD_CTRL } else { 0 }) | (if alt { MOD_ALT } else { 0 }) | (if shift { MOD_SHIFT } else { 0 });
    Some(build(mods, &name))
}

/// Ayarlardaki kısayollar → DLL tablosu (boş/geçersiz atlanır; bas-konuş oyuna da geçer).
pub fn to_native(panel: &str, screenshot: &str, mute: &str, deafen: &str, push_to_talk: &str) -> Vec<HotkeySpec> {
    let mut list = Vec::new();
    for (id, value) in [(ID_PANEL, panel), (ID_SCREENSHOT, screenshot), (ID_MUTE, mute), (ID_PUSH_TO_TALK, push_to_talk), (ID_DEAFEN, deafen)] {
        if let Some((vk, mods, _)) = parse(value) {
            let mods = if id == ID_PUSH_TO_TALK { mods | MOD_PASS_THROUGH } else { mods };
            list.push(HotkeySpec { id, vk, mods });
        }
    }
    list
}

/// Aynı kısayolu kullanan iki eylem (çakışma) var mı: ilk çakışan çift.
pub fn conflict(values: &[(&str, &str)]) -> Option<(String, String)> {
    for (i, (a, va)) in values.iter().enumerate() {
        let ca = canonical(va);
        if ca.is_empty() {
            continue;
        }
        for (b, vb) in values.iter().skip(i + 1) {
            if canonical(vb) == ca {
                return Some((a.to_string(), b.to_string()));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_like_js() {
        assert_eq!(parse("Shift+2").map(|p| (p.0, p.1)), Some((0x32, MOD_SHIFT)));
        assert_eq!(parse("ctrl+alt+f12").map(|p| (p.0, p.1)), Some((0x7B, MOD_CTRL | MOD_ALT)));
        assert_eq!(canonical("shift+ctrl+m"), "Ctrl+Shift+M");
        assert_eq!(parse("Shift+Shift+2"), None);
        assert_eq!(format_keys("Space"), vec!["Boşluk"]);
        assert_eq!(from_gpui("f8", false, false, false).as_deref(), Some("F8"));
        assert_eq!(from_gpui("escape", false, false, false), None);
        let native = to_native("Shift+2", "F12", "Shift+M", "", "V");
        assert_eq!(native.len(), 4);
        assert_eq!(native.iter().find(|h| h.id == ID_PUSH_TO_TALK).map(|h| h.mods), Some(MOD_PASS_THROUGH));
        assert_eq!(conflict(&[("panel", "Shift+2"), ("mute", "shift+2")]), Some(("panel".into(), "mute".into())));
    }
}
