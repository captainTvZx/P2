//! Proje ve kanal yönetimi (Workspace.tsx proje menüsü, ProjectSettingsDialog.tsx,
//! ChannelPermissions.tsx, AdminAccess.tsx): proje oluştur/sil/ayarla, kategori ve kanal
//! oluştur/sil, varsayılan kanal, kanal izinleri (rol/grup/kişi kuralı), proje/kanal erişimi.

use reqwest::Method;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::admin::PermissionInfo;
use crate::api::{Api, ApiError};
use crate::models::{Channel, ChannelCategory, Project};

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AccessMember {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub owner: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AccessChannel {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub category: String,
    pub visible_user_ids: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AccessProject {
    pub id: String,
    pub name: String,
    pub owner_id: String,
    pub member_ids: Vec<String>,
    pub channels: Vec<AccessChannel>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct AccessData {
    pub members: Vec<AccessMember>,
    pub projects: Vec<AccessProject>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Override {
    pub target_type: String,
    pub target_id: String,
    pub allow: Vec<String>,
    pub deny: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct PermTarget {
    pub id: String,
    pub handle: String,
    pub name: String,
    pub username: String,
    pub display_name: String,
    pub color: Option<String>,
    pub everyone: bool,
    pub owner: bool,
    pub member_count: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ChannelPermissions {
    pub catalog: Vec<PermissionInfo>,
    pub overrides: Vec<Override>,
    pub roles: Vec<PermTarget>,
    pub groups: Vec<PermTarget>,
    pub members: Vec<PermTarget>,
    pub mine: Vec<String>,
    pub can_edit: bool,
}

/// "Yeni Oyun!" → "yeni-oyun" (Workspace.tsx ProjectCreator `slug`).
pub fn project_slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.to_lowercase().chars() {
        let mapped = match c {
            'ı' | 'î' => 'i',
            'ğ' => 'g',
            'ü' | 'û' => 'u',
            'ş' => 's',
            'ö' => 'o',
            'ç' => 'c',
            'â' => 'a',
            c if c.is_ascii_alphanumeric() => c,
            _ => '-',
        };
        if mapped == '-' && (out.is_empty() || out.ends_with('-')) {
            continue;
        }
        out.push(mapped);
    }
    out.trim_end_matches('-').chars().take(40).collect()
}

fn proj(org: &str, project: &str) -> String {
    format!("/organizations/{org}/projects/{project}")
}

impl Api {
    pub fn create_project(&self, org: &str, name: &str, description: &str) -> Result<Project, ApiError> {
        self.typed(Method::POST, &format!("/organizations/{org}/projects"), Some(json!({ "name": name.trim(), "slug": project_slug(name), "description": description.trim() })), true)
    }

    /// `patch`: `{ iconUrl?, coverUrl?, libraryEnabled?, defaultChannelId?, name? }`
    pub fn update_project(&self, org: &str, project: &str, patch: Value) -> Result<Project, ApiError> {
        self.typed(Method::PATCH, &proj(org, project), Some(patch), true)
    }

    pub fn delete_project(&self, org: &str, project: &str) -> Result<Value, ApiError> {
        self.request(Method::DELETE, &proj(org, project), None, true)
    }

    pub fn create_category(&self, org: &str, project: &str, name: &str) -> Result<ChannelCategory, ApiError> {
        self.typed(Method::POST, &format!("{}/channel-categories", proj(org, project)), Some(json!({ "name": name.trim() })), true)
    }

    pub fn delete_category(&self, org: &str, project: &str, category: &str) -> Result<Value, ApiError> {
        self.request(Method::DELETE, &format!("{}/channel-categories/{category}", proj(org, project)), None, true)
    }

    /// `kind`: TEXT | ANNOUNCEMENT | VOICE | FORUM
    pub fn create_channel(&self, org: &str, project: &str, category: &str, name: &str, kind: &str) -> Result<Channel, ApiError> {
        self.typed(Method::POST, &format!("{}/channels", proj(org, project)), Some(json!({ "categoryId": category, "name": name.trim(), "type": kind })), true)
    }

    pub fn update_channel(&self, org: &str, project: &str, channel: &str, patch: Value) -> Result<Value, ApiError> {
        self.request(Method::PATCH, &format!("{}/channels/{channel}", proj(org, project)), Some(patch), true)
    }

    pub fn delete_channel(&self, org: &str, project: &str, channel: &str) -> Result<Value, ApiError> {
        self.request(Method::DELETE, &format!("{}/channels/{channel}", proj(org, project)), None, true)
    }

    pub fn channel_permissions(&self, org: &str, project: &str, channel: &str) -> Result<ChannelPermissions, ApiError> {
        self.typed(Method::GET, &format!("{}/channels/{channel}/permissions", proj(org, project)), None, true)
    }

    /// Hedefin (ROLE/GROUP/USER) kanal kuralı; ikisi de boşsa sunucu kuralı siler.
    pub fn set_channel_permission(&self, org: &str, project: &str, channel: &str, target_type: &str, target: &str, allow: &[String], deny: &[String]) -> Result<Value, ApiError> {
        self.request(Method::PUT, &format!("{}/channels/{channel}/permissions/{target_type}/{target}", proj(org, project)), Some(json!({ "allow": allow, "deny": deny })), true)
    }

    pub fn clear_channel_permission(&self, org: &str, project: &str, channel: &str, target_type: &str, target: &str) -> Result<Value, ApiError> {
        self.request(Method::DELETE, &format!("{}/channels/{channel}/permissions/{target_type}/{target}", proj(org, project)), None, true)
    }

    pub fn access(&self, org: &str) -> Result<AccessData, ApiError> {
        self.typed(Method::GET, &format!("/organizations/{org}/admin/access"), None, true)
    }

    pub fn set_project_access(&self, org: &str, project: &str, user: &str, visible: bool) -> Result<Value, ApiError> {
        self.request(Method::PATCH, &format!("/organizations/{org}/admin/access/projects/{project}/members/{user}"), Some(json!({ "visible": visible })), true)
    }

    pub fn set_channel_access(&self, org: &str, channel: &str, user: &str, visible: bool) -> Result<Value, ApiError> {
        self.request(Method::PATCH, &format!("/organizations/{org}/admin/access/channels/{channel}/members/{user}"), Some(json!({ "visible": visible })), true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert_eq!(project_slug("Yeni Oyun!"), "yeni-oyun");
        assert_eq!(project_slug("  Çağrı Şöleni 2 "), "cagri-soleni-2");
    }
}
