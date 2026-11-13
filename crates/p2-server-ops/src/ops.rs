//! Sunucu yönetimi — server-main.cjs'teki `state`, `configure`, `startServer`, `stopServer`,
//! `restartServer`, `installService`, `removeService`, yedek al/yükle. Bütün çağrılar bloklayan;
//! arayüz bunları arka plan iş parçacığında çalıştırır.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use sqlx::{Connection as _, PgConnection};

use crate::config::{ConfigureInput, ServerConfig, random_secret};
use crate::log::Log;
use crate::service::ServiceControl;
use crate::update::{StubUpdater, UpdateProvider, UpdateState};

/// IPC dönüşlerinin (`{ ok, message, alreadyRunning, service, canceled }`) karşılığı.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    pub ok: bool,
    pub message: Option<String>,
    pub already_running: bool,
    pub service: bool,
    pub canceled: bool,
    /// Yedek alma: oluşan dosya.
    pub path: Option<PathBuf>,
}

impl Outcome {
    pub fn ok() -> Self {
        Self { ok: true, ..Default::default() }
    }
    pub fn fail(message: impl Into<String>) -> Self {
        Self { ok: false, message: Some(message.into()), ..Default::default() }
    }
    pub fn canceled() -> Self {
        Self { ok: false, canceled: true, ..Default::default() }
    }
}

/// `state()` anlık görüntüsü (aynı alanlar; `iconUrl` yerine arayüz gömülü ikonu kullanır).
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerState {
    pub version: String,
    pub configured: bool,
    pub running: bool,
    pub managed: bool,
    pub port: u16,
    pub public_ip: String,
    pub address: String,
    pub local_address: String,
    pub data_dir: String,
    pub updates_dir: String,
    pub log: String,
    pub error: String,
    pub service: bool,
    pub update: UpdateState,
}

/// Konsolun başlattığı API süreci.
struct ApiChild {
    child: Mutex<Child>,
    /// `stopServer` süreci sahiplendi: çıkışı "beklenmeyen kapanış" sayılmaz.
    released: AtomicBool,
}

struct Inner {
    data_dir: PathBuf,
    headless: bool,
    log: Log,
    service: Box<dyn ServiceControl>,
    updater: Mutex<Arc<dyn UpdateProvider>>,
    api: Mutex<Option<Arc<ApiChild>>>,
    stopping: AtomicBool,
    public_ip: Mutex<String>,
    /// `broadcast()` sayacı: arayüz değişince durumu yeniden okur.
    generation: AtomicU64,
    server_exe: PathBuf,
    version: String,
}

#[derive(Clone)]
pub struct Ops {
    inner: Arc<Inner>,
}

/// Oluşturma seçenekleri. Varsayılan: gerçek veri klasörü, `P2_SERVER_NO_SERVICE`'e göre hizmet,
/// `P2_SERVER_EXE` ya da exe'nin yanındaki `p2-server(.exe)`.
pub struct OpsOptions {
    pub data_dir: PathBuf,
    pub headless: bool,
    pub service: Box<dyn ServiceControl>,
    pub server_exe: PathBuf,
    pub version: String,
}

impl Default for OpsOptions {
    fn default() -> Self {
        let exe_name = if cfg!(windows) { "p2-server.exe" } else { "p2-server" };
        let server_exe = std::env::var_os("P2_SERVER_EXE").filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| {
            std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join(exe_name))).unwrap_or_else(|| PathBuf::from(exe_name))
        });
        Self { data_dir: crate::data_dir(), headless: false, service: crate::service::from_env(), server_exe, version: crate::VERSION.to_string() }
    }
}

