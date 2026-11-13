//! İstemci ve sunucu güncelleyicilerinin ortak parçası: arayüz durumu, olay → durum eşlemesi,
//! arka plan denetim/indirme iş parçacıkları ve durdurulabilir zamanlayıcılar.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::error::UpdateError;
use crate::updater::{CheckResult, Updater, UpdaterEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    #[default]
    Idle,
    Checking,
    Downloading,
    Ready,
    /// Yalnızca sunucu güncelleyicisi kullanır (API durdurulmuş, kurulum başlatılıyor).
    Installing,
}

/// Arayüze yayılan durum — Electron'daki `state` nesnesiyle aynı alanlar.
/// Sunucu konsolu: `{status, current, version, percent, error}`;
/// istemci ayarlar ekranı ayrıca `feed`, `startup`, `text` kullanır.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct UpdateState {
    pub status: Status,
    /// Çalışan sürüm.
    pub current: String,
    /// Bulunan/indirilen sürüm.
    pub version: String,
    /// İndirme yüzdesi (yuvarlanmış, 0–100).
    pub percent: u32,
    /// Sessiz olmayan son hata; yoksa boş.
    pub error: String,
    /// İstemci: güncelleme kaynağı adresi.
    pub feed: String,
    /// İstemci: açılış turu sürüyor (arayüz kendi ekranında ilerleme gösterir).
    pub startup: bool,
    /// İstemci: açılış turu metni.
    pub text: String,
}

pub type Broadcast = Arc<dyn Fn(&UpdateState) + Send + Sync>;

pub(crate) struct Core {
    state: Mutex<UpdateState>,
    broadcast: Broadcast,
    pub updater: Mutex<Option<Arc<Updater>>>,
    checking: AtomicBool,
}

impl Core {
    pub fn new(current: String, broadcast: Broadcast) -> Self {
        Self { state: Mutex::new(UpdateState { current, ..Default::default() }), broadcast, updater: Mutex::new(None), checking: AtomicBool::new(false) }
    }

    pub fn get(&self) -> UpdateState {
        self.state.lock().unwrap().clone()
    }

    /// `setState(patch)` + yayın. Kilit yayından önce bırakılır (geri çağrı `state()`
    /// okuyabilsin).
    pub fn set(&self, patch: impl FnOnce(&mut UpdateState)) -> UpdateState {
        let snapshot = {
            let mut state = self.state.lock().unwrap();
            patch(&mut state);
            state.clone()
        };
        (self.broadcast)(&snapshot);
        snapshot
    }

    /// Koşullu geçiş (ör. ready → installing): koşul tutarsa uygular ve yayınlar.
    pub fn transition(&self, allowed: impl FnOnce(&UpdateState) -> bool, patch: impl FnOnce(&mut UpdateState)) -> bool {
        let snapshot = {
            let mut state = self.state.lock().unwrap();
            if !allowed(&state) {
                return false;
            }
            patch(&mut state);
            state.clone()
        };
        (self.broadcast)(&snapshot);
        true
    }

    /// electron-updater olaylarının durum karşılığı (updater.cjs / server-updater.cjs `load`).
    pub fn apply(&self, event: &UpdaterEvent) {
        match event {
            UpdaterEvent::CheckingForUpdate => {
                self.set(|s| {
                    s.status = Status::Checking;
                    s.error.clear();
                });
            }
            UpdaterEvent::UpdateNotAvailable(_) => {
                self.set(|s| {
                    s.status = Status::Idle;
                    s.percent = 0;
                });
            }
            UpdaterEvent::UpdateAvailable(info) => {
                self.set(|s| {
                    s.status = Status::Downloading;
                    s.version = info.version.clone();
                    s.percent = 0;
                    s.error.clear();
                });
            }
            UpdaterEvent::DownloadProgress(progress) => {
                self.set(|s| {
                    s.status = Status::Downloading;
                    s.percent = progress.percent.round().clamp(0.0, 100.0) as u32;
                });
            }
            UpdaterEvent::UpdateDownloaded(done) => {
                self.set(|s| {
                    s.status = Status::Ready;
                    if !done.info.version.is_empty() {
                        s.version = done.info.version.clone();
                    }
                    s.percent = 100;
                });
            }
            UpdaterEvent::Error(error) => {
                let message = if error.is_quiet() { String::new() } else { error.message.clone() };
                self.set(|s| {
                    s.status = Status::Idle;
                    s.percent = 0;
                    s.error = message;
                });
            }
        }
    }

    pub fn current_updater(&self) -> Option<Arc<Updater>> {
        self.updater.lock().unwrap().clone()
    }

    /// Bloklayıcı denetim; güncelleme varsa indirme ayrı iş parçacığında başlar (electron-updater
    /// `autoDownload = true`). Aynı anda ikinci denetim yapılmaz (`None`).
    pub fn check_and_download(&self) -> Option<Result<CheckResult, UpdateError>> {
        let updater = self.current_updater()?;
        if self.checking.swap(true, Ordering::SeqCst) {
            return None;
        }
        let result = updater.check_for_updates();
        self.checking.store(false, Ordering::SeqCst);
        if matches!(&result, Ok(r) if r.is_update_available) {
            let _ = std::thread::Builder::new().name("p2-updater-download".into()).spawn(move || {
                let _ = updater.download_update();
            });
        }
        Some(result)
    }
}

/// Durdurulabilir zamanlayıcı (setInterval/setTimeout). Bırakıldığında (drop) durur; iş
/// parçacığı beklenmez, böylece kendi geri çağrısının içinden de bırakılabilir.
pub(crate) struct Ticker {
    stop: Arc<(Mutex<bool>, Condvar)>,
}

impl Ticker {
    /// `repeat` false ise tek sefer (setTimeout).
    pub fn start(delay: Duration, repeat: bool, name: &str, task: impl Fn() + Send + 'static) -> Self {
        let stop = Arc::new((Mutex::new(false), Condvar::new()));
        let flag = stop.clone();
        let _ = std::thread::Builder::new().name(name.into()).spawn(move || {
            loop {
                let deadline = Instant::now() + delay;
                let (lock, cvar) = &*flag;
                let mut stopped = lock.lock().unwrap();
                while !*stopped {
                    let now = Instant::now();
                    if now >= deadline {
                        break;
                    }
                    stopped = cvar.wait_timeout(stopped, deadline - now).unwrap().0;
                }
                if *stopped {
                    return;
                }
                drop(stopped);
                task();
                if !repeat {
                    return;
                }
            }
        });
        Self { stop }
    }
}

impl Drop for Ticker {
    fn drop(&mut self) {
        let (lock, cvar) = &*self.stop;
        *lock.lock().unwrap() = true;
        cvar.notify_all();
    }
}

/// `waitFor(test, timeout)`: 200 ms arayla koşulu yoklar.
pub(crate) fn wait_for(timeout: Duration, mut test: impl FnMut() -> bool) -> bool {
    let started = Instant::now();
    loop {
        if test() {
            return true;
        }
        if started.elapsed() > timeout {
            return false;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}
