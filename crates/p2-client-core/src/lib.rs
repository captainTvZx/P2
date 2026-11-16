//! P2 istemci çekirdeği: sunucu API'si, oturum ve yerel ayarlar.
//! `src/apiBase.ts` + App.tsx `api()` + Electron `session:*` IPC karşılığı.
//!
//! Çağrılar engelleyicidir (blocking); arayüz bunları GPUI'nin arka plan yürütücüsünde çalıştırır.

pub mod activity;
pub mod admin;
pub mod api;
pub mod board;
pub mod chat_extra;
pub mod community;
pub mod documents;
pub mod library_api;
pub mod migrate;
pub mod manage;
pub mod models;
pub mod realtime;
pub mod store;

pub use api::{Api, ApiError};
pub use reqwest::Method;
pub use models::*;
pub use activity::*;
pub use admin::*;
pub use board::*;
pub use chat_extra::*;
pub use community::*;
pub use documents::*;
pub use library_api::*;
pub use manage::*;
