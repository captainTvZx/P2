//! P2 kütüphanesi (patcher): içerik tanımlı parçalama, manifest ve kurulum planı, indirme/
//! kurulum/doğrulama/kaldırma, oyun başlatma, sürüm yükleme. Electron'un `electron/patcher*.cjs`
//! ve `cdc.cjs` modüllerinin karşılığı; sunucu ve dosya düzeni aynı.

pub mod cdc;
pub mod engine;
pub mod library;
pub mod manifest;

pub use engine::{Event, JobState, LaunchSpec, Launched, Launcher, Patcher, Sink, UploadState};
pub use library::{InstalledProject, LibraryState};
pub use manifest::{Manifest, ManifestFile, plan_install};
