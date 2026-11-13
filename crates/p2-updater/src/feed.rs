//! Güncelleme kaynağı adresleri — electron-updater generic sağlayıcısı + P2'nin kanal yolları.

use std::time::{SystemTime, UNIX_EPOCH};

use reqwest::Url;

use crate::error::{ErrorKind, UpdateError};
use crate::yml::{FileInfo, UpdateInfo};

/// İstemcinin bağlı olduğu API adresinden güncelleme kaynağı (updater.cjs `configure`):
/// `http(s)://adres[:port]` → `…/updates/client/`. Adres http/https değilse `None`.
pub fn client_feed_url(api_base: &str) -> Option<String> {
    let base = normalize_base(api_base)?;
    Some(format!("{base}/updates/client/"))
}

/// updater.cjs: baştaki/sondaki boşluk ve sondaki tüm `/` atılır; şema http/https olmalı.
pub fn normalize_base(api_base: &str) -> Option<String> {
    let base = api_base.trim().trim_end_matches('/');
    let lower = base.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") { Some(base.to_string()) } else { None }
}

/// Sunucu konsolunun kendi makinesindeki API'den güncellemesi (server-updater.cjs `init`).
pub fn server_feed_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/updates/server/")
}

/// electron-updater `newBaseUrl`: yol `/` ile bitmiyorsa eklenir (yoksa göreli çözümde son
/// parça düşerdi).
pub fn base_url(feed: &str) -> Result<Url, UpdateError> {
    let mut url = Url::parse(feed).map_err(|e| UpdateError::new(ErrorKind::InvalidUpdateInfo, format!("Geçersiz güncelleme adresi {feed}: {e}")))?;
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    Ok(url)
}

/// electron-updater `newUrlFromBase`: göreli yol tabana göre çözülür; tabanın sorgu dizesi
/// taşınır, yoksa istenirse `noCache=<zaman, 32 tabanında>` eklenir.
pub fn url_from_base(path: &str, base: &Url, add_no_cache_query: bool) -> Result<Url, UpdateError> {
    let mut url = base.join(path).map_err(|e| UpdateError::new(ErrorKind::InvalidUpdateInfo, format!("Geçersiz dosya yolu {path}: {e}")))?;
    match base.query() {
        Some(query) if !query.is_empty() => url.set_query(Some(query)),
        _ if add_no_cache_query => {
            let millis = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
            url.set_query(Some(&format!("noCache={}", radix32(millis))));
        }
        _ => {}
    }
    Ok(url)
}

/// JS `Number.prototype.toString(32)`.
pub(crate) fn radix32(mut value: u64) -> String {
    const DIGITS: &[u8; 32] = b"0123456789abcdefghijklmnopqrstuv";
    if value == 0 {
        return "0".into();
    }
    let mut out = Vec::new();
    while value > 0 {
        out.push(DIGITS[(value % 32) as usize]);
        value /= 32;
    }
    out.reverse();
    String::from_utf8(out).unwrap()
}

/// Kanal dosyası adı: Windows'ta platform eki yok (`latest` → `latest.yml`).
pub fn channel_file(channel: &str) -> String {
    format!("{channel}.yml")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedFile {
    pub url: Url,
    pub info: FileInfo,
}

/// electron-updater `resolveFiles`: her dosyanın adresi tabana göre çözülür; özeti olmayan
/// dosya hata.
pub fn resolve_files(info: &UpdateInfo, base: &Url) -> Result<Vec<ResolvedFile>, UpdateError> {
    info.file_list()?
        .into_iter()
        .map(|file| {
            if file.sha512.is_none() && file.sha2.is_none() {
                return Err(UpdateError::new(ErrorKind::NoChecksum, format!("Update info doesn't contain nor sha256 neither sha512 checksum: {}", file.url)));
            }
            Ok(ResolvedFile { url: url_from_base(&file.url, base, false)?, info: file })
        })
        .collect()
}

/// Node `process.arch` karşılığı.
pub fn node_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x64",
        "x86" => "ia32",
        "aarch64" => "arm64",
        other => other,
    }
}

