//! İstemci (p2-client) ↔ overlay host süreci (p2-overlay) arasındaki satır başına bir JSON
//! protokolü. İstemci host'u oyun pid'i başına başlatır; stdin'e `ToHost`, host stdout'a
//! `FromHost` yazar. Electron'daki `overlayHost.cjs` ↔ overlay penceresi IPC'sinin karşılığı.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct HotkeySpec {
    pub id: u8,
    pub vk: u16,
    /// bit0 Shift, bit1 Ctrl, bit2 Alt, bit7 geçirgen (tuş oyuna da gider)
    pub mods: u8,
}

impl HotkeySpec {
    pub const PANEL_DEFAULT: HotkeySpec = HotkeySpec { id: 1, vk: 0x32, mods: 0x1 };

    pub fn to_ipc(&self) -> crate::Hotkey {
        crate::Hotkey { vk: self.vk, mods: self.mods, id: self.id }
    }

    /// Kart üzerindeki tuş çipleri: Shift+2 → ["Shift", "2"]
    pub fn keys(&self) -> Vec<String> {
        let mut keys = Vec::new();
        if self.mods & 0x2 != 0 {
            keys.push("Ctrl".to_string());
        }
        if self.mods & 0x4 != 0 {
            keys.push("Alt".to_string());
        }
        if self.mods & 0x1 != 0 {
            keys.push("Shift".to_string());
        }
        keys.push(vk_label(self.vk));
        keys
    }
}

