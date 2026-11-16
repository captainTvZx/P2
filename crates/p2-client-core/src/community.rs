//! Proje toplulukları (docs/proje-bazli-yapi.md): proje davet kodları (önizleme, katılma), projelerim,
//! herkesin proje açması, projeden ayrılma, proje kişi listesi ve proje yönetim paneli uçları
//! (`/projects/:p/admin/...`, roller, kullanıcı grupları, yasaklar, ayarlar, kendi yetkilerim).
//!
//! Eski kuruluş adresli uçlar (`admin.rs`, `manage.rs`) sunucuda proje kapsamına eşlenir ama projeyi
//! tahmin eder; yeni arayüz buradaki proje adresli uçları kullanmalı.

use std::collections::BTreeMap;

use reqwest::Method;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::admin::{AdminChannel, MyPermissions, Overview, PermissionData, TeamMember, TeamPerson, TeamRole, role_handle};
use crate::api::{Api, ApiError, enc};
use crate::manage::AccessData;
use crate::models::{Auth, Organization, Project, RegisterInput};

/// Davet önizlemesindeki proje.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct InviteProject {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub cover_url: Option<String>,
    pub member_count: u64,
}

/// `GET /invites/:code` (oturum gerekmez). `valid=false` ise `reason`: INVITE_EXPIRED / INVITE_FULL / INVITE_REVOKED.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct InvitePreview {
    pub code: String,
    pub project: InviteProject,
    pub expires_at: Option<String>,
    pub max_uses: Option<u64>,
    pub use_count: u64,
    pub valid: bool,
    pub reason: Option<String>,
}

/// `POST /invites/:code/join` / `POST /projects/join`.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JoinResult {
    pub project: Project,
    pub organization: Organization,
    pub project_id: String,
    pub organization_id: String,
    /// Zaten üyeyse `false`.
    #[serde(default)]
    pub joined: bool,
}

/// `GET /projects`: üye olduğum projeler (her kuruluştan) ve oradaki rolüm.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct MyProject {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub organization_id: String,
    pub owner_id: Option<String>,
    pub icon_url: Option<String>,
    pub cover_url: Option<String>,
    pub default_channel_id: Option<String>,
    pub library_enabled: bool,
    /// OWNER / ADMIN / MEMBER
    pub role: String,
    pub owner: bool,
    pub primary_owner: bool,
}

/// Projenin kişi listesi (`GET /projects/:p/members`).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectMember {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub email: Option<String>,
    pub title: Option<String>,
    pub avatar_url: Option<String>,
    pub status: String,
    pub role: String,
    pub owner: bool,
    pub primary_owner: bool,
    pub joined_at: String,
}

impl From<ProjectMember> for crate::models::Member {
    /// Kenar çubuğundaki kişi listesi `Member` kullanır; proje kişi listesi ona çevrilir.
    fn from(m: ProjectMember) -> Self {
        Self { id: m.id, username: m.username, display_name: m.display_name, title: m.title, avatar_url: m.avatar_url, status: m.status, role: m.role, email: m.email }
    }
}

/// Davet / katılma / proje hata kodlarının arayüz metni (sunucu iletisi yoksa ya da koda özel metin
/// gösterilecekse). Bilinmeyen kodda `None`.
pub fn community_error_text(code: &str) -> Option<&'static str> {
    Some(match code {
        "INVITE_NOT_FOUND" => "Bu davet kodu bulunamadı. Kodu doğru yazdığından emin ol.",
        "INVITE_REVOKED" => "Bu davet kodu iptal edilmiş. Proje yöneticisinden yeni bir kod iste.",
        "INVITE_EXPIRED" => "Bu davetin süresi dolmuş. Proje yöneticisinden yeni bir kod iste.",
        "INVITE_FULL" => "Bu davetin kullanım sınırı dolmuş. Proje yöneticisinden yeni bir kod iste.",
        "PROJECT_BANNED" => "Bu projeden yasaklandın; yasak kalkmadan tekrar katılamazsın.",
        "OWNER_CANNOT_LEAVE" => "Projenin sahibi olduğun için ayrılamazsın. Önce Yönetim › Üyeler'den sahipliği başka birine devret.",
        "NO_SHARED_PROJECT" => "Bu kişiyle artık ortak bir projen yok; yeni mesaj gönderemez ve arayamazsın. Eski mesajlar okunabilir kalır.",
        _ => return None,
    })
}

