//! class-validator + ValidationPipe({ whitelist, transform }) + `validationException` karşılığı.
//!
//! NestJS kuralı (server/src/validation.ts): hatalar özellik sırasıyla toplanır; içinde
//! "güvenli" (context.safe) mesaj taşıyan ilk hata kullanıcıya gösterilir, yoksa genel
//! "Girdi alanlarını kontrol edin." mesajı ve ilk hatalı alan döner.

use axum::body::Bytes;
use axum::http::StatusCode;
use serde_json::{Map, Value, json};

use crate::error::ApiError;

pub struct Checker<'a> {
    body: &'a Map<String, Value>,
    /// (alan, güvenli mesaj varsa o)
    errors: Vec<(&'static str, Option<&'static str>)>,
}

impl<'a> Checker<'a> {
    pub fn new(body: &'a Map<String, Value>) -> Self {
        Self { body, errors: Vec::new() }
    }

    /// Bir alanın kısıtları: her biri (geçti mi, güvenli mesaj). İlk güvenli mesaj seçilir.
    pub fn field(&mut self, name: &'static str, checks: &[(bool, Option<&'static str>)]) {
        let failed: Vec<_> = checks.iter().filter(|(ok, _)| !ok).collect();
        if !failed.is_empty() {
            self.errors.push((name, failed.iter().find_map(|(_, safe)| *safe)));
        }
    }

    pub fn str(&self, name: &str) -> Option<&'a str> {
        self.body.get(name).and_then(Value::as_str)
    }

    pub fn present(&self, name: &str) -> bool {
        !matches!(self.body.get(name), None | Some(Value::Null))
    }

    pub fn finish(self) -> Result<(), ApiError> {
        let Some(first) = self.errors.first() else { return Ok(()) };
        let (field, message) = self
            .errors
            .iter()
            .find_map(|(field, safe)| safe.map(|m| (*field, m)))
            .unwrap_or((first.0, "Girdi alanlarını kontrol edin."));
        Err(ApiError::bad_request("VALIDATION_ERROR", message).field(field))
    }
}

/// Gövdeyi Express'in json() ayrıştırıcısı gibi okur: boş gövde `{}` sayılır.
pub fn body(bytes: &Bytes) -> Result<Map<String, Value>, ApiError> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(Map::new());
    }
    match serde_json::from_slice::<Value>(bytes) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Ok(Map::new()),
        Err(error) => Err(ApiError {
            status: StatusCode::BAD_REQUEST,
            body: json!({ "message": error.to_string(), "error": "Bad Request", "statusCode": 400 }),
        }),
    }
}

/// class-validator `@Length(min, max)` uzunluğu (validator.js isLength).
pub fn js_len(value: &str) -> usize {
    // validator.js isLength (class-validator @Length/@MaxLength): vekil çiftleri ve varyasyon seçicileri düşülür.
    crate::tasks::vlen(value)
}

/// JS `string.length` (UTF-16 kod birimi) — elle yazılmış uzunluk kontrolleri için.
pub fn utf16_len(value: &str) -> usize {
    value.encode_utf16().count()
}

/// validator.js `isEmail` için yaklaşık (Faz 0; tam karşılık Faz 2'de sözleşme testleriyle).
pub fn is_email(value: &str) -> bool {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| {
        regex::Regex::new(r"^[A-Za-z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?(?:\.[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?)+$").unwrap()
    })
    .is_match(value)
        // validator.js (allow_numeric_tld: false): TLD en az 2 harf ya da punycode (xn--…).
        // Sözleşme testi yakaladı: "x@test.p2" NestJS'te geçersiz.
        && value.split('@').next_back().is_some_and(|domain| {
            domain.rsplit('.').next().is_some_and(|tld| {
                (tld.len() >= 2 && tld.chars().all(|c| c.is_alphabetic())) || tld.to_ascii_lowercase().starts_with("xn--")
            })
        })
}

/// JS `String.prototype.slice(0, n)` (UTF-16 kod birimi); vekil çifti yarıda kalırsa düşer.
pub fn js_slice(value: &str, n: usize) -> String {
    let mut units = 0;
    let mut out = String::new();
    for ch in value.chars() {
        let w = ch.len_utf16();
        if units + w > n {
            break;
        }
        units += w;
        out.push(ch);
    }
    out
}

/// JS `Number(x)`: boşluk kırpılır, boş → 0, geçersiz → None (NaN).
pub fn js_number(value: &str) -> Option<f64> {
    let t = value.trim();
    if t.is_empty() {
        return Some(0.0);
    }
    if let Some(hex) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return i64::from_str_radix(hex, 16).ok().map(|v| v as f64);
    }
    if t.chars().any(|c| c.is_ascii_alphabetic() && !matches!(c, 'e' | 'E')) && !matches!(t, "Infinity" | "+Infinity" | "-Infinity") {
        return None;
    }
    t.parse::<f64>().ok().filter(|f| !f.is_nan())
}

/// JSON değerinden dizgi (yoksa ya da dizgi değilse None).
pub fn opt_str(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).map(str::to_string)
}
