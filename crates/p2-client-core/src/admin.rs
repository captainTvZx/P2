//! Yönetim paneli uçları (AdminPanel.tsx, AdminTeam.tsx): genel bakış, üyeler, roller,
//! yetki kataloğu, davet kodları, geri dönüşüm, kanallar, kuruluş adı, kütüphane bayrağı,
//! kendi etkin yetkilerim (`permissions/me`).

use std::collections::BTreeMap;

use reqwest::Method;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{Api, ApiError};

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OverviewOrg {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OverviewMembers {
    pub total: u64,
    pub owners: u64,
    pub roles: u64,
    pub online: u64,
    pub joined_this_week: u64,
    pub two_factor: u64,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OverviewContent {
    pub projects: u64,
    pub channels: u64,
    pub deleted_channels: u64,
    pub groups: u64,
    pub channel_messages: u64,
    pub direct_messages: u64,
    pub group_messages: u64,
    pub messages: u64,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct OverviewStorage {
    pub bytes: u64,
    pub files: u64,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct OverviewServer {
    pub version: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Overview {
    pub organization: OverviewOrg,
    /// Proje paneli: genel bakış bu projenin sayılarıdır.
    pub project: Option<crate::models::Project>,
    pub server: OverviewServer,
    pub members: OverviewMembers,
    pub content: OverviewContent,
    pub invites: u64,
    pub sessions: u64,
    pub storage: OverviewStorage,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct RoleRef {
    pub id: String,
    pub handle: String,
    pub name: String,
    pub color: Option<String>,
    pub founder: bool,
    pub via_group: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TeamPerson {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub status: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TeamMember {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub email: String,
    pub avatar_url: Option<String>,
    pub status: String,
    pub two_factor_enabled: bool,
    pub owner: bool,
    /// Proje paneli: OWNER / ADMIN / MEMBER ve birincil sahip.
    pub role: String,
    pub primary_owner: bool,
    pub channel_messages: u64,
    pub joined_at: String,
    pub last_client_version: Option<String>,
    pub last_client_seen_at: Option<String>,
    pub roles: Vec<RoleRef>,
    pub groups: Vec<RoleRef>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct GroupRef {
    pub id: String,
    pub handle: String,
    pub name: String,
    pub color: Option<String>,
    pub member_count: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct TeamRole {
    pub id: String,
    pub handle: String,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub everyone: bool,
    pub founder: bool,
    pub permissions: Vec<String>,
    pub members: Vec<TeamPerson>,
    pub groups: Vec<GroupRef>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct PermissionInfo {
    pub id: String,
    pub group: String,
    pub label: String,
    pub hint: String,
    pub channel: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct PermissionMember {
    pub id: String,
    pub owner: bool,
    pub allow: Vec<String>,
    pub deny: Vec<String>,
    pub effective: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct PermissionData {
    pub catalog: Vec<PermissionInfo>,
    pub members: Vec<PermissionMember>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Invite {
    pub id: String,
    pub code: String,
    pub max_uses: Option<u64>,
    pub use_count: u64,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub created_by: TeamPerson,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct DeletedProject {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub deleted_at: String,
    pub purge_at: Option<String>,
    pub owner: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AdminChannel {
    pub id: String,
    pub name: String,
    pub topic: Option<String>,
    #[serde(rename = "type")]
    pub kind: String,
    pub deleted_at: Option<String>,
    pub purge_at: Option<String>,
    pub messages: u64,
    pub category: String,
    pub project: String,
    pub project_id: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct LibraryProject {
    pub id: String,
    pub name: String,
    pub icon_url: Option<String>,
    pub library_enabled: bool,
}

/// Kendi etkin yetkilerim (`permissions/me`): arayüz hangi düğmeyi göstereceğini buradan bilir.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct MyPermissions {
    /// Hangi projenin yetkileri (`/projects/:p/permissions/me`); kuruluş adresinde proje verilmezse boş (birleşim).
    #[serde(rename = "projectId")]
    pub project_id: Option<String>,
    pub owner: bool,
    pub permissions: Vec<String>,
    pub channels: BTreeMap<String, Vec<String>>,
}

impl MyPermissions {
    pub fn can(&self, permission: &str) -> bool {
        self.owner || self.permissions.iter().any(|p| p == permission)
    }

    pub fn can_in(&self, channel: &str, permission: &str) -> bool {
        self.owner || self.channels.get(channel).map_or_else(|| self.can(permission), |list| list.iter().any(|p| p == permission))
    }
}

/// "Moderatör Ekibi" → "moderator-ekibi" (AdminTeam.tsx rol kimliği).
pub fn role_handle(name: &str) -> String {
    let mut out = String::new();
    for c in name.to_lowercase().chars() {
        let mapped = match c {
            'ı' | 'i' | 'î' => 'i',
            'ğ' => 'g',
            'ü' | 'û' => 'u',
            'ş' => 's',
            'ö' => 'o',
            'ç' => 'c',
            'â' | 'á' | 'à' => 'a',
            c if c.is_ascii_alphanumeric() => c,
            _ => '-',
        };
        if mapped == '-' && (out.is_empty() || out.ends_with('-')) {
            continue;
        }
        out.push(mapped);
    }
    out.trim_end_matches('-').chars().take(24).collect()
}

fn admin(org: &str) -> String {
    format!("/organizations/{org}/admin")
}

impl Api {
    pub fn my_permissions(&self, org: &str) -> Result<MyPermissions, ApiError> {
        self.typed(Method::GET, &format!("/organizations/{org}/permissions/me"), None, true)
    }

    pub fn admin_overview(&self, org: &str) -> Result<Overview, ApiError> {
        self.typed(Method::GET, &format!("{}/overview", admin(org)), None, true)
    }

    pub fn admin_members(&self, org: &str) -> Result<Vec<TeamMember>, ApiError> {
        self.typed(Method::GET, &format!("{}/members", admin(org)), None, true)
    }

    pub fn admin_permissions(&self, org: &str) -> Result<PermissionData, ApiError> {
        self.typed(Method::GET, &format!("{}/permissions", admin(org)), None, true)
    }

    pub fn roles(&self, org: &str) -> Result<Vec<TeamRole>, ApiError> {
        self.typed(Method::GET, &format!("/organizations/{org}/roles"), None, true)
    }

    pub fn create_role(&self, org: &str, name: &str, description: &str, color: &str) -> Result<TeamRole, ApiError> {
        let body = json!({ "handle": role_handle(name), "name": name.trim(), "description": description.trim(), "color": color });
        self.typed(Method::POST, &format!("/organizations/{org}/roles"), Some(body), true)
    }

    pub fn update_role(&self, org: &str, role: &str, patch: Value) -> Result<Value, ApiError> {
        self.request(Method::PATCH, &format!("/organizations/{org}/roles/{role}"), Some(patch), true)
    }

    pub fn set_role_permissions(&self, org: &str, role: &str, permissions: &[String]) -> Result<Value, ApiError> {
        self.request(Method::PATCH, &format!("/organizations/{org}/roles/{role}/permissions"), Some(json!({ "permissions": permissions })), true)
    }

    pub fn delete_role(&self, org: &str, role: &str) -> Result<Value, ApiError> {
        self.request(Method::DELETE, &format!("/organizations/{org}/roles/{role}"), None, true)
    }

    pub fn assign_role(&self, org: &str, role: &str, user: &str, on: bool) -> Result<Value, ApiError> {
        self.request(if on { Method::POST } else { Method::DELETE }, &format!("/organizations/{org}/roles/{role}/members/{user}"), None, true)
    }

    pub fn reset_member_permissions(&self, org: &str, user: &str) -> Result<Value, ApiError> {
        self.request(Method::PATCH, &format!("{}/members/{user}/permissions", admin(org)), Some(json!({ "allow": [], "deny": [] })), true)
    }

    pub fn remove_member(&self, org: &str, user: &str) -> Result<Value, ApiError> {
        self.request(Method::DELETE, &format!("{}/members/{user}", admin(org)), None, true)
    }

    pub fn revoke_sessions(&self, org: &str, user: &str) -> Result<Value, ApiError> {
        self.request(Method::POST, &format!("{}/members/{user}/sessions/revoke", admin(org)), Some(json!({})), true)
    }

    pub fn transfer_owner(&self, org: &str, user: &str) -> Result<Value, ApiError> {
        self.request(Method::POST, &format!("{}/members/{user}/transfer-owner", admin(org)), Some(json!({})), true)
    }

    pub fn invites(&self, org: &str) -> Result<Vec<Invite>, ApiError> {
        self.typed(Method::GET, &format!("{}/invites", admin(org)), None, true)
    }

    pub fn create_invite(&self, org: &str, max_uses: Option<u64>, days: Option<u64>) -> Result<Invite, ApiError> {
        let mut body = json!({});
        if let Some(n) = max_uses.filter(|n| *n > 0) {
            body["maxUses"] = json!(n);
        }
        if let Some(d) = days.filter(|d| *d > 0) {
            body["expiresInDays"] = json!(d);
        }
        self.typed(Method::POST, &format!("{}/invites", admin(org)), Some(body), true)
    }

    pub fn rotate_invite(&self, org: &str) -> Result<Invite, ApiError> {
        self.typed(Method::POST, &format!("{}/invites/rotate", admin(org)), Some(json!({})), true)
    }

    pub fn revoke_invite(&self, org: &str, invite: &str) -> Result<Value, ApiError> {
        self.request(Method::DELETE, &format!("{}/invites/{invite}", admin(org)), None, true)
    }

    pub fn project_trash(&self, org: &str) -> Result<Vec<DeletedProject>, ApiError> {
        self.typed(Method::GET, &format!("{}/projects/trash", admin(org)), None, true)
    }

    pub fn purge_project(&self, org: &str, project: &str) -> Result<Value, ApiError> {
        self.request(Method::DELETE, &format!("{}/projects/{project}/purge", admin(org)), None, true)
    }

    pub fn admin_channels(&self, org: &str) -> Result<Vec<AdminChannel>, ApiError> {
        self.typed(Method::GET, &format!("{}/channels", admin(org)), None, true)
    }

    pub fn rename_organization(&self, org: &str, name: &str) -> Result<Value, ApiError> {
        self.request(Method::PATCH, &admin(org), Some(json!({ "name": name.trim() })), true)
    }

    pub fn library_projects(&self, org: &str) -> Result<Vec<LibraryProject>, ApiError> {
        self.typed(Method::GET, &format!("/organizations/{org}/projects"), None, true)
    }

    pub fn set_library_enabled(&self, org: &str, project: &str, on: bool) -> Result<LibraryProject, ApiError> {
        self.typed(Method::PATCH, &format!("/organizations/{org}/projects/{project}"), Some(json!({ "libraryEnabled": on })), true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles() {
        assert_eq!(role_handle("Moderatör Ekibi"), "moderator-ekibi");
        assert_eq!(role_handle("  Ses  Şefi! "), "ses-sefi");
        let me = MyPermissions { permissions: vec!["member.invite".into()], ..Default::default() };
        assert!(me.can("member.invite") && !me.can("member.kick"));
    }
}
