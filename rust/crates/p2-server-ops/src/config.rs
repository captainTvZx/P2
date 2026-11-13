//! `server.json`: `{ databaseUrl, port, accessSecret, refreshSecret }` (2 boşluk girintili JSON).
//! Electron sürümüyle aynı dosya; mevcut kurulumlar taşınmadan çalışır.

use std::path::Path;

use serde_json::{Map, Value};

/// Dosyanın okunmuş hali. Bilinmeyen anahtarlar okunurken yok sayılır (Electron da yazarken düşürür).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ServerConfig {
    pub database_url: Option<String>,
    /// JS `Number(config?.port || 4000)` için ham değer.
    pub port: Option<Value>,
    pub access_secret: Option<String>,
    pub refresh_secret: Option<String>,
}

impl ServerConfig {
    /// `readConfig()`: dosya yoksa ya da JSON bozuksa `None`.
    pub fn read(file: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(file).ok()?;
        let value: Value = serde_json::from_str(&text).ok()?;
        Some(Self::from_value(&value))
    }

    pub fn from_value(value: &Value) -> Self {
        let text = |key: &str| match value.get(key) {
            Some(Value::String(s)) => Some(s.clone()),
            Some(Value::Null) | None => None,
            Some(other) => Some(js_string(other)),
        };
        Self {
            database_url: text("databaseUrl"),
            port: value.get("port").cloned().filter(|v| !v.is_null()),
            access_secret: text("accessSecret"),
            refresh_secret: text("refreshSecret"),
        }
    }

    /// `Boolean(config?.databaseUrl)`.
    pub fn configured(&self) -> bool {
        self.database_url.as_deref().is_some_and(|v| !v.is_empty())
    }

    /// `Number(config?.port || 4000)`; geçersizse 4000.
    pub fn port(&self) -> u16 {
        port_of(self.port.as_ref())
    }

    /// `JSON.stringify(config, null, 2)` — anahtar sırası Electron'daki gibi.
    pub fn to_json(&self) -> String {
        let mut map = Map::new();
        map.insert("databaseUrl".into(), self.database_url.clone().map(Value::String).unwrap_or(Value::Null));
        map.insert("port".into(), self.port.clone().unwrap_or(Value::Null));
        map.insert("accessSecret".into(), self.access_secret.clone().map(Value::String).unwrap_or(Value::Null));
        map.insert("refreshSecret".into(), self.refresh_secret.clone().map(Value::String).unwrap_or(Value::Null));
        serde_json::to_string_pretty(&Value::Object(map)).unwrap()
    }

