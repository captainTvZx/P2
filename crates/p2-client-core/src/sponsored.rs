//! Sponsorlu içerik uçları (`/sponsored`, `/server/sponsored`): uygulama içi sade reklam kartları.
//! Sunucu yalnız gün başına toplam sayar; istemci kişisel veri göndermez.

use reqwest::Method;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{Api, ApiError};

/// Kartların görünebildiği yerler.
pub const SPONSORED_PLACEMENTS: &[&str] = &["discover", "news", "library"];

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SponsoredAd {
    pub id: String,
    pub sponsor: String,
    pub title: String,
    pub body: String,
    pub image_url: Option<String>,
    pub link_url: String,
    pub cta: String,
    pub placements: Vec<String>,
    pub weight: i64,
    // Yalnız yönetim listesinde dolu:
    pub active: bool,
    pub live: bool,
    pub starts_at: Option<String>,
    pub ends_at: Option<String>,
    pub created_at: Option<String>,
    pub impressions: u64,
    pub clicks: u64,
    pub days: Vec<SponsoredDay>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SponsoredDay {
    pub day: String,
    pub impressions: u64,
    pub clicks: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct SponsoredFeed {
    pub enabled: bool,
    pub ads: Vec<SponsoredAd>,
}

impl Api {
    /// Yayındaki kartlar (`placement` boşsa hepsi).
    pub fn sponsored(&self, placement: &str) -> Result<SponsoredFeed, ApiError> {
        self.typed(Method::GET, &format!("/sponsored?placement={placement}"), None, true)
    }

    /// `view` | `click` (sunucu kişi başına günde bir kez sayar).
    pub fn sponsored_event(&self, id: &str, kind: &str) -> Result<Value, ApiError> {
        self.request(Method::POST, &format!("/sponsored/{id}/events"), Some(json!({ "kind": kind })), true)
    }

    pub fn server_sponsored(&self) -> Result<SponsoredFeed, ApiError> {
        self.typed(Method::GET, "/server/sponsored", None, true)
    }

    pub fn server_create_sponsored(&self, body: Value) -> Result<SponsoredAd, ApiError> {
        self.typed(Method::POST, "/server/sponsored", Some(body), true)
    }

    pub fn server_update_sponsored(&self, id: &str, patch: Value) -> Result<SponsoredAd, ApiError> {
        self.typed(Method::PATCH, &format!("/server/sponsored/{id}"), Some(patch), true)
    }

    pub fn server_delete_sponsored(&self, id: &str) -> Result<Value, ApiError> {
        self.request(Method::DELETE, &format!("/server/sponsored/{id}"), None, true)
    }

    pub fn server_sponsored_enabled(&self, enabled: bool) -> Result<Value, ApiError> {
        self.request(Method::PATCH, "/server/sponsored/settings", Some(json!({ "enabled": enabled })), true)
    }
}
