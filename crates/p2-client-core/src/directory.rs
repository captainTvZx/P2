//! Sunucu tarayıcısı: P2 Server kurulumlarının dizini (`p2-server` directory.rs) ve sunucunun
//! kendi kimliği (`/instance`). Oturum gerekmez.

use std::time::Duration;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::api::{Api, ApiError};

/// Varsayılan dizin: merkezi P2 sunucusu.
pub const DEFAULT_DIRECTORY: &str = "https://p2.tdagaming.net";

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct DirServer {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Dizine göre yol (`/directory/servers/<id>/icon`) ya da yok.
    pub icon_url: Option<String>,
    /// "public" | "private"
    pub access: String,
    pub version: String,
    pub members: i64,
    pub online: i64,
    /// Son 12 dakikada duyurdu.
    pub live: bool,
    /// Yalnız açık sunucuda.
    pub url: Option<String>,
}

impl DirServer {
    pub fn private(&self) -> bool {
        self.access == "private"
    }
}

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder().timeout(Duration::from_secs(20)).connect_timeout(Duration::from_secs(10)).build().expect("HTTP istemcisi")
}

fn base(directory: &str) -> String {
    let d = directory.trim().trim_end_matches('/');
    if d.is_empty() { DEFAULT_DIRECTORY.into() } else { d.into() }
}

fn error_of(status: u16, body: &Value) -> ApiError {
    ApiError::from_body(status, body)
}

/// Dizindeki sunucular (açık + özel; doğrulanmış olanlar).
pub fn servers(directory: &str, query: &str) -> Result<Vec<DirServer>, ApiError> {
    let q: String = query.trim().bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
    let r = client().get(format!("{}/directory/servers?q={q}", base(directory))).send().map_err(|_| ApiError::unreachable())?;
    let status = r.status().as_u16();
    let body: Value = r.json().unwrap_or(Value::Null);
    if !(200..300).contains(&status) {
        return Err(error_of(status, &body));
    }
    serde_json::from_value(body["servers"].clone()).map_err(|_| ApiError::unreachable())
}

/// Sunucuya katıl: açık sunucuda adres, özel sunucuda kod doğruysa adres döner (yanlışsa `BAD_CODE`).
pub fn join(directory: &str, id: &str, code: &str) -> Result<String, ApiError> {
    let r = client().post(format!("{}/directory/servers/{id}/join", base(directory))).json(&json!({ "code": code.trim() })).send().map_err(|_| ApiError::unreachable())?;
    let status = r.status().as_u16();
    let body: Value = r.json().unwrap_or(Value::Null);
    if !(200..300).contains(&status) {
        return Err(error_of(status, &body));
    }
    body["url"].as_str().map(str::to_string).ok_or_else(ApiError::unreachable)
}

/// Dizindeki sunucu simgesi (ham bayt).
pub fn icon(directory: &str, path: &str) -> Option<Vec<u8>> {
    let r = client().get(format!("{}{path}", base(directory))).send().ok()?;
    r.status().is_success().then(|| r.bytes().ok().map(|b| b.to_vec())).flatten()
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct IceServer {
    pub urls: Vec<String>,
    pub username: Option<String>,
    pub credential: Option<String>,
}

impl Api {
    /// Sunucunun verdiği STUN/TURN listesi (geçici TURN kimliğiyle). Eski sunucuda 404 → boş.
    pub fn ice_servers(&self) -> Result<Vec<IceServer>, ApiError> {
        let v = self.request(reqwest::Method::GET, "/calls/ice-servers", None, true)?;
        Ok(serde_json::from_value(v["iceServers"].clone()).unwrap_or_default())
    }
}