    /// Yazar (POSIX'te 0600). Klasörü oluşturur.
    pub fn write(&self, file: &Path) -> std::io::Result<()> {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(file, self.to_json())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(file, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }
}

fn js_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// JS truthy + `Number(...)`: 0/""/null → 4000; sayıya çevrilemeyen ya da aralık dışı → 4000.
pub fn port_of(value: Option<&Value>) -> u16 {
    let n = match value {
        Some(Value::Number(n)) => n.as_f64().unwrap_or(0.),
        Some(Value::String(s)) if !s.is_empty() => s.trim().parse::<f64>().unwrap_or(f64::NAN),
        Some(Value::Bool(true)) => 1.,
        _ => 0.,
    };
    if n == 0. || n.is_nan() || n.fract() != 0. || !(1. ..=65535.).contains(&n) { 4000 } else { n as u16 }
}

/// `crypto.randomBytes(48).toString('hex')`.
pub fn random_secret() -> String {
    let mut bytes = [0u8; 48];
    getrandom::fill(&mut bytes).expect("rastgele bayt üretilemedi");
    hex::encode(bytes)
}

/// JS `encodeURIComponent`: `A-Z a-z 0-9 - _ . ! ~ * ' ( )` dışındaki UTF-8 baytları `%XX`.
pub fn encode_uri_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')' => out.push(byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Yapılandırma formu (`server-console:configure` girdisi). Sayılar JS `Number(...)` sonucudur.
#[derive(Clone, Debug, Default)]
pub struct ConfigureInput {
    pub host: String,
    pub db_port: f64,
    pub api_port: f64,
    pub username: String,
    pub password: String,
    pub database: String,
    pub ssl: bool,
}

/// Doğrulanmış yapılandırma girdisi.
#[derive(Clone, Debug, PartialEq)]
pub struct Validated {
    pub host: String,
    pub db_port: u16,
    pub api_port: u16,
    pub username: String,
    pub password: String,
    pub database: String,
    pub ssl: bool,
}

pub const INVALID_SETTINGS: &str = "Veritabanı veya API portu ayarları geçersiz.";

impl ConfigureInput {
    /// configure() başındaki denetim: aynı kurallar, aynı ileti.
    pub fn validate(&self) -> Result<Validated, String> {
        let host = self.host.trim().to_string();
        let username = self.username.trim().to_string();
        let database = self.database.trim().to_lowercase();
        // `Number(input?.apiPort || 4000)`: 0/NaN → 4000.
        let api = if self.api_port == 0. || self.api_port.is_nan() { 4000. } else { self.api_port };
        let port_ok = |n: f64| n.fract() == 0. && (1. ..=65535.).contains(&n);
        let name_ok = regex::Regex::new(r"^[a-z][a-z0-9_]{0,62}$").unwrap().is_match(&database);
        if host.is_empty() || username.is_empty() || self.password.is_empty() || !port_ok(self.db_port) || !port_ok(api) || !name_ok {
            return Err(INVALID_SETTINGS.into());
        }
        Ok(Validated { host, db_port: self.db_port as u16, api_port: api as u16, username, password: self.password.clone(), database, ssl: self.ssl })
    }
}

impl Validated {
    /// configure() içindeki `databaseUrl` şablonu.
    pub fn database_url(&self) -> String {
        format!(
            "postgresql://{}:{}@{}:{}/{}?schema=public{}",
            encode_uri_component(&self.username),
            encode_uri_component(&self.password),
            self.host,
            self.db_port,
            self.database,
            if self.ssl { "&sslmode=require" } else { "" }
        )
    }
}

/// `cleanDatabaseUrl`: Prisma'ya özgü `schema` parametresi atılır (pg_dump/pg_restore ve sqlx için).
pub fn clean_database_url(value: &str) -> anyhow::Result<String> {
    let mut url = url::Url::parse(value).map_err(|_| anyhow::anyhow!("Invalid URL"))?;
    let kept: Vec<(String, String)> = url.query_pairs().filter(|(k, _)| k != "schema").map(|(k, v)| (k.into_owned(), v.into_owned())).collect();
    if kept.is_empty() {
        url.set_query(None);
    } else {
        url.query_pairs_mut().clear().extend_pairs(kept);
    }
    Ok(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_matches_electron_layout() {
        let config = ServerConfig {
            database_url: Some("postgresql://a:b@h:5432/p2?schema=public".into()),
            port: Some(Value::from(4000)),
            access_secret: Some("x".into()),
            refresh_secret: Some("y".into()),
        };
        assert_eq!(
            config.to_json(),
            "{\n  \"databaseUrl\": \"postgresql://a:b@h:5432/p2?schema=public\",\n  \"port\": 4000,\n  \"accessSecret\": \"x\",\n  \"refreshSecret\": \"y\"\n}"
        );
        let back = ServerConfig::from_value(&serde_json::from_str(&config.to_json()).unwrap());
        assert_eq!(back, config);
    }

    #[test]
    fn port_semantics() {
        assert_eq!(port_of(None), 4000);
        assert_eq!(port_of(Some(&Value::from(0))), 4000);
        assert_eq!(port_of(Some(&Value::from(4140))), 4140);
        assert_eq!(port_of(Some(&Value::from("4141"))), 4141);
        assert_eq!(port_of(Some(&Value::from("abc"))), 4000);
    }

    #[test]
    fn encode_like_js() {
        assert_eq!(encode_uri_component("p@ss w/rd!*'()~"), "p%40ss%20w%2Frd!*'()~");
        assert_eq!(encode_uri_component("şifre"), "%C5%9Fifre");
    }

    #[test]
    fn validation_rules() {
        let ok = ConfigureInput { host: " 127.0.0.1 ".into(), db_port: 5432., api_port: 0., username: "postgres".into(), password: "x".into(), database: " P2_Test ".into(), ssl: true };
        let v = ok.validate().unwrap();
        assert_eq!(v.api_port, 4000);
        assert_eq!(v.database, "p2_test");
        assert_eq!(v.database_url(), "postgresql://postgres:x@127.0.0.1:5432/p2_test?schema=public&sslmode=require");
        for bad in [
            ConfigureInput { host: "".into(), ..ok.clone() },
            ConfigureInput { password: "".into(), ..ok.clone() },
            ConfigureInput { db_port: 70000., ..ok.clone() },
            ConfigureInput { db_port: 1.5, ..ok.clone() },
            ConfigureInput { api_port: f64::NAN, db_port: f64::NAN, ..ok.clone() },
            ConfigureInput { database: "9db".into(), ..ok.clone() },
            ConfigureInput { database: "a-b".into(), ..ok.clone() },
        ] {
            assert_eq!(bad.validate().unwrap_err(), INVALID_SETTINGS);
        }
    }

    #[test]
    fn clean_url() {
        assert_eq!(clean_database_url("postgresql://u:p@h:5432/db?schema=public").unwrap(), "postgresql://u:p@h:5432/db");
        assert_eq!(clean_database_url("postgresql://u:p@h:5432/db?schema=public&sslmode=require").unwrap(), "postgresql://u:p@h:5432/db?sslmode=require");
    }
}
