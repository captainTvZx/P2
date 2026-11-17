//! Sunucu API istemcisi — `apiBase.ts` (`p2Fetch`, `refreshClientSession`) ve App.tsx `api()`.
//!
//! - Ağ hatası / zaman aşımı → `unreachable` (App.tsx ile aynı mesaj).
//! - Hata yanıtı → sunucunun `{code, message, field}` gövdesi.
//! - Kimlikli istek 401 alırsa yenileme jetonuyla TEK yenileme yapılır (aynı anda gelen
//!   istekler aynı yenilemeyi bekler), istek yeni jetonla tekrarlanır; yenileme de başarısızsa
//!   `SESSION_EXPIRED`.
//! - "Beni hatırla" kapalıysa yenileme jetonu diske yazılmaz.

use std::sync::Mutex;
use std::time::Duration;

use reqwest::Method;
use reqwest::blocking::Client;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::models::*;
use crate::store;

pub const UNREACHABLE_MESSAGE: &str = "P2 sunucusuna ulaşılamıyor. PostgreSQL ve backend durumunu kontrol edin.";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ApiError {
    pub status: u16,
    pub code: String,
    pub message: String,
    pub field: Option<String>,
    pub unreachable: bool,
}

impl ApiError {
    fn unreachable() -> Self {
        Self { message: UNREACHABLE_MESSAGE.into(), unreachable: true, ..Default::default() }
    }
    fn from_body(status: u16, body: &Value) -> Self {
        let message = match &body["message"] {
            Value::String(s) => s.clone(),
            Value::Array(items) => items.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" "),
            _ => "Bir hata oluştu.".into(),
        };
        Self {
            status,
            code: body["code"].as_str().unwrap_or_default().into(),
            message,
            field: body["field"].as_str().map(Into::into),
            unreachable: false,
        }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ApiError {}

#[derive(Default)]
struct Tokens {
    access: String,
    refresh: String,
    persist: bool,
}

pub struct Api {
    base: String,
    version: String,
    http: Client,
    /// Büyük gövdeler (kütüphane parçaları, en çok 4 MB): daha uzun zaman aşımı.
    bulk: Client,
    tokens: Mutex<Tokens>,
    refreshing: Mutex<()>,
    /// Overlay host gibi ikincil süreçler: belirteç yenilenmez (yenileme belirtecini döndürür,
    /// istemcinin oturumunu düşürürdü); 401 doğrudan döner, istemci yeni erişim belirteci yollar.
    no_refresh: std::sync::atomic::AtomicBool,
}

impl Api {
    pub fn new(base: &str, client_version: &str) -> Self {
        // Electron'un fetch'inde zaman aşımı yok; Chromium TCP bağlantısını Windows SYN yinelemeleri
        // boyunca (~21 sn) bekler. Bazı sunucularda ilk bağlantı 15–20 sn sürebiliyor; 5 sn'lik sınır
        // "sunucuya ulaşılamıyor"a düşürüyordu.
        let http = Client::builder()
            .timeout(Duration::from_secs(60))
            .connect_timeout(Duration::from_secs(30))
            .build()
            .expect("HTTP istemcisi");
        Self {
            base: base.trim_end_matches('/').to_string(),
            version: client_version.into(),
            http,
            bulk: Client::builder().timeout(Duration::from_secs(120)).connect_timeout(Duration::from_secs(10)).build().expect("HTTP istemcisi"),
            tokens: Mutex::new(Tokens { persist: true, ..Default::default() }),
            refreshing: Mutex::new(()),
            no_refresh: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    /// Oturumu kurar (giriş/yenileme sonrası). `persist`: "Beni hatırla".
    pub fn set_auth(&self, auth: &Auth, persist: bool) {
        let mut t = self.tokens.lock().unwrap();
        t.access = auth.access_token.clone();
        t.refresh = auth.refresh_token.clone();
        t.persist = persist;
        if persist {
            store::save_session(&auth.refresh_token);
        } else {
            store::clear_session();
        }
    }

    /// Yalnız erişim belirteci (kalıcı oturuma dokunmaz) ve yenileme kapalı: ikincil süreçler için.
    pub fn set_access_only(&self, access: &str) {
        self.no_refresh.store(true, std::sync::atomic::Ordering::Relaxed);
        let mut t = self.tokens.lock().unwrap();
        t.access = access.to_string();
        t.refresh.clear();
        t.persist = false;
    }

    pub fn clear_auth(&self) {
        *self.tokens.lock().unwrap() = Tokens { persist: true, ..Default::default() };
        store::clear_session();
    }

    pub fn access_token(&self) -> String {
        self.tokens.lock().unwrap().access.clone()
    }

    fn send(&self, method: &Method, path: &str, body: Option<&Value>, token: Option<&str>) -> Result<(u16, Value), ApiError> {
        let mut req = self.http.request(method.clone(), format!("{}{}", self.base, path)).header("X-P2-Client-Version", &self.version);
        if let Some(token) = token.filter(|t| !t.is_empty()) {
            req = req.bearer_auth(token);
        }
        if let Some(body) = body {
            req = req.json(body);
        }
        let response = req.send().map_err(|_| ApiError::unreachable())?;
        let status = response.status().as_u16();
        let text = response.text().unwrap_or_default();
        let value = serde_json::from_str(&text).unwrap_or_else(|_| json!({}));
        Ok((status, value))
    }

    /// Yenileme: aynı anda gelen çağrılar tek yenilemeyi paylaşır.
    fn refresh_once(&self, stale_access: &str) -> Result<String, ApiError> {
        let _guard = self.refreshing.lock().unwrap();
        if self.no_refresh.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(ApiError { status: 401, code: "TOKEN_EXPIRED".into(), message: "Oturum belirteci yenilenmeli.".into(), ..Default::default() });
        }
        {
            let t = self.tokens.lock().unwrap();
            if !t.access.is_empty() && t.access != stale_access {
                return Ok(t.access.clone()); // başka bir çağrı zaten yeniledi
            }
        }
        let refresh = {
            let t = self.tokens.lock().unwrap();
            if t.refresh.is_empty() { store::load_session().unwrap_or_default() } else { t.refresh.clone() }
        };
        if refresh.is_empty() {
            return Err(ApiError { status: 401, code: "SESSION_EXPIRED".into(), message: "Oturum süresi doldu.".into(), ..Default::default() });
        }
        let auth = self.refresh(&refresh)?;
        let persist = self.tokens.lock().unwrap().persist;
        self.set_auth(&auth, persist);
        Ok(auth.access_token)
    }

    /// Ham istek. `auth`: kimlikli mi (Bearer + 401'de yenileme).
    pub fn request(&self, method: Method, path: &str, body: Option<Value>, auth: bool) -> Result<Value, ApiError> {
        let token = auth.then(|| self.access_token());
        let (status, value) = self.send(&method, path, body.as_ref(), token.as_deref())?;
        if status == 401 && auth && !path.starts_with("/auth/refresh") {
            let fresh = self.refresh_once(token.as_deref().unwrap_or_default()).map_err(|e| ApiError {
                code: if e.unreachable { e.code } else { "SESSION_EXPIRED".into() },
                ..e
            })?;
            let (status, value) = self.send(&method, path, body.as_ref(), Some(&fresh))?;
            return if (200..300).contains(&status) { Ok(value) } else { Err(ApiError::from_body(status, &value)) };
        }
        if (200..300).contains(&status) { Ok(value) } else { Err(ApiError::from_body(status, &value)) }
    }

    pub fn typed<T: DeserializeOwned>(&self, method: Method, path: &str, body: Option<Value>, auth: bool) -> Result<T, ApiError> {
        let value = self.request(method, path, body, auth)?;
        serde_json::from_value(value).map_err(|e| ApiError { message: format!("Beklenmeyen sunucu yanıtı: {e}"), ..Default::default() })
    }

    /// Kimlikli ikili istek (kütüphane parçaları): 401'de bir kez yenileyip tekrarlar.
    /// Dönen durum kodu çağırana bırakılır (ör. yüklemede 422 = parça içeriği değişti).
    fn bytes_request(&self, method: Method, path: &str, body: Option<Vec<u8>>) -> Result<(u16, Vec<u8>), ApiError> {
        let attempt = |token: &str| -> Result<(u16, Vec<u8>), ApiError> {
            let mut req = self.bulk.request(method.clone(), format!("{}{}", self.base, path)).header("X-P2-Client-Version", &self.version).bearer_auth(token);
            if let Some(b) = &body {
                req = req.header("Content-Type", "application/octet-stream").body(b.clone());
            }
            let r = req.send().map_err(|_| ApiError::unreachable())?;
            let status = r.status().as_u16();
            Ok((status, r.bytes().map(|b| b.to_vec()).unwrap_or_default()))
        };
        let token = self.access_token();
        let (status, data) = attempt(&token)?;
        if status == 401 {
            let fresh = self.refresh_once(&token)?;
            return attempt(&fresh);
        }
        Ok((status, data))
    }

    pub fn get_bytes(&self, path: &str) -> Result<Vec<u8>, ApiError> {
        let (status, data) = self.bytes_request(Method::GET, path, None)?;
        if (200..300).contains(&status) { Ok(data) } else { Err(ApiError { status, message: format!("HTTP {status}"), ..Default::default() }) }
    }

    /// Büyük JSON gövdeli kimlikli istek (dosya yükleme): uzun zaman aşımlı istemciyle.
    pub fn typed_bulk<T: DeserializeOwned>(&self, method: Method, path: &str, body: Value) -> Result<T, ApiError> {
        let attempt = |token: &str| -> Result<(u16, Value), ApiError> {
            let r = self.bulk.request(method.clone(), format!("{}{}", self.base, path)).header("X-P2-Client-Version", &self.version).bearer_auth(token).json(&body).send().map_err(|_| ApiError::unreachable())?;
            let status = r.status().as_u16();
            Ok((status, serde_json::from_str(&r.text().unwrap_or_default()).unwrap_or_else(|_| json!({}))))
        };
        let token = self.access_token();
        let (mut status, mut value) = attempt(&token)?;
        if status == 401 {
            let fresh = self.refresh_once(&token)?;
            (status, value) = attempt(&fresh)?;
        }
        if !(200..300).contains(&status) {
            return Err(ApiError::from_body(status, &value));
        }
        serde_json::from_value(value).map_err(|e| ApiError { message: format!("Beklenmeyen sunucu yanıtı: {e}"), ..Default::default() })
    }

    /// Ham gövdeyle PUT; durum kodunu döndürür.
    pub fn put_bytes(&self, path: &str, data: Vec<u8>) -> Result<u16, ApiError> {
        self.bytes_request(Method::PUT, path, Some(data)).map(|r| r.0)
    }

    // ---------------------------------------------------------------- uç noktalar

    pub fn health(&self) -> Result<Health, ApiError> {
        let h: Health = self.typed(Method::GET, "/health", None, false)?;
        if h.status == "ok" { Ok(h) } else { Err(ApiError::unreachable()) }
    }

    pub fn setup_installed(&self) -> Result<bool, ApiError> {
        let v = self.request(Method::GET, "/setup/status", None, false)?;
        Ok(v["installed"].as_bool().unwrap_or(false))
    }

    pub fn login(&self, identifier: &str, password: &str, remember: bool, code: Option<&str>) -> Result<Auth, ApiError> {
        let mut body = json!({ "identifier": identifier, "password": password, "rememberMe": remember });
        if let Some(code) = code.map(|c| c.chars().filter(char::is_ascii_digit).collect::<String>()).filter(|c| !c.is_empty()) {
            body["code"] = json!(code);
        }
        self.typed(Method::POST, "/auth/login", Some(body), false)
    }

    pub fn register(&self, input: &RegisterInput) -> Result<Auth, ApiError> {
        self.typed(Method::POST, "/auth/register", Some(serde_json::to_value(input).unwrap()), false)
    }

    pub fn setup_admin(&self, input: &RegisterInput) -> Result<Auth, ApiError> {
        self.typed(Method::POST, "/setup/admin", Some(serde_json::to_value(input).unwrap()), false)
    }

    pub fn refresh(&self, refresh_token: &str) -> Result<Auth, ApiError> {
        self.typed(Method::POST, "/auth/refresh", Some(json!({ "refreshToken": refresh_token })), false)
    }

    pub fn logout(&self) {
        let refresh = self.tokens.lock().unwrap().refresh.clone();
        let _ = self.request(Method::POST, "/auth/logout", Some(json!({ "refreshToken": refresh })), false);
        self.clear_auth();
    }

    pub fn me(&self) -> Result<User, ApiError> {
        self.typed(Method::GET, "/auth/me", None, true)
    }

    pub fn organizations(&self) -> Result<Vec<Organization>, ApiError> {
        self.typed(Method::GET, "/organizations", None, true)
    }

    pub fn create_organization(&self, name: &str, slug: &str) -> Result<Organization, ApiError> {
        self.typed(Method::POST, "/organizations", Some(json!({ "name": name, "slug": slug })), true)
    }

    pub fn join_organization(&self, code: &str) -> Result<Organization, ApiError> {
        self.typed(Method::POST, "/organizations/join", Some(json!({ "code": code })), true)
    }
}

fn digits(code: &str) -> String {
    code.chars().filter(char::is_ascii_digit).collect()
}

pub(crate) fn enc(value: &str) -> String {
    value
        .bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

/// Çalışma alanı uçları (Workspace.tsx / ChannelContent.tsx).
impl Api {
    /// socket.io kökü (apiBase.ts `SOCKET_BASE`): adres bir proxy arkasındaysa ".../api" eki kaldırılır.
    pub fn socket_base(&self) -> String {
        self.base.strip_suffix("/api").unwrap_or(&self.base).to_string()
    }

    pub fn projects(&self, org: &str) -> Result<Vec<Project>, ApiError> {
        self.typed(Method::GET, &format!("/organizations/{org}/projects"), None, true)
    }

    pub fn channels(&self, org: &str, project: &str) -> Result<Vec<ChannelCategory>, ApiError> {
        self.typed(Method::GET, &format!("/organizations/{org}/projects/{project}/channels"), None, true)
    }

    // ------------------------------------------- mesajlar (kanal, DM, grup — aynı ekler)

    pub fn mark_read(&self, target: &ChatTarget) -> Result<(), ApiError> {
        self.request(Method::POST, &format!("{}/read", target.base()), Some(json!({})), true).map(|_| ())
    }

    /// İlk sayfa (kanal 100 / DM-grup 200) ya da `before` (ISO zaman) öncesi; eskiden yeniye.
    /// Tepkiler `me` için özetlenmiş döner.
    pub fn messages(&self, target: &ChatTarget, before: Option<&str>, limit: u32, me: &str) -> Result<Vec<Message>, ApiError> {
        let mut path = format!("{}/messages", target.base());
        if let Some(before) = before {
            path.push_str(&format!("?before={}&limit={limit}", enc(before)));
        }
        let mut rows: Vec<Message> = self.typed(Method::GET, &path, None, true)?;
        rows.iter_mut().for_each(|m| m.normalize(me));
        Ok(rows)
    }

    pub fn send_message(&self, target: &ChatTarget, content: &str, quoted: Option<&str>, me: &str) -> Result<Message, ApiError> {
        let mut body = json!({ "content": content });
        if let Some(q) = quoted {
            body["quotedMessageId"] = json!(q);
        }
        let mut m: Message = self.typed(Method::POST, &format!("{}/messages", target.base()), Some(body), true)?;
        m.normalize(me);
        Ok(m)
    }

    pub fn edit_message(&self, target: &ChatTarget, message: &str, content: &str, me: &str) -> Result<Message, ApiError> {
        let mut m: Message = self.typed(Method::PATCH, &format!("{}/messages/{message}", target.base()), Some(json!({ "content": content })), true)?;
        m.normalize(me);
        Ok(m)
    }

    pub fn delete_message(&self, target: &ChatTarget, message: &str) -> Result<(), ApiError> {
        self.request(Method::DELETE, &format!("{}/messages/{message}", target.base()), None, true).map(|_| ())
    }

    /// Aç/kapat; üç uç da özetlenmiş liste döndürür.
    pub fn toggle_reaction(&self, target: &ChatTarget, message: &str, emoji: &str) -> Result<Vec<Reaction>, ApiError> {
        self.typed(Method::POST, &format!("{}/messages/{message}/reactions", target.base()), Some(json!({ "emoji": emoji })), true)
    }

    // ------------------------------------------------------------ birebir sohbetler

    pub fn conversations(&self, org: &str) -> Result<Vec<Conversation>, ApiError> {
        self.typed(Method::GET, &format!("/organizations/{org}/direct"), None, true)
    }

    /// Varsa mevcut sohbeti, yoksa yenisini döndürür. `project` verilirse o projenin içindeki sohbet,
    /// verilmezse Ana sayfa sohbeti (ikisi ayrıdır). Gizlenmiş sohbet açılınca listeye geri döner.
    pub fn open_conversation(&self, org: &str, user_id: &str, project: Option<&str>) -> Result<Conversation, ApiError> {
        let mut body = json!({ "userId": user_id });
        if let Some(project) = project {
            body["projectId"] = json!(project);
        }
        self.typed(Method::POST, &format!("/organizations/{org}/direct"), Some(body), true)
    }

    /// Sohbeti kendi listemden kaldırır / geri getirir (mesajlar silinmez).
    pub fn hide_conversation(&self, org: &str, conversation: &str, hidden: bool) -> Result<(), ApiError> {
        self.request(Method::PUT, &format!("/organizations/{org}/direct/{conversation}/hidden"), Some(json!({ "hidden": hidden })), true).map(|_| ())
    }

    // ------------------------------------------------------------------- gruplar

    pub fn groups(&self, org: &str, project: &str) -> Result<Vec<ChatGroup>, ApiError> {
        self.typed(Method::GET, &format!("/organizations/{org}/groups?projectId={}", enc(project)), None, true)
    }

    /// Ad boşsa sunucu üyelerin adlarından üretir.
    pub fn create_group(&self, org: &str, project: &str, name: &str, user_ids: &[String]) -> Result<ChatGroup, ApiError> {
        let mut body = json!({ "projectId": project, "userIds": user_ids });
        if !name.trim().is_empty() {
            body["name"] = json!(name.trim());
        }
        self.typed(Method::POST, &format!("/organizations/{org}/groups"), Some(body), true)
    }

    /// Ana sayfa grupları (projeye bağlı olmayanlar).
    pub fn home_groups(&self, org: &str) -> Result<Vec<ChatGroup>, ApiError> {
        self.typed(Method::GET, &format!("/organizations/{org}/groups?home=1"), None, true)
    }

    /// Ana sayfa grubu kurar (projeye bağlı değil; üyeler arkadaşlarım ya da proje paylaştıklarım).
    pub fn create_home_group(&self, org: &str, name: &str, user_ids: &[String]) -> Result<ChatGroup, ApiError> {
        let mut body = json!({ "userIds": user_ids });
        if !name.trim().is_empty() {
            body["name"] = json!(name.trim());
        }
        self.typed(Method::POST, &format!("/organizations/{org}/groups"), Some(body), true)
    }

    /// Grubu yeniden adlandırır (boş ad: üyelerden türetilir).
    pub fn rename_group(&self, org: &str, group: &str, name: &str) -> Result<ChatGroup, ApiError> {
        self.typed(Method::PATCH, &format!("/organizations/{org}/groups/{group}"), Some(json!({ "name": name.trim() })), true)
    }

    pub fn add_group_members(&self, org: &str, group: &str, user_ids: &[String]) -> Result<ChatGroup, ApiError> {
        self.typed(Method::POST, &format!("/organizations/{org}/groups/{group}/members"), Some(json!({ "userIds": user_ids })), true)
    }

    pub fn leave_group(&self, org: &str, group: &str) -> Result<(), ApiError> {
        self.request(Method::POST, &format!("/organizations/{org}/groups/{group}/leave"), Some(json!({})), true).map(|_| ())
    }

    // -------------------------------------------------------------- hesap ve profil

    pub fn update_profile(&self, display_name: &str, title: &str) -> Result<User, ApiError> {
        self.typed(Method::PATCH, "/auth/profile", Some(json!({ "displayName": display_name.trim(), "title": title.trim() })), true)
    }

    /// Profil fotoğrafı (`/uploads/...` yolu; `None` kaldırır).
    pub fn set_avatar(&self, url: Option<&str>) -> Result<User, ApiError> {
        self.typed(Method::PATCH, "/auth/profile", Some(json!({ "avatarUrl": url.unwrap_or("") })), true)
    }

    pub fn two_factor_setup(&self) -> Result<TwoFactorSetup, ApiError> {
        self.typed(Method::POST, "/auth/2fa/setup", Some(json!({})), true)
    }

    pub fn two_factor_enable(&self, code: &str) -> Result<(), ApiError> {
        self.request(Method::POST, "/auth/2fa/enable", Some(json!({ "code": digits(code) })), true).map(|_| ())
    }

    pub fn two_factor_disable(&self, code: &str) -> Result<(), ApiError> {
        self.request(Method::POST, "/auth/2fa/disable", Some(json!({ "code": digits(code) })), true).map(|_| ())
    }

    pub fn members(&self, org: &str, project: Option<&str>) -> Result<Vec<Member>, ApiError> {
        let mut path = format!("/organizations/{org}/members");
        if let Some(p) = project {
            path.push_str(&format!("?projectId={}", enc(p)));
        }
        self.typed(Method::GET, &path, None, true)
    }

    /// Ray rozetleri: proje başına okunmamış mesaj sayısı (`GET /organizations/:org/projects/unread`).
    pub fn projects_unread(&self, org: &str) -> Result<std::collections::HashMap<String, u32>, ApiError> {
        self.typed(Method::GET, &format!("/organizations/{org}/projects/unread"), None, true)
    }
}

/// ModePicker `connect`: adres site kökü ya da doğrudan API olabilir; `/health` yanıt
/// veren ilk aday döner. Şemasız adrese `https://` eklenir.
pub fn resolve_server(address: &str) -> Option<String> {
    let raw = address.trim().trim_end_matches('/');
    if raw.is_empty() {
        return None;
    }
    let base = if raw.to_ascii_lowercase().starts_with("http://") || raw.to_ascii_lowercase().starts_with("https://") {
        raw.to_string()
    } else {
        format!("https://{raw}")
    };
    [base.clone(), format!("{base}/api")].into_iter().find(|candidate| Api::new(candidate, "").health().is_ok())
}

/// Organizasyon slug'ı (App.tsx `slugify`): Türkçe karakterleri sadeleştirir.
pub fn slugify(value: &str) -> String {
    let lowered: String = value
        .chars()
        .flat_map(|c| match c {
            'I' => vec!['ı'],
            'İ' => vec!['i'],
            _ => c.to_lowercase().collect(),
        })
        .map(|c| match c {
            'ı' | 'i' | 'î' | 'ï' | 'í' | 'ì' => 'i',
            'ş' => 's',
            'ğ' => 'g',
            'ü' | 'û' | 'ú' | 'ù' => 'u',
            'ö' | 'ô' | 'ó' | 'ò' => 'o',
            'ç' => 'c',
            'â' | 'á' | 'à' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            c => c,
        })
        .collect();
    let mut out = String::new();
    for c in lowered.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').chars().take(40).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_matches_app_tsx() {
        assert_eq!(slugify("Kırmızı Çekici Stüdyosu"), "kirmizi-cekici-studyosu");
        assert_eq!(slugify("  İstanbul  Oyun--Ekibi! "), "istanbul-oyun-ekibi");
        assert_eq!(slugify("Proje X"), "proje-x");
    }

    #[test]
    fn error_body_mapping() {
        let e = ApiError::from_body(401, &json!({ "code": "TWO_FACTOR_REQUIRED", "message": "Doğrulama kodunu gir.", "field": "code", "context": { "safe": true } }));
        assert_eq!((e.code.as_str(), e.message.as_str(), e.field.as_deref()), ("TWO_FACTOR_REQUIRED", "Doğrulama kodunu gir.", Some("code")));
    }

    #[test]
    fn unreachable_server() {
        let api = Api::new("http://127.0.0.1:9", "test");
        let e = api.health().unwrap_err();
        assert!(e.unreachable);
        assert_eq!(e.message, UNREACHABLE_MESSAGE);
    }
}
