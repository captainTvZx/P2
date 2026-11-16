//! Sunucu (kurulum) yönetimi uçları (`/server/...`): genel bakış, kişiler (yasak, oturum, 2FA,
//! sunucu yetkileri), bütün projeler (gir, devret, sil, geri yükle, kalıcı sil) ve kurulum ayarları.

use reqwest::Method;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{Api, ApiError};

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ServerPermissionInfo {
    pub id: String,
    pub label: String,
    pub description: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ServerMe {
    pub owner: bool,
    pub server_permissions: Vec<String>,
    pub panel: bool,
    pub catalog: Vec<ServerPermissionInfo>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ServerOverview {
    pub name: Option<String>,
    pub organization_id: Option<String>,
    pub version: String,
    pub users: u64,
    pub banned: u64,
    pub projects: u64,
    pub deleted_projects: u64,
    pub sessions: u64,
    pub channels: u64,
    pub online: u64,
    pub registration_policy: String,
    pub project_creation: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ServerBanInfo {
    pub reason: Option<String>,
    pub created_at: Option<String>,
    pub banned_by: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ServerUserProject {
    pub id: String,
    pub name: String,
    pub role: String,
    pub primary_owner: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ServerUser {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub email: String,
    pub avatar_url: Option<String>,
    pub status: String,
    pub two_factor_enabled: bool,
    pub created_at: Option<String>,
    pub last_seen_at: Option<String>,
    /// Kurulum sahibi (ilk yönetici / kuruluş sahibi): bütün yetkiler, dokunulamaz.
    pub admin: bool,
    /// Doğrudan verilmiş sunucu yetkileri.
    pub permissions: Vec<String>,
    pub sessions: u64,
    pub ban: Option<ServerBanInfo>,
    pub projects: Vec<ServerUserProject>,
    pub online: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ServerProjectOwner {
    pub id: String,
    pub username: String,
    pub display_name: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ServerProject {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub organization_id: Option<String>,
    pub visibility: String,
    pub created_at: Option<String>,
    pub deleted_at: Option<String>,
    pub purge_at: Option<String>,
    pub owner: Option<ServerProjectOwner>,
    pub members: u64,
    /// Ben üye miyim.
    pub member: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ServerSettings {
    pub name: Option<String>,
    pub organization_id: Option<String>,
    /// `open` | `invite`
    pub registration_policy: String,
    /// `permission` | `everyone`
    pub project_creation: String,
}

impl Api {
    pub fn server_me(&self) -> Result<ServerMe, ApiError> {
        self.typed(Method::GET, "/server/me", None, true)
    }

    pub fn server_overview(&self) -> Result<ServerOverview, ApiError> {
        self.typed(Method::GET, "/server/overview", None, true)
    }

    pub fn server_users(&self, query: &str) -> Result<Vec<ServerUser>, ApiError> {
        let q: String = query.trim().chars().map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c.to_string() } else { c.to_string().bytes().map(|b| format!("%{b:02X}")).collect() }).collect();
        self.typed(Method::GET, &format!("/server/users?q={q}"), None, true)
    }

    pub fn server_revoke_sessions(&self, user: &str) -> Result<Value, ApiError> {
        self.request(Method::POST, &format!("/server/users/{user}/sessions/revoke"), None, true)
    }

    pub fn server_reset_two_factor(&self, user: &str) -> Result<Value, ApiError> {
        self.request(Method::POST, &format!("/server/users/{user}/2fa/reset"), None, true)
    }

    pub fn server_ban(&self, user: &str, reason: &str) -> Result<Value, ApiError> {
        let reason = reason.trim();
        self.request(Method::PUT, &format!("/server/users/{user}/ban"), Some(json!({ "reason": if reason.is_empty() { Value::Null } else { json!(reason) } })), true)
    }

    pub fn server_unban(&self, user: &str) -> Result<Value, ApiError> {
        self.request(Method::DELETE, &format!("/server/users/{user}/ban"), None, true)
    }

    /// Kişinin doğrudan sunucu yetkilerini (tam liste) yazar.
    pub fn server_set_permissions(&self, user: &str, permissions: &[String]) -> Result<Value, ApiError> {
        self.request(Method::PUT, &format!("/server/users/{user}/permissions"), Some(json!({ "permissions": permissions })), true)
    }

    pub fn server_projects(&self) -> Result<Vec<ServerProject>, ApiError> {
        self.typed(Method::GET, "/server/projects", None, true)
    }

    /// Projeye gir (sahip üye olarak eklenir): `{ project, organization, organizationId, joined }`.
    pub fn server_enter_project(&self, project: &str) -> Result<Value, ApiError> {
        self.request(Method::POST, &format!("/server/projects/{project}/enter"), None, true)
    }

    pub fn server_transfer_project(&self, project: &str, user: &str) -> Result<Value, ApiError> {
        self.request(Method::POST, &format!("/server/projects/{project}/transfer"), Some(json!({ "userId": user })), true)
    }

    pub fn server_delete_project(&self, project: &str) -> Result<Value, ApiError> {
        self.request(Method::DELETE, &format!("/server/projects/{project}"), None, true)
    }

    pub fn server_restore_project(&self, project: &str) -> Result<Value, ApiError> {
        self.request(Method::POST, &format!("/server/projects/{project}/restore"), None, true)
    }

    pub fn server_purge_project(&self, project: &str) -> Result<Value, ApiError> {
        self.request(Method::DELETE, &format!("/server/projects/{project}/purge"), None, true)
    }

    pub fn server_settings(&self) -> Result<ServerSettings, ApiError> {
        self.typed(Method::GET, "/server/settings", None, true)
    }

    /// `name`, `registrationPolicy`, `projectCreation` alanlarından verilenleri yazar.
    pub fn server_update_settings(&self, patch: Value) -> Result<ServerSettings, ApiError> {
        self.typed(Method::PATCH, "/server/settings", Some(patch), true)
    }
}
