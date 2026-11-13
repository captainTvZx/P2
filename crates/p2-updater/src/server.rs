//! Sunucu konsolu güncelleyicisi — `electron/server-updater.cjs` karşılığı.
//!
//! Onay istemez: indirme biter bitmez API durdurulur, kurulum sessizce yapılır ve P2 Server
//! yeniden açılır. Konsol yalnızca durumu gösterir.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use crate::engine::{Broadcast, Core, Status, Ticker, UpdateState};
use crate::error::UpdateError;
use crate::feed;
use crate::install::{InstallerLauncher, NsisLauncher};
use crate::updater::{Updater, UpdaterConfig, UpdaterEvent, default_cache_dir};

/// API'yi durdurur (`stopServer({ selfStop: true })`). Hata metni arayüzde gösterilir.
pub type StopServer = Arc<dyn Fn() -> Result<(), String> + Send + Sync>;
pub type Hook = Arc<dyn Fn() + Send + Sync>;

#[derive(Clone)]
pub struct ServerUpdaterOptions {
    /// Bu makinedeki API portu; kaynak `http://127.0.0.1:<port>/updates/server/`.
    pub port: u16,
    pub current_version: String,
    pub cache_dir: PathBuf,
    /// Kademeli yayın kimliği (Electron: `userData/.updaterId`); `None`: yok sayılır.
    pub staging_id_file: Option<PathBuf>,
    /// Yedek denetim aralığı (5 dk). İndirilmiş ama kurulamamış güncelleme her turda yeniden
    /// kurulmaya çalışılır.
    pub check_interval: Duration,
    pub launcher: Arc<dyn InstallerLauncher>,
    pub broadcast: Broadcast,
    /// API'yi durdurur; tanımlı değilse kurulum yapılmaz.
    pub stop_server: Option<StopServer>,
    /// Konsolun "kapatılsın mı?" sorusunu kapatır (pencere doğrudan kapanmalı).
    pub before_quit: Option<Hook>,
    /// Kurulum başlatıldıktan sonra uygulamayı kapatır (`app.quit()`).
    pub quit: Hook,
}

impl ServerUpdaterOptions {
    pub fn new(port: u16, current_version: impl Into<String>) -> Self {
        Self {
            port,
            current_version: current_version.into(),
            cache_dir: default_cache_dir(),
            staging_id_file: None,
            check_interval: Duration::from_secs(5 * 60),
            launcher: Arc::new(NsisLauncher),
            broadcast: Arc::new(|_| {}),
            stop_server: None,
            before_quit: None,
            quit: Arc::new(|| {}),
        }
    }
}

struct Inner {
    core: Core,
    options: Mutex<ServerUpdaterOptions>,
    timer: Mutex<Option<Ticker>>,
}

#[derive(Clone)]
pub struct ServerUpdater {
    inner: Arc<Inner>,
}

impl ServerUpdater {
    /// `init(options)`: kaynağı ayarlar, hemen arka planda denetler ve 5 dakikalık turu başlatır.
    /// (Electron'daki `app.isPackaged` denetimi çağıranın işidir: geliştirmede çağırmayın.)
    pub fn start(options: ServerUpdaterOptions) -> Result<Self, UpdateError> {
        let inner = Arc::new(Inner {
            core: Core::new(options.current_version.clone(), options.broadcast.clone()),
            options: Mutex::new(options.clone()),
            timer: Mutex::new(None),
        });
        let mut config = UpdaterConfig::new(feed::server_feed_url(options.port), options.current_version.clone(), options.cache_dir.clone());
        config.staging_id_file = options.staging_id_file.clone();
        let weak = Arc::downgrade(&inner);
        let updater = Updater::new(config, Arc::new(move |event: &UpdaterEvent| on_event(&weak, event)))?;
        *inner.core.updater.lock().unwrap() = Some(Arc::new(updater));
        let this = Self { inner };
        this.check();
        let weak = Arc::downgrade(&this.inner);
        let ticker = Ticker::start(options.check_interval, true, "p2-updater-server-timer", move || {
            let Some(inner) = weak.upgrade() else { return };
            let this = ServerUpdater { inner };
            if this.state().status == Status::Ready {
                this.install();
            } else {
                this.check_blocking();
            }
        });
        *this.inner.timer.lock().unwrap() = Some(ticker);
        Ok(this)
    }

