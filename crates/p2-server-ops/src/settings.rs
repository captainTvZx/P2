//! `p2-server.ini`: sunucunun kimliği (ad, simge, açıklama), sunucu tarayıcısındaki görünürlüğü
//! (açık / kodla katılım) ve TURN ayarları. Veri klasöründe durur (Windows `%ProgramData%\P2`,
//! Linux kurulumunda `P2_SERVER_DATA_DIR`). Sunucu açılırken okur, çalışırken değişikliği izler;
//! Server konsolu aynı dosyayı düzenler. Sunucu kapalıyken elle de düzenlenebilir.
//!
//! Dosya yoksa ilk okumada varsayılanlarla ve rastgele bir kimlikle oluşturulur. Bilinmeyen
//! anahtarlar yok sayılır; yazarken dosya açıklamalı şablonla yeniden yazılır.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "p2-server.ini";
/// Varsayılan sunucu dizini (sunucu tarayıcısı): merkezi P2 sunucusu.
pub const DEFAULT_DIRECTORY: &str = "https://p2.tdagaming.net";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Access {
    /// Tarayıcıda seçilip girilir.
    #[default]
    Public,
    /// Tarayıcıda görünür ama katılım kodu ister.
    Private,
}

impl Access {
    pub fn as_str(self) -> &'static str {
        match self {
            Access::Public => "public",
            Access::Private => "private",
        }
    }
    pub fn parse(s: &str) -> Self {
        if s.trim().eq_ignore_ascii_case("private") { Access::Private } else { Access::Public }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ServerSettings {
    // [server]
    pub name: String,
    pub description: String,
    /// Simge: veri klasörüne göre dosya yolu (png/jpg/webp) ya da https adresi. Boş = P2 simgesi.
    pub icon: String,
    /// İstemcilerin bağlandığı adres (ör. `https://p2.ornek.com`). Tarayıcıda listelenmek için gerekir.
    pub public_url: String,
    pub access: Access,
    pub access_code: String,
    // [directory]
    /// Sunucu tarayıcısında görünsün (açık ya da özel).
    pub listed: bool,
    pub directory_url: String,
    /// Bu sunucu tarayıcı dizinini barındırsın (yalnız merkezi sunucu).
    pub directory_host: bool,
    // [turn]
    /// TURN adresleri (virgülle). Boşsa TURN verilmez.
    pub turn_urls: Vec<String>,
    /// coturn `static-auth-secret` ile aynı.
    pub turn_secret: String,
    /// Geçici TURN kimliğinin ömrü (sn).
    pub turn_ttl: u64,
    // [identity] — elle değiştirilmez.
    pub instance_id: String,
    pub instance_key: String,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            name: "P2 Server".into(),
            description: String::new(),
            icon: String::new(),
            public_url: String::new(),
            access: Access::Public,
            access_code: String::new(),
            listed: true,
            directory_url: DEFAULT_DIRECTORY.into(),
            directory_host: false,
            turn_urls: Vec::new(),
            turn_secret: String::new(),
            turn_ttl: 86_400,
            instance_id: String::new(),
            instance_key: String::new(),
        }
    }
}

pub fn path(data_dir: &Path) -> PathBuf {
    data_dir.join(FILE_NAME)
}

fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    let _ = getrandom::fill(&mut buf);
    hex::encode(buf)
}

/// Basit INI ayrıştırıcı: `[bölüm]`, `anahtar = değer`, `;`/`#` açıklama satırları. Değer sonundaki
/// ` ; açıklama` atılır; tırnaklı değerler tırnaksız alınır.
pub fn parse_ini(text: &str) -> BTreeMap<(String, String), String> {
    let mut out = BTreeMap::new();
    let mut section = String::new();
    for raw in text.lines() {
        let line = raw.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_lowercase();
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        let v = v.trim();
        // Tırnaklı değer: kapanan tırnağa kadar (içinde ; ve # olabilir). Tırnaksız: " ;" / " #" sonrası açıklama.
        let value = if let Some(rest) = v.strip_prefix('"') {
            rest.rfind('"').map(|i| rest[..i].replace("\\\"", "\"")).unwrap_or_else(|| rest.to_string())
        } else {
            let end = v.find(" ;").or_else(|| v.find(" #")).unwrap_or(v.len());
            v[..end].trim_end().to_string()
        };
        out.insert((section.clone(), k.trim().to_lowercase()), value);
    }
    out
}