pub fn vk_label(vk: u16) -> String {
    match vk {
        0x30..=0x39 | 0x41..=0x5a => char::from(vk as u8).to_string(),
        0x70..=0x87 => format!("F{}", vk - 0x6f),
        0x60..=0x69 => format!("Num{}", vk - 0x60),
        0x20 => "Boşluk".into(),
        0x0d => "Enter".into(),
        0x09 => "Tab".into(),
        0x1b => "Esc".into(),
        0xc0 => "`".into(),
        0x2d => "Insert".into(),
        0x24 => "Home".into(),
        0x23 => "End".into(),
        _ => "?".into(),
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Toast {
    #[serde(default)]
    pub id: String,
    /// info | message | call | screenshot | error
    #[serde(default)]
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub sticky: bool,
    #[serde(default)]
    pub tag: Option<String>,
    /// Tuş çipleri (ör. "Shift", "2").
    #[serde(default)]
    pub keys: Vec<String>,
    /// Eylem düğmeleri (overlayTypes.ts `OverlayToastAction`); tıklanınca `FromHost::ToastAction`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<ToastAction>,
    /// Kartın solundaki avatar (yoksa P2 logosu).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<ToastAvatar>,
    /// Eylemle birlikte geri dönen serbest yük.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToastAction {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub primary: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToastAvatar {
    pub user_id: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Installed {
    pub version: String,
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub play_seconds: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CallPerson {
    pub name: String,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub deafened: bool,
    #[serde(default)]
    pub speaking: bool,
    #[serde(default)]
    pub muted: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CallInfo {
    pub active: bool,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub deafened: bool,
    #[serde(default)]
    pub people: Vec<CallPerson>,
}

/// Ayarlar > Kütüphane > "Oyun içi overlay · Görünüm" (settings.ts `overlay*`). Açılışta
/// `init.look` ile, sonra her değişiklikte `look` komutuyla gelir.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Look {
    /// FPS ve ping göstergesi (HUD) açık.
    pub hud: bool,
    /// top-left | top-center | top-right
    pub hud_position: String,
    /// Göstergede CPU / RAM / paket kaybı da yazsın.
    pub hud_details: bool,
    /// top-right | top-left | bottom-right | bottom-left
    pub toast_corner: String,
    /// Sabitlenmiş katmanların (panel kapalıyken görünen gösterge) saydamlığı, 0.35–1.
    pub opacity: f32,
    /// Overlay arayüz ölçeği, 0.8–1.5 (Electron `setZoom`).
    pub scale: f32,
    /// Animasyonları azalt (bildirim giriş hareketi kapanır).
    pub reduce_motion: bool,
}

impl Default for Look {
    fn default() -> Self {
        Self { hud: false, hud_position: "top-left".into(), hud_details: true, toast_corner: "top-right".into(), opacity: 0.92, scale: 1.0, reduce_motion: false }
    }
}

impl Look {
    /// Ayar dosyasından gelen değerleri Electron'daki sınırlara çeker.
    pub fn clamped(mut self) -> Self {
        self.opacity = if self.opacity.is_finite() { self.opacity.clamp(0.35, 1.0) } else { 0.92 };
        // OverlayApp.tsx: setZoom(max(.5, min(2, scale))); ayar arayüzü 0.8–1.5 verir.
        self.scale = if self.scale.is_finite() && self.scale > 0. { self.scale.clamp(0.5, 2.0) } else { 1.0 };
        if !matches!(self.hud_position.as_str(), "top-left" | "top-center" | "top-right") {
            self.hud_position = "top-left".into();
        }
        if !matches!(self.toast_corner.as_str(), "top-right" | "top-left" | "bottom-right" | "bottom-left") {
            self.toast_corner = "top-right".into();
        }
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ToHost {
    #[serde(rename_all = "camelCase")]
    Init {
        /// Oyun pid'i: host önceden (oyun başlamadan) hazır bekletilir, pid sonra gelir.
        #[serde(default)]
        pid: u32,
        api: String,
        access_token: String,
        client_version: String,
        org: String,
        project: String,
        project_name: String,
        user_id: String,
        user_name: String,
        hotkeys: Vec<HotkeySpec>,
        #[serde(default)]
        light: bool,
        /// Ekran görüntülerinin klasörü (boş: Resimler\P2).
        #[serde(default)]
        shot_dir: String,
        /// Ayarlar: HUD açık başlasın, bildirim köşesi (top-right | top-left | bottom-right | bottom-left).
        #[serde(default)]
        hud: bool,
        #[serde(default)]
        toast_corner: String,
        /// Görünüm ayarlarının tamamı (yoksa `hud` + `toast_corner` kullanılır).
        #[serde(default)]
        look: Option<Look>,
        /// Pencere düzeni dosyası (Electron `localStorage['p2:overlay:layout:<proje>']` karşılığı,
        /// aynı anahtar/biçim). Boşsa düzen saklanmaz.
        #[serde(default)]
        layout_path: String,
        /// Kurulu sürüm/kanal ve oynama süresi (Oyun özeti, Ayarlar araçları).
        #[serde(default)]
        installed: Option<Installed>,
    },
    #[serde(rename_all = "camelCase")]
    Token { access_token: String },
    /// Görünüm ayarları değişti (açık oyunlara anında uygulanır).
    Look(Look),
    Toast(Toast),
    Dismiss { tag: String },
    Visible { on: bool },
    Call(CallInfo),
    Hotkeys { list: Vec<HotkeySpec> },
    Shot,
    Stop,
    /// Ana pencereden "şu aracı (hedefiyle) aç" (overlayTypes.ts `OverlayOpenPayload`): panel açılır.
    Open {
        window: String,
        #[serde(default)]
        target: Option<serde_json::Value>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum FromHost {
    /// Paylaşımlı bellek kuruldu: enjeksiyon yapılabilir.
    Session,
    /// DLL bağlandı ve oyun kare çizmeye başladı.
    Ready { width: u32, height: u32 },
    Visible { on: bool },
    Hotkey { id: u8, down: bool },
    /// Panelden istemciye eylem: call-mute | call-deafen | call-leave | call-accept | call-decline |
    /// call-join:<kanal> | call-start:<kişi> | call-invite:<kişi> | open-task:<id> | open-profile:<id> | ...
    Action { name: String },
    /// Bildirim kartındaki düğme (overlayTypes.ts `OverlayToastActionDetail`).
    #[serde(rename_all = "camelCase")]
    ToastAction {
        toast_id: String,
        #[serde(default)]
        tag: Option<String>,
        action_id: String,
        #[serde(default)]
        payload: Option<serde_json::Value>,
    },
    Shot { path: String, width: u32, height: u32 },
    TokenExpired,
    Log { text: String },
}

impl ToHost {
    pub fn line(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

impl FromHost {
    pub fn line(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let msg = ToHost::Toast(Toast { title: "Selam".into(), keys: vec!["Shift".into()], ..Default::default() });
        let line = msg.line();
        assert!(line.contains("\"type\":\"toast\""), "{line}");
        assert_eq!(serde_json::from_str::<ToHost>(&line).unwrap(), msg);
        let v: ToHost = serde_json::from_str(r#"{"type":"token","accessToken":"x"}"#).unwrap();
        assert_eq!(v, ToHost::Token { access_token: "x".into() });
        assert_eq!(FromHost::TokenExpired.line(), r#"{"type":"token-expired"}"#);
        assert_eq!(HotkeySpec::PANEL_DEFAULT.keys(), vec!["Shift", "2"]);
    }

    #[test]
    fn look_roundtrip_and_clamp() {
        let look = Look { scale: 1.25, opacity: 0.6, hud_position: "top-right".into(), ..Default::default() };
        let line = ToHost::Look(look.clone()).line();
        assert!(line.contains("\"type\":\"look\"") && line.contains("\"hudPosition\":\"top-right\""), "{line}");
        assert_eq!(serde_json::from_str::<ToHost>(&line).unwrap(), ToHost::Look(look));
        // Eski istemcinin init'i (look yok) okunabilmeli.
        let old = r#"{"type":"init","api":"a","accessToken":"t","clientVersion":"v","org":"o","project":"p","projectName":"n","userId":"u","userName":"x","hotkeys":[]}"#;
        assert!(matches!(serde_json::from_str::<ToHost>(old).unwrap(), ToHost::Init { look: None, .. }));
        let c = Look { opacity: 0.1, scale: 9., hud_position: "x".into(), toast_corner: "y".into(), ..Default::default() }.clamped();
        assert_eq!((c.opacity, c.scale, c.hud_position.as_str(), c.toast_corner.as_str()), (0.35, 2.0, "top-left", "top-right"));
    }
}