fn sleep_ms(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

impl Ops {
    pub fn new(options: OpsOptions) -> Self {
        let log = Log::new(options.data_dir.join("server.log"));
        let updater: Arc<dyn UpdateProvider> = Arc::new(StubUpdater::new(&options.version));
        Self {
            inner: Arc::new(Inner {
                data_dir: options.data_dir,
                headless: options.headless,
                log,
                service: options.service,
                updater: Mutex::new(updater),
                api: Mutex::new(None),
                stopping: AtomicBool::new(false),
                public_ip: Mutex::new(String::new()),
                generation: AtomicU64::new(0),
                server_exe: options.server_exe,
                version: options.version,
            }),
        }
    }

    pub fn data_dir(&self) -> &Path {
        &self.inner.data_dir
    }
    pub fn config_file(&self) -> PathBuf {
        self.inner.data_dir.join("server.json")
    }
    pub fn log_file(&self) -> PathBuf {
        self.inner.data_dir.join("server.log")
    }
    pub fn updates_dir(&self) -> PathBuf {
        self.inner.data_dir.join("updates")
    }
    pub fn version(&self) -> &str {
        &self.inner.version
    }

    /// Güncelleyiciyi bağlar (p2-updater hazır olunca).
    pub fn set_updater(&self, updater: Arc<dyn UpdateProvider>) {
        *self.inner.updater.lock().unwrap() = updater;
        self.broadcast();
    }
    pub fn updater(&self) -> Arc<dyn UpdateProvider> {
        self.inner.updater.lock().unwrap().clone()
    }

    /// Değişiklik sayacı (`broadcast` her çağrıldığında artar).
    pub fn generation(&self) -> u64 {
        self.inner.generation.load(Ordering::Relaxed)
    }
    pub fn broadcast(&self) {
        self.inner.generation.fetch_add(1, Ordering::Relaxed);
    }

    pub fn read_config(&self) -> Option<ServerConfig> {
        ServerConfig::read(&self.config_file())
    }

    /// `writeLog`.
    pub fn write_log(&self, value: &str) {
        if self.inner.log.write(value) {
            self.broadcast();
        }
    }

    pub fn last_error(&self) -> String {
        self.inner.log.last_error()
    }

    pub fn port(&self) -> u16 {
        self.read_config().map(|c| c.port()).unwrap_or(4000)
    }

    pub fn api_alive(&self) -> bool {
        crate::net::api_alive(self.port(), Duration::from_millis(900))
    }

    pub fn service_registered(&self) -> bool {
        self.inner.service.registered()
    }

    pub fn managed(&self) -> bool {
        self.inner.api.lock().unwrap().is_some()
    }

    /// `resolvePublicIp` (ağ isteği; arka planda çağır).
    pub fn resolve_public_ip(&self) {
        if let Some(ip) = crate::net::public_ip() {
            *self.inner.public_ip.lock().unwrap() = ip;
        }
        self.broadcast();
    }

    pub fn public_ip(&self) -> String {
        self.inner.public_ip.lock().unwrap().clone()
    }

    /// `state()`.
    pub fn state(&self) -> ServerState {
        let config = self.read_config();
        let service = self.service_registered();
        let port = config.as_ref().map(|c| c.port()).unwrap_or(4000);
        let public_ip = self.public_ip();
        let host = if public_ip.is_empty() {
            "public-ip-bekleniyor".to_string()
        } else if public_ip.contains(':') {
            format!("[{public_ip}]")
        } else {
            public_ip.clone()
        };
        ServerState {
            version: self.inner.version.clone(),
            configured: config.as_ref().is_some_and(|c| c.configured()),
            running: crate::net::api_alive(port, Duration::from_millis(900)),
            managed: self.managed(),
            port,
            public_ip,
            address: format!("http://{host}:{port}"),
            local_address: format!("http://127.0.0.1:{port}"),
            data_dir: self.inner.data_dir.display().to_string(),
            updates_dir: self.updates_dir().display().to_string(),
            log: self.inner.log.recent(),
            error: self.last_error(),
            service,
            update: self.updater().state(),
        }
    }

    /// `configure(input)`: denetim, gerekirse CREATE DATABASE, göçler, server.json, yeniden başlatma.
    /// Hata iletisi IPC'deki gibi `message` alanında döner (ve günlüğe yazılır).
    pub fn configure(&self, input: &ConfigureInput) -> Outcome {
        match self.configure_inner(input) {
            Ok(outcome) => outcome,
            Err(error) => {
                self.write_log(&crate::error_stack(&error));
                Outcome::fail(crate::error_message(&error))
            }
        }
    }

    fn configure_inner(&self, input: &ConfigureInput) -> anyhow::Result<Outcome> {
        let valid = input.validate().map_err(|m| anyhow::anyhow!(m))?;
        let connect = |database: &str| {
            sqlx::postgres::PgConnectOptions::new()
                .host(&valid.host)
                .port(valid.db_port)
                .username(&valid.username)
                .password(&valid.password)
                .database(database)
                .ssl_mode(if valid.ssl { sqlx::postgres::PgSslMode::Require } else { sqlx::postgres::PgSslMode::Disable })
        };
        let refused = |e: sqlx::Error| -> anyhow::Error {
            if let sqlx::Error::Io(io) = &e {
                if io.kind() == std::io::ErrorKind::ConnectionRefused {
                    return anyhow::anyhow!("connect ECONNREFUSED {}:{}", valid.host, valid.db_port);
                }
            }
            e.into()
        };
        crate::block_on(async {
            let mut admin = PgConnection::connect_with(&connect("postgres")).await.map_err(refused)?;
            let result = async {
                let exists: Option<i32> = sqlx::query_scalar("SELECT 1 FROM pg_database WHERE datname = $1").bind(&valid.database).fetch_optional(&mut admin).await?;
                if exists.is_none() {
                    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE DATABASE \"{}\" ENCODING 'UTF8'", valid.database))).execute(&mut admin).await?;
                }
                anyhow::Ok(())
            }
            .await;
            let _ = admin.close().await;
            result?;
            let mut target = PgConnection::connect_with(&connect(&valid.database)).await.map_err(refused)?;
            let result = crate::migrate::apply(&mut target).await;
            let _ = target.close().await;
            result.map(|_| ())
        })?;
        let previous = self.read_config().unwrap_or_default();
        let config = ServerConfig {
            database_url: Some(valid.database_url()),
            port: Some(serde_json::Value::from(valid.api_port)),
            access_secret: Some(previous.access_secret.filter(|s| !s.is_empty()).unwrap_or_else(random_secret)),
            refresh_secret: Some(previous.refresh_secret.filter(|s| !s.is_empty()).unwrap_or_else(random_secret)),
        };
        config.write(&self.config_file())?;
        self.write_log(&format!("Yapılandırma kaydedildi; API portu {}", valid.api_port));
        self.restart();
        Ok(Outcome::ok())
    }

    /// Exe yeni yazıldıysa (son 3 dk) hizmet güncellemeden sonra yeniden başlıyordur.
    fn just_updated() -> bool {
        std::env::current_exe()
            .and_then(std::fs::metadata)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .is_some_and(|age| age < Duration::from_secs(180))
    }

    /// `startServer()` (IPC sarmalayıcısıyla: hata → günlük + `{ ok: false, message }`).
    pub fn start(&self) -> Outcome {
        match self.start_inner() {
            Ok(outcome) => outcome,
            Err(error) => {
                self.write_log(&crate::error_stack(&error));
                Outcome::fail(crate::error_message(&error))
            }
        }
    }

    fn start_inner(&self) -> anyhow::Result<Outcome> {
        let Some(config) = self.read_config().filter(|c| c.configured()) else {
            return Ok(Outcome::fail("Önce PostgreSQL bağlantısını yapılandırın."));
        };
        if self.api_alive() {
            self.broadcast();
            return Ok(Outcome { already_running: true, ..Outcome::ok() });
        }
        if self.managed() {
            return Ok(Outcome::ok());
        }
        // Görünür konsol kayıtlı hizmeti yönetir; hizmet sürecinin kendisi WinSW'yi yeniden çağırmaz.
        if !self.inner.headless && self.service_registered() {
            if Self::just_updated() {
                for _ in 0..150 {
                    if self.api_alive() {
                        self.broadcast();
                        return Ok(Outcome { service: true, ..Outcome::ok() });
                    }
                    sleep_ms(400);
                }
            }
            let started = self.inner.service.elevated_service("start");
            for _ in 0..30 {
                if self.api_alive() {
                    self.broadcast();
                    return Ok(Outcome { service: true, ..Outcome::ok() });
                }
                sleep_ms(400);
            }
            return Ok(Outcome::fail(if started { "Windows hizmeti başladı ancak API yanıt vermedi." } else { "Windows hizmeti başlatılamadı." }));
        }
        let database_url = config.database_url.clone().unwrap_or_default();
        crate::migrate::apply_url_blocking(&database_url)?;
        let port = config.port();
        self.write_log(&format!("P2 API başlatılıyor: 0.0.0.0:{port}"));
        std::fs::create_dir_all(&self.inner.data_dir)?;
        let log_out = std::fs::OpenOptions::new().create(true).append(true).open(self.log_file())?;
        let log_err = log_out.try_clone()?;
        let mut command = Command::new(&self.inner.server_exe);
        command
            .env("DATABASE_URL", crate::config::clean_database_url(&database_url).unwrap_or(database_url.clone()))
            .env("JWT_ACCESS_SECRET", config.access_secret.clone().unwrap_or_default())
            .env("JWT_REFRESH_SECRET", config.refresh_secret.clone().unwrap_or_default())
            .env("P2_SERVER_VERSION", &self.inner.version)
            .env("PORT", port.to_string())
            .env("P2_UPDATE_DIR", self.updates_dir())
            .env("P2_UPLOAD_DIR", std::env::var_os("P2_UPLOAD_DIR").filter(|v| !v.is_empty()).unwrap_or_else(|| self.inner.data_dir.join("uploads").into_os_string()))
            .env("P2_RELEASE_DIR", std::env::var_os("P2_RELEASE_DIR").filter(|v| !v.is_empty()).unwrap_or_else(|| self.inner.data_dir.join("releases").into_os_string()))
            .stdin(Stdio::null())
            .stdout(log_out)
            .stderr(log_err);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt as _;
            // detached + windowsHide: konsol kapanınca da API yaşar, pencere açılmaz.
            const DETACHED_PROCESS: u32 = 0x0000_0008;
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
            command.creation_flags(if self.inner.headless { 0x0800_0000 } else { DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP });
        }
        #[cfg(unix)]
        if !self.inner.headless {
            use std::os::unix::process::CommandExt as _;
            command.process_group(0);
        }
        let child = command.spawn().map_err(|e| anyhow::anyhow!("{} başlatılamadı: {e}", self.inner.server_exe.display()))?;
        let handle = Arc::new(ApiChild { child: Mutex::new(child), released: AtomicBool::new(false) });
        *self.inner.api.lock().unwrap() = Some(handle.clone());
        let ops = self.clone();
        std::thread::Builder::new()
            .name("p2-api-izle".into())
            .spawn(move || ops.watch(handle))
            .ok();
        let ops = self.clone();
        std::thread::spawn(move || {
            sleep_ms(1000);
            ops.broadcast();
        });
        Ok(Outcome::ok())
    }

    /// `apiProcess.once('exit')`.
    fn watch(&self, handle: Arc<ApiChild>) {
        loop {
            if handle.released.load(Ordering::SeqCst) {
                return;
            }
            let status = handle.child.lock().unwrap().try_wait();
            match status {
                Ok(None) => sleep_ms(250),
                Ok(Some(status)) => {
                    if handle.released.load(Ordering::SeqCst) {
                        return;
                    }
                    {
                        let mut slot = self.inner.api.lock().unwrap();
                        if slot.as_ref().is_some_and(|h| Arc::ptr_eq(h, &handle)) {
                            *slot = None;
                        }
                    }
                    if !self.inner.stopping.load(Ordering::SeqCst) {
                        let code = status.code().map(|c| c.to_string()).unwrap_or_else(|| "null".into());
                        self.write_log(&format!("P2 API kapandı (kod {code})"));
                    }
                    self.broadcast();
                    return;
                }
                Err(_) => return,
            }
        }
    }

    /// `stopServer(options)`. `self_stop`: hizmet kayıtlıysa bile `sc stop` yerine imzalı kapatma isteği.
    pub fn stop(&self, self_stop: bool) -> Outcome {
        self.inner.stopping.store(true, Ordering::SeqCst);
        let child = self.inner.api.lock().unwrap().take();
        if let Some(handle) = child {
            handle.released.store(true, Ordering::SeqCst);
            let mut child = handle.child.lock().unwrap();
            let _ = child.kill();
            for _ in 0..25 {
                if !matches!(child.try_wait(), Ok(None)) {
                    break;
                }
                sleep_ms(100);
            }
        } else if !self_stop && self.service_registered() {
            if !self.inner.service.elevated_service("stop") {
                self.inner.stopping.store(false, Ordering::SeqCst);
                return Outcome::fail("Windows hizmeti durdurulamadı.");
            }
            for _ in 0..30 {
                if !self.api_alive() {
                    break;
                }
                sleep_ms(300);
            }
        } else if self.api_alive() {
            let secret = self.read_config().and_then(|c| c.access_secret).unwrap_or_default();
            crate::net::request_stop(self.port(), &secret);
            for _ in 0..20 {
                if !self.api_alive() {
                    break;
                }
                sleep_ms(300);
            }
        }
        self.inner.stopping.store(false, Ordering::SeqCst);
        self.write_log("P2 API yönetim konsolundan durduruldu");
        self.broadcast();
        let running = self.api_alive();
        Outcome { ok: !running, message: running.then(|| "API süreci durmadı.".into()), ..Default::default() }
    }

    /// `restartServer()` (IPC sarmalayıcısıyla).
    pub fn restart(&self) -> Outcome {
        let stopped = self.stop(false);
        if !stopped.ok {
            return stopped;
        }
        self.start()
    }

    /// `installService()`.
    pub fn install_service(&self) -> Outcome {
        if !self.inner.service.descriptor_ok() {
            self.write_log(&format!("Windows hizmeti yapılandırması bulunamadı: {}", self.service_descriptor().display()));
            return Outcome::fail("Windows hizmeti yapılandırması oluşturulamadı.");
        }
        self.stop(false);
        let launched = self.inner.service.elevated_script("p2-server-install.bat");
        for _ in 0..20 {
            if self.service_registered() {
                self.write_log("P2 Server Windows açılış servisi olarak kaydedildi");
                self.broadcast();
                return Outcome::ok();
            }
            sleep_ms(500);
        }
        let _ = self.start();
        Outcome::fail(if launched { "Servis kaydı tamamlanamadı." } else { "Yönetici izni verilmedi veya servis betiği bulunamadı." })
    }

    /// `removeService()`.
    pub fn remove_service(&self) -> Outcome {
        let removed = self.inner.service.elevated_script("p2-server-remove.bat");
        for _ in 0..20 {
            if !self.service_registered() {
                break;
            }
            sleep_ms(500);
        }
        let _ = self.start();
        self.write_log("P2 Server Windows açılış servisi kaldırıldı");
        self.broadcast();
        Outcome { ok: removed && !self.service_registered(), ..Default::default() }
    }

    fn service_descriptor(&self) -> PathBuf {
        std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("P2ServerService.xml"))).unwrap_or_default()
    }

    /// Açılışta (Electron `ensureServiceDescriptor()`): XML yoksa günlüğe not düşer.
    pub fn ensure_service_descriptor(&self) -> bool {
        let ok = self.inner.service.descriptor_ok();
        if !ok {
            self.write_log(&format!("Windows hizmeti yapılandırması bulunamadı: {}", self.service_descriptor().display()));
        }
        ok
    }

    /// `createServerBackup` (dosya seçimi arayüzde yapılır; `destination` seçilen yol).
    pub fn create_backup(&self, destination: &Path, include_releases: bool) -> Outcome {
        let Some(url) = self.read_config().filter(|c| c.configured()).and_then(|c| c.database_url) else {
            return Outcome::fail("Önce PostgreSQL bağlantısını yapılandır.");
        };
        self.write_log("P2 Server yedeği hazırlanıyor");
        let result = crate::backup::create_backup(crate::backup::CreateOptions {
            database_url: &url,
            data_dir: &self.inner.data_dir,
            destination,
            version: &self.inner.version,
            include_releases,
        });
        match result {
            Ok(created) => {
                self.write_log(&format!("Yedek oluşturuldu: {}", created.path.display()));
                Outcome { path: Some(created.path), ..Outcome::ok() }
            }
            Err(error) => {
                self.write_log(&format!("Yedek oluşturulamadı: {}", crate::error_stack(&error)));
                Outcome::fail(crate::error_message(&error))
            }
        }
    }

    /// Yedek yüklenebilir mi (dosya seçiminden önce, Electron'daki ilk denetim).
    pub fn restore_precheck(&self) -> Option<Outcome> {
        if self.read_config().filter(|c| c.configured()).is_none() {
            return Some(Outcome::fail("Önce hedef PostgreSQL bağlantısını yapılandır."));
        }
        None
    }

    /// `restoreServerBackup` (dosya seçimi ve onay arayüzde yapılır).
    pub fn restore_backup(&self, source: &Path) -> Outcome {
        let Some(url) = self.read_config().filter(|c| c.configured()).and_then(|c| c.database_url) else {
            return Outcome::fail("Önce hedef PostgreSQL bağlantısını yapılandır.");
        };
        self.write_log(&format!("Yedek yükleniyor: {}", source.display()));
        let stop = || self.stop(false);
        let start = || self.start();
        let result = crate::backup::restore_backup(crate::backup::RestoreOptions {
            database_url: &url,
            data_dir: &self.inner.data_dir,
            source,
            stop: Some(&stop),
            start: Some(&start),
        });
        match result {
            Ok(manifest) => {
                let created = manifest.get("createdAt").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).unwrap_or("tarih bilinmiyor").to_string();
                self.write_log(&format!("Yedek başarıyla yüklendi ({created})"));
                Outcome::ok()
            }
            Err(error) => {
                self.write_log(&format!("Yedek yüklenemedi: {}", crate::error_stack(&error)));
                Outcome::fail(crate::error_message(&error))
            }
        }
    }

    /// "Veri klasörü": klasörü oluşturur ve dosya gezgininde açar.
    pub fn open_data(&self) -> bool {
        let _ = std::fs::create_dir_all(&self.inner.data_dir);
        if std::env::var_os("P2_CONSOLE_NO_SHELL").is_some() {
            return true;
        }
        let opener = if cfg!(windows) { "explorer.exe" } else if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
        let _ = Command::new(opener).arg(&self.inner.data_dir).spawn();
        true
    }
}
