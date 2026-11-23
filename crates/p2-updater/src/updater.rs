//! Düşük düzey güncelleyici — electron-updater `NsisUpdater` + `GenericProvider` karşılığı.
//! Tüm çağrılar bloklayıcıdır; arayüz iş parçacığından değil, arka plan iş parçacığından
//! çağrılmalıdır (üst düzey [`crate::ServerUpdater`]/[`crate::ClientUpdater`] bunu kendisi yapar).

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use reqwest::StatusCode;
use reqwest::blocking::Client;
use reqwest::header::{CONTENT_LENGTH, CONTENT_RANGE, RANGE};
use semver::Version;
use serde::{Deserialize, Serialize};

use crate::error::{ErrorKind, UpdateError, from_reqwest};
use crate::feed::{self, ResolvedFile};
use crate::hash;
use crate::install::{InstallerLauncher, nsis_install_args};
use crate::version;
use crate::yml::{self, UpdateInfo};

/// electron-builder `updaterCacheDirName` — package.json adı `p2-desktop` olduğu için hem
/// istemci hem sunucu kurulumu `%LOCALAPPDATA%\p2-desktop-updater` kullanır.
pub const CACHE_DIR_NAME: &str = "p2-desktop-updater";

/// electron-updater `getAppCacheDir()` + `updaterCacheDirName`.
pub fn default_cache_dir() -> PathBuf {
    #[cfg(not(windows))]
    {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        let base = if cfg!(target_os = "macos") { home.join("Library").join("Caches") } else { home.join(".cache") };
        return base.join(CACHE_DIR_NAME);
    }
    #[allow(unreachable_code)]
    let base = std::env::var_os("LOCALAPPDATA")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default().join("AppData").join("Local"));
    base.join(CACHE_DIR_NAME)
}

#[derive(Debug, Clone)]
pub struct UpdaterConfig {
    /// Generic sağlayıcı adresi (ör. `http://127.0.0.1:4000/updates/server/`).
    pub feed_url: String,
    /// Kanal (`latest` → `latest.yml`).
    pub channel: String,
    /// Çalışan sürüm.
    pub current_version: String,
    /// Önbellek kökü; indirmeler `<cache_dir>/pending` altına iner.
    pub cache_dir: PathBuf,
    /// Eski sürüme dönüşe izin (electron-updater varsayılanı false; P2 de kullanmıyor).
    pub allow_downgrade: bool,
    /// Kademeli yayın için kalıcı kullanıcı kimliği dosyası (Electron: `userData/.updaterId`).
    /// `None` ise latest.yml'deki `stagingPercentage` yok sayılır (herkes dahil).
    pub staging_id_file: Option<PathBuf>,
    /// Bağlantı kurma süresi sınırı.
    pub connect_timeout: Duration,
    /// Okuma sessizlik sınırı (builder-util-runtime: 60 sn soket zaman aşımı).
    pub read_timeout: Duration,
    /// İndirme denemesi (ağ hatası/5xx'te kalınan yerden sürdürülür).
    pub download_attempts: u32,
    /// Kanal dosyasına `?noCache=` eklensin mi (electron-updater: yetki başlığı yoksa evet).
    pub add_no_cache_query: bool,
    /// İlerleme olayları arasındaki en kısa süre (builder-util-runtime: 1 sn).
    pub progress_interval: Duration,
}

impl UpdaterConfig {
    pub fn new(feed_url: impl Into<String>, current_version: impl Into<String>, cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            feed_url: feed_url.into(),
            channel: "latest".into(),
            current_version: current_version.into(),
            cache_dir: cache_dir.into(),
            allow_downgrade: false,
            staging_id_file: None,
            connect_timeout: Duration::from_secs(20),
            read_timeout: Duration::from_secs(60),
            download_attempts: 3,
            add_no_cache_query: true,
            progress_interval: Duration::from_secs(1),
        }
    }
}