/// Proje davet kodu (yönetim paneli).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectInvite {
    pub id: String,
    pub code: String,
    pub project_id: String,
    pub max_uses: Option<u64>,
    pub use_count: u64,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub revoked: bool,
    pub revoked_at: Option<String>,
    pub created_by: TeamPerson,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectBan {
    pub user_id: String,
    pub reason: Option<String>,
    pub created_at: String,
    pub user: TeamPerson,
    pub banned_by: Option<TeamPerson>,
}

/// Proje kullanıcı grubu (@devs).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectUserGroup {
    pub id: String,
    pub handle: String,
    pub name: String,
    pub description: Option<String>,
    pub color: Option<String>,
    pub members: Vec<TeamPerson>,
}

/// `GET /organizations/:org/permissions/me` (proje vermeden) cevabındaki proje başına döküm.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct ProjectGrant {
    pub owner: bool,
    pub permissions: Vec<String>,
}

/// Birleşik yetki cevabı: `projects` proje başına, `channels` her kanal kendi projesine göre.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AllPermissions {
    pub owner: bool,
    pub permissions: Vec<String>,
    pub channels: BTreeMap<String, Vec<String>>,
    pub projects: BTreeMap<String, ProjectGrant>,
}

impl AllPermissions {
    pub fn can_in_project(&self, project: &str, permission: &str) -> bool {
        self.projects.get(project).is_some_and(|g| g.owner || g.permissions.iter().any(|p| p == permission))
    }
}

fn padmin(project: &str) -> String {
    format!("/projects/{project}/admin")
}

impl Api {
    // ---------------------------------------------------------------- katılma

    /// Davet kodunu katılmadan önce göster (oturum gerekmez).
    pub fn invite_preview(&self, code: &str) -> Result<InvitePreview, ApiError> {
        self.typed(Method::GET, &format!("/invites/{}", enc(code.trim())), None, false)
    }

    pub fn join_project(&self, code: &str) -> Result<JoinResult, ApiError> {
        self.typed(Method::POST, "/projects/join", Some(json!({ "code": code.trim() })), true)
    }

    /// Kayıt + davet kodu: kod önce doğrulanır, hesap açılınca o projeye katılınır.
    pub fn register_with_invite(&self, input: &RegisterInput, code: &str) -> Result<Auth, ApiError> {
        let mut body = serde_json::to_value(input).unwrap_or_else(|_| json!({}));
        body["inviteCode"] = json!(code.trim());
        self.typed(Method::POST, "/auth/register", Some(body), false)
    }

    // ---------------------------------------------------------------- projelerim

    pub fn my_projects(&self) -> Result<Vec<MyProject>, ApiError> {
        self.typed(Method::GET, "/projects", None, true)
    }

    /// Herkes proje açar; kuruluş verilmezse sunucu seçer.
    pub fn open_project(&self, name: &str, description: &str, organization: Option<&str>) -> Result<Project, ApiError> {
        let mut body = json!({ "name": name.trim(), "slug": crate::manage::project_slug(name), "description": description.trim() });
        if let Some(org) = organization {
            body["organizationId"] = json!(org);
        }
        self.typed(Method::POST, "/projects", Some(body), true)
    }

    pub fn leave_project(&self, project: &str) -> Result<Value, ApiError> {
        self.request(Method::POST, &format!("/projects/{project}/leave"), Some(json!({})), true)
    }

    pub fn project_members(&self, project: &str) -> Result<Vec<ProjectMember>, ApiError> {
        self.typed(Method::GET, &format!("/projects/{project}/members"), None, true)
    }

    pub fn project_permissions(&self, project: &str) -> Result<MyPermissions, ApiError> {
        self.typed(Method::GET, &format!("/projects/{project}/permissions/me"), None, true)
    }

    pub fn all_permissions(&self, org: &str) -> Result<AllPermissions, ApiError> {
        self.typed(Method::GET, &format!("/organizations/{org}/permissions/me"), None, true)
    }

    // ---------------------------------------------------------------- proje yönetim paneli

    /// Ad, açıklama, ikon/kapak (`/uploads/...`), varsayılan kanal, kütüphane bayrağı.
    pub fn update_project_settings(&self, project: &str, patch: Value) -> Result<Project, ApiError> {
        self.typed(Method::PATCH, &padmin(project), Some(patch), true)
    }

    pub fn project_overview(&self, project: &str) -> Result<Overview, ApiError> {
        self.typed(Method::GET, &format!("{}/overview", padmin(project)), None, true)
    }

    pub fn project_admin_members(&self, project: &str) -> Result<Vec<TeamMember>, ApiError> {
        self.typed(Method::GET, &format!("{}/members", padmin(project)), None, true)
    }

