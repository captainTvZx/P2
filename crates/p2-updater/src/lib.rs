//! P2 otomatik güncelleme — Electron sürümündeki `electron-updater` (generic sağlayıcı, NSIS)
//! ile `electron/updater.cjs` (istemci) ve `electron/server-updater.cjs` (sunucu konsolu)
//! karşılığı. GPUI bağımlılığı yoktur; tüm iş arka plan iş parçacıklarında yürür, durum
//! değişiklikleri geri çağrıyla bildirilir.
//!
//! Katmanlar:
//! - [`Updater`]: düşük düzey, bloklayıcı — latest.yml çek, sürüm karşılaştır, indir
//!   (sürdürülebilir, sha512 doğrulamalı, `pending` önbelleği), NSIS kurulumunu başlat.
//! - [`ServerUpdater`]: sunucu konsolu durum makinesi (onaysız: indir → API'yi durdur → kur).
//! - [`ClientUpdater`]: istemci durum makinesi (kaynak = bağlı API, otomatik kurulum, açılış turu).
//!
//! Kaynak adresleri: istemci `<api>/updates/client/`, sunucu `http://127.0.0.1:<port>/updates/server/`
//! (p2-server `GET /updates/{channel}/{file}`; dosyalar `packaging/publish.mjs` ile yayınlanır).

mod client;
mod engine;
mod error;
mod feed;
mod hash;
mod install;
mod server;
mod updater;
mod version;
mod yml;

pub use client::{ClientUpdater, ClientUpdaterOptions, ConfigureError, IdleCheck, Log, StartupOutcome};
pub use engine::{Broadcast, Status, UpdateState};
pub use error::{ErrorKind, UpdateError, quiet_message};
pub use feed::{ResolvedFile, base_url, cache_file_name, channel_file, client_feed_url, find_file, node_arch, normalize_base, resolve_files, server_feed_url, url_from_base};
pub use hash::{sha512_base64, sha512_base64_file, verify_file};
pub use install::{InstallerLauncher, NsisLauncher, nsis_install_args};
pub use server::{Hook, ServerUpdater, ServerUpdaterOptions, StopServer};
pub use updater::{CACHE_DIR_NAME, CheckResult, DownloadedUpdate, Listener, Progress, Updater, UpdaterConfig, UpdaterEvent, default_cache_dir};
pub use version::{compare as compare_versions, has_prerelease, is_newer_available, parse_version};
pub use yml::{FileInfo, UpdateInfo, parse_update_info};