/// electron-updater `ProgressInfo`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub total: u64,
    pub delta: u64,
    pub transferred: u64,
    pub percent: f64,
    pub bytes_per_second: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DownloadedUpdate {
    pub info: UpdateInfo,
    /// Kurulum dosyası (`<cache>/pending/<ad>.exe`).
    pub file: PathBuf,
    pub sha512: Option<String>,
    pub is_admin_rights_required: bool,
    /// Önceki bir oturumda indirilmiş ve doğrulanmış dosya yeniden kullanıldı.
    pub from_cache: bool,
    /// Yarım kalmış indirme bu bayttan sürdürüldü (0: baştan).
    pub resumed_from: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UpdaterEvent {
    CheckingForUpdate,
    UpdateAvailable(UpdateInfo),
    UpdateNotAvailable(UpdateInfo),
    DownloadProgress(Progress),
    UpdateDownloaded(DownloadedUpdate),
    /// Electron'daki `error` olayı: denetim/indirme/kurulum hatası (iptal hariç).
    Error(UpdateError),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CheckResult {
    pub is_update_available: bool,
    pub update_info: UpdateInfo,
}

pub type Listener = Arc<dyn Fn(&UpdaterEvent) + Send + Sync>;

#[derive(Default)]
struct Slots {
    available: Option<UpdateInfo>,
    downloaded: Option<DownloadedUpdate>,
    /// electron-updater `DownloadedUpdateHelper.fileInfo` (aynı oturumda yeniden kullanım için).
    downloaded_file: Option<ResolvedFile>,
}

/// `pending/update-info.json` — electron-updater ile aynı biçim (Electron sürümünün önbelleği
/// geçişte yeniden kullanılabilsin).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedInfo {
    file_name: Option<String>,
    sha512: Option<String>,
    #[serde(default)]
    is_admin_rights_required: bool,
}

/// Yarım indirmenin kime ait olduğu (`temp-<ad>.json`): özet değişmişse baştan indirilir.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct PartialMarker {
    url: String,
    sha512: Option<String>,
    sha2: Option<String>,
}

pub struct Updater {
    config: UpdaterConfig,
    current: Version,
    base: reqwest::Url,
    http: Client,
    listener: Listener,
    slots: Mutex<Slots>,
    /// electron-updater `downloadPromise`: aynı anda tek indirme.
    download_lock: Mutex<()>,
    install_called: AtomicBool,
    cancelled: AtomicBool,
}

