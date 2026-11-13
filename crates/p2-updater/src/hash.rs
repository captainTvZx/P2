//! Dosya özeti — latest.yml'deki `sha512` base64 (electron-builder), eski sürümlerde hex olabilir.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use base64::Engine as _;
use sha2::{Digest, Sha256, Sha512};

use crate::error::{ErrorKind, UpdateError};

/// Dosyanın SHA-512 özeti, base64 (electron-updater `hashFile` ile aynı çıktı).
pub fn sha512_base64_file(path: &Path) -> std::io::Result<String> {
    let mut hasher = Sha512::new();
    stream(path, |chunk| hasher.update(chunk))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(hasher.finalize()))
}

pub fn sha512_base64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(Sha512::digest(bytes))
}

fn sha256_hex_file(path: &Path) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    stream(path, |chunk| hasher.update(chunk))?;
    Ok(hex::encode(hasher.finalize()))
}

fn stream(path: &Path, mut sink: impl FnMut(&[u8])) -> std::io::Result<()> {
    let mut file = File::open(path)?;
    let mut buffer = vec![0u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            return Ok(());
        }
        sink(&buffer[..read]);
    }
}

/// builder-util-runtime `DigestTransform` kuralı: 128 karakterlik, `+`/`Z`/`=` içermeyen sha512
/// hex kabul edilir, diğerleri base64.
fn sha512_is_hex(expected: &str) -> bool {
    expected.len() == 128 && !expected.contains('+') && !expected.contains('Z') && !expected.contains('=')
}

/// İndirilen dosyayı beklenen özetle karşılaştırır. sha512 yoksa sha2 (sha256 hex) kullanılır.
pub fn verify_file(path: &Path, sha512: Option<&str>, sha2: Option<&str>) -> Result<(), UpdateError> {
    let read_error = |error| UpdateError::io("Özet hesaplanamadı", error);
    if let Some(expected) = sha512 {
        let actual = if sha512_is_hex(expected) {
            let mut hasher = Sha512::new();
            stream(path, |chunk| hasher.update(chunk)).map_err(read_error)?;
            hex::encode(hasher.finalize())
        } else {
            sha512_base64_file(path).map_err(read_error)?
        };
        if actual != expected {
            return Err(UpdateError::new(ErrorKind::ChecksumMismatch, format!("sha512 checksum mismatch, expected {expected}, got {actual}")));
        }
        return Ok(());
    }
    if let Some(expected) = sha2 {
        let actual = sha256_hex_file(path).map_err(read_error)?;
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(UpdateError::new(ErrorKind::ChecksumMismatch, format!("sha256 checksum mismatch, expected {expected}, got {actual}")));
        }
        return Ok(());
    }
    Err(UpdateError::new(ErrorKind::NoChecksum, "Update info doesn't contain nor sha256 neither sha512 checksum"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bilinen_ozetler() {
        // `printf abc | openssl dgst -sha512 -binary | base64`
        assert_eq!(sha512_base64(b"abc"), "3a81oZNherrMQXNJriBBMRLm+k6JqX6iCp7u5ktV05ohkpkqJ0/BqDa6PCOj/uu9RU1EI2Q86A4qmslPpUyknw==");
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.bin");
        std::fs::write(&file, b"abc").unwrap();
        assert_eq!(sha512_base64_file(&file).unwrap(), sha512_base64(b"abc"));
        verify_file(&file, Some(&sha512_base64(b"abc")), None).unwrap();
        let hex512 = hex::encode(Sha512::digest(b"abc"));
        verify_file(&file, Some(&hex512), None).unwrap();
        verify_file(&file, None, Some("BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD")).unwrap();
        let err = verify_file(&file, Some(&sha512_base64(b"abd")), None).unwrap_err();
        assert_eq!(err.kind, ErrorKind::ChecksumMismatch);
        assert!(err.message.starts_with("sha512 checksum mismatch, expected "));
        assert_eq!(verify_file(&file, None, None).unwrap_err().kind, ErrorKind::NoChecksum);
    }
}