fn truthy(v: &str) -> bool {
    matches!(v.trim().to_lowercase().as_str(), "1" | "true" | "yes" | "on" | "evet" | "açık")
}

impl ServerSettings {
    pub fn from_ini(text: &str) -> Self {
        let map = parse_ini(text);
        let get = |s: &str, k: &str| map.get(&(s.to_string(), k.to_string())).cloned();
        let d = Self::default();
        Self {
            name: get("server", "name").filter(|v| !v.trim().is_empty()).unwrap_or(d.name),
            description: get("server", "description").unwrap_or_default(),
            icon: get("server", "icon").unwrap_or_default(),
            public_url: get("server", "public_url").unwrap_or_default().trim_end_matches('/').to_string(),
            access: get("server", "access").map(|v| Access::parse(&v)).unwrap_or_default(),
            access_code: get("server", "access_code").unwrap_or_default(),
            listed: get("directory", "list").map(|v| truthy(&v)).unwrap_or(d.listed),
            directory_url: get("directory", "url").filter(|v| !v.trim().is_empty()).map(|v| v.trim_end_matches('/').to_string()).unwrap_or(d.directory_url),
            directory_host: get("directory", "host").map(|v| truthy(&v)).unwrap_or(false),
            turn_urls: get("turn", "urls").map(|v| v.split(',').map(|u| u.trim().to_string()).filter(|u| !u.is_empty()).collect()).unwrap_or_default(),
            turn_secret: get("turn", "secret").unwrap_or_default(),
            turn_ttl: get("turn", "ttl").and_then(|v| v.trim().parse().ok()).filter(|n| *n >= 60).unwrap_or(d.turn_ttl),
            instance_id: get("identity", "id").unwrap_or_default(),
            instance_key: get("identity", "key").unwrap_or_default(),
        }
    }

    /// Açıklamalı şablonla INI metni.
    pub fn to_ini(&self) -> String {
        // Tek satır; ; # " ya da baş/son boşluk içeren değer tırnaklanır.
        let one = |s: &str| {
            let s = s.replace(['\r', '\n'], " ");
            if s.contains([';', '#', '"']) || s.trim() != s { format!("\"{}\"", s.replace('"', "\\\"")) } else { s }
        };
        format!(
            "; P2 Server ayarları. Sunucu açılırken okunur, çalışırken yapılan değişiklikler birkaç saniye içinde uygulanır.
; Server konsolundan (Ayarlar > Sunucu tarayıcısı) da düzenlenebilir.

[server]
; Sunucu tarayıcısında görünen ad ve açıklama.
name = {name}
description = {description}
; Simge: bu klasöre göre dosya yolu (png, jpg, webp; kare, en az 256 px) ya da https adresi. Boş = P2 simgesi.
icon = {icon}
; İstemcilerin bağlandığı adres, ör. https://p2.ornek.com (tarayıcıda listelenmek için gerekli).
public_url = {public_url}
; public: tarayıcıdan seçilip girilir. private: listede görünür, katılmak için kod gerekir.
access = {access}
; private sunucuya katılım kodu (yeni üyeler kayıt olurken de sorulur).
access_code = {access_code}

[directory]
; Sunucu tarayıcısında görünsün mü (true / false).
list = {list}
; Sunucu tarayıcısı dizini.
url = {directory}
; Bu sunucu dizini barındırsın mı (yalnız merkezi sunucu için true).
host = {host}

[turn]
; Sesli/görüntülü aramada doğrudan bağlantı kurulamazsa kullanılacak TURN sunucusu (coturn).
; Örnek: turn:p2.ornek.com:3478?transport=udp, turn:p2.ornek.com:3478?transport=tcp
urls = {turn_urls}
; coturn static-auth-secret ile aynı gizli anahtar.
secret = {turn_secret}
; Geçici TURN kimliğinin ömrü (saniye).
ttl = {turn_ttl}

[identity]
; Sunucu tarayıcısındaki kimlik. Değiştirmeyin; silinirse yeni kimlik üretilir (eski kayıt düşer).
id = {id}
key = {key}
",
            name = one(&self.name),
            description = one(&self.description),
            icon = one(&self.icon),
            public_url = one(&self.public_url),
            access = self.access.as_str(),
            access_code = one(&self.access_code),
            list = self.listed,
            directory = one(&self.directory_url),
            host = self.directory_host,
            turn_urls = self.turn_urls.join(", "),
            turn_secret = one(&self.turn_secret),
            turn_ttl = self.turn_ttl,
            id = self.instance_id,
            key = self.instance_key,
        )
    }