    pub fn project_admin_permissions(&self, project: &str) -> Result<PermissionData, ApiError> {
        self.typed(Method::GET, &format!("{}/permissions", padmin(project)), None, true)
    }

    pub fn project_admin_channels(&self, project: &str) -> Result<Vec<AdminChannel>, ApiError> {
        self.typed(Method::GET, &format!("{}/channels", padmin(project)), None, true)
    }

    pub fn project_access(&self, project: &str) -> Result<AccessData, ApiError> {
        self.typed(Method::GET, &format!("{}/access", padmin(project)), None, true)
    }

    pub fn set_member_access(&self, project: &str, user: &str, visible: bool) -> Result<Value, ApiError> {
        self.request(Method::PATCH, &format!("{}/access/members/{user}", padmin(project)), Some(json!({ "visible": visible })), true)
    }

    pub fn set_member_channel_access(&self, project: &str, channel: &str, user: &str, visible: bool) -> Result<Value, ApiError> {
        self.request(Method::PATCH, &format!("{}/access/channels/{channel}/members/{user}", padmin(project)), Some(json!({ "visible": visible })), true)
    }

    pub fn kick_member(&self, project: &str, user: &str) -> Result<Value, ApiError> {
        self.request(Method::DELETE, &format!("{}/members/{user}", padmin(project)), None, true)
    }

    pub fn project_bans(&self, project: &str) -> Result<Vec<ProjectBan>, ApiError> {
        self.typed(Method::GET, &format!("{}/bans", padmin(project)), None, true)
    }

    pub fn ban_member(&self, project: &str, user: &str, reason: &str) -> Result<Value, ApiError> {
        self.request(Method::PUT, &format!("{}/bans/{user}", padmin(project)), Some(json!({ "reason": reason.trim() })), true)
    }

    pub fn unban_member(&self, project: &str, user: &str) -> Result<Value, ApiError> {
        self.request(Method::DELETE, &format!("{}/bans/{user}", padmin(project)), None, true)
    }

    pub fn transfer_project_owner(&self, project: &str, user: &str) -> Result<Value, ApiError> {
        self.request(Method::POST, &format!("{}/members/{user}/transfer-owner", padmin(project)), Some(json!({})), true)
    }

    pub fn set_member_permissions(&self, project: &str, user: &str, allow: &[String], deny: &[String]) -> Result<Value, ApiError> {
        self.request(Method::PATCH, &format!("{}/members/{user}/permissions", padmin(project)), Some(json!({ "allow": allow, "deny": deny })), true)
    }

    pub fn project_invites(&self, project: &str, include_revoked: bool) -> Result<Vec<ProjectInvite>, ApiError> {
        let suffix = if include_revoked { "?includeRevoked=1" } else { "" };
        self.typed(Method::GET, &format!("{}/invites{suffix}", padmin(project)), None, true)
    }

    pub fn create_project_invite(&self, project: &str, max_uses: Option<u64>, days: Option<u64>) -> Result<ProjectInvite, ApiError> {
        let mut body = json!({});
        if let Some(n) = max_uses.filter(|n| *n > 0) {
            body["maxUses"] = json!(n);
        }
        if let Some(d) = days.filter(|d| *d > 0) {
            body["expiresInDays"] = json!(d);
        }
        self.typed(Method::POST, &format!("{}/invites", padmin(project)), Some(body), true)
    }

    pub fn rotate_project_invite(&self, project: &str) -> Result<ProjectInvite, ApiError> {
        self.typed(Method::POST, &format!("{}/invites/rotate", padmin(project)), Some(json!({})), true)
    }

    pub fn revoke_project_invite(&self, project: &str, invite: &str) -> Result<Value, ApiError> {
        self.request(Method::DELETE, &format!("{}/invites/{invite}", padmin(project)), None, true)
    }

    // ---------------------------------------------------------------- roller ve gruplar

    pub fn project_roles(&self, project: &str) -> Result<Vec<TeamRole>, ApiError> {
        self.typed(Method::GET, &format!("/projects/{project}/roles"), None, true)
    }

    pub fn create_project_role(&self, project: &str, name: &str, description: &str, color: &str) -> Result<TeamRole, ApiError> {
        let body = json!({ "handle": role_handle(name), "name": name.trim(), "description": description.trim(), "color": color });
        self.typed(Method::POST, &format!("/projects/{project}/roles"), Some(body), true)
    }

    pub fn update_project_role(&self, project: &str, role: &str, patch: Value) -> Result<Value, ApiError> {
        self.request(Method::PATCH, &format!("/projects/{project}/roles/{role}"), Some(patch), true)
    }