impl Updater {
    pub fn new(config: UpdaterConfig, listener: Listener) -> Result<Self, UpdateError> {
        let Some(current) = version::parse_version(&config.current_version) else {
            return Err(UpdateError::new(ErrorKind::InvalidVersion, format!("App version is not a valid semver version: \"{}\"", config.current_version)));
        };
        let base = feed::base_url(&config.feed_url)?;
        let http = Client::builder()
            .connect_timeout(config.connect_timeout)
            // Bloklayıcı istemcide bu süre her okuma/yazma işlemine ayrı uygulanır (toplam değil).
            .timeout(config.read_timeout)
            .user_agent(concat!("p2-updater/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| UpdateError::new(ErrorKind::Network, format!("net::ERR_FAILED: HTTP istemcisi kurulamadı: {e}")))?;
        Ok(Self {
            config,
            current,
            base,
            http,
            listener,
            slots: Mutex::new(Slots::default()),
            download_lock: Mutex::new(()),
            install_called: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
        })
    }

    pub fn config(&self) -> &UpdaterConfig {
        &self.config
    }

    pub fn current_version(&self) -> &Version {
        &self.current
    }

    pub fn pending_dir(&self) -> PathBuf {
        self.config.cache_dir.join("pending")
    }

    fn emit(&self, event: UpdaterEvent) {
        (self.listener)(&event);
    }

    /// electron-updater `checkForUpdates` (autoDownload kapalıymış gibi; indirme ayrı çağrılır).
    /// Hata hem döndürülür hem `Error` olayı olarak yayılır.
    pub fn check_for_updates(&self) -> Result<CheckResult, UpdateError> {
        self.emit(UpdaterEvent::CheckingForUpdate);
        let result = self.do_check();
        if let Err(error) = &result {
            self.emit(UpdaterEvent::Error(error.clone()));
        }
        result
    }

    fn do_check(&self) -> Result<CheckResult, UpdateError> {
        let info = self.fetch_update_info()?;
        if !self.is_update_available(&info)? {
            self.emit(UpdaterEvent::UpdateNotAvailable(info.clone()));
            return Ok(CheckResult { is_update_available: false, update_info: info });
        }
        self.slots.lock().unwrap().available = Some(info.clone());
        self.emit(UpdaterEvent::UpdateAvailable(info.clone()));
        Ok(CheckResult { is_update_available: true, update_info: info })
    }

    /// `GenericProvider.getLatestVersion`: 404 → kanal bulunamadı; bağlantı reddi 3 kez
    /// (0, 1, 2 sn arayla) yeniden denenir.
    pub fn fetch_update_info(&self) -> Result<UpdateInfo, UpdateError> {
        let file = feed::channel_file(&self.config.channel);
        let url = feed::url_from_base(&file, &self.base, self.config.add_no_cache_query)?;
        let mut attempt = 0u64;
        loop {
            match self.http.get(url.clone()).send() {
                Ok(response) => {
                    let status = response.status();
                    if status == StatusCode::NOT_FOUND {
                        return Err(UpdateError::new(ErrorKind::HttpStatus(404), format!("Cannot find channel \"{file}\" update info: HttpError: 404 Not Found ({url})")));
                    }
                    if status.as_u16() >= 400 {
                        return Err(UpdateError::new(ErrorKind::HttpStatus(status.as_u16()), format!("HttpError: {status} ({url})")));
                    }
                    let text = response.text().map_err(|e| from_reqwest(url.as_str(), &e))?;
                    return yml::parse_update_info(&text).map_err(|e| UpdateError::new(e.kind, format!("{} ({url})", e.message)));
                }
                Err(error) if error.is_connect() && attempt < 3 => {
                    std::thread::sleep(Duration::from_millis(1000 * attempt));
                    attempt += 1;
                }
                Err(error) => return Err(from_reqwest(url.as_str(), &error)),
            }
        }
    }

    /// `isUpdateAvailable`: eşit değil + kademeli yayına dahil + yeni (ya da izinli eski).
    fn is_update_available(&self, info: &UpdateInfo) -> Result<bool, UpdateError> {
        let Some(latest) = version::parse_version(&info.version) else {
            return version::is_newer_available(&self.current, &info.version, self.config.allow_downgrade);
        };
        if version::compare(&latest, &self.current).is_eq() {
            return Ok(false);
        }
        if !self.within_rollout(info) {
            return Ok(false);
        }
        version::is_newer_available(&self.current, &info.version, self.config.allow_downgrade)
    }

    /// `isStagingMatch`: kimliğin son 4 baytı / 0xFFFFFFFF < yüzde/100.
    fn within_rollout(&self, info: &UpdateInfo) -> bool {
        let (Some(raw), Some(id_file)) = (&info.staging_percentage, &self.config.staging_id_file) else { return true };
        let digits: String = raw.trim().chars().take_while(|c| c.is_ascii_digit() || *c == '-').collect();
        let Ok(percentage) = digits.parse::<i64>() else { return true };
        let id = staging_user_id(id_file);
        let bytes = id.as_bytes();
        let value = u32::from_be_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
        (value as f64 / u32::MAX as f64) < percentage as f64 / 100.0
    }

    /// Son denetimde bulunan güncelleme.
    pub fn available(&self) -> Option<UpdateInfo> {
        self.slots.lock().unwrap().available.clone()
    }

    /// İndirilip doğrulanmış güncelleme.
    pub fn downloaded(&self) -> Option<DownloadedUpdate> {
        self.slots.lock().unwrap().downloaded.clone()
    }

    /// Süren indirmeyi durdurur (yarım dosya sürdürülmek üzere saklanır).
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// electron-updater `downloadUpdate`: önbellekte doğrulanmış dosya varsa indirmeden
    /// `UpdateDownloaded`; yoksa `pending/temp-<ad>` dosyasına indirir, özeti doğrular, adını
    /// değiştirir ve `update-info.json` yazar. Hata (iptal hariç) `Error` olayı olarak da yayılır.
    pub fn download_update(&self) -> Result<DownloadedUpdate, UpdateError> {
        let _single = self.download_lock.lock().unwrap_or_else(|e| e.into_inner());
        self.cancelled.store(false, Ordering::SeqCst);
        let Some(info) = self.available() else {
            let error = UpdateError::new(ErrorKind::Install, "Please check update first");
            self.emit(UpdaterEvent::Error(error.clone()));
            return Err(error);
        };
        let result = self.do_download(&info);
        if let Err(error) = &result
            && error.kind != ErrorKind::Cancelled
        {
            self.emit(UpdaterEvent::Error(error.clone()));
        }
        result
    }

    fn do_download(&self, info: &UpdateInfo) -> Result<DownloadedUpdate, UpdateError> {
        let files = feed::resolve_files(info, &self.base)?;
        let file = feed::find_file(&files, "exe").ok_or_else(|| UpdateError::new(ErrorKind::NoFiles, "No files provided"))?;
        let pending = self.pending_dir();
        fs::create_dir_all(&pending).map_err(|e| UpdateError::io("Önbellek klasörü oluşturulamadı", e))?;
        let name = feed::cache_file_name(&file, "exe");
        let update_file = pending.join(&name);

        // Aynı oturumda indirilmiş: yalnızca varlık denetlenir (electron-updater gibi).
        {
            let slots = self.slots.lock().unwrap();
            if let (Some(done), Some(done_file)) = (&slots.downloaded, &slots.downloaded_file)
                && done.file == update_file
            {
                if &done.info == info && done_file.info == file.info && update_file.exists() {
                    let again = DownloadedUpdate { from_cache: true, resumed_from: 0, ..done.clone() };
                    drop(slots);
                    return Ok(self.finish(again, file, false));
                }
            } else if let Some(cached) = self.valid_cached_file(&file) {
                drop(slots);
                let done = DownloadedUpdate {
                    info: info.clone(),
                    file: cached.0,
                    sha512: file.info.sha512.clone(),
                    is_admin_rights_required: cached.1,
                    from_cache: true,
                    resumed_from: 0,
                };
                return Ok(self.finish(done, file, false));
            }
        }

        let temp = pending.join(format!("temp-{name}"));
        let marker = pending.join(format!("temp-{name}.json"));
        let resumed_from = match self.fetch_to_temp(&file, &temp, &marker) {
            Ok(resumed) => resumed,
            Err(error) => {
                if error.kind != ErrorKind::Cancelled && !matches!(error.kind, ErrorKind::Network) && !is_server_error(&error) {
                    // Kalıcı hata: yarım dosya da atılır (electron-updater `removeFileIfAny`).
                    let _ = fs::remove_file(&temp);
                    let _ = fs::remove_file(&marker);
                }
                return Err(error);
            }
        };
        if let Err(error) = hash::verify_file(&temp, file.info.sha512.as_deref(), file.info.sha2.as_deref()) {
            let _ = fs::remove_file(&temp);
            let _ = fs::remove_file(&marker);
            return Err(error);
        }
        rename_with_retry(&temp, &update_file)?;
        let _ = fs::remove_file(&marker);
        let done = DownloadedUpdate {
            info: info.clone(),
            file: update_file,
            sha512: file.info.sha512.clone(),
            is_admin_rights_required: file.info.is_admin_rights_required,
            from_cache: false,
            resumed_from,
        };
        Ok(self.finish(done, file, true))
    }

    fn finish(&self, done: DownloadedUpdate, file: ResolvedFile, save_cache: bool) -> DownloadedUpdate {
        if save_cache {
            let cached = CachedInfo {
                file_name: done.file.file_name().map(|n| n.to_string_lossy().into_owned()),
                sha512: done.sha512.clone(),
                is_admin_rights_required: done.is_admin_rights_required,
            };
            if let Ok(text) = serde_json::to_string(&cached) {
                let _ = fs::write(self.pending_dir().join("update-info.json"), text);
            }
        }
        {
            let mut slots = self.slots.lock().unwrap();
            slots.downloaded = Some(done.clone());
            slots.downloaded_file = Some(file);
        }
        self.emit(UpdaterEvent::UpdateDownloaded(done.clone()));
        done
    }

    /// `DownloadedUpdateHelper.getValidCachedUpdateFile`: update-info.json'daki özet güncel
    /// dosyanınkiyle aynı ve diskteki dosyanın özeti tutuyorsa yeniden kullan; aksi hâlde
    /// pending klasörü boşaltılır.
    fn valid_cached_file(&self, file: &ResolvedFile) -> Option<(PathBuf, bool)> {
        let pending = self.pending_dir();
        let text = fs::read_to_string(pending.join("update-info.json")).ok()?;
        let Ok(cached) = serde_json::from_str::<CachedInfo>(text.trim_start_matches('\u{feff}')) else {
            empty_dir(&pending);
            return None;
        };
        let Some(name) = cached.file_name.filter(|n| !n.is_empty()) else {
            empty_dir(&pending);
            return None;
        };
        if cached.sha512 != file.info.sha512 {
            empty_dir(&pending);
            return None;
        }
        let path = pending.join(Path::new(&name).file_name()?);
        if !path.is_file() {
            return None;
        }
        if hash::verify_file(&path, file.info.sha512.as_deref(), file.info.sha2.as_deref()).is_err() {
            empty_dir(&pending);
            return None;
        }
        Some((path, cached.is_admin_rights_required))
    }

    /// Geçici dosyaya indirir; ağ hatasında ya da 5xx'te `Range` ile kalınan yerden sürdürür
    /// (1 sn, 2 sn … bekleyerek). Döner: ilk denemede sürdürülen bayt sayısı.
    fn fetch_to_temp(&self, file: &ResolvedFile, temp: &Path, marker: &Path) -> Result<u64, UpdateError> {
        let wanted = PartialMarker { url: file.info.url.clone(), sha512: file.info.sha512.clone(), sha2: file.info.sha2.clone() };
        let same_owner = fs::read_to_string(marker).ok().and_then(|t| serde_json::from_str::<PartialMarker>(&t).ok()).is_some_and(|m| m == wanted);
        if !same_owner {
            let _ = fs::remove_file(temp);
            fs::write(marker, serde_json::to_string(&wanted).unwrap_or_default()).map_err(|e| UpdateError::io("Geçici dosya yazılamadı", e))?;
        }
        let attempts = self.config.download_attempts.max(1);
        let mut first_resume = None;
        let mut attempt = 0;
        loop {
            let existing = fs::metadata(temp).map(|m| m.len()).unwrap_or(0);
            first_resume.get_or_insert(existing);
            match self.fetch_once(file, temp, existing) {
                Ok(()) => return Ok(first_resume.unwrap_or(0)),
                Err(error) => {
                    let retryable = matches!(error.kind, ErrorKind::Network) || is_server_error(&error);
                    attempt += 1;
                    if !retryable || attempt >= attempts {
                        return Err(error);
                    }
                    std::thread::sleep(Duration::from_millis(1000 * attempt as u64));
                }
            }
        }
    }

    fn fetch_once(&self, file: &ResolvedFile, temp: &Path, existing: u64) -> Result<(), UpdateError> {
        let url = file.url.as_str();
        if let Some(size) = file.info.size
            && existing > 0
            && existing >= size
        {
            if existing == size {
                self.report_final(size, 0, Instant::now());
                return Ok(());
            }
            let _ = fs::remove_file(temp);
            return self.fetch_once(file, temp, 0);
        }
        let mut request = self.http.get(file.url.clone());
        if existing > 0 {
            request = request.header(RANGE, format!("bytes={existing}-"));
        }
        let mut response = request.send().map_err(|e| from_reqwest(url, &e))?;
        let status = response.status();
        if status == StatusCode::RANGE_NOT_SATISFIABLE {
            let _ = fs::remove_file(temp);
            return self.fetch_once(file, temp, 0);
        }
        if status.as_u16() >= 400 {
            let reason = status.canonical_reason().unwrap_or("");
            return Err(UpdateError::new(ErrorKind::HttpStatus(status.as_u16()), format!("Cannot download \"{url}\", status {}: {reason}", status.as_u16())));
        }
        let header_u64 = |name: reqwest::header::HeaderName| response.headers().get(name).and_then(|v| v.to_str().ok().map(str::to_string));
        let (mut writer, offset) = if status == StatusCode::PARTIAL_CONTENT && existing > 0 {
            let start = header_u64(CONTENT_RANGE).and_then(|r| r.strip_prefix("bytes ").and_then(|r| r.split('-').next()).and_then(|s| s.trim().parse::<u64>().ok()));
            if start != Some(existing) {
                let _ = fs::remove_file(temp);
                return self.fetch_once(file, temp, 0);
            }
            (OpenOptions::new().append(true).open(temp).map_err(|e| UpdateError::io("Geçici dosya açılamadı", e))?, existing)
        } else {
            (File::create(temp).map_err(|e| UpdateError::io("Geçici dosya oluşturulamadı", e))?, 0)
        };
        let length = header_u64(CONTENT_LENGTH).and_then(|v| v.parse::<u64>().ok());
        let total = file.info.size.or(length.map(|l| l + offset)).unwrap_or(0);

        let started = Instant::now();
        let mut next_update = started + self.config.progress_interval;
        let mut transferred = offset;
        let mut delta = 0u64;
        let mut buffer = vec![0u8; 256 * 1024];
        loop {
            if self.cancelled.load(Ordering::SeqCst) {
                let _ = writer.flush();
                return Err(UpdateError::new(ErrorKind::Cancelled, "cancelled"));
            }
            let read = match response.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => n,
                Err(error) => {
                    let _ = writer.flush();
                    let code = if error.kind() == std::io::ErrorKind::TimedOut { "net::ERR_TIMED_OUT (ETIMEDOUT)" } else { "net::ERR_CONNECTION_RESET" };
                    return Err(UpdateError::new(ErrorKind::Network, format!("{code}: {url}: {error}")));
                }
            };
            writer.write_all(&buffer[..read]).map_err(|e| UpdateError::io("Geçici dosyaya yazılamadı", e))?;
            transferred += read as u64;
            delta += read as u64;
            let now = Instant::now();
            if now >= next_update && transferred != total {
                next_update = now + self.config.progress_interval;
                let elapsed = now.duration_since(started).as_secs_f64().max(0.001);
                (self.listener)(&UpdaterEvent::DownloadProgress(Progress {
                    total,
                    delta,
                    transferred,
                    percent: if total > 0 { transferred as f64 / total as f64 * 100.0 } else { 0.0 },
                    bytes_per_second: ((transferred - offset) as f64 / elapsed).round() as u64,
                }));
                delta = 0;
            }
        }
        writer.flush().map_err(|e| UpdateError::io("Geçici dosyaya yazılamadı", e))?;
        writer.sync_all().ok();
        drop(writer);
        if let Some(length) = length
            && transferred - offset < length
        {
            return Err(UpdateError::new(ErrorKind::Network, format!("net::ERR_CONTENT_LENGTH_MISMATCH: {url}")));
        }
        self.report_final(transferred, delta, started);
        Ok(())
    }

    /// builder-util-runtime `_flush`: akış bitince her zaman %100.
    fn report_final(&self, total: u64, delta: u64, started: Instant) {
        let elapsed = started.elapsed().as_secs_f64().max(0.001);
        self.emit(UpdaterEvent::DownloadProgress(Progress { total, delta, transferred: total, percent: 100.0, bytes_per_second: (total as f64 / elapsed).round() as u64 }));
    }

    /// electron-updater `quitAndInstall(isSilent, isForceRunAfter)` — kurulum programını
    /// başlatır; uygulamayı kapatmak çağıranın işidir. Aynı örnekte ikinci çağrı yok sayılır
    /// (`Ok(false)`). Başlatılamazsa `Error` olayı yayılır.
    pub fn quit_and_install(&self, silent: bool, force_run_after: bool, launcher: &dyn InstallerLauncher) -> Result<bool, UpdateError> {
        if self.install_called.swap(true, Ordering::SeqCst) {
            return Ok(false);
        }
        let Some(done) = self.downloaded() else {
            self.install_called.store(false, Ordering::SeqCst);
            let error = UpdateError::new(ErrorKind::Install, "No update filepath provided, can't quit and install");
            self.emit(UpdaterEvent::Error(error.clone()));
            return Err(error);
        };
        let args = nsis_install_args(silent, force_run_after, None, None);
        if let Err(io) = launcher.launch(&done.file, &args, done.is_admin_rights_required) {
            self.install_called.store(false, Ordering::SeqCst);
            let error = UpdateError::new(ErrorKind::Install, format!("Cannot run installer: {io}"));
            self.emit(UpdaterEvent::Error(error.clone()));
            return Err(error);
        }
        Ok(true)
    }
}

fn is_server_error(error: &UpdateError) -> bool {
    matches!(error.kind, ErrorKind::HttpStatus(code) if (500..=599).contains(&code))
}

/// fs-extra `emptyDir`: klasör kalır, içi boşalır.
fn empty_dir(dir: &Path) {
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let _ = fs::remove_dir_all(&path);
            } else {
                let _ = fs::remove_file(&path);
            }
        }
    }
}