    /// Okur; dosya yoksa ya da kimlik eksikse kimlik üretip yazar.
    pub fn load_or_create(data_dir: &Path) -> Self {
        let file = path(data_dir);
        let mut s = std::fs::read_to_string(&file).map(|t| Self::from_ini(&t)).unwrap_or_default();
        if s.instance_id.is_empty() || s.instance_key.is_empty() {
            if s.instance_id.is_empty() {
                s.instance_id = random_hex(12);
            }
            if s.instance_key.is_empty() {
                s.instance_key = random_hex(24);
            }
            let _ = s.save(data_dir);
        }
        s
    }

    pub fn save(&self, data_dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(data_dir)?;
        let file = path(data_dir);
        let tmp = file.with_extension("ini.tmp");
        std::fs::write(&tmp, self.to_ini())?;
        std::fs::rename(&tmp, &file)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o640));
        }
        Ok(())
    }

    /// Simge dosyasının tam yolu (adres değilse).
    pub fn icon_file(&self, data_dir: &Path) -> Option<PathBuf> {
        let icon = self.icon.trim();
        if icon.is_empty() || icon.starts_with("http://") || icon.starts_with("https://") {
            return None;
        }
        let p = PathBuf::from(icon);
        Some(if p.is_absolute() { p } else { data_dir.join(p) })
    }

    /// Özel sunucunun kodu dizine açık gönderilmez: `sha256(id ":" kod)` (onaltılık).
    pub fn code_hash(&self) -> Option<String> {
        (self.access == Access::Private && !self.access_code.trim().is_empty()).then(|| code_hash(&self.instance_id, &self.access_code))
    }
}

pub fn code_hash(instance_id: &str, code: &str) -> String {
    use sha2::Digest as _;
    hex::encode(sha2::Sha256::digest(format!("{instance_id}:{}", code.trim()).as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ini_round_trip() {
        let mut s = ServerSettings { name: "Ekip ; Sunucusu".into(), description: "Açıklama".into(), access: Access::Private, access_code: "1234".into(), ..Default::default() };
        s.turn_urls = vec!["turn:a:3478?transport=udp".into(), "turn:a:3478?transport=tcp".into()];
        s.instance_id = "abc".into();
        s.instance_key = "def".into();
        let back = ServerSettings::from_ini(&s.to_ini());
        assert_eq!(back.name, "Ekip ; Sunucusu");
        assert_eq!(back.access, Access::Private);
        assert_eq!(back.turn_urls.len(), 2);
        assert_eq!(back.instance_id, "abc");
        assert!(back.listed);
        assert_eq!(back.directory_url, DEFAULT_DIRECTORY);
    }

    #[test]
    fn defaults_and_truthy() {
        let s = ServerSettings::from_ini("[directory]\nlist = false\nhost = evet\n[server]\naccess = PRIVATE\n");
        assert!(!s.listed && s.directory_host);
        assert_eq!(s.access, Access::Private);
        assert_eq!(s.name, "P2 Server");
        assert_eq!(code_hash("x", " 12 "), code_hash("x", "12"));
    }
}
