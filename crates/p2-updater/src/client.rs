//! İstemci güncelleyicisi — `electron/updater.cjs` karşılığı.
//!
//! Güncelleme dosyaları kullanıcının zaten bağlı olduğu P2 sunucusundan gelir
//! (`GET <api>/updates/client/latest.yml`). Sessizce indirilir, hazır olunca arayüzde şerit
//! gösterilir; oyun/arama yokken 20 sn sonra kendiliğinden, ya da kullanıcı "yeniden başlat"
//! derse/uygulama kapanırken kurulur.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use crate::engine::{Broadcast, Core, Status, Ticker, UpdateState, wait_for};
use crate::feed;
use crate::install::{InstallerLauncher, NsisLauncher};
use crate::server::Hook;
use crate::updater::{Updater, UpdaterConfig, UpdaterEvent, default_cache_dir};

pub type Log = Arc<dyn Fn(&str) + Send + Sync>;
pub type IdleCheck = Arc<dyn Fn() -> bool + Send + Sync>;

#[derive(Clone)]
pub struct ClientUpdaterOptions {
    pub current_version: String,
    /// Kurulmuş (paketlenmiş) uygulama mı? Değilse hiçbir şey yapılmaz (`app.isPackaged`).
    pub packaged: bool,
    pub cache_dir: PathBuf,
    /// Kullanıcı verisi klasörü: `update-source.json` (son API adresi) ve `.updaterId` burada.
    pub data_dir: PathBuf,
    /// Yedek denetim aralığı (5 dk; asıl haber sunucunun `update:published` duyurusundan gelir).
    pub check_interval: Duration,
    /// İndirme bitince otomatik kurulum gecikmesi (20 sn).
    pub auto_install_delay: Duration,
    /// Bekleyen otomatik kurulum zamanlayıcısı yeniden kurulurken kullanılan gecikme (60 sn).
    pub auto_install_retry: Duration,
    /// Kurulumdan önce bekleme (800 ms; durdurulan süreçler dosyaları bıraksın).
    pub install_settle: Duration,
    pub launcher: Arc<dyn InstallerLauncher>,
    pub broadcast: Broadcast,
    pub log: Log,
    /// Kurulumdan önce: eski birleşik sunucu görevini/süreçlerini durdurma vb. (updater.cjs
    /// `stopServerTask` + `stopServerProcesses` + `askServerToStop` + `serverGone`).
    pub before_install: Option<Hook>,
    /// Kurulum başlatıldıktan sonra uygulamayı kapatır (`app.quit()`).
    pub quit: Hook,
}

impl ClientUpdaterOptions {
    pub fn new(current_version: impl Into<String>, data_dir: impl Into<PathBuf>) -> Self {
        Self {
            current_version: current_version.into(),
            packaged: true,
            cache_dir: default_cache_dir(),
            data_dir: data_dir.into(),
            check_interval: Duration::from_secs(5 * 60),
            auto_install_delay: Duration::from_secs(20),
            auto_install_retry: Duration::from_secs(60),
            install_settle: Duration::from_millis(800),
            launcher: Arc::new(NsisLauncher),
            broadcast: Arc::new(|_| {}),
            log: Arc::new(|_| {}),
            before_install: None,
            quit: Arc::new(|| {}),
        }
    }
}

/// `configure` başarısızlık nedenleri (updater.cjs'teki `reason` metinleri).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigureError {
    /// Geliştirme modu (`reason: 'dev'`).
    Dev,
    /// Adres http/https değil (`'adres yok'`).
    NoAddress,
    /// Kaynak ayarlanamadı (`'adres'`).
    Address,
}

