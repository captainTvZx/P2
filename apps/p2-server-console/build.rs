//! Pencere/görev çubuğu simgesi: assets/icon-server.png → çok boyutlu ICO (PNG girdili) → kaynak 1.
//! GPUI Windows penceresi simgeyi exe'nin 1 numaralı kaynağından yükler.

use std::io::Write as _;
use std::path::Path;

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let png = Path::new(&manifest).join("../../assets/icon-server.png");
    println!("cargo:rerun-if-changed={}", png.display());
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=P2_PRODUCT_VERSION");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let source = image::open(&png).expect("icon-server.png okunamadı");
    let sizes = [256u32, 64, 48, 32, 24, 16];
    let mut images = Vec::new();
    for size in sizes {
        let resized = source.resize_exact(size, size, image::imageops::FilterType::Lanczos3);
        let mut bytes = Vec::new();
        resized.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
        images.push((size, bytes));
    }
    let mut ico = Vec::new();
    ico.write_all(&[0, 0, 1, 0]).unwrap();
    ico.write_all(&(images.len() as u16).to_le_bytes()).unwrap();
    let mut offset = 6 + 16 * images.len() as u32;
    for (size, bytes) in &images {
        let dim = if *size >= 256 { 0u8 } else { *size as u8 };
        ico.write_all(&[dim, dim, 0, 0]).unwrap();
        ico.write_all(&1u16.to_le_bytes()).unwrap();
        ico.write_all(&32u16.to_le_bytes()).unwrap();
        ico.write_all(&(bytes.len() as u32).to_le_bytes()).unwrap();
        ico.write_all(&offset.to_le_bytes()).unwrap();
        offset += bytes.len() as u32;
    }
    for (_, bytes) in &images {
        ico.write_all(bytes).unwrap();
    }
    let ico_path = out.join("icon-server.ico");
    std::fs::write(&ico_path, ico).unwrap();
    let rc = out.join("console.rc");
    // Sürüm bilgisi: Electron `P2 Server.exe` ile aynı alanlar (Faz 8 kurulum paketi exe'yi bu adla koyar).
    let version = product_version();
    std::fs::write(&rc, format!("1 ICON \"{}\"\n{}", ico_path.display().to_string().replace('\\', "/"), version_info(&version, "P2 Server", "P2 Server", "P2 Server.exe"))).unwrap();
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
