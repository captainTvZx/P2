//! Google Authenticator uyumlu TOTP (RFC 6238, SHA-1, 6 hane, 30 sn). `server/src/totp.ts` karşılığı.

use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;

const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

pub fn generate_secret() -> String {
    let raw: [u8; 20] = rand::random();
    encode(&raw)
}

/// totp.ts gibi: 5 bitlik tam gruplar, dolgu yok.
fn encode(bytes: &[u8]) -> String {
    let bits: String = bytes.iter().map(|b| format!("{b:08b}")).collect();
    bits.as_bytes()
        .chunks_exact(5)
        .map(|chunk| ALPHABET[usize::from_str_radix(std::str::from_utf8(chunk).unwrap(), 2).unwrap()] as char)
        .collect()
}

fn decode(secret: &str) -> Vec<u8> {
    let bits: String = secret
        .to_uppercase()
        .bytes()
        .filter_map(|c| ALPHABET.iter().position(|&a| a == c))
        .map(|i| format!("{i:05b}"))
        .collect();
    bits.as_bytes().chunks_exact(8).map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 2).unwrap()).collect()
}

pub fn code_for(secret: &str, counter: u64) -> String {
    let mut mac = Hmac::<Sha1>::new_from_slice(&decode(secret)).expect("HMAC her anahtar uzunluğunu kabul eder");
    mac.update(&counter.to_be_bytes());
    let digest = mac.finalize().into_bytes();
    let offset = (digest[digest.len() - 1] & 0x0f) as usize;
    let value = (u32::from(digest[offset] & 0x7f) << 24)
        | (u32::from(digest[offset + 1]) << 16)
        | (u32::from(digest[offset + 2]) << 8)
        | u32::from(digest[offset + 3]);
    format!("{:06}", value % 1_000_000)
}

/// Saat kaymasına karşı bir adım öncesi ve sonrası da kabul edilir.
pub fn verify(secret: &str, code: &str) -> bool {
    let clean: String = code.chars().filter(char::is_ascii_digit).collect();
    if clean.len() != 6 {
        return false;
    }
    let counter = (chrono::Utc::now().timestamp_millis() / 30_000) as i64;
    (-1..=1).any(|step| code_for(secret, (counter + step) as u64) == clean)
}

/// JS `encodeURIComponent`.
pub fn encode_uri_component(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub fn otpauth_uri(secret: &str, account: &str) -> String {
    let issuer = "P2";
    format!(
        "otpauth://totp/{}:{}?secret={secret}&issuer={}&algorithm=SHA1&digits=6&period=30",
        encode_uri_component(issuer),
        encode_uri_component(account),
        encode_uri_component(issuer)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc6238_vector() {
        // RFC 6238 Ek B: anahtar "12345678901234567890", T=59 → 94287082 (8 hane) → son 6: 287082
        let secret = encode(b"12345678901234567890");
        assert_eq!(code_for(&secret, 59 / 30), "287082");
    }

    #[test]
    fn roundtrip_and_length() {
        let s = generate_secret();
        assert_eq!(s.len(), 32);
        assert_eq!(decode(&s).len(), 20);
    }

    #[test]
    fn uri_component() {
        assert_eq!(encode_uri_component("a b@c.com"), "a%20b%40c.com");
        assert_eq!(encode_uri_component("ş"), "%C5%9F");
    }
}
