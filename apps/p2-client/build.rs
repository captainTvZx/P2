//! Windows kaynakları (Faz 8): uygulama simgesi (`assets/icon.ico`, kaynak 1 — GPUI penceresi ve
//! kısayollar bunu kullanır) ve sürüm bilgisi (Electron `P2 Client.exe` ile aynı alanlar:
//! CompanyName "P2 Team", FileDescription/ProductName "P2 Client", LegalCopyright).
//!
//! Ürün sürümü kök `Cargo.toml` `[workspace.package] version`; `P2_PRODUCT_VERSION` ortam değişkeni verilirse o
//! kullanılır (`packaging/build-installers.mjs --version`). Kurulum paketinin sürümüyle exe'nin
//! bildirdiği sürüm aynı olmalı, yoksa güncelleyici aynı sürümü tekrar tekrar kurar.

use std::path::Path;

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let repo = Path::new(&manifest).join("../..");
    let icon = repo.join("assets/icon.ico");
    println!("cargo:rerun-if-changed={}", icon.display());
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=P2_PRODUCT_VERSION");
    let version = product_version();
    println!("cargo:rustc-env=P2_PRODUCT_VERSION={version}");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let rc = out.join("client.rc");
    let icon_line = if icon.exists() { format!("1 ICON \"{}\"\n", icon.canonicalize().unwrap().display().to_string().replace('\\', "/").trim_start_matches("//?/")) } else { String::new() };
    std::fs::write(&rc, format!("{icon_line}{}", version_info(&version, "P2 Client", "P2 Client", "P2 Client.exe"))).unwrap();
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