    pub fn state(&self) -> UpdateState {
        self.inner.core.get()
    }

    /// Kancaları sonradan değiştirmek için (ör. konsol penceresi hazır olunca).
    pub fn set_hooks(&self, stop_server: Option<StopServer>, before_quit: Option<Hook>, quit: Option<Hook>) {
        let mut options = self.inner.options.lock().unwrap();
        if stop_server.is_some() {
            options.stop_server = stop_server;
        }
        if before_quit.is_some() {
            options.before_quit = before_quit;
        }
        if let Some(quit) = quit {
            options.quit = quit;
        }
    }

    fn busy(&self) -> bool {
        matches!(self.state().status, Status::Downloading | Status::Ready | Status::Installing)
    }

    /// `check()`: indirme sürüyor/hazır/kuruluyorsa bir şey yapmaz; değilse arka planda denetler.
    /// Hemen döner; güncel durumu verir.
    pub fn check(&self) -> UpdateState {
        if !self.busy() {
            let this = self.clone();
            let _ = std::thread::Builder::new().name("p2-updater-check".into()).spawn(move || this.check_blocking());
        }
        self.state()
    }

    /// Bloklayıcı denetim (indirme yine arka planda sürer).
    pub fn check_blocking(&self) -> UpdateState {
        if !self.busy() {
            let _ = self.inner.core.check_and_download();
        }
        self.state()
    }

    /// `install()`: yalnız `ready` iken. API durdurulur (başarısızsa `ready` + hata), konsol
    /// kapanış sorusu kapatılır, kurulum `--updated /S --force-run` ile başlatılır, uygulama
    /// kapatılır. Bloklayıcıdır.
    pub fn install(&self) -> bool {
        let stop_server = self.inner.options.lock().unwrap().stop_server.clone();
        self.install_with(stop_server)
    }

    /// `install(stopServer)` — durdurma işlevi çağıran tarafından verilebilir.
    pub fn install_with(&self, stop_server: Option<StopServer>) -> bool {
        let core = &self.inner.core;
        if core.get().status != Status::Ready || core.current_updater().is_none() {
            return false;
        }
        let Some(stop_server) = stop_server else {
            core.set(|s| s.error = "Sunucu durdurma işlevi tanımlı değil.".into());
            return false;
        };
        if !core.transition(
            |s| s.status == Status::Ready,
            |s| {
                s.status = Status::Installing;
                s.error.clear();
            },
        ) {
            return false;
        }
        if let Err(message) = stop_server() {
            let message = if message.is_empty() { "Sunucu durdurulamadı.".to_string() } else { message };
            core.set(|s| {
                s.status = Status::Ready;
                s.error = message;
            });
            return false;
        }
        let (launcher, before_quit, quit) = {
            let options = self.inner.options.lock().unwrap();
            (options.launcher.clone(), options.before_quit.clone(), options.quit.clone())
        };
        if let Some(before_quit) = before_quit {
            before_quit();
        }
        let Some(updater) = core.current_updater() else { return false };
        // Başlatılamazsa `Error` olayı durumu idle + hata yapar (Electron'da da dispatchError);
        // sonraki turda önbellekteki dosya yeniden bulunup kurulum tekrar denenir.
        match updater.quit_and_install(true, true, launcher.as_ref()) {
            Ok(true) => {
                quit();
                true
            }
            _ => false,
        }
    }

    /// Zamanlayıcıyı durdurur ve süren indirmeyi iptal eder (uygulama kapanırken).
    pub fn stop(&self) {
        self.inner.timer.lock().unwrap().take();
        if let Some(updater) = self.inner.core.current_updater() {
            updater.cancel();
        }
    }
}

fn on_event(weak: &Weak<Inner>, event: &UpdaterEvent) {
    let Some(inner) = weak.upgrade() else { return };
    inner.core.apply(event);
    // server-updater.cjs: `update-downloaded` → `void install()`.
    if let UpdaterEvent::UpdateDownloaded(_) = event {
        let this = ServerUpdater { inner };
        let _ = std::thread::Builder::new().name("p2-updater-install".into()).spawn(move || {
            this.install();
        });
    }
}
