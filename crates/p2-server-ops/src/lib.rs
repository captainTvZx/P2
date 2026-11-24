//! P2 Server işletim katmanı — `electron/server-main.cjs` (arayüz dışı kısım),
//! `server/ops/backup.cjs`, `deploy/linux/migrate.cjs` ve `deploy/linux/backup-cli.cjs` karşılığı.
//!
//! - [`config`]: `server.json` (aynı dosya, aynı anahtarlar: mevcut kurulumlar olduğu gibi çalışır).
//! - [`log`]: `server.log` (aynı satır biçimi) ve son hata yakalama.
//! - [`migrate`]: `_p2_migrations` göç algoritması; SQL derleme anında gömülür.
//! - [`backup`]: `.p2backup` (tar.gz; backup.cjs ile karşılıklı uyumlu).
//! - [`ops`]: durum anlık görüntüsü, başlat/durdur/yeniden başlat, yapılandır, Windows hizmeti.
//! - [`update`]: güncelleme katmanı için durum modeli ve sağlayıcı arayüzü.

pub mod backup;
pub mod config;
pub mod log;
pub mod migrate;
pub mod net;
pub mod ops;
pub mod service;
pub mod settings;
pub mod update;

use std::path::PathBuf;
use std::sync::OnceLock;

pub use ops::{Ops, Outcome, ServerState};
pub use update::{UpdateProvider, UpdateState};

/// Ürün sürümü (kök Cargo.toml `[workspace.package] version`; paketleme P2_PRODUCT_VERSION ile verir).
pub const VERSION: &str = env!("P2_PRODUCT_VERSION");

/// Sunucu veri klasörü. Windows: `%ProgramData%\P2` (Electron ile aynı); diğerleri: Electron'un
/// `userData/server` yolu. `P2_SERVER_DATA_DIR` her zaman önceliklidir (testler geçici klasör verir).
pub fn data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("P2_SERVER_DATA_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    if cfg!(windows) {
        let base = std::env::var("ProgramData").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| "C:\\ProgramData".into());
        PathBuf::from(base).join("P2")
    } else {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"));
        config.join("P2 Server").join("server")
    }
}

/// Veritabanı işleri için paylaşılan tokio çalışma zamanı. Arayüz (GPUI) ve CLI eşzamanlı
/// (bloklayan) API'yi kullanır; sqlx burada sürülür.
pub fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("p2-ops")
            .enable_all()
            .build()
            .expect("tokio çalışma zamanı kurulamadı")
    })
}

/// Bloklayan çağrı: bir tokio çalışma zamanının içinden çağrılmamalı.
pub fn block_on<F: std::future::Future>(future: F) -> F::Output {
    runtime().block_on(future)
}

/// Gizli pencere bayrağı (`windowsHide: true`).
#[cfg(windows)]
pub(crate) fn hide(command: &mut std::process::Command) -> &mut std::process::Command {
    use std::os::windows::process::CommandExt as _;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    command.creation_flags(CREATE_NO_WINDOW)
}
#[cfg(not(windows))]
pub(crate) fn hide(command: &mut std::process::Command) -> &mut std::process::Command {
    command
}

/// `new Date().toISOString()`.
pub fn iso_now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

/// Hata metni: `error?.message`. PostgreSQL hatalarında sunucunun iletisi (node-pg ile aynı), bağlantı
/// reddinde node'un `connect ECONNREFUSED host:port` biçimi.
pub fn error_message(error: &anyhow::Error) -> String {
    for cause in error.chain() {
        if let Some(sqlx::Error::Database(db)) = cause.downcast_ref::<sqlx::Error>() {
            return db.message().to_string();
        }
        if let Some(sqlx::Error::Io(io)) = cause.downcast_ref::<sqlx::Error>() {
            if io.kind() == std::io::ErrorKind::ConnectionRefused {
                return "connect ECONNREFUSED".into();
            }
        }
    }
    error.to_string()
}

/// `error?.stack || error`: günlüğe yazılan uzun biçim ("Error: ileti" + neden zinciri).
pub fn error_stack(error: &anyhow::Error) -> String {
    let mut out = format!("Error: {}", error_message(error));
    for cause in error.chain().skip(1) {
        out.push_str(&format!("\n    nedeni: {cause}"));
    }
    out
}
