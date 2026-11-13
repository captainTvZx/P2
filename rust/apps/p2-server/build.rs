//! Windows sürüm bilgisi (Faz 8): `p2-server.exe` hizmet ikilisi, Görev Yöneticisi ve dosya
//! özelliklerinde "P2 Server (API)" görünür. Sürüm `package.json` ya da `P2_PRODUCT_VERSION`.

use std::path::Path;

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let repo = Path::new(&manifest).join("../../..");
    println!("cargo:rerun-if-changed={}", repo.join("package.json").display());
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=P2_PRODUCT_VERSION");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let rc = out.join("server.rc");
    std::fs::write(&rc, version_info(&product_version(&repo), "P2 Server (API)", "P2 Server", "p2-server.exe")).unwrap();
    embed_resource::compile(&rc, embed_resource::NONE).manifest_optional().unwrap();
}

fn product_version(repo: &Path) -> String {
    if let Ok(v) = std::env::var("P2_PRODUCT_VERSION")
        && !v.trim().is_empty()
    {
        return v.trim().to_string();
    }
    let package = std::fs::read_to_string(repo.join("package.json")).unwrap_or_default();
    package
        .lines()
        .find_map(|l| l.trim().strip_prefix("\"version\":").map(|v| v.trim().trim_end_matches(',').trim_matches('"').to_string()))
        .unwrap_or_else(|| "0.0.0".into())
}

/// RC `VERSIONINFO` bloğu (electron-builder/rcedit'in yazdığı alanlar).
fn version_info(version: &str, description: &str, product: &str, original: &str) -> String {
    let core = version.split(['-', '+']).next().unwrap_or("0.0.0");
    let mut parts: Vec<u16> = core.split('.').map(|p| p.parse().unwrap_or(0)).collect();
    parts.resize(4, 0);
    let numeric = parts.iter().map(u16::to_string).collect::<Vec<_>>().join(",");
    let year = 2026;
    format!(
        r#"
1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "P2 Team"
      VALUE "FileDescription", "{description}"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "{product}"
      VALUE "LegalCopyright", "Copyright \251 {year} P2 Team"
      VALUE "OriginalFilename", "{original}"
      VALUE "ProductName", "{product}"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    )
}
