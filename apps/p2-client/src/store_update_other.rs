//! Windows dışı: Microsoft Store paketi yok; `store_update.rs` ile aynı arayüz, hepsi etkisiz.

use p2_updater::UpdateState;

#[allow(dead_code)]
pub const FEED: &str = "";
#[allow(dead_code)]
pub const EXECUTION_ALIAS: &str = "";

pub fn is_packaged() -> bool {
    false
}

#[allow(dead_code)]
pub fn local_state_dir() -> Option<std::path::PathBuf> {
    None
}

pub fn ensure_desktop_shortcut() -> Option<std::thread::JoinHandle<()>> {
    None
}

pub fn state() -> UpdateState {
    UpdateState::default()
}

pub fn start() {}

pub fn check() {}

pub fn install() {}
