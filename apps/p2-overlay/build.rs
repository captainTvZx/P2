//! Windows kaynakları: istemciyle aynı uygulama simgesi (`assets/icon.ico`, kaynak 1) ve sürüm
//! bilgisi. Overlay host'u görünür pencere açmaz; simge Görev Yöneticisi ve Gezgin'de görünür
//! (Electron'da overlay aynı `P2 Client.exe` sürecindeydi, simgesi oydu).

use std::path::Path;

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let repo = Path::new(&manifest).join("../..");
    let icon = repo.join("assets/icon.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=P2_PRODUCT_VERSION");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let rc = out.join("overlay.rc");
    let icon_line = if icon.exists() { format!("1 ICON \"{}\"\n", icon.canonicalize().unwrap().display().to_string().replace('\\', "/").trim_start_matches("//?/")) } else { String::new() };
    std::fs::write(&rc, format!("{icon_line}{}", version_info(&product_version(), "P2 Overlay", "P2 Client", "p2-overlay.exe"))).unwrap();
    embed_resource::compile(&rc, embed_resource::NONE).manifest_optional().unwrap();
}

fn product_version() -> String {
    if let Ok(v) = std::env::var("P2_PRODUCT_VERSION")
        && !v.trim().is_empty()
    {
        return v.trim().to_string();
    }
    // Tek kaynak: kök Cargo.toml [workspace.package] version (Cargo bunu CARGO_PKG_VERSION verir).
    std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into())
}

/// RC `VERSIONINFO` bloğu (p2-client/build.rs ile aynı alanlar).
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
      VALUE "InternalName", "p2-overlay"
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
