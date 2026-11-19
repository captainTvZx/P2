//! Yama motoru — electron/patcher.cjs: kurulum (indir → birleştir → doğrula), duraklat/sürdür/
//! iptal, hız sınırı, doğrula/onar, kaldır, başlat/durdur (oynama süresi), sürüm yükleme
//! (klasörü içerik tanımlı parçalara böl, eksik parçaları gönder, kaldığı yerden devam).
//!
//! Ağır iş kendi iş parçacıklarında yapılır; arayüz `Event` alır. Sunucu uçları ve dosya düzeni
//! (`<kök>/<klasör>/.p2/manifest.json`, `.p2/chunks/`) Electron ile aynıdır.

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use p2_client_core::{Api, ApiError, Method};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::cdc::Chunker;
use crate::library::{InstalledProject, LibraryState};
use crate::manifest::{Manifest, ManifestFile, Source, chunk_ranges, plan_install, project_folder_name, safe_relative_path};

/// Aynı ağdan genel adrese dönüşte (hairpin NAT) tek bağlantı ~0,3 MB/sn'de kalıyor; 8–16 paralel
/// bağlantı toplamı ~4–5 katına çıkarıyor (2026-11-19 ölçümü).
const DOWNLOAD_PARALLEL: usize = 12;
const UPLOAD_PARALLEL: usize = 4;
const READ_BLOCK: usize = 8 * 1024 * 1024;

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or_default()
}