    pub fn set_project_role_permissions(&self, project: &str, role: &str, permissions: &[String]) -> Result<Value, ApiError> {
        self.request(Method::PATCH, &format!("/projects/{project}/roles/{role}/permissions"), Some(json!({ "permissions": permissions })), true)
    }

    pub fn delete_project_role(&self, project: &str, role: &str) -> Result<Value, ApiError> {
        self.request(Method::DELETE, &format!("/projects/{project}/roles/{role}"), None, true)
    }

    pub fn assign_project_role(&self, project: &str, role: &str, user: &str, on: bool) -> Result<Value, ApiError> {
        self.request(if on { Method::POST } else { Method::DELETE }, &format!("/projects/{project}/roles/{role}/members/{user}"), None, true)
    }

    pub fn link_project_role_group(&self, project: &str, role: &str, group: &str, on: bool) -> Result<Value, ApiError> {
        self.request(if on { Method::POST } else { Method::DELETE }, &format!("/projects/{project}/roles/{role}/groups/{group}"), None, true)
    }

    pub fn project_user_groups(&self, project: &str) -> Result<Vec<ProjectUserGroup>, ApiError> {
        self.typed(Method::GET, &format!("/projects/{project}/user-groups"), None, true)
    }

    pub fn create_project_user_group(&self, project: &str, handle: &str, name: &str, color: &str) -> Result<ProjectUserGroup, ApiError> {
        self.typed(Method::POST, &format!("/projects/{project}/user-groups"), Some(json!({ "handle": handle, "name": name.trim(), "color": color })), true)
    }

    pub fn update_project_user_group(&self, project: &str, group: &str, patch: Value) -> Result<ProjectUserGroup, ApiError> {
        self.typed(Method::PATCH, &format!("/projects/{project}/user-groups/{group}"), Some(patch), true)
    }

    pub fn delete_project_user_group(&self, project: &str, group: &str) -> Result<Value, ApiError> {
        self.request(Method::DELETE, &format!("/projects/{project}/user-groups/{group}"), None, true)
    }

    pub fn add_project_user_group_members(&self, project: &str, group: &str, users: &[String]) -> Result<ProjectUserGroup, ApiError> {
        self.typed(Method::POST, &format!("/projects/{project}/user-groups/{group}/members"), Some(json!({ "userIds": users })), true)
    }

    pub fn remove_project_user_group_member(&self, project: &str, group: &str, user: &str) -> Result<Value, ApiError> {
        self.request(Method::DELETE, &format!("/projects/{project}/user-groups/{group}/members/{user}"), None, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn onizleme_ve_katilma() {
        let preview: InvitePreview = serde_json::from_value(json!({
            "code": "P2-AB12CD34", "project": { "id": "p", "name": "Oyun", "memberCount": 3, "iconUrl": null },
            "expiresAt": null, "maxUses": 5, "useCount": 1, "valid": true, "reason": null
        }))
        .unwrap();
        assert!(preview.valid && preview.project.member_count == 3 && preview.max_uses == Some(5));
        let joined: JoinResult = serde_json::from_value(json!({
            "project": { "id": "p", "name": "Oyun", "slug": "oyun" },
            "organization": { "id": "o", "name": "Ekip", "slug": "ekip", "role": "MEMBER" },
            "projectId": "p", "organizationId": "o", "joined": true
        }))
        .unwrap();
        assert_eq!(joined.organization.role, "MEMBER");
    }

    #[test]
    fn birlesik_yetkiler() {
        let all: AllPermissions = serde_json::from_value(json!({
            "owner": false, "permissions": ["channel.view"], "channels": { "c": ["channel.view"] },
            "projects": { "a": { "owner": true, "permissions": [] }, "b": { "owner": false, "permissions": ["member.invite"] } }
        }))
        .unwrap();
        assert!(all.can_in_project("a", "member.kick"));
        assert!(all.can_in_project("b", "member.invite") && !all.can_in_project("b", "member.kick"));
        assert!(!all.can_in_project("c", "channel.view"));
    }

    #[test]
    fn hata_metinleri() {
        for code in ["INVITE_NOT_FOUND", "INVITE_REVOKED", "INVITE_EXPIRED", "INVITE_FULL", "PROJECT_BANNED", "OWNER_CANNOT_LEAVE", "NO_SHARED_PROJECT"] {
            assert!(community_error_text(code).is_some(), "{code}");
        }
        assert!(community_error_text("BAŞKA").is_none());
    }
}