/// electron-updater `findFile(files, "exe")`: `.exe` ile bitenler arasından adında mimari
/// geçen, yoksa ilki; hiç `.exe` yoksa listenin ilki.
pub fn find_file(files: &[ResolvedFile], extension: &str) -> Option<ResolvedFile> {
    let suffix = format!(".{}", extension.to_ascii_lowercase());
    let filtered: Vec<&ResolvedFile> = files.iter().filter(|f| f.url.path().to_ascii_lowercase().ends_with(&suffix)).collect();
    let arch = node_arch();
    filtered
        .iter()
        .find(|f| f.url.path().contains(arch) || f.info.url.contains(arch))
        .or(filtered.first())
        .map(|f| (*f).clone())
        .or_else(|| files.first().cloned())
}

/// electron-updater `getCacheUpdateFileName`: adres yolu (çözülmüş) `.exe` ile bitiyorsa
/// onun son parçası, değilse latest.yml'deki adın son parçası. Dizin dışına çıkamaz.
pub fn cache_file_name(file: &ResolvedFile, extension: &str) -> String {
    let decoded = percent_decode(file.url.path());
    let source = if decoded.to_ascii_lowercase().ends_with(&format!(".{}", extension.to_ascii_lowercase())) { decoded } else { file.info.url.clone() };
    let name = source.rsplit(['/', '\\']).next().unwrap_or("").to_string();
    if name.is_empty() || name == "." || name == ".." { format!("update.{extension}") } else { name }
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Some(value) = std::str::from_utf8(&bytes[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(value);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kaynak_adresleri() {
        assert_eq!(client_feed_url(" http://192.168.1.5:4000/// ").as_deref(), Some("http://192.168.1.5:4000/updates/client/"));
        assert_eq!(client_feed_url("HTTPS://p2.example/api/").as_deref(), Some("HTTPS://p2.example/api/updates/client/"));
        assert_eq!(client_feed_url("192.168.1.5:4000"), None);
        assert_eq!(client_feed_url(""), None);
        assert_eq!(server_feed_url(4000), "http://127.0.0.1:4000/updates/server/");
    }

    #[test]
    fn adres_cozumleme() {
        let base = base_url("http://h:1/updates/client").unwrap();
        assert_eq!(base.as_str(), "http://h:1/updates/client/");
        let channel = url_from_base(&channel_file("latest"), &base, true).unwrap();
        assert_eq!(channel.path(), "/updates/client/latest.yml");
        assert!(channel.query().unwrap().starts_with("noCache="));
        let with_query = base_url("http://h:1/u/?token=1").unwrap();
        assert_eq!(url_from_base("latest.yml", &with_query, true).unwrap().as_str(), "http://h:1/u/latest.yml?token=1");
        let absolute = url_from_base("https://cdn.example/x.exe", &base, false).unwrap();
        assert_eq!(absolute.as_str(), "https://cdn.example/x.exe");
        assert_eq!(radix32(0), "0");
        assert_eq!(radix32(31), "v");
        assert_eq!(radix32(1023), "vv");
        assert_eq!(radix32(1024), "100");
    }

    #[test]
    fn dosya_secimi_ve_adi() {
        let base = base_url("http://h/updates/server/").unwrap();
        let info = UpdateInfo {
            version: "1.0.0".into(),
            files: vec![
                FileInfo { url: "notes.txt".into(), sha512: Some("A".into()), ..Default::default() },
                FileInfo { url: "P2%20Setup-ia32.exe".into(), sha512: Some("B".into()), ..Default::default() },
                FileInfo { url: format!("P2 Setup-{}.exe", node_arch()), sha512: Some("C".into()), ..Default::default() },
            ],
            ..Default::default()
        };
        let files = resolve_files(&info, &base).unwrap();
        let chosen = find_file(&files, "exe").unwrap();
        assert_eq!(chosen.info.sha512.as_deref(), Some("C"));
        assert_eq!(cache_file_name(&chosen, "exe"), format!("P2 Setup-{}.exe", node_arch()));
        let evil = ResolvedFile { url: Url::parse("http://h/latest").unwrap(), info: FileInfo { url: "../../etc/evil".into(), ..Default::default() } };
        assert_eq!(cache_file_name(&evil, "exe"), "evil");
        let nochk = UpdateInfo { version: "1.0.0".into(), files: vec![FileInfo { url: "a.exe".into(), ..Default::default() }], ..Default::default() };
        assert_eq!(resolve_files(&nochk, &base).unwrap_err().kind, ErrorKind::NoChecksum);
    }
}
