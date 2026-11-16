//! Sohbetin ek uçları (ChannelContent.tsx, GroupChat.tsx, attachments.tsx, ChannelBrowser.tsx):
//! iş parçacığı / forum konusu yanıtları, iletme, dosya yükleme ve işareti, kanal tercihleri
//! (ayrıl/katıl, yıldız, bildirim), kanal içi arama.

use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::api::{Api, ApiError, enc};
use crate::models::{ChatTarget, Message};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UploadedFile {
    pub name: String,
    #[serde(rename = "type")]
    pub mime: String,
    pub size: u64,
    pub url: String,
}

/// `[[p2-file|ad|tür|boyut|yol]]` — attachments.tsx `attachmentMarker` ile aynı (ad ve tür
/// encodeURIComponent ile kodlanır; yol göreli `/uploads/...`).
pub fn attachment_marker(file: &UploadedFile) -> String {
    let path = match file.url.find("/uploads/") {
        Some(i) => &file.url[i..],
        None => file.url.as_str(),
    };
    format!("[[p2-file|{}|{}|{}|{}]]", uri_component(&file.name), uri_component(&file.mime), file.size, path)
}

/// encodeURIComponent: `A-Z a-z 0-9 - _ . ! ~ * ' ( )` dışındakiler kodlanır.
fn uri_component(value: &str) -> String {
    value
        .bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

pub fn mime_for(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or_default().to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "pdf" => "application/pdf",
        "txt" | "log" | "md" | "csv" => "text/plain",
        "json" => "application/json",
        "zip" => "application/zip",
        "mp4" => "video/mp4",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        _ => "application/octet-stream",
    }
}

pub fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Preference {
    /// CHANNEL | PERSON
    pub kind: String,
    pub target_id: String,
    #[serde(default)]
    pub starred: bool,
    /// ALL | MENTIONS | MUTED
    #[serde(default)]
    pub notify: String,
    #[serde(default)]
    pub hidden: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub id: String,
    pub content: String,
    pub created_at: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub author: crate::models::Author,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct HistoryItem {
    pub id: String,
    pub content: String,
    pub created_at: String,
}

impl ChatTarget {
    /// İş parçacığı (yanıt dizisi) desteği: kanal ve grup.
    pub fn supports_threads(&self) -> bool {
        !matches!(self, ChatTarget::Direct { .. })
    }
}

impl Api {
    /// Bir mesajın yanıtları (kanal/grup iş parçacığı) ya da forum konusunun yanıtları.
    pub fn replies(&self, target: &ChatTarget, parent: &str, forum: bool, me: &str) -> Result<Vec<Message>, ApiError> {
        let path = if forum { format!("{}/topics/{parent}/replies", target.base()) } else { format!("{}/messages/{parent}/replies", target.base()) };
        let mut rows: Vec<Message> = self.typed(Method::GET, &path, None, true)?;
        rows.iter_mut().for_each(|m| m.normalize(me));
        Ok(rows)
    }

    pub fn send_reply(&self, target: &ChatTarget, parent: &str, forum: bool, content: &str, quoted: Option<&str>, me: &str) -> Result<Message, ApiError> {
        let path = if forum { format!("{}/topics/{parent}/replies", target.base()) } else { format!("{}/messages/{parent}/replies", target.base()) };
        let mut body = json!({ "content": content });
        if let Some(q) = quoted {
            body["quotedMessageId"] = json!(q);
        }
        let mut m: Message = self.typed(Method::POST, &path, Some(body), true)?;
        m.normalize(me);
        Ok(m)
    }

    /// Kanal mesajını aynı projedeki başka bir kanala iletir.
    pub fn forward_message(&self, target: &ChatTarget, message: &str, to_channel: &str) -> Result<Value, ApiError> {
        self.request(Method::POST, &format!("{}/messages/{message}/forward", target.base()), Some(json!({ "targetChannelId": to_channel })), true)
    }

    /// Dosya yükler (en çok 250 MB; uploads.ts data URL bekler).
    pub fn upload_file(&self, name: &str, data: &[u8]) -> Result<UploadedFile, ApiError> {
        let mime = mime_for(name);
        let body = json!({ "name": name, "type": mime, "data": format!("data:{mime};base64,{}", base64(data)) });
        self.typed_bulk(Method::POST, "/uploads", body)
    }

    pub fn preferences(&self, org: &str) -> Result<Vec<Preference>, ApiError> {
        self.typed(Method::GET, &format!("/organizations/{org}/preferences"), None, true)
    }

    /// `patch`: `{ hidden?, starred?, notify? }`
    pub fn set_preference(&self, org: &str, kind: &str, target: &str, patch: Value) -> Result<Preference, ApiError> {
        let mut body = patch;
        body["kind"] = json!(kind);
        body["targetId"] = json!(target);
        self.typed(Method::PUT, &format!("/organizations/{org}/preferences"), Some(body), true)
    }

    pub fn search_channel(&self, target: &ChatTarget, query: &str) -> Result<Vec<SearchHit>, ApiError> {
        self.typed(Method::GET, &format!("{}/search?q={}", target.base(), enc(query)), None, true)
    }

    /// Kanal mesajının düzenleme geçmişi (eski sürümler).
    pub fn message_history(&self, target: &ChatTarget, message: &str) -> Result<Vec<HistoryItem>, ApiError> {
        self.typed(Method::GET, &format!("{}/messages/{message}/history", target.base()), None, true)
    }

    pub fn remove_group_member(&self, org: &str, group: &str, user: &str) -> Result<Value, ApiError> {
        self.request(Method::DELETE, &format!("/organizations/{org}/groups/{group}/members/{user}"), None, true)
    }

    /// Kullanıcı grupları (`@tasarim` gibi etiketler; ChannelContent.tsx `systemMentions`).
    pub fn user_groups(&self, org: &str) -> Result<Vec<UserGroupSummary>, ApiError> {
        self.typed(Method::GET, &format!("/organizations/{org}/user-groups"), None, true)
    }

    /// Bahsetme kaynağı: kullanıcı grupları artık projeye özel (`/projects/:p/user-groups`).
    pub fn project_mention_groups(&self, project: &str) -> Result<Vec<UserGroupSummary>, ApiError> {
        self.typed(Method::GET, &format!("/projects/{project}/user-groups"), None, true)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct UserGroupSummary {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub handle: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub members: Vec<Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_matches_js() {
        let f = UploadedFile { name: "rapor son.pdf".into(), mime: "application/pdf".into(), size: 12700, url: "http://10.0.0.5:4000/uploads/a.pdf".into() };
        assert_eq!(attachment_marker(&f), "[[p2-file|rapor%20son.pdf|application%2Fpdf|12700|/uploads/a.pdf]]");
        assert_eq!(base64(b"Man"), "TWFu");
        assert_eq!(base64(b"Ma"), "TWE=");
        assert_eq!(base64(b"M"), "TQ==");
        assert_eq!(base64("çay".as_bytes()), "w6dheQ==");
    }
}
