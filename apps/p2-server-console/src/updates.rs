//! p2-updater bağlantısı (`serverUpdates.init(...)` karşılığı).
//!
//! - `stop_server`: konsolun durdurma işlevi, selfStop kipinde (imzalı `POST /updates/stop`).
//! - `before_quit`: kapanış bayrağı; `quit`: uygulamayı kapatır (ana döngü bayrağı görür).
//! - `broadcast`: arayüz güncelleme katmanını yeniden çizsin diye değişiklik sayacını artırır.
//!
//! Otomatik pilotta (`P2_CONSOLE_AUTOPILOT`) gerçek kurucu asla çalıştırılmaz: sahte başlatıcı yalnız iz yazar.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use p2_server_ops::{Ops, UpdateProvider, UpdateState};
use p2_updater::{InstallerLauncher, ServerUpdater, ServerUpdaterOptions, Status};

/// Kurulum için uygulamanın kapanması istendi (`app.quit()`); ana döngü okur.
pub static QUIT: AtomicBool = AtomicBool::new(false);
/// `quitting` (kapanış sorusu yok; Rust konsolu zaten sormuyor, iz için tutulur).
pub static QUITTING: AtomicBool = AtomicBool::new(false);

pub struct Provider(pub ServerUpdater);

fn status(s: Status) -> &'static str {
    match s {
        Status::Idle => "idle",
        Status::Checking => "checking",
        Status::Downloading => "downloading",
        Status::Ready => "ready",
        Status::Installing => "installing",
    }
}

impl UpdateProvider for Provider {
    fn state(&self) -> UpdateState {
        let s = self.0.state();
        UpdateState { status: status(s.status).into(), current: s.current, version: s.version, percent: s.percent, error: s.error }
    }
    fn check(&self) {
        self.0.check();
    }
    fn install(&self) {
        let updater = self.0.clone();
        std::thread::spawn(move || {
            updater.install();
        });
    }
}

/// Kurucuyu çalıştırmayan başlatıcı (testler).
struct DryLauncher;
impl InstallerLauncher for DryLauncher {
    fn launch(&self, installer: &Path, args: &[String], elevate: bool) -> std::io::Result<()> {
        crate::trace(&format!("güncelleme (kuru): {} {args:?} yönetici={elevate}", installer.display()));
        Ok(())
    }
}

/// Güncelleyiciyi başlatır ve konsola bağlar. Port: yapılandırılmış API portu; sürüm: ürün sürümü.
pub fn start(ops: &Ops) {
    if std::env::var_os("P2_CONSOLE_FAKE_UPDATE").is_some() {
        return; // StubUpdater sabit durumu gösteriyor
    }
    let mut options = ServerUpdaterOptions::new(ops.port(), ops.version().to_string());
    if std::env::var_os("P2_CONSOLE_AUTOPILOT").is_some() {
        options.launcher = Arc::new(DryLauncher);
        options.cache_dir = ops.data_dir().join(".updater-cache");
    }
    let stop_ops = ops.clone();
    options.stop_server = Some(Arc::new(move || {
        let r = stop_ops.stop(true);
        if r.ok { Ok(()) } else { Err(r.message.unwrap_or_default()) }
    }));
    options.before_quit = Some(Arc::new(|| QUITTING.store(true, Ordering::SeqCst)));
    options.quit = Arc::new(|| QUIT.store(true, Ordering::SeqCst));
    let notify = ops.clone();
    options.broadcast = Arc::new(move |_| notify.broadcast());
    match ServerUpdater::start(options) {
        Ok(updater) => ops.set_updater(Arc::new(Provider(updater))),
        Err(error) => crate::trace(&format!("güncelleyici başlatılamadı: {error}")),
    }
}
