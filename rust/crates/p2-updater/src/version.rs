//! Sürüm karşılaştırma — electron-updater'ın `isUpdateAvailable` kuralı (node-semver).

use std::cmp::Ordering;

use semver::Version;

use crate::error::{ErrorKind, UpdateError};

/// node-semver `parse` (katı mod): baştaki/sondaki boşluk kırpılır, isteğe bağlı `v` öneki
/// kabul edilir, 256 karakterden uzun metin geçersizdir.
pub fn parse_version(text: &str) -> Option<Version> {
    if text.len() > 256 {
        return None;
    }
    let text = text.trim();
    let text = text.strip_prefix('v').unwrap_or(text);
    Version::parse(text).ok()
}

/// node-semver önceliği: derleme üst verisi (`+…`) yok sayılır; ön sürüm (`-beta.1`) kararlı
/// sürümden küçüktür, sayısal tanımlayıcılar sayısal, diğerleri sözlük sırasıyla karşılaştırılır.
pub fn compare(a: &Version, b: &Version) -> Ordering {
    a.cmp_precedence(b)
}

/// electron-updater `AppUpdater.isUpdateAvailable` (stagingPercentage/minimumSystemVersion
/// denetimleri hariç; onlar `Updater` içinde):
/// eşitse yok; yeniyse var; eskiyse yalnızca `allow_downgrade` açıkken var.
pub fn is_newer_available(current: &Version, latest: &str, allow_downgrade: bool) -> Result<bool, UpdateError> {
    let Some(latest) = parse_version(latest) else {
        return Err(UpdateError::new(
            ErrorKind::InvalidVersion,
            format!("This file could not be downloaded, or the latest version (from update server) does not have a valid semver version: \"{latest}\""),
        ));
    };
    Ok(match compare(&latest, current) {
        Ordering::Equal => false,
        Ordering::Greater => true,
        Ordering::Less => allow_downgrade,
    })
}

/// electron-updater: çalışan sürüm ön sürümse `allowPrerelease` açılır (yalnız GitHub
/// sağlayıcısını etkiler; generic sağlayıcıda bilgi amaçlıdır).
pub fn has_prerelease(version: &Version) -> bool {
    !version.pre.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        parse_version(text).unwrap()
    }

    #[test]
    fn ayristirma() {
        assert_eq!(v(" v1.2.3 "), Version::new(1, 2, 3));
        assert!(parse_version("1.2").is_none());
        assert!(parse_version("01.2.3").is_none());
        assert!(parse_version("V1.2.3").is_none());
        assert!(parse_version(&"1".repeat(300)).is_none());
        assert!(has_prerelease(&v("1.0.0-beta.2")));
        assert!(!has_prerelease(&v("1.0.0+build.5")));
    }

    #[test]
    fn oncelik() {
        assert_eq!(compare(&v("1.0.0+a"), &v("1.0.0+b")), Ordering::Equal);
        assert_eq!(compare(&v("1.0.0-alpha"), &v("1.0.0")), Ordering::Less);
        assert_eq!(compare(&v("1.0.0-alpha.2"), &v("1.0.0-alpha.10")), Ordering::Less);
        assert_eq!(compare(&v("1.0.0-alpha.1"), &v("1.0.0-alpha.beta")), Ordering::Less);
        assert_eq!(compare(&v("1.0.0-beta.11"), &v("1.0.0-rc.1")), Ordering::Less);
        assert_eq!(compare(&v("0.10.0"), &v("0.9.9")), Ordering::Greater);
    }

    #[test]
    fn guncelleme_var_mi() {
        let current = v("0.9.9");
        assert!(is_newer_available(&current, "0.9.10", false).unwrap());
        assert!(is_newer_available(&current, "1.0.0-beta.1", false).unwrap());
        assert!(!is_newer_available(&current, "0.9.9", false).unwrap());
        assert!(!is_newer_available(&current, "0.9.9+rebuild", false).unwrap());
        assert!(!is_newer_available(&current, "0.9.8", false).unwrap());
        assert!(is_newer_available(&current, "0.9.8", true).unwrap());
        assert!(!is_newer_available(&current, "0.9.9-rc.1", false).unwrap());
        assert_eq!(is_newer_available(&current, "latest", false).unwrap_err().kind, ErrorKind::InvalidVersion);
        let beta = v("1.0.0-beta.1");
        assert!(is_newer_available(&beta, "1.0.0", false).unwrap());
        assert!(is_newer_available(&beta, "1.0.0-beta.2", false).unwrap());
    }
}