fn sha256(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

/// Kurulum işinin anlık durumu (patcher.cjs `job`).
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JobState {
    pub project_id: String,
    pub release_id: String,
    pub active: bool,
    /// preparing | downloading | waiting-for-exit | installing | done | failed | canceled
    pub phase: String,
    pub done: u64,
    pub total: u64,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub bytes_written: u64,
    pub bytes_to_write: u64,
    pub speed: u64,
    pub eta: u64,
    pub paused: bool,
    /// İptal istendi; iş parçacıkları kapanıyor (arayüz hemen "iptal ediliyor" gösterir).
    pub canceling: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UploadState {
    pub project_id: String,
    pub release_id: Option<String>,
    /// scanning | hashing | registering | uploading | ready | failed | canceled
    pub phase: String,
    pub percent: u32,
    pub done: u64,
    pub total: u64,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Job(JobState),
    Upload(UploadState),
    Running { project_id: String, running: bool },
    Played { project_id: String, seconds: u64 },
    Crash { project_id: String, code: i32 },
    /// library.json değişti (kurulu sürüm, klasör, oynama süresi).
    State,
}

pub type Sink = Arc<dyn Fn(Event) + Send + Sync>;

struct JobCtl {
    paused: AtomicBool,
    canceled: AtomicBool,
    /// İşin anlık durumu: sayfa yeniden açılınca ve duraklat/iptal anında buradan okunur.
    job: Arc<Mutex<JobState>>,
    /// Hız örneği: (son örnek anı, o andaki bytes_done, kayan ortalama B/sn) ve son olay anı.
    meter: Mutex<(Instant, u64, f64, Instant)>,
}

impl JobCtl {
    fn new(job: JobState) -> Self {
        let now = Instant::now();
        Self { paused: AtomicBool::new(false), canceled: AtomicBool::new(false), job: Arc::new(Mutex::new(job)), meter: Mutex::new((now, 0, 0.0, now - Duration::from_secs(1))) }
    }

    fn stopping(&self) -> bool {
        self.paused.load(Ordering::SeqCst) || self.canceled.load(Ordering::SeqCst)
    }

    /// İndirilen baytları ekler; hızı (kayan ortalama) ve kalan süreyi günceller. En çok ~8 kez/sn
    /// olay döndürür (arayüzü boğmadan akıcı ilerleme).
    fn progress(&self, bytes: i64, force: bool) -> Option<JobState> {
        let mut j = self.job.lock().unwrap();
        j.bytes_done = (j.bytes_done as i64 + bytes).max(0) as u64;
        let mut m = self.meter.lock().unwrap();
        let now = Instant::now();
        let dt = now.duration_since(m.0).as_secs_f64();
        if dt >= 0.5 {
            let rate = j.bytes_done.saturating_sub(m.1) as f64 / dt;
            m.2 = if m.2 <= 0.0 { rate } else { m.2 * 0.6 + rate * 0.4 };
            m.0 = now;
            m.1 = j.bytes_done;
            j.speed = m.2 as u64;
            j.eta = if j.speed > 0 { j.bytes_total.saturating_sub(j.bytes_done) / j.speed } else { 0 };
        }
        if force || now.duration_since(m.3) >= Duration::from_millis(120) {
            m.3 = now;
            return Some(j.clone());
        }
        None
    }

    /// Duraklayınca hız sıfırlanır; sürünce ölçüm baştan başlar.
    fn reset_meter(&self) {
        let done = self.job.lock().unwrap().bytes_done;
        let mut m = self.meter.lock().unwrap();
        m.0 = Instant::now();
        m.1 = done;
        m.2 = 0.0;
    }
}

/// Token kovası hız sınırı (patcher.cjs `throttle`); 0 = sınırsız.
struct Limiter {
    kbps: AtomicU64,
    state: Mutex<(f64, Instant)>,
}

impl Limiter {
    fn take(&self, bytes: u64) {
        let kbps = self.kbps.load(Ordering::Relaxed);
        if kbps == 0 {
            return;
        }
        // Paylaşılan kova eksiye düşebilir (borç): her iş parçacığı kendi payını kovadan düşer ve
        // borç ödenene dek bekler. Böylece paralel bağlantıların toplamı sınırı aşmaz.
        let rate = kbps as f64 * 1024.0;
        let wait = {
            let mut s = self.state.lock().unwrap();
            let now = Instant::now();
            s.0 = (s.0 + now.duration_since(s.1).as_secs_f64() * rate).min(rate * 0.25);
            s.1 = now;
            s.0 -= bytes as f64;
            (s.0 < 0.0).then(|| Duration::from_secs_f64((-s.0 / rate).min(5.0)))
        };
        if let Some(w) = wait {
            std::thread::sleep(w);
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HashEntry {
    size: u64,
    chunks: Vec<String>,
    lengths: Vec<u64>,
}

/// Oyun başlatıcı kanca (overlay enjeksiyonu): `None` dönerse düz başlatılır.
pub struct LaunchSpec {
    pub project_id: String,
    pub project_name: String,
    pub exe: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

/// Başlatılmış oyun: kök pid ve oyun (süreç ağacı) bitene kadar bekleyip çıkış kodunu döndüren iş.
pub struct Launched {
    pub pid: u32,
    pub wait: Box<dyn FnOnce() -> i32 + Send>,
}

pub type Launcher = Arc<dyn Fn(&LaunchSpec) -> Option<Launched> + Send + Sync>;

pub struct Patcher {
    api: Arc<Api>,
    dir: PathBuf,
    client_version: String,
    state: Mutex<LibraryState>,
    jobs: Mutex<HashMap<String, Arc<JobCtl>>>,
    running: Mutex<HashMap<String, u32>>,
    pending_folders: Mutex<HashMap<String, String>>,
    launcher: Mutex<Option<Launcher>>,
    limiter: Limiter,
    sink: Sink,
}

fn api_err(e: ApiError) -> String {
    if e.message.is_empty() { format!("HTTP {}", e.status) } else { e.message }
}

impl Patcher {
    /// `dir`: kütüphane klasörü (library.json, hash önbelleği, yükleme durumları).
    pub fn new(api: Arc<Api>, dir: PathBuf, client_version: &str, sink: Sink) -> Arc<Self> {
        let state = LibraryState::load(&dir.join("library.json"));
        Arc::new(Self {
            api,
            dir,
            client_version: client_version.into(),
            state: Mutex::new(state),
            jobs: Mutex::new(HashMap::new()),
            running: Mutex::new(HashMap::new()),
            pending_folders: Mutex::new(HashMap::new()),
            launcher: Mutex::new(None),
            limiter: Limiter { kbps: AtomicU64::new(0), state: Mutex::new((0.0, Instant::now())) },
            sink,
        })
    }

    pub fn set_launcher(&self, launcher: Launcher) {
        *self.launcher.lock().unwrap() = Some(launcher);
    }

    pub fn state(&self) -> LibraryState {
        self.state.lock().unwrap().clone()
    }

    pub fn is_running(&self, project: &str) -> bool {
        self.running.lock().unwrap().contains_key(project)
    }

    fn save(&self) {
        let _ = self.state.lock().unwrap().save(&self.dir.join("library.json"));
        (self.sink)(Event::State);
    }

    fn log(&self, line: &str) {
        let _ = fs::create_dir_all(&self.dir);
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(self.dir.join("library.log")) {
            let _ = writeln!(f, "[{}] {line}", now_ms());
        }
    }

    pub fn set_root(&self, root: &str) {
        self.state.lock().unwrap().root = root.to_string();
        self.save();
    }

    /// Kullanıcının ek başlatma argümanları (ör. "-windowed -log").
    pub fn set_user_args(&self, project: &str, args: &str) {
        if let Some(p) = self.state.lock().unwrap().projects.get_mut(project) {
            p.user_args = args.trim().to_string();
        }
        self.save();
    }

    /// Kurulu oyunun klasörü (Gezgin'de açmak için).
    pub fn install_dir(&self, project: &str) -> Option<PathBuf> {
        let st = self.state.lock().unwrap();
        st.projects.get(project).map(|p| st.project_dir(&p.folder))
    }

    /// İndirme/yükleme hız sınırı (MB/sn — ayarlardaki "5 MB/s"; 0 = sınırsız). patcher.cjs
    /// `library:set-limit` ile aynı: kbps = mbps × 1024, saniyede kbps × 1024 bayt.
    pub fn set_limit_mbps(&self, mbps: u64) {
        self.limiter.kbps.store(mbps * 1024, Ordering::Relaxed);
    }

    /// Bayt/sn cinsinden geçerli sınır (0 = sınırsız); tanılama ve testler için.
    pub fn limit_bytes_per_sec(&self) -> u64 {
        self.limiter.kbps.load(Ordering::Relaxed) * 1024
    }

    /// Bu projede kurulum işi sürüyor mu.
    pub fn job_active(&self, project: &str) -> bool {
        self.jobs.lock().unwrap().contains_key(project)
    }

    /// Herhangi bir kurulum işi sürüyor mu (otomatik güncelleme kuyruğu tek tek ilerler).
    pub fn any_job_active(&self) -> bool {
        !self.jobs.lock().unwrap().is_empty()
    }

    /// Süren işlerin anlık durumları (kütüphane sayfası yeniden açılınca boş görünmesin).
    pub fn jobs(&self) -> Vec<JobState> {
        // Liste kilidi iş kilitlerinden önce bırakılır (kurulum iş parçacığıyla kilitlenmesin).
        let ctls: Vec<Arc<JobCtl>> = self.jobs.lock().unwrap().values().cloned().collect();
        ctls.iter().map(|c| c.job.lock().unwrap().clone()).collect()
    }

    /// Duraklat/sürdür: süren parça istekleri hemen kesilir, arayüz anında güncellenir.
    pub fn pause(&self, project: &str, paused: bool) {
        let Some(ctl) = self.jobs.lock().unwrap().get(project).cloned() else { return };
        ctl.paused.store(paused, Ordering::SeqCst);
        ctl.reset_meter();
        let snapshot = {
            let mut j = ctl.job.lock().unwrap();
            j.paused = paused;
            j.speed = 0;
            j.clone()
        };
        (self.sink)(Event::Job(snapshot));
    }

    pub fn cancel(&self, project: &str) {
        let Some(ctl) = self.jobs.lock().unwrap().get(project).cloned() else { return };
        ctl.canceled.store(true, Ordering::SeqCst);
        let snapshot = {
            let mut j = ctl.job.lock().unwrap();
            j.canceling = true;
            j.speed = 0;
            j.clone()
        };
        (self.sink)(Event::Job(snapshot));
    }

    fn mark_installed(&self, org: &str, project: &str, release: &str, status: &str, extra: Value) {
        let device = self.state.lock().unwrap().device_id.clone();
        let mut body = json!({ "deviceId": device, "status": status, "clientVersion": self.client_version });
        if let (Some(b), Some(e)) = (body.as_object_mut(), extra.as_object()) {
            b.extend(e.clone());
        }
        let _ = self.api.request(Method::POST, &format!("/organizations/{org}/projects/{project}/releases/{release}/installed"), Some(body), true);
    }

    fn ensure_folder(&self, project_name: &str) -> String {
        let st = self.state.lock().unwrap();
        let root = PathBuf::from(&st.root);
        let mut folder = project_folder_name(project_name, None);
        let taken = st.projects.values().any(|p| p.folder == folder);
        let exists = root.join(&folder).exists();
        // Kayıtsız ve kurulumu bitmemiş klasör önceki başarısız denemenin artığıdır: yeniden kullanılır.
        let leftover = !taken && exists && !root.join(&folder).join(".p2").join("manifest.json").exists();
        if taken || (exists && !leftover) {
            folder = project_folder_name(project_name, Some(&uuid::Uuid::new_v4().to_string()[..8]));
        }
        folder
    }

    fn read_installed(dir: &Path) -> Option<Manifest> {
        serde_json::from_slice(&fs::read(dir.join(".p2").join("manifest.json")).ok()?).ok()
    }

    // ------------------------------------------------------------ kurulum

    /// Sürümü kurar/günceller (kendi iş parçacığında). Aynı projede iş sürüyorsa yok sayılır.
    pub fn install(self: &Arc<Self>, org: String, project: String, project_name: String, release: String, channel: String, version: String, pinned: Option<bool>) {
        if self.state.lock().unwrap().root.is_empty() {
            (self.sink)(Event::Job(JobState { project_id: project, release_id: release, phase: "failed".into(), error: Some("Önce kütüphane klasörünü seç.".into()), ..Default::default() }));
            return;
        }
        let ctl = Arc::new(JobCtl::new(JobState { project_id: project.clone(), release_id: release.clone(), active: true, phase: "preparing".into(), ..Default::default() }));
        {
            let mut jobs = self.jobs.lock().unwrap();
            if jobs.contains_key(&project) {
                return;
            }
            jobs.insert(project.clone(), ctl.clone());
        }
        let this = self.clone();
        std::thread::Builder::new()
            .name("p2-kurulum".into())
            .spawn(move || {
                let job = ctl.job.clone();
                let result = this.run_install(&org, &project, &project_name, &release, &channel, &version, pinned, &ctl, &job);
                let failed = matches!(&result, Err(e) if e != "CANCELED");
                // Kilit sırası: iş kilidi bırakılmadan iş listesi kilitlenmez (arayüz `jobs()` ters
                // sırayla okur → kilitlenme). Önce durum kopyalanır, sonra listeden çıkarılır, en son olay.
                let snapshot = {
                    let mut j = job.lock().unwrap();
                    j.active = false;
                    j.speed = 0;
                    j.canceling = false;
                    match result {
                        Ok(()) => j.phase = "done".into(),
                        Err(e) if e == "CANCELED" => j.phase = "canceled".into(),
                        Err(e) => {
                            this.log(&format!("kurulum hatası: {e}"));
                            j.phase = "failed".into();
                            j.error = Some(e);
                        }
                    }
                    j.clone()
                };
                if failed {
                    this.mark_installed(&org, &project, &release, "FAILED", json!({}));
                }
                this.jobs.lock().unwrap().remove(&project);
                (this.sink)(Event::Job(snapshot));
            })
            .expect("kurulum iş parçacığı");
    }

    #[allow(clippy::too_many_arguments)]
    fn run_install(&self, org: &str, project: &str, project_name: &str, release: &str, channel: &str, version: &str, pinned: Option<bool>, ctl: &Arc<JobCtl>, job: &Arc<Mutex<JobState>>) -> Result<(), String> {
        let existing = self.state.lock().unwrap().projects.get(project).cloned();
        let folder = existing.as_ref().map(|e| e.folder.clone()).or_else(|| self.pending_folders.lock().unwrap().get(project).cloned()).unwrap_or_else(|| self.ensure_folder(project_name));
        self.pending_folders.lock().unwrap().insert(project.into(), folder.clone());
        let dir = self.state.lock().unwrap().project_dir(&folder);
        let send = |j: &JobState| (self.sink)(Event::Job(j.clone()));
        { let s = job.lock().unwrap().clone(); send(&s); }

        let manifest: Manifest = self.api.typed(Method::GET, &format!("/organizations/{org}/projects/{project}/releases/{release}/manifest"), None, true).map_err(api_err)?;
        let installed = Self::read_installed(&dir);
        let plan = plan_install(installed.as_ref(), &manifest);
        {
            let mut j = job.lock().unwrap();
            j.total = plan.downloads.len() as u64;
            j.bytes_total = plan.bytes_to_download;
            j.bytes_to_write = plan.bytes_to_write;
        }
        let meta = dir.join(".p2");
        let cache = meta.join("chunks");
        fs::create_dir_all(&cache).map_err(|e| format!("Klasör açılamadı: {e}"))?;
        self.mark_installed(org, project, release, "UPDATING", json!({}));

        // İndirme: 4 işçi, her parça sha256 ile doğrulanır, önbelleğe atomik yazılır.
        job.lock().unwrap().phase = "downloading".into();
        { let s = job.lock().unwrap().clone(); send(&s); }
        let queue = Arc::new(Mutex::new(plan.downloads.clone()));
        let failure: Arc<Mutex<Option<String>>> = Arc::default();
        ctl.reset_meter();
        std::thread::scope(|scope| {
            for _ in 0..DOWNLOAD_PARALLEL {
                let (queue, failure, cache) = (queue.clone(), failure.clone(), cache.clone());
                scope.spawn(move || {
                    while failure.lock().unwrap().is_none() {
                        let Some(hash) = queue.lock().unwrap().pop() else { break };
                        if let Err(e) = self.download_chunk(org, project, &hash, &cache, ctl) {
                            failure.lock().unwrap().get_or_insert(e);
                            break;
                        }
                    }
                });
            }
        });
        if let Some(e) = failure.lock().unwrap().take() {
            return Err(e);
        }

        // Oyun açıksa montaj beklenir.
        if self.is_running(project) {
            job.lock().unwrap().phase = "waiting-for-exit".into();
            { let s = job.lock().unwrap().clone(); send(&s); }
            while self.is_running(project) {
                if ctl.canceled.load(Ordering::SeqCst) {
                    return Err("CANCELED".into());
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        }

        // Montaj: her dosya .p2new'e yazılır, parçalar doğrulanır, sonra yerine taşınır.
        job.lock().unwrap().phase = "installing".into();
        { let s = job.lock().unwrap().clone(); send(&s); }
        let root = dir.canonicalize().unwrap_or(dir.clone());
        for build in &plan.builds {
            let out = dir.join(build.path.replace('/', std::path::MAIN_SEPARATOR_STR));
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let resolved = out.parent().and_then(|p| p.canonicalize().ok()).unwrap_or_default();
            if !resolved.starts_with(&root) {
                return Err(format!("Güvenli olmayan yol: {}", build.path));
            }
            let tmp = out.with_extension(format!("{}p2new", out.extension().map(|e| format!("{}.", e.to_string_lossy())).unwrap_or_default()));
            {
                let mut handle = fs::File::create(&tmp).map_err(|e| e.to_string())?;
                for chunk in &build.chunks {
                    if ctl.canceled.load(Ordering::SeqCst) {
                        drop(handle);
                        let _ = fs::remove_file(&tmp);
                        return Err("CANCELED".into());
                    }
                    let read_at = |path: &Path, offset: u64, length: u64| -> Option<Vec<u8>> {
                        let mut f = fs::File::open(path).ok()?;
                        f.seek(SeekFrom::Start(offset)).ok()?;
                        let mut buf = vec![0u8; length as usize];
                        f.read_exact(&mut buf).ok()?;
                        Some(buf)
                    };
                    let data = match &chunk.from {
                        Source::Net => fs::read(cache.join(&chunk.hash)).ok(),
                        Source::Local { src_path, src_offset } => read_at(&dir.join(src_path.replace('/', std::path::MAIN_SEPARATOR_STR)), *src_offset, chunk.length),
                        Source::SelfFile { src_offset } => read_at(&out, *src_offset, chunk.length).or_else(|| fs::read(cache.join(&chunk.hash)).ok()),
                    }
                    .unwrap_or_default();
                    if sha256(&data) != chunk.hash {
                        return Err(format!("Parça doğrulanamadı: {}", build.path));
                    }
                    handle.write_all(&data).map_err(|e| e.to_string())?;
                    job.lock().unwrap().bytes_written += data.len() as u64;
                }
                handle.sync_all().map_err(|e| e.to_string())?;
            }
            let mut moved = false;
            for _ in 0..5 {
                if fs::rename(&tmp, &out).is_ok() {
                    moved = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(600));
            }
            if !moved {
                return Err(format!("Dosya değiştirilemedi (oyun açık olabilir): {}", build.path));
            }
            { let s = job.lock().unwrap().clone(); send(&s); }
        }
        for rel in &plan.deletes {
            let _ = fs::remove_file(dir.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR)));
        }
        prune_empty_dirs(&dir, &dir);
        let mut stored = serde_json::to_value(&manifest).unwrap_or_default();
        stored["releaseId"] = json!(release);
        stored["channel"] = json!(channel);
        stored["installedAt"] = json!(now_ms());
        let tmp = meta.join("manifest.json.tmp");
        fs::write(&tmp, serde_json::to_vec(&stored).unwrap_or_default()).map_err(|e| e.to_string())?;
        fs::rename(&tmp, meta.join("manifest.json")).map_err(|e| e.to_string())?;
        let _ = fs::remove_dir_all(&cache);

        let pinned = pinned.unwrap_or(existing.as_ref().is_some_and(|e| e.pinned));
        {
            let mut st = self.state.lock().unwrap();
            st.projects.insert(
                project.into(),
                InstalledProject {
                    folder,
                    channel: channel.into(),
                    release_id: release.into(),
                    // Sürüm numarası ("1.0.3"); manifestteki `version` biçim sürümüdür.
                    version: Value::String(version.to_string()),
                    installed_at: now_ms(),
                    pinned,
                    executable: manifest.executable.clone(),
                    launch_args: manifest.launch_args.clone().unwrap_or_default(),
                    user_args: existing.as_ref().map(|e| e.user_args.clone()).unwrap_or_default(),
                    play_seconds: existing.as_ref().map(|e| e.play_seconds).unwrap_or_default(),
                    last_played_at: existing.as_ref().and_then(|e| e.last_played_at),
                    extra: existing.map(|e| e.extra).unwrap_or_default(),
                },
            );
        }
        self.pending_folders.lock().unwrap().remove(project);
        self.save();
        self.mark_installed(org, project, release, "INSTALLED", json!({ "pinned": pinned }));
        Ok(())
    }

    /// Bir parçayı akış olarak indirir: ilerleme her blokta işlenir; duraklat/iptal süren isteği
    /// hemen keser. Yarım veri saklanır, sürdürünce ya da bağlantı kopunca `Range` ile kaldığı
    /// yerden devam edilir (ilerleme geri gitmez, indirilen boşa gitmez).
    fn download_chunk(&self, org: &str, project: &str, hash: &str, cache: &Path, ctl: &JobCtl) -> Result<(), String> {
        let path = cache.join(hash);
        if let Ok(meta) = fs::metadata(&path) {
            // Önceki (iptal edilmiş) denemeden kalan parça: ilerlemeye sayılır.
            ctl.job.lock().unwrap().done += 1;
            if let Some(s) = ctl.progress(meta.len() as i64, true) {
                (self.sink)(Event::Job(s));
            }
            return Ok(());
        }
        let url = format!("/organizations/{org}/projects/{project}/releases/chunks/{hash}");
        let mut partial: Vec<u8> = Vec::new();
        let mut attempt = 0;
        loop {
            if ctl.canceled.load(Ordering::SeqCst) {
                return Err("CANCELED".into());
            }
            while ctl.paused.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(100));
                if ctl.canceled.load(Ordering::SeqCst) {
                    return Err("CANCELED".into());
                }
            }
            let result = self.api.get_bytes_stream(&url, partial.len() as u64, &mut |piece| {
                if ctl.stopping() {
                    return false;
                }
                self.limiter.take(piece.len() as u64);
                partial.extend_from_slice(piece);
                if let Some(s) = ctl.progress(piece.len() as i64, false) {
                    (self.sink)(Event::Job(s));
                }
                true
            });
            // Tamamlandı (duraklatma son blokta gelse de): yarım veriyle birlikte doğrulanır.
            let whole = !partial.is_empty() && sha256(&partial) == hash;
            match result {
                _ if whole => {
                    let tmp = cache.join(format!("{hash}.part"));
                    fs::write(&tmp, &partial).map_err(|e| e.to_string())?;
                    fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
                    {
                        let mut j = ctl.job.lock().unwrap();
                        j.bytes_written += partial.len() as u64;
                        j.done += 1;
                    }
                    if let Some(s) = ctl.progress(0, true) {
                        (self.sink)(Event::Job(s));
                    }
                    return Ok(());
                }
                // Duraklat/iptal ile kesildi: yarım veri kalır, deneme hakkı yenmez.
                Ok(None) => continue,
                // Özet tutmadı: yarım veri bozuk olabilir, baştan alınır.
                Ok(Some(_)) => {
                    ctl.progress(-(partial.len() as i64), true);
                    partial.clear();
                    attempt += 1;
                    if attempt >= 3 {
                        return Err("CHUNK_HASH_MISMATCH".into());
                    }
                }
                // Bağlantı hatası: yarım veri korunur, kaldığı yerden yeniden denenir.
                Err(e) => {
                    attempt += 1;
                    if attempt >= 5 {
                        ctl.progress(-(partial.len() as i64), true);
                        return Err(api_err(e));
                    }
                    std::thread::sleep(Duration::from_millis(500 * attempt));
                }
            }
        }
    }

    // ------------------------------------------------------------ doğrula / kaldır

    /// Kurulu dosyaları manifestle karşılaştırır; bozukları manifestten düşüp yeniden kurar.
    /// Dönen: onarılacak dosya sayısı (0 = hepsi sağlam).
    pub fn verify(self: &Arc<Self>, org: &str, project: &str, project_name: &str) -> Result<usize, String> {
        let info = self.state.lock().unwrap().projects.get(project).cloned().ok_or("Kurulu değil.")?;
        let dir = self.state.lock().unwrap().project_dir(&info.folder);
        let manifest = Self::read_installed(&dir).ok_or("Kurulum bilgisi okunamadı.")?;
        let mut broken = Vec::new();
        for file in &manifest.files {
            let ok = (|| -> Option<bool> {
                let mut f = fs::File::open(dir.join(file.path.replace('/', std::path::MAIN_SEPARATOR_STR))).ok()?;
                for ((offset, length), hash) in chunk_ranges(file).into_iter().zip(&file.chunks) {
                    f.seek(SeekFrom::Start(offset)).ok()?;
                    let mut buf = vec![0u8; length as usize];
                    f.read_exact(&mut buf).ok()?;
                    if &sha256(&buf) != hash {
                        return Some(false);
                    }
                }
                Some(true)
            })()
            .unwrap_or(false);
            if !ok {
                broken.push(file.path.clone());
            }
        }
        if broken.is_empty() {
            return Ok(0);
        }
        let mut stored: Value = serde_json::from_slice(&fs::read(dir.join(".p2").join("manifest.json")).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        if let Some(files) = stored["files"].as_array_mut() {
            files.retain(|f| !broken.iter().any(|b| f["path"].as_str() == Some(b.as_str())));
        }
        fs::write(dir.join(".p2").join("manifest.json"), serde_json::to_vec(&stored).unwrap_or_default()).map_err(|e| e.to_string())?;
        self.install(org.into(), project.into(), project_name.into(), info.release_id.clone(), info.channel.clone(), info.version.as_str().map(str::to_string).unwrap_or_else(|| info.version.to_string()), None);
        Ok(broken.len())
    }

    pub fn uninstall(&self, org: &str, project: &str) -> Result<(), String> {
        if self.is_running(project) {
            return Err("Oyun açıkken kaldırılamaz.".into());
        }
        let info = self.state.lock().unwrap().projects.remove(project);
        if let Some(info) = info {
            let dir = self.state.lock().unwrap().project_dir(&info.folder);
            let _ = fs::remove_dir_all(dir);
            if !info.release_id.is_empty() {
                self.mark_installed(org, project, &info.release_id, "REMOVED", json!({}));
            }
        }
        self.save();
        Ok(())
    }

    // ------------------------------------------------------------ başlat / durdur

    /// Oyunu başlatır (başlatıcı kanca varsa overlay ile); çıkınca oynama süresi eklenir.
    pub fn launch(self: &Arc<Self>, org: &str, project: &str, name: &str) -> Result<(), String> {
        if self.is_running(project) {
            return Err("Oyun zaten açık.".into());
        }
        let info = self.state.lock().unwrap().projects.get(project).cloned().ok_or("Kurulu değil.")?;
        let dir = self.state.lock().unwrap().project_dir(&info.folder);
        let rel = safe_relative_path(&info.executable).ok_or("Başlatılacak dosya geçersiz.")?;
        let exe = dir.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));
        if !exe.exists() {
            return Err("Başlatılacak dosya bulunamadı; doğrula/onar dene.".into());
        }
        let args: Vec<String> = format!("{} {}", info.launch_args, info.user_args).split_whitespace().map(str::to_string).collect();
        let launcher = self.launcher.lock().unwrap().clone();
        let spec = LaunchSpec { project_id: project.into(), project_name: name.into(), exe: exe.clone(), args: args.clone(), cwd: dir.clone() };
        let launched = match launcher.and_then(|l| l(&spec)) {
            Some(l) => l,
            None => {
                let mut child: Child = Command::new(&exe).args(&args).current_dir(&dir).spawn().map_err(|e| format!("Başlatılamadı: {e}"))?;
                Launched { pid: child.id(), wait: Box::new(move || child.wait().ok().and_then(|s| s.code()).unwrap_or(0)) }
            }
        };
        self.running.lock().unwrap().insert(project.into(), launched.pid);
        let wait = launched.wait;
        (self.sink)(Event::Running { project_id: project.into(), running: true });
        let (this, org, project, release) = (self.clone(), org.to_string(), project.to_string(), info.release_id.clone());
        let started = Instant::now();
        std::thread::spawn(move || {
            let code = wait();
            this.running.lock().unwrap().remove(&project);
            let seconds = started.elapsed().as_secs();
            if let Some(entry) = this.state.lock().unwrap().projects.get_mut(&project) {
                entry.play_seconds += seconds;
                entry.last_played_at = Some(now_ms());
            }
            this.save();
            (this.sink)(Event::Running { project_id: project.clone(), running: false });
            (this.sink)(Event::Played { project_id: project.clone(), seconds });
            if code != 0 {
                (this.sink)(Event::Crash { project_id: project.clone(), code });
            }
            this.mark_installed(&org, &project, &release, "INSTALLED", json!({ "launched": true, "playSeconds": seconds }));
        });
        Ok(())
    }

    /// Çalışan oyunu (süreç ağacıyla) kapat.
    pub fn stop(&self, project: &str) -> Result<(), String> {
        let pid = *self.running.lock().unwrap().get(project).ok_or("Çalışmıyor.")?;
        let mut cmd = Command::new("taskkill");
        cmd.args(["/PID", &pid.to_string(), "/T", "/F"]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
    }

    // ------------------------------------------------------------ yükleme

    fn hash_file(path: &Path) -> std::io::Result<HashEntry> {
        let mut f = fs::File::open(path)?;
        let size = f.metadata()?.len();
        let mut chunker = Chunker::default();
        let mut entry = HashEntry { size, ..Default::default() };
        let mut block = vec![0u8; READ_BLOCK];
        let push = |piece: Vec<u8>, e: &mut HashEntry| {
            e.lengths.push(piece.len() as u64);
            e.chunks.push(sha256(&piece));
        };
        loop {
            let n = f.read(&mut block)?;
            if n == 0 {
                break;
            }
            for piece in chunker.feed(&block[..n], false) {
                push(piece, &mut entry);
            }
        }
        for piece in chunker.finish() {
            push(piece, &mut entry);
        }
        Ok(entry)
    }

    /// Klasörü manifest dosyalarına çevirir; değişmeyen dosyalar önbellekten gelir.
    pub fn hash_folder(&self, source: &Path, progress: &dyn Fn(u64, u64)) -> Result<Vec<ManifestFile>, String> {
        let cache_file = self.dir.join("hash-cache.json");
        let mut cache: HashMap<String, HashEntry> = fs::read(&cache_file).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        let mut found = Vec::new();
        walk(source, source, &mut found);
        let total: u64 = found.iter().map(|(full, _)| fs::metadata(full).map(|m| m.len()).unwrap_or(0)).sum();
        let mut done = 0;
        let mut files = Vec::new();
        for (full, rel) in found {
            let meta = fs::metadata(&full).map_err(|e| e.to_string())?;
            let mtime = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_millis()).unwrap_or(0);
            let key = format!("{}|{rel}|{}|{mtime}", source.display(), meta.len());
            let entry = match cache.get(&key).filter(|e| e.lengths.len() == e.chunks.len()) {
                Some(e) => e.clone(),
                None => {
                    let e = Self::hash_file(&full).map_err(|e| e.to_string())?;
                    cache.insert(key, e.clone());
                    e
                }
            };
            let path = safe_relative_path(&rel).ok_or(format!("Geçersiz dosya yolu: {rel}"))?;
            files.push(ManifestFile { path, size: entry.size, chunks: entry.chunks, lengths: Some(entry.lengths) });
            done += meta.len();
            progress(done, total);
        }
        let _ = fs::create_dir_all(&self.dir);
        let _ = fs::write(&cache_file, serde_json::to_vec(&cache).unwrap_or_default());
        Ok(files)
    }

    /// Sürüm yükle: klasörü parçala, sürümü kaydet, eksik parçaları gönder. Engellenen çağrı.
    #[allow(clippy::too_many_arguments)]
    pub fn upload(&self, org: &str, project: &str, source: &Path, executable: &str, launch_args: &str, channel: &str, version: &str, notes: &str, overlay: bool, cancel: &AtomicBool) -> Result<String, String> {
        let state = |phase: &str, percent: u32, release: Option<String>, done: u64, total: u64| (self.sink)(Event::Upload(UploadState { project_id: project.into(), release_id: release, phase: phase.into(), percent, done, total, error: None }));
        state("scanning", 0, None, 0, 0);
        let files = self.hash_folder(source, &|d, t| state("hashing", if t > 0 { (d * 100 / t) as u32 } else { 0 }, None, d, t))?;
        let exe = safe_relative_path(executable).ok_or("Başlatılacak dosya yolu geçersiz.")?;
        if !files.iter().any(|f| f.path == exe) {
            return Err("Başlatılacak dosya klasörde bulunamadı.".into());
        }
        state("registering", 100, None, 0, 0);
        let created = self
            .api
            .request(Method::POST, &format!("/organizations/{org}/projects/{project}/releases"), Some(json!({ "channel": channel, "version": version, "notes": notes, "executable": exe, "launchArgs": launch_args, "overlay": overlay, "files": files })), true)
            .map_err(api_err)?;
        let release = created["release"]["id"].as_str().unwrap_or_default().to_string();
        let mut index: HashMap<String, (String, u64, u64)> = HashMap::new();
        for f in &files {
            for (hash, (offset, length)) in f.chunks.iter().zip(chunk_ranges(f)) {
                index.entry(hash.clone()).or_insert((f.path.clone(), offset, length));
            }
        }
        let total = index.len() as u64;
        let mut pending: Vec<String> = serde_json::from_value(created["missing"].clone()).unwrap_or_default();
        loop {
            let done = Arc::new(AtomicU64::new(total - pending.len() as u64));
            let queue = Arc::new(Mutex::new(pending.clone()));
            let failure: Arc<Mutex<Option<String>>> = Arc::default();
            std::thread::scope(|scope| {
                for _ in 0..UPLOAD_PARALLEL {
                    let (queue, failure, done, index, release) = (queue.clone(), failure.clone(), done.clone(), &index, &release);
                    scope.spawn(move || {
                        while failure.lock().unwrap().is_none() {
                            if cancel.load(Ordering::SeqCst) {
                                failure.lock().unwrap().get_or_insert("CANCELED".into());
                                break;
                            }
                            let Some(hash) = queue.lock().unwrap().pop() else { break };
                            let (rel, offset, length) = index[&hash].clone();
                            let result = (|| -> Result<(), String> {
                                let mut f = fs::File::open(source.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR))).map_err(|e| e.to_string())?;
                                f.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
                                let mut buf = vec![0u8; length as usize];
                                f.read_exact(&mut buf).map_err(|e| e.to_string())?;
                                for attempt in 0..5 {
                                    self.limiter.take(buf.len() as u64);
                                    match self.api.put_bytes(&format!("/organizations/{org}/projects/{project}/releases/{release}/chunks/{hash}"), buf.clone()) {
                                        Ok(s) if (200..300).contains(&s) => return Ok(()),
                                        Ok(422) => return Err(format!("Parça içeriği değişti: {rel}")),
                                        _ if attempt == 4 => return Err("Parça yüklenemedi.".into()),
                                        _ => std::thread::sleep(Duration::from_millis(1000 << attempt)),
                                    }
                                }
                                Ok(())
                            })();
                            match result {
                                Ok(()) => {
                                    let d = done.fetch_add(1, Ordering::SeqCst) + 1;
                                    state("uploading", if total > 0 { (d * 100 / total) as u32 } else { 100 }, Some(release.clone()), d, total);
                                }
                                Err(e) => {
                                    failure.lock().unwrap().get_or_insert(e);
                                }
                            }
                        }
                    });
                }
            });
            if let Some(e) = failure.lock().unwrap().take() {
                return Err(e);
            }
            // Sunucuya göre hâlâ eksik var mı?
            let check = self.api.request(Method::POST, &format!("/organizations/{org}/projects/{project}/releases/{release}/missing"), Some(json!({})), true).map_err(api_err)?;
            pending = serde_json::from_value(check["missing"].clone()).unwrap_or_default();
            if pending.is_empty() {
                break;
            }
        }
        (self.sink)(Event::Upload(UploadState { project_id: project.into(), release_id: Some(release.clone()), phase: "ready".into(), percent: 100, done: total, total, error: None }));
        Ok(release)
    }
}

/// patcher.cjs `EXCLUDE`: hata ayıklama ve araç çıktıları yüklenmez.
fn excluded(rel: &str, dir: bool) -> bool {
    let r = if dir { format!("{rel}/") } else { rel.to_string() };
    let lower = r.to_ascii_lowercase();
    [".pdb", ".ilk", ".exp", ".lib", ".map"].iter().any(|e| lower.ends_with(e)) || lower.starts_with("crashes/") || lower.contains("/crashes/") || lower.starts_with(".p2/") || lower.contains("/.p2/")
}

fn walk(dir: &Path, base: &Path, out: &mut Vec<(PathBuf, String)>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        let full = e.path();
        let rel = full.strip_prefix(base).unwrap_or(&full).to_string_lossy().replace('\\', "/");
        if excluded(&rel, ft.is_dir()) {
            continue;
        }
        if ft.is_dir() {
            walk(&full, base, out);
        } else if ft.is_file() {
            out.push((full, rel));
        }
    }
}

