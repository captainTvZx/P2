//! Etkinlik akışı (server/src/activity.ts, ActivityPanel.tsx): etiketlenme, yanıt, tepki,
//! görev atama, sürüm yayını.

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::api::{Api, ApiError};
use crate::models::Author;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ActivityItem {
    pub id: String,
    /// MENTION | REPLY | REACTION | RELEASE | ASSIGNED
    pub kind: String,
    #[serde(default)]
    pub preview: String,
    #[serde(default)]
    pub emoji: Option<String>,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub channel_id: Option<String>,
    #[serde(default)]
    pub channel_name: Option<String>,
    #[serde(default)]
    pub group_id: Option<String>,
    #[serde(default)]
    pub group_name: Option<String>,
    #[serde(default)]
    pub task_id: Option<String>,
    #[serde(default)]
    pub document_id: Option<String>,
    #[serde(default)]
    pub read_at: Option<String>,
    pub created_at: String,
    #[serde(default)]
    pub actor: Author,
}

impl ActivityItem {
    /// ActivityPanel.tsx `title`
    pub fn title(&self) -> String {
        match self.kind.as_str() {
            "RELEASE" => p2_i18n::t("core.act.release"),
            "ASSIGNED" => p2_i18n::t("core.act.assigned"),
            "MENTION" => p2_i18n::t("core.act.mention"),
            "REPLY" => p2_i18n::t("core.act.reply"),
            _ => p2_i18n::tv("core.act.reaction", &[("emoji", self.emoji.as_deref().unwrap_or("👍"))]),
        }
    }

    /// ActivityPanel.tsx `place`
    pub fn place(&self) -> String {
        if self.document_id.is_some() {
            return match &self.channel_name {
                Some(n) => p2_i18n::tv("core.place.documentIn", &[("name", n)]),
                None => p2_i18n::t("core.place.document"),
            };
        }
        if self.task_id.is_some() {
            return if self.kind == "ASSIGNED" { p2_i18n::t("core.place.task") } else { p2_i18n::t("core.place.taskComment") };
        }
        if let Some(g) = &self.group_name {
            return g.clone();
        }
        match &self.channel_name {
            Some(c) => format!("#{c}"),
            None => p2_i18n::t("core.place.chat"),
        }
    }

    pub fn badge(&self) -> String {
        match self.kind.as_str() {
            "MENTION" => "@".into(),
            "REPLY" => "↩".into(),
            "RELEASE" => "⬇".into(),
            "ASSIGNED" => "✓".into(),
            _ => self.emoji.clone().unwrap_or_else(|| "♥".into()),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ActivityPage {
    #[serde(default)]
    pub items: Vec<ActivityItem>,
    #[serde(default)]
    pub unread: u32,
}

impl Api {
    pub fn activity(&self, org: &str) -> Result<ActivityPage, ApiError> {
        self.typed(Method::GET, &format!("/organizations/{org}/activity?limit=60"), None, true)
    }

    pub fn read_all_activity(&self, org: &str) -> Result<(), ApiError> {
        self.request(Method::POST, &format!("/organizations/{org}/activity/read"), Some(json!({})), true).map(|_| ())
    }

    pub fn read_activity(&self, org: &str, id: &str) -> Result<(), ApiError> {
        self.request(Method::POST, &format!("/organizations/{org}/activity/{id}/read"), Some(json!({})), true).map(|_| ())
    }
}
