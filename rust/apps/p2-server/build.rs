//! Windows sürüm bilgisi (Faz 8): `p2-server.exe` hizmet ikilisi, Görev Yöneticisi ve dosya
//! özelliklerinde "P2 Server (API)" görünür. Sürüm `package.json` ya da `P2_PRODUCT_VERSION`.

use std::path::Path;

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let repo = Path::new(&manifest).join("../../..");
    println!("cargo:rerun-if-changed={}", repo.join("package.json").display());
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", repo.join("build/icon-server.png").display());
    println!("cargo:rerun-if-env-changed=P2_PRODUCT_VERSION");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    // Görev Yöneticisi / Gezgin simgesi: konsolla aynı build/icon-server.png → çok boyutlu ICO (kaynak 1).
    let icon_line = server_icon(&repo.join("build/icon-server.png"), &out).map(|ico| format!("1 ICON \"{}\"\n", ico.display().to_string().replace('\\', "/"))).unwrap_or_default();
    let rc = out.join("server.rc");
    std::fs::write(&rc, icon_line + &version_info(&product_version(&repo), "P2 Server (API)", "P2 Server", "p2-server.exe")).unwrap();
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

/// PNG → PNG girdili çok boyutlu ICO (p2-server-console/build.rs ile aynı yöntem).
fn server_icon(png: &Path, out: &Path) -> Option<std::path::PathBuf> {
    use std::io::Write as _;
    let source = image::open(png).ok()?;
    let sizes = [256u32, 64, 48, 32, 24, 16];
    let mut images = Vec::new();
    for size in sizes {
        let resized = source.resize_exact(size, size, image::imageops::FilterType::Lanczos3);
        let mut bytes = Vec::new();
        resized.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).ok()?;
        images.push((size, bytes));
    }
    let mut ico = Vec::new();
    ico.write_all(&[0, 0, 1, 0]).ok()?;
    ico.write_all(&(images.len() as u16).to_le_bytes()).ok()?;
    let mut offset = 6 + 16 * images.len() as u32;
    for (size, bytes) in &images {
        let dim = if *size >= 256 { 0u8 } else { *size as u8 };
        ico.write_all(&[dim, dim, 0, 0]).ok()?;
        ico.write_all(&1u16.to_le_bytes()).ok()?;
        ico.write_all(&32u16.to_le_bytes()).ok()?;
        ico.write_all(&(bytes.len() as u32).to_le_bytes()).ok()?;
        ico.write_all(&offset.to_le_bytes()).ok()?;
        offset += bytes.len() as u32;
    }
    for (_, bytes) in &images {
        ico.write_all(bytes).ok()?;
    }
    let path = out.join("icon-server.ico");
    std::fs::write(&path, ico).ok()?;
    Some(path)
}