/// electron-updater: hedef kilitliyse (EBUSY) 60 kez 500 ms arayla yeniden denenir.
fn rename_with_retry(from: &Path, to: &Path) -> Result<(), UpdateError> {
    let mut tries = 0;
    loop {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(error) => {
                let busy = matches!(error.raw_os_error(), Some(32) | Some(33) | Some(5)) || error.kind() == std::io::ErrorKind::PermissionDenied;
                tries += 1;
                if !busy || tries >= 60 {
                    return Err(UpdateError::io("Cannot rename temp file to final file", error));
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    }
}

/// electron-updater `getOrCreateStagingUserId`: dosyadaki UUID; yoksa/bozuksa yenisi yazılır.
fn staging_user_id(file: &Path) -> uuid::Uuid {
    if let Ok(text) = fs::read_to_string(file)
        && let Ok(id) = uuid::Uuid::parse_str(text.trim())
    {
        return id;
    }
    let id = uuid::Uuid::new_v4();
    if let Some(parent) = file.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(file, id.to_string());
    id
}

#[cfg(test)]
mod tests {
    use super::*;

    fn updater(dir: &Path, staging: Option<PathBuf>) -> Updater {
        let mut config = UpdaterConfig::new("http://127.0.0.1:9/updates/server", "0.9.9", dir);
        config.staging_id_file = staging;
        Updater::new(config, Arc::new(|_| {})).unwrap()
    }

    #[test]
    fn gecersiz_surum_reddedilir() {
        let dir = tempfile::tempdir().unwrap();
        let config = UpdaterConfig::new("http://h/", "dev", dir.path());
        assert_eq!(Updater::new(config, Arc::new(|_| {})).err().unwrap().kind, ErrorKind::InvalidVersion);
    }

    #[test]
    fn kademeli_yayin() {
        let dir = tempfile::tempdir().unwrap();
        let id_file = dir.path().join(".updaterId");
        // Son 4 bayt 0x80000000 → yüzde ≈ 50,0000001
        fs::write(&id_file, "00000000-0000-0000-0000-000080000000").unwrap();
        let u = updater(dir.path(), Some(id_file.clone()));
        let mut info = UpdateInfo { version: "1.0.0".into(), ..Default::default() };
        info.staging_percentage = Some("51".into());
        assert!(u.is_update_available(&info).unwrap());
        info.staging_percentage = Some("50".into());
        assert!(!u.is_update_available(&info).unwrap());
        info.staging_percentage = Some("abc".into());
        assert!(u.is_update_available(&info).unwrap());
        // Kimlik dosyası yoksa oluşturulur.
        let fresh = dir.path().join("yeni").join(".updaterId");
        let _ = staging_user_id(&fresh);
        assert!(uuid::Uuid::parse_str(fs::read_to_string(&fresh).unwrap().trim()).is_ok());
    }

    #[test]
    fn onbellek_dogrulama() {
        let dir = tempfile::tempdir().unwrap();
        let u = updater(dir.path(), None);
        let pending = u.pending_dir();
        fs::create_dir_all(&pending).unwrap();
        fs::write(pending.join("P2-Setup.exe"), b"kurulum").unwrap();
        let sha = hash::sha512_base64(b"kurulum");
        fs::write(pending.join("update-info.json"), format!("{{\"fileName\":\"P2-Setup.exe\",\"sha512\":\"{sha}\",\"isAdminRightsRequired\":false}}")).unwrap();
        let file = ResolvedFile { url: reqwest::Url::parse("http://h/P2-Setup.exe").unwrap(), info: yml::FileInfo { url: "P2-Setup.exe".into(), sha512: Some(sha), ..Default::default() } };
        assert_eq!(u.valid_cached_file(&file).unwrap().0, pending.join("P2-Setup.exe"));
        // Yeni sürümün özeti farklı → pending boşaltılır.
        let other = ResolvedFile { info: yml::FileInfo { sha512: Some("başka".into()), ..file.info.clone() }, ..file.clone() };
        assert!(u.valid_cached_file(&other).is_none());
        assert!(!pending.join("P2-Setup.exe").exists());
        assert!(pending.exists());
    }

    #[test]
    fn indirilmemisken_kurulum_hata() {
        struct Never;
        impl InstallerLauncher for Never {
            fn launch(&self, _: &Path, _: &[String], _: bool) -> std::io::Result<()> {
                panic!("çağrılmamalı")
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let u = updater(dir.path(), None);
        assert_eq!(u.quit_and_install(true, true, &Never).unwrap_err().kind, ErrorKind::Install);
    }
}