impl ConfigureError {
    pub fn reason(&self) -> &'static str {
        match self {
            ConfigureError::Dev => "dev",
            ConfigureError::NoAddress => "adres yok",
            ConfigureError::Address => "adres",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupOutcome {
    /// Güncelleme yok/zamanında bitmedi: uygulama normal açılır.
    None,
    /// Kurulum başladı: uygulama açılmamalı (kapanıyor).
    Installing,
}

struct Inner {
    core: Core,
    options: ClientUpdaterOptions,
    timer: Mutex<Option<Ticker>>,
    auto_timer: Mutex<Option<Ticker>>,
    busy: AtomicBool,
    idle_check: Mutex<IdleCheck>,
    installing: AtomicBool,
}

#[derive(Clone)]
pub struct ClientUpdater {
    inner: Arc<Inner>,
}

impl ClientUpdater {
    pub fn new(options: ClientUpdaterOptions) -> Self {
        Self {
            inner: Arc::new(Inner {
                core: Core::new(options.current_version.clone(), options.broadcast.clone()),
                options,
                timer: Mutex::new(None),
                auto_timer: Mutex::new(None),
                busy: AtomicBool::new(false),
                idle_check: Mutex::new(Arc::new(|| true)),
                installing: AtomicBool::new(false),
            }),
        }
    }

    fn log(&self, message: &str) {
        (self.inner.options.log)(message);
    }

    pub fn state(&self) -> UpdateState {
        self.inner.core.get()
    }

    fn source_file(&self) -> PathBuf {
        self.inner.options.data_dir.join("update-source.json")
    }

    /// Açılışta hangi sunucuya bakılacağı (arayüz her bağlandığında kaydedilir).
    pub fn saved_base(&self) -> String {
        std::fs::read_to_string(self.source_file())
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(text.trim_start_matches('\u{feff}')).ok())
            .and_then(|value| value.get("base").and_then(|b| b.as_str()).map(str::to_string))
            .unwrap_or_default()
    }

    fn save_base(&self, base: &str) {
        let _ = std::fs::create_dir_all(&self.inner.options.data_dir);
        let _ = std::fs::write(self.source_file(), serde_json::json!({ "base": base }).to_string());
    }

    /// Arayüz hangi sunucuya bağlıysa güncelleme de oradan gelir. Kaynak değiştiyse hemen
    /// denetler ve 5 dakikalık turu yeniden kurar. Döner: kaynak adresi.
    pub fn configure(&self, api_base: &str) -> Result<String, ConfigureError> {
        self.configure_inner(api_base, true)
    }

    fn configure_inner(&self, api_base: &str, initial_check: bool) -> Result<String, ConfigureError> {
        if !self.inner.options.packaged {
            return Err(ConfigureError::Dev);
        }
        let Some(base) = feed::normalize_base(api_base) else { return Err(ConfigureError::NoAddress) };
        let feed_url = format!("{base}/updates/client/");
        self.save_base(&base);
        if self.state().feed == feed_url && self.inner.timer.lock().unwrap().is_some() {
            return Ok(feed_url);
        }
        let options = &self.inner.options;
        let mut config = UpdaterConfig::new(feed_url.clone(), options.current_version.clone(), options.cache_dir.clone());
        config.staging_id_file = Some(options.data_dir.join(".updaterId"));
        let weak = Arc::downgrade(&self.inner);
        let updater = match Updater::new(config, Arc::new(move |event: &UpdaterEvent| on_event(&weak, event))) {
            Ok(updater) => updater,
            Err(error) => {
                self.log(&format!("Güncelleme adresi ayarlanamadı: {error}"));
                return Err(ConfigureError::Address);
            }
        };
        *self.inner.core.updater.lock().unwrap() = Some(Arc::new(updater));
        self.inner.core.set(|s| s.feed = feed_url.clone());
        self.log(&format!("Güncelleme kaynağı: {feed_url}"));
        if initial_check {
            self.check();
        }
        let weak = Arc::downgrade(&self.inner);
        let ticker = Ticker::start(options.check_interval, true, "p2-updater-client-timer", move || {
            if let Some(inner) = weak.upgrade() {
                ClientUpdater { inner }.check_blocking();
            }
        });
        *self.inner.timer.lock().unwrap() = Some(ticker);
        Ok(feed_url)
    }

    fn can_check(&self) -> bool {
        let state = self.state();
        self.inner.options.packaged && !state.feed.is_empty() && !matches!(state.status, Status::Downloading | Status::Ready)
    }

    /// `check()`: indirme sürüyorsa ya da kurulum hazırsa anlamsız; değilse arka planda denetler.
    /// Sunucunun `update:published` duyurusu geldiğinde de bu çağrılır.
    pub fn check(&self) -> UpdateState {
        if self.can_check() {
            let this = self.clone();
            let _ = std::thread::Builder::new().name("p2-updater-check".into()).spawn(move || {
                this.check_blocking();
            });
        }
        self.state()
    }

    /// Bloklayıcı denetim (indirme arka planda sürer).
    pub fn check_blocking(&self) -> UpdateState {
        if self.can_check()
            && let Some(Err(error)) = self.inner.core.check_and_download()
        {
            self.log(&format!("Kontrol başarısız: {error}"));
        }
        self.state()
    }

    /// Oyun (overlay'li) ya da arama sürüyor mu — sürüyorsa otomatik kurulum ertelenir.
    pub fn set_busy(&self, busy: bool) {
        self.inner.busy.store(busy, Ordering::SeqCst);
    }

    pub fn set_idle_check(&self, check: IdleCheck) {
        *self.inner.idle_check.lock().unwrap() = check;
    }

    /// updater.cjs `scheduleAutoInstall`. Gecikme: bekleyen zamanlayıcı yokken 20 sn, varken
    /// (yeniden kurulurken) 60 sn. Not: Electron'da meşgulken yeniden deneme de geri çağrının
    /// içinden yapıldığı için (o anda bekleyen zamanlayıcı yok) fiilen 20 sn arayla olur; aynısı
    /// korunmuştur.
    fn schedule_auto_install(&self) {
        let mut slot = self.inner.auto_timer.lock().unwrap();
        let had_pending = slot.take().is_some();
        let delay = if self.state().status == Status::Ready && !had_pending { self.inner.options.auto_install_delay } else { self.inner.options.auto_install_retry };
        let weak = Arc::downgrade(&self.inner);
        *slot = Some(Ticker::start(delay, false, "p2-updater-auto-install", move || {
            let Some(inner) = weak.upgrade() else { return };
            let this = ClientUpdater { inner };
            // Geri çağrı başında zamanlayıcı boşaltılır (JS: autoTimer = null).
            drop(this.inner.auto_timer.lock().unwrap().take());
            if this.state().status != Status::Ready {
                return;
            }
            let idle_check = this.inner.idle_check.lock().unwrap().clone();
            let idle = !this.inner.busy.load(Ordering::SeqCst) && std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| idle_check())).unwrap_or(false);
            if !idle {
                this.log("Güncelleme hazır; oyun/arama sürdüğü için kurulum erteleniyor");
                this.schedule_auto_install();
                return;
            }
            this.log("Güncelleme hazır; kendiliğinden kuruluyor");
            this.install();
        }));
    }

    /// Kullanıcı "yeniden başlat" dedi ya da otomatik kurulum: `before_install` kancası, kısa
    /// bekleme, sessiz kurulum (`--updated /S --force-run`), uygulamayı kapatma. Bloklayıcıdır.
    pub fn install(&self) -> bool {
        if self.state().status != Status::Ready || self.inner.core.current_updater().is_none() || self.inner.installing.swap(true, Ordering::SeqCst) {
            return false;
        }
        self.log("Güncelleme kuruluyor: sunucu durduruluyor");
        if let Some(hook) = &self.inner.options.before_install {
            hook();
        }
        std::thread::sleep(self.inner.options.install_settle);
        let Some(updater) = self.inner.core.current_updater() else {
            self.inner.installing.store(false, Ordering::SeqCst);
            return false;
        };
        match updater.quit_and_install(true, true, self.inner.options.launcher.as_ref()) {
            Ok(true) => {
                (self.inner.options.quit)();
                true
            }
            Ok(false) => true,
            Err(error) => {
                self.inner.installing.store(false, Ordering::SeqCst);
                self.log(&format!("Kurulum başlatılamadı: {error}"));
                false
            }
        }
    }

    /// Kapanış kancası: hazır güncelleme varsa sessizce kurulur ve uygulama yeniden açılır.
    /// Uygulama zaten kapanmakta olduğu için `quit` kancası çağrılmaz. Kurulum başladıysa true.
    pub fn install_if_ready(&self) -> bool {
        if self.inner.installing.load(Ordering::SeqCst) || self.state().status != Status::Ready {
            return false;
        }
        let Some(updater) = self.inner.core.current_updater() else { return false };
        self.inner.installing.store(true, Ordering::SeqCst);
        match updater.quit_and_install(true, true, self.inner.options.launcher.as_ref()) {
            Ok(_) => true,
            Err(error) => {
                self.inner.installing.store(false, Ordering::SeqCst);
                self.log(&format!("Kapanışta kurulum başlatılamadı: {error}"));
                false
            }
        }
    }

    /// Açılış turu: pencere açılmadan önce denetler, güncelleme varsa indirir ve kurar. Her
    /// adımda süre sınırı var (denetim 12 sn; indirme 90 sn ilerlemezse ya da 30 dk'yı aşarsa
    /// vazgeçilir, güncelleme arka planda sürer). Bloklayıcıdır.
    pub fn startup_check(&self, api_base: &str) -> StartupOutcome {
        if !self.inner.options.packaged {
            return StartupOutcome::None;
        }
        let core = &self.inner.core;
        let finish = |outcome| {
            core.set(|s| {
                s.startup = false;
                s.text.clear();
            });
            outcome
        };
        core.set(|s| {
            s.startup = true;
            s.text = p2_i18n::t("upd2.checking");
        });
        if self.configure_inner(api_base, false).is_err() {
            return finish(StartupOutcome::None);
        }
        if matches!(self.state().status, Status::Idle | Status::Checking) {
            match core.check_and_download() {
                Some(Err(error)) => {
                    self.log(&format!("Açılış denetimi başarısız: {error}"));
                    return finish(StartupOutcome::None);
                }
                Some(Ok(result)) if !result.is_update_available => return finish(StartupOutcome::None),
                _ => {}
            }
        }
        // Güncelleme yoksa ya da denetim 12 saniyede sonuçlanmazsa beklemeden devam.
        let found = wait_for(Duration::from_secs(12), || matches!(self.state().status, Status::Downloading | Status::Ready));
        if !found {
            return finish(StartupOutcome::None);
        }
        core.set(|s| s.text = p2_i18n::t("upd2.downloading"));
        let mut last_percent = u32::MAX;
        let mut last_move = Instant::now();
        let progressing = wait_for(Duration::from_secs(30 * 60), || {
            let state = self.state();
            if state.percent != last_percent {
                last_percent = state.percent;
                last_move = Instant::now();
            }
            // İndirme 90 saniye boyunca ilerlemediyse ya da hata verdiyse uygulamayı bekletmeye değmez.
            last_move.elapsed() > Duration::from_secs(90) || matches!(state.status, Status::Ready | Status::Idle)
        });
        if !progressing || self.state().status != Status::Ready {
            return finish(StartupOutcome::None);
        }
        core.set(|s| s.text = p2_i18n::t("upd2.installing"));
        if self.install() { StartupOutcome::Installing } else { finish(StartupOutcome::None) }
    }

    /// Zamanlayıcıları durdurur ve süren indirmeyi iptal eder.
    pub fn stop(&self) {
        self.inner.timer.lock().unwrap().take();
        self.inner.auto_timer.lock().unwrap().take();
        if let Some(updater) = self.inner.core.current_updater() {
            updater.cancel();
        }
    }
}

fn on_event(weak: &Weak<Inner>, event: &UpdaterEvent) {
    let Some(inner) = weak.upgrade() else { return };
    if let UpdaterEvent::Error(error) = event {
        (inner.options.log)(&format!("Güncelleme hatası: {error}"));
    }
    inner.core.apply(event);
    if let UpdaterEvent::UpdateDownloaded(_) = event {
        ClientUpdater { inner }.schedule_auto_install();
    }
}
