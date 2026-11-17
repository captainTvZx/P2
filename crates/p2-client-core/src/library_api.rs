//! Kütüphane sayfasının sunucu verileri (src/library.ts, api/notes.ts, api/screenshots.ts):
//! sürümler ve kategoriler, proje merkezi (gönderi, başarım, inceleme, oynama süresi),
//! proje notları, ekran görüntüleri.

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

use crate::api::{Api, ApiError};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HubUser {
    pub id: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseView {
    pub id: String,
    pub channel: String,
    pub version: String,
    #[serde(default)]
    pub notes: Option<String>,
    /// DRAFT | PUBLISHED | ARCHIVED
    pub status: String,
    #[serde(default)]
    pub executable: String,
    #[serde(default)]
    pub total_size: u64,
    #[serde(default)]
    pub file_count: u64,
    #[serde(default)]
    pub chunk_count: u64,
    #[serde(default)]
    pub install_count: u64,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub live_at: Option<String>,
    #[serde(default)]
    pub created_by: Option<HubUser>,
    #[serde(default)]
    pub launch_args: Option<String>,
    /// Sürüm bazında overlay bayrağı (varsayılan açık).
    #[serde(default = "yes")]
    pub overlay: bool,
}

fn yes() -> bool {
    true
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ReleasePermissions {
    #[serde(default)]
    pub publish: bool,
    #[serde(default)]
    pub download: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ReleaseList {
    #[serde(default)]
    pub releases: Vec<ReleaseView>,
    /// Kategori → canlı sürüm.
    #[serde(default)]
    pub latest: BTreeMap<String, Option<ReleaseView>>,
    #[serde(default)]
    pub channels: Vec<String>,
    #[serde(default)]
    pub permissions: ReleasePermissions,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HubPost {
    pub id: String,
    /// POST | RELEASE
    pub kind: String,
    pub content: String,
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub author_id: String,
    #[serde(default)]
    pub author: HubUser,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HubUnlock {
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub user: HubUser,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HubAchievement {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub unlocked_by_me: bool,
    #[serde(default)]
    pub unlocks: Vec<HubUnlock>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HubReview {
    pub id: String,
    #[serde(default)]
    pub user_id: String,
    pub recommended: bool,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub user: HubUser,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Playtime {
    #[serde(default)]
    pub mine: u64,
    #[serde(default)]
    pub last_played_at: Option<String>,
    #[serde(default)]
    pub team: u64,
    #[serde(default)]
    pub players: u64,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct HubPermissions {
    #[serde(default)]
    pub post: bool,
    #[serde(default)]
    pub manage: bool,
    #[serde(default)]
    pub review: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct HubData {
    #[serde(default)]
    pub posts: Vec<HubPost>,
    #[serde(default)]
    pub achievements: Vec<HubAchievement>,
    #[serde(default)]
    pub reviews: Vec<HubReview>,
    #[serde(default)]
    pub playtime: Playtime,
    #[serde(default)]
    pub permissions: HubPermissions,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectNote {
    pub id: String,
    pub author_id: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
    /// PRIVATE | TEAM
    pub visibility: String,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub version: i64,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub author: HubUser,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectScreenshot {
    pub id: String,
    pub upload_path: String,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default)]
    pub caption: Option<String>,
    #[serde(default)]
    pub taken_at: String,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub user: HubUser,
}

/// ReleaseDiff (library.ts): önceki paketle dosya farkı.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseDiff {
    #[serde(default)]
    pub against: Option<DiffAgainst>,
    #[serde(default)]
    pub added: u64,
    #[serde(default)]
    pub removed: u64,
    #[serde(default)]
    pub changed: u64,
    #[serde(default)]
    pub unchanged: u64,
    #[serde(default)]
    pub added_bytes: u64,
    #[serde(default)]
    pub removed_bytes: u64,
    #[serde(default)]
    pub changed_bytes: u64,
    #[serde(default)]
    pub download_bytes: u64,
    #[serde(default)]
    pub files: DiffFiles,
    #[serde(default)]
    pub truncated: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiffAgainst {
    pub version: String,
    #[serde(default)]
    pub channel: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiffFile {
    pub path: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub previous_size: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct DiffFiles {
    #[serde(default)]
    pub added: Vec<DiffFile>,
    #[serde(default)]
    pub removed: Vec<DiffFile>,
    #[serde(default)]
    pub changed: Vec<DiffFile>,
}

/// Bir sürümü kuranlar (InstallsDialog).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseInstall {
    #[serde(default)]
    pub device_id: String,
    #[serde(default)]
    pub release_id: Option<String>,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub play_seconds: Option<u64>,
    #[serde(default)]
    pub user: HubUser,
}

/// "1,4 GB"
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 { format!("{bytes} B") } else { format!("{:.1} {}", v, UNITS[u]).replace('.', ",") }
}

/// "3 sa 12 dk"
pub fn human_duration(seconds: u64) -> String {
    let (h, m) = (seconds / 3600, (seconds % 3600) / 60);
    match (h, m) {
        (0, 0) => p2_i18n::t("core.dur.zero"),
        (0, m) => p2_i18n::tv("core.dur.m", &[("m", &m.to_string())]),
        (h, 0) => p2_i18n::tv("core.dur.h", &[("h", &h.to_string())]),
        (h, m) => p2_i18n::tv("core.dur.hm", &[("h", &h.to_string()), ("m", &m.to_string())]),
    }
}

fn root(org: &str, project: &str) -> String {
    format!("/organizations/{org}/projects/{project}")
}

/// `encodeURIComponent` (kategori adı serbest metin).
fn urlencoding(value: &str) -> String {
    let mut out = String::new();
    for b in value.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

impl Api {
    pub fn releases(&self, org: &str, project: &str) -> Result<ReleaseList, ApiError> {
        self.typed(Method::GET, &format!("{}/releases", root(org, project)), None, true)
    }

    /// Kategorinin canlı (yoksa son yayınlanmış) sürümü — otomatik güncelleme denetimi.
    pub fn latest_release(&self, org: &str, project: &str, channel: &str) -> Result<Option<ReleaseView>, ApiError> {
        #[derive(Deserialize)]
        struct Latest {
            release: Option<ReleaseView>,
        }
        let path = format!("{}/releases/latest?channel={}", root(org, project), urlencoding(channel));
        Ok(self.typed::<Latest>(Method::GET, &path, None, true)?.release)
    }

    pub fn publish_release(&self, org: &str, project: &str, release: &str) -> Result<Value, ApiError> {
        self.request(Method::POST, &format!("{}/releases/{release}/publish", root(org, project)), Some(json!({})), true)
    }

    pub fn make_release_live(&self, org: &str, project: &str, release: &str) -> Result<Value, ApiError> {
        self.request(Method::POST, &format!("{}/releases/{release}/live", root(org, project)), Some(json!({})), true)
    }

    pub fn hub(&self, org: &str, project: &str) -> Result<HubData, ApiError> {
        self.typed(Method::GET, &format!("{}/hub", root(org, project)), None, true)
    }

    pub fn create_hub_post(&self, org: &str, project: &str, content: &str) -> Result<Value, ApiError> {
        self.request(Method::POST, &format!("{}/hub/posts", root(org, project)), Some(json!({ "content": content.trim() })), true)
    }

    pub fn notes(&self, org: &str, project: &str) -> Result<Vec<ProjectNote>, ApiError> {
        self.typed(Method::GET, &format!("{}/notes", root(org, project)), None, true)
    }

    pub fn create_note(&self, org: &str, project: &str, title: &str, body: &str, team: bool) -> Result<ProjectNote, ApiError> {
        self.typed(Method::POST, &format!("{}/notes", root(org, project)), Some(json!({ "title": title.trim(), "body": body, "visibility": if team { "TEAM" } else { "PRIVATE" } })), true)
    }

    /// `version` iyimser kilit: farklıysa 409 NOTE_CONFLICT.
    pub fn update_note(&self, org: &str, project: &str, note: &str, patch: Value) -> Result<ProjectNote, ApiError> {
        self.typed(Method::PATCH, &format!("{}/notes/{note}", root(org, project)), Some(patch), true)
    }

    pub fn delete_note(&self, org: &str, project: &str, note: &str) -> Result<(), ApiError> {
        self.request(Method::DELETE, &format!("{}/notes/{note}", root(org, project)), None, true).map(|_| ())
    }

    pub fn screenshots(&self, org: &str, project: &str) -> Result<Vec<ProjectScreenshot>, ApiError> {
        self.typed(Method::GET, &format!("{}/screenshots?limit=60", root(org, project)), None, true)
    }

    pub fn update_screenshot(&self, org: &str, project: &str, id: &str, caption: &str) -> Result<ProjectScreenshot, ApiError> {
        self.typed(Method::PATCH, &format!("{}/screenshots/{id}", root(org, project)), Some(json!({ "caption": caption })), true)
    }

    pub fn delete_screenshot(&self, org: &str, project: &str, id: &str) -> Result<(), ApiError> {
        self.request(Method::DELETE, &format!("{}/screenshots/{id}", root(org, project)), None, true).map(|_| ())
    }

    pub fn update_hub_post(&self, org: &str, project: &str, post: &str, content: &str) -> Result<Value, ApiError> {
        self.request(Method::PATCH, &format!("{}/hub/posts/{post}", root(org, project)), Some(json!({ "content": content })), true)
    }

    pub fn delete_hub_post(&self, org: &str, project: &str, post: &str) -> Result<(), ApiError> {
        self.request(Method::DELETE, &format!("{}/hub/posts/{post}", root(org, project)), None, true).map(|_| ())
    }

    pub fn create_achievement(&self, org: &str, project: &str, title: &str, description: &str, icon: &str) -> Result<Value, ApiError> {
        self.request(Method::POST, &format!("{}/hub/achievements", root(org, project)), Some(json!({ "title": title.trim(), "description": description.trim(), "icon": icon })), true)
    }

    pub fn unlock_achievement(&self, org: &str, project: &str, id: &str, unlocked: bool) -> Result<Value, ApiError> {
        self.request(Method::POST, &format!("{}/hub/achievements/{id}/unlock", root(org, project)), Some(json!({ "unlocked": unlocked })), true)
    }

    pub fn delete_achievement(&self, org: &str, project: &str, id: &str) -> Result<(), ApiError> {
        self.request(Method::DELETE, &format!("{}/hub/achievements/{id}", root(org, project)), None, true).map(|_| ())
    }

    pub fn save_review(&self, org: &str, project: &str, recommended: bool, body: &str) -> Result<Value, ApiError> {
        self.request(Method::PUT, &format!("{}/hub/reviews", root(org, project)), Some(json!({ "recommended": recommended, "body": body })), true)
    }

    pub fn delete_review(&self, org: &str, project: &str) -> Result<(), ApiError> {
        self.request(Method::DELETE, &format!("{}/hub/reviews", root(org, project)), None, true).map(|_| ())
    }

    pub fn release_diff(&self, org: &str, project: &str, release: &str) -> Result<ReleaseDiff, ApiError> {
        self.typed(Method::GET, &format!("{}/releases/{release}/diff", root(org, project)), None, true)
    }

    pub fn release_installs(&self, org: &str, project: &str) -> Result<Vec<ReleaseInstall>, ApiError> {
        #[derive(Deserialize)]
        struct Rows {
            #[serde(default)]
            installs: Vec<ReleaseInstall>,
        }
        Ok(self.typed::<Rows>(Method::GET, &format!("{}/releases/installs", root(org, project)), None, true)?.installs)
    }

    /// `patch`: { notes, launchArgs, overlay, channel?, version? } (kategori/sürüm yalnız taslakta).
    pub fn update_release(&self, org: &str, project: &str, release: &str, patch: Value) -> Result<Value, ApiError> {
        self.request(Method::PATCH, &format!("{}/releases/{release}", root(org, project)), Some(patch), true)
    }

    pub fn delete_release(&self, org: &str, project: &str, release: &str) -> Result<(), ApiError> {
        self.request(Method::DELETE, &format!("{}/releases/{release}", root(org, project)), None, true).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn humanize() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1536 * 1024 * 1024), "1,5 GB");
        p2_i18n::set_language("tr");
        assert_eq!(human_duration(3600 * 3 + 12 * 60), "3 sa 12 dk");
        assert_eq!(human_duration(59), "0 dk");
    }
}
