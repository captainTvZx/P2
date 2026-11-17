//! Arkadaşlar ve engeller (docs/arkadaslar.md): hesaba bağlı, projelerden bağımsız.
//! Sunucu uçları: `/friends`, `/friends/requests…`, `/blocks/:userId`.

use reqwest::Method;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{Api, ApiError};
use crate::models::Member;

/// Arkadaş (durumuyla birlikte).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Friend {
    pub id: String,
    pub username: String,
    pub display_name: String,
    pub title: Option<String>,
    pub avatar_url: Option<String>,
    pub status: String,
    /// Arkadaşlığın kabul edildiği an (ISO).
    pub since: Option<String>,
}

impl Friend {
    pub fn member(&self) -> Member {
        Member { id: self.id.clone(), username: self.username.clone(), display_name: self.display_name.clone(), title: self.title.clone(), avatar_url: self.avatar_url.clone(), status: self.status.clone(), role: String::new(), email: None }
    }
}

/// Bekleyen istek (gelen: `user` gönderen; giden: `user` alıcı) ya da engellenen kişi (`id` boş).
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct FriendEntry {
    pub id: String,
    pub user: Friend,
    pub created_at: String,
}

/// `GET /friends`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Friends {
    pub friends: Vec<Friend>,
    pub incoming: Vec<FriendEntry>,
    pub outgoing: Vec<FriendEntry>,
    pub blocked: Vec<FriendEntry>,
}

/// Bir kişiyle ilişkim (kişi menüleri ve profil kartı bununla seçenek gösterir).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Relation {
    None,
    Friend,
    /// Ben istek gönderdim (istek kimliği).
    Outgoing(String),
    /// Bana istek geldi (istek kimliği).
    Incoming(String),
    Blocked,
}

impl Friends {
    pub fn relation(&self, user_id: &str) -> Relation {
        if self.blocked.iter().any(|b| b.user.id == user_id) {
            Relation::Blocked
        } else if self.friends.iter().any(|f| f.id == user_id) {
            Relation::Friend
        } else if let Some(r) = self.incoming.iter().find(|r| r.user.id == user_id) {
            Relation::Incoming(r.id.clone())
        } else if let Some(r) = self.outgoing.iter().find(|r| r.user.id == user_id) {
            Relation::Outgoing(r.id.clone())
        } else {
            Relation::None
        }
    }

    pub fn is_blocked(&self, user_id: &str) -> bool {
        self.blocked.iter().any(|b| b.user.id == user_id)
    }
}

/// Arkadaşlık hata kodlarının çeviri anahtarı (`p2-i18n`); bilinmeyen kodda `None`.
pub fn friend_error_key(code: &str) -> Option<&'static str> {
    Some(match code {
        "USER_NOT_FOUND" => "friends.err.notFound",
        "SELF_FRIEND" => "friends.err.self",
        "ALREADY_FRIENDS" => "friends.err.already",
        "FRIEND_REQUEST_EXISTS" => "friends.err.exists",
        "BLOCKED_BY_ME" => "friends.err.blockedByMe",
        "USER_BLOCKED" => "friends.err.blocked",
        "NO_SHARED_PROJECT" => "friends.err.noContact",
        "PROJECT_CONVERSATION_CLOSED" | "NOT_PROJECT_MEMBER" => "friends.err.projectClosed",
        "FRIEND_REQUEST_NOT_FOUND" => "friends.err.requestGone",
        _ => return None,
    })
}

impl Api {
    pub fn friends(&self) -> Result<Friends, ApiError> {
        self.typed(Method::GET, "/friends", None, true)
    }

    /// İstek gönderir; karşı taraf zaten istek göndermişse sunucu doğrudan kabul eder (`status` = ACCEPTED).
    pub fn friend_request_by_name(&self, username: &str) -> Result<Value, ApiError> {
        self.request(Method::POST, "/friends/requests", Some(json!({ "username": username })), true)
    }

    pub fn friend_request(&self, user_id: &str) -> Result<Value, ApiError> {
        self.request(Method::POST, "/friends/requests", Some(json!({ "userId": user_id })), true)
    }

    pub fn accept_friend(&self, request_id: &str) -> Result<Value, ApiError> {
        self.request(Method::POST, &format!("/friends/requests/{request_id}/accept"), None, true)
    }

    pub fn decline_friend(&self, request_id: &str) -> Result<Value, ApiError> {
        self.request(Method::POST, &format!("/friends/requests/{request_id}/decline"), None, true)
    }

    pub fn cancel_friend_request(&self, request_id: &str) -> Result<Value, ApiError> {
        self.request(Method::DELETE, &format!("/friends/requests/{request_id}"), None, true)
    }

    pub fn remove_friend(&self, user_id: &str) -> Result<Value, ApiError> {
        self.request(Method::DELETE, &format!("/friends/{user_id}"), None, true)
    }

    pub fn block_user(&self, user_id: &str) -> Result<Value, ApiError> {
        self.request(Method::POST, &format!("/blocks/{user_id}"), None, true)
    }

    pub fn unblock_user(&self, user_id: &str) -> Result<Value, ApiError> {
        self.request(Method::DELETE, &format!("/blocks/{user_id}"), None, true)
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn friends_parse_and_relation() {
        let f: Friends = serde_json::from_value(json!({
            "friends": [{ "id": "a", "username": "ada", "displayName": "Ada", "status": "ONLINE", "since": "2026-11-17T10:00:00.000Z" }],
            "incoming": [{ "id": "r1", "user": { "id": "b", "username": "bora", "displayName": "Bora" }, "createdAt": "2026-11-17T10:00:00.000Z" }],
            "outgoing": [{ "id": "r2", "user": { "id": "c", "username": "can", "displayName": "Can" }, "createdAt": "x" }],
            "blocked": [{ "user": { "id": "d", "username": "deniz", "displayName": "Deniz" }, "createdAt": "x" }]
        }))
        .unwrap();
        assert_eq!(f.relation("a"), Relation::Friend);
        assert_eq!(f.relation("b"), Relation::Incoming("r1".into()));
        assert_eq!(f.relation("c"), Relation::Outgoing("r2".into()));
        assert_eq!(f.relation("d"), Relation::Blocked);
        assert_eq!(f.relation("z"), Relation::None);
        assert!(f.is_blocked("d"));
        assert_eq!(f.friends[0].member().status, "ONLINE");
        assert_eq!(friend_error_key("ALREADY_FRIENDS"), Some("friends.err.already"));
    }
}
