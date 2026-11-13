//! Sunucu güncellemesi için durum modeli (`server-updater.cjs` `state`) ve sağlayıcı arayüzü.
//! Asıl güncelleyici ayrı bir crate'te (`p2-updater`) yazılıyor; konsol yalnız bu arayüzü bilir.
//! Bağlanana kadar [`StubUpdater`] kullanılır (her zaman `idle`).

use std::sync::Mutex;

/// `{ status, current, version, percent, error }`. `status`: idle | checking | downloading | ready | installing.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UpdateState {
    pub status: String,
    pub current: String,
    pub version: String,
    pub percent: u32,
    pub error: String,
}

impl UpdateState {
    pub fn idle(current: &str) -> Self {
        Self { status: "idle".into(), current: current.into(), version: String::new(), percent: 0, error: String::new() }
    }
}

/// Konsolun güncelleyiciden beklediği. `stop_server` kurulumdan önce API'yi durdurmak için verilir
/// (Electron: `install(stopServer)` → `stopServer({ selfStop: true })`).
pub trait UpdateProvider: Send + Sync {
    fn state(&self) -> UpdateState;
    /// "Güncelleme denetle".
    fn check(&self);
    /// "Yeniden dene" (indirilmiş ama kurulamamış güncelleme).
    fn install(&self);
}

/// Bağlanmamış güncelleyici. `P2_CONSOLE_FAKE_UPDATE="durum:yüzde:sürüm[:hata]"` ile sabit bir durum
/// gösterebilir (görsel karşılaştırma testleri).
pub struct StubUpdater {
    state: Mutex<UpdateState>,
}

impl StubUpdater {
    pub fn new(current: &str) -> Self {
        let mut state = UpdateState::idle(current);
        if let Ok(fake) = std::env::var("P2_CONSOLE_FAKE_UPDATE") {
            let mut parts = fake.splitn(4, ':');
            state.status = parts.next().unwrap_or("idle").to_string();
            state.percent = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
            state.version = parts.next().unwrap_or("").to_string();
            state.error = parts.next().unwrap_or("").to_string();
        }
        Self { state: Mutex::new(state) }
    }
    pub fn set(&self, state: UpdateState) {
        *self.state.lock().unwrap() = state;
    }
}

impl UpdateProvider for StubUpdater {
    fn state(&self) -> UpdateState {
        self.state.lock().unwrap().clone()
    }
    fn check(&self) {}
    fn install(&self) {}
}