fn prune_empty_dirs(dir: &Path, root: &Path) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        if e.file_type().is_ok_and(|t| t.is_dir()) && e.file_name() != ".p2" {
            prune_empty_dirs(&e.path(), root);
        }
    }
    if dir != root && fs::read_dir(dir).is_ok_and(|mut d| d.next().is_none()) {
        let _ = fs::remove_dir(dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limiter_caps_parallel_total() {
        // 8 MB/sn sınır, 8 iş parçacığı × 32 × 64 KB = 16 MB → en az ~1,75 sn sürmeli.
        let limiter = Arc::new(Limiter { kbps: AtomicU64::new(8 * 1024), state: Mutex::new((0.0, Instant::now())) });
        let started = Instant::now();
        std::thread::scope(|s| {
            for _ in 0..8 {
                let l = limiter.clone();
                s.spawn(move || {
                    for _ in 0..32 {
                        l.take(64 * 1024);
                    }
                });
            }
        });
        let secs = started.elapsed().as_secs_f64();
        assert!(secs > 1.6, "sınır aşıldı: 16 MB {secs:.2} sn'de");
        assert!(secs < 3.0, "gereğinden yavaş: {secs:.2} sn");
    }

    #[test]
    fn exclusions() {
        assert!(excluded("Game.pdb", false));
        assert!(excluded("Saved/Crashes", true) || excluded("crashes", true));
        assert!(excluded(".p2", true));
        assert!(!excluded("Game.exe", false));
    }
}
