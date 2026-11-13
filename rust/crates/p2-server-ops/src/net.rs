//! Yerel API ile konuşma (sağlık denetimi, imzalı kapatma isteği) ve public IP sorgusu.
//! Yerel istekler bağımlılıksız, düz HTTP/1.1 ile yapılır (zaman aşımı hassas olsun diye).

use std::io::{Read as _, Write as _};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use hmac::{Hmac, KeyInit as _, Mac as _};
use sha2::Sha256;

/// 127.0.0.1:`port` üzerinde tek bir HTTP isteği; (durum kodu, gövde).
pub fn local_request(port: u16, method: &str, path: &str, body: Option<&str>, timeout: Duration) -> Option<(u16, String)> {
    let deadline = Instant::now() + timeout;
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&addr, timeout).ok()?;
    let left = |d: Instant| d.saturating_duration_since(Instant::now()).max(Duration::from_millis(1));
    stream.set_write_timeout(Some(left(deadline))).ok()?;
    let mut request = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n");
    if let Some(body) = body {
        request.push_str(&format!("Content-Type: application/json\r\nContent-Length: {}\r\n", body.len()));
    }
    request.push_str("\r\n");
    if let Some(body) = body {
        request.push_str(body);
    }
    stream.write_all(request.as_bytes()).ok()?;
    let mut raw = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        stream.set_read_timeout(Some(left(deadline))).ok()?;
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => raw.extend_from_slice(&buf[..n]),
            Err(_) => {
                if raw.is_empty() {
                    return None;
                }
                break;
            }
        }
        if Instant::now() >= deadline {
            break;
        }
    }
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = text.split_once("\r\n\r\n")?;
    let status: u16 = head.split_whitespace().nth(1)?.parse().ok()?;
    let chunked = head.lines().any(|l| l.to_ascii_lowercase().starts_with("transfer-encoding:") && l.to_ascii_lowercase().contains("chunked"));
    let body = if chunked { dechunk(body) } else { body.to_string() };
    Some((status, body))
}

fn dechunk(body: &str) -> String {
    let mut out = String::new();
    let mut rest = body;
    while let Some((size, after)) = rest.split_once("\r\n") {
        let Ok(n) = usize::from_str_radix(size.trim(), 16) else { break };
        if n == 0 || after.len() < n {
            break;
        }
        out.push_str(&after[..n]);
        rest = after[n..].trim_start_matches("\r\n");
    }
    out
}

/// `apiAlive(timeout = 900)`: `/health` gövdesi `{"status":"ok"}` ise canlı.
pub fn api_alive(port: u16, timeout: Duration) -> bool {
    local_request(port, "GET", "/health", None, timeout)
        .and_then(|(_, body)| serde_json::from_str::<serde_json::Value>(&body).ok())
        .is_some_and(|v| v.get("status").and_then(|s| s.as_str()) == Some("ok"))
}

/// `createHmac('sha256', accessSecret).update('p2-shutdown').digest('hex')`.
pub fn shutdown_token(access_secret: &str) -> String {
    if access_secret.is_empty() {
        return String::new();
    }
    let mut mac = Hmac::<Sha256>::new_from_slice(access_secret.as_bytes()).expect("HMAC her anahtar uzunluğunu kabul eder");
    mac.update(b"p2-shutdown");
    hex::encode(mac.finalize().into_bytes())
}

/// `POST /updates/stop {token}` (4 sn zaman aşımı; sonuç önemsenmez).
pub fn request_stop(port: u16, access_secret: &str) {
    let body = serde_json::json!({ "token": shutdown_token(access_secret) }).to_string();
    let _ = local_request(port, "POST", "/updates/stop", Some(&body), Duration::from_secs(4));
}

/// `resolvePublicIp`: api.ipify.org (6 sn). `P2_PUBLIC_IP` ayarlıysa ağa çıkılmadan o kullanılır (testler).
/// Bloklayan çağrı; tokio çalışma zamanı dışında çağrılmalı.
pub fn public_ip() -> Option<String> {
    if let Ok(fixed) = std::env::var("P2_PUBLIC_IP") {
        return Some(fixed).filter(|v| valid_ip(v));
    }
    let client = reqwest::blocking::Client::builder().timeout(Duration::from_secs(6)).build().ok()?;
    let value: serde_json::Value = client.get("https://api.ipify.org?format=json").send().ok()?.json().ok()?;
    let ip = value.get("ip").map(|v| match v {
        serde_json::Value::String(s) => s.trim().to_string(),
        other => other.to_string(),
    })?;
    valid_ip(&ip).then_some(ip)
}

/// `/^(?:\d{1,3}\.){3}\d{1,3}$/.test(value) || value.includes(':')`.
pub fn valid_ip(value: &str) -> bool {
    static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"^(?:\d{1,3}\.){3}\d{1,3}$").unwrap()).is_match(value) || value.contains(':')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_matches_node_hmac() {
        // node -e "console.log(require('crypto').createHmac('sha256','gizli').update('p2-shutdown').digest('hex'))"
        assert_eq!(shutdown_token("gizli"), "cd833671433bed560bce5d24fad75baec02bad9ebe7269553220e10413cfec71");
        assert_eq!(shutdown_token(""), "");
    }

    #[test]
    fn ip_validation() {
        assert!(valid_ip("85.10.1.2"));
        assert!(valid_ip("2a02::1"));
        assert!(!valid_ip("public"));
        assert!(!valid_ip(""));
    }
}
