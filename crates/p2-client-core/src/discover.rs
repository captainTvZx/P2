//! Keşfet: herkese açık projeler ve haberleri (`/discover/...`). Açık projeye davet kodu olmadan
//! katılınır; özel projeler bu uçlarda hiç görünmez.

use std::collections::BTreeMap;

use reqwest::Method;
use serde::Deserialize;
use serde_json::json;

use crate::api::{Api, ApiError, enc};
use crate::community::JoinResult;

/// Proje görünürlüğü.
pub const VISIBILITY_PUBLIC: &str = "PUBLIC";
pub const VISIBILITY_PRIVATE: &str = "PRIVATE";

/// Keşfet kategorileri (sunucudaki `CATEGORIES` ile aynı sıra).
pub const DISCOVER_CATEGORIES: &[&str] = &["GAME", "DEV", "COMMUNITY", "EDUCATION", "MUSIC", "OTHER"];

/// Keşfet kartı.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct DiscoverProject {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub tagline: Option<String>,
    pub category: Option<String>,
    pub icon_url: Option<String>,
    pub cover_url: Option<String>,
    pub organization_id: String,
    pub member_count: u64,
    pub online_count: u64,
    pub post_count: u64,
    /// Zaten üyesiyim.
    pub joined: bool,
    pub created_at: String,
}

impl DiscoverProject {
    /// Kategori kimliği (boşsa `OTHER`).
    pub fn category_id(&self) -> &str {
        self.category.as_deref().filter(|c| !c.is_empty()).unwrap_or("OTHER")
    }

    /// Kart metni: tanıtım satırı, yoksa açıklama.
    pub fn blurb(&self) -> Option<&str> {
        self.tagline.as_deref().filter(|t| !t.trim().is_empty()).or_else(|| self.description.as_deref().filter(|d| !d.trim().is_empty()))
    }
}

/// `GET /discover/projects`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct DiscoverList {
    pub projects: Vec<DiscoverProject>,
    /// Kategori → açık proje sayısı (aramadan bağımsız).
    pub categories: BTreeMap<String, u64>,
    pub total: u64,
    /// Açık projelerde şu an çevrimiçi olan (tekil) kişi sayısı.
    pub online: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct DiscoverPerson {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub online: bool,
    pub owner: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct DiscoverChannel {
    pub id: String,
    pub name: String,
    /// TEXT / ANNOUNCEMENT / VOICE / FORUM
    #[serde(rename = "type")]
    pub kind: String,
    pub category: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct NewsProject {
    pub id: String,
    pub name: String,
    pub icon_url: Option<String>,
    pub cover_url: Option<String>,
    pub category: Option<String>,
    pub joined: bool,
}

/// Açık bir projenin gönderisi (`GET /discover/news`).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct NewsItem {
    pub id: String,
    pub project_id: String,
    /// POST / RELEASE
    pub kind: String,
    pub title: String,
    pub excerpt: String,
    pub content: String,
    /// Gönderinin kapağı, yoksa projenin kapağı.
    pub cover_url: Option<String>,
    pub own_cover: bool,
    pub created_at: String,
    pub author: Option<DiscoverPerson>,
    pub project: NewsProject,
}

/// `GET /discover/projects/:id`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct DiscoverDetail {
    #[serde(flatten)]
    pub project: DiscoverProject,
    pub owner: Option<DiscoverPerson>,
    /// Bu projeden yasaklıyım (katıl düğmesi kapalı).
    pub banned: bool,
    pub channels: Vec<DiscoverChannel>,
    pub members: Vec<DiscoverPerson>,
    pub news: Vec<NewsItem>,
}

impl Api {
    /// Açık projeler. `sort`: `popular` (varsayılan) / `new` / `active`.
    pub fn discover_projects(&self, query: &str, category: Option<&str>, sort: &str) -> Result<DiscoverList, ApiError> {
        let path = format!("/discover/projects?q={}&category={}&sort={}", enc(query.trim()), enc(category.unwrap_or_default()), enc(sort));
        self.typed(Method::GET, &path, None, true)
    }

    pub fn discover_project(&self, project: &str) -> Result<DiscoverDetail, ApiError> {
        self.typed(Method::GET, &format!("/discover/projects/{}", enc(project)), None, true)
    }

    /// Açık projeye kodsuz katıl (yasaklıysa `PROJECT_BANNED`, özelse `PROJECT_NOT_FOUND`).
    pub fn discover_join(&self, project: &str) -> Result<JoinResult, ApiError> {
        self.typed(Method::POST, &format!("/discover/projects/{}/join", enc(project)), Some(json!({})), true)
    }

    pub fn discover_news(&self, limit: u32) -> Result<Vec<NewsItem>, ApiError> {
        self.typed(Method::GET, &format!("/discover/news?limit={limit}"), None, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kart_ve_ayrinti_cozulur() {
        let list: DiscoverList = serde_json::from_value(json!({
            "projects": [{ "id": "p1", "name": "Aurora", "tagline": "Uzay oyunu", "category": "GAME", "memberCount": 12, "onlineCount": 3, "joined": false }],
            "categories": { "GAME": 1 }, "total": 1
        }))
        .unwrap();
        assert_eq!(list.projects[0].category_id(), "GAME");
        assert_eq!(list.projects[0].blurb(), Some("Uzay oyunu"));
        assert_eq!(list.categories["GAME"], 1);
        let detail: DiscoverDetail = serde_json::from_value(json!({
            "id": "p1", "name": "Aurora", "description": "Açıklama", "memberCount": 2, "banned": true,
            "channels": [{ "id": "c", "name": "genel", "type": "TEXT", "category": "GENEL" }],
            "members": [{ "id": "u", "displayName": "Ada", "online": true }], "news": []
        }))
        .unwrap();
        assert_eq!(detail.project.name, "Aurora");
        assert_eq!(detail.project.category_id(), "OTHER");
        assert_eq!(detail.project.blurb(), Some("Açıklama"));
        assert!(detail.banned && detail.members[0].online);
        assert_eq!(detail.channels[0].kind, "TEXT");
    }
}
