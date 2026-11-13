//! Derleme anında `migrations/*/migration.sql` dosyalarını (bu crate'in içinde; eski NestJS/Prisma
//! sunucusunun `server/prisma/migrations` klasörünün bayt bayt aynısı) ve ürün sürümünü (kök
//! `Cargo.toml` `[workspace.package] version`) gömer. Electron sürümü bunları çalışma anında paket
//! kökünden okuyordu; Rust ikilisi tek dosya olarak dağıtılabilsin diye içine alınır.
//!
//! Göç klasör adları üretimdeki `_p2_migrations` kayıtlarıyla eşleşir: ad DEĞİŞTİRİLMEZ, uygulanmış
//! bir göçün içeriği düzenlenmez (yeni değişiklik = yeni klasör).

use std::path::Path;

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let root = Path::new(&manifest).join("migrations");
    println!("cargo:rerun-if-changed={}", root.display());
    println!("cargo:rerun-if-env-changed=P2_PRODUCT_VERSION");

    // JS: readdirSync(...).filter(isDirectory).map(name).sort() — adlar ASCII, bayt sırası aynı.
    let mut names: Vec<String> = std::fs::read_dir(&root)
        .expect("göç klasörü okunamadı")
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    let mut out = String::from("/// (ad, SQL) — `migrations/` alfabetik sırada.\npub static MIGRATIONS: &[(&str, &str)] = &[\n");
    for name in &names {
        let file = root.join(name).join("migration.sql");
        println!("cargo:rerun-if-changed={}", file.display());
        out.push_str(&format!("    ({name:?}, include_str!({:?})),\n", file.display().to_string()));
    }
    out.push_str("];\n");
    let dest = Path::new(&std::env::var("OUT_DIR").unwrap()).join("migrations.rs");
    std::fs::write(dest, out).unwrap();

    // Paketleme (packaging/build-installers.mjs) `P2_PRODUCT_VERSION` ile sürümü değiştirebilir.
    let version = match std::env::var("P2_PRODUCT_VERSION") {
        Ok(v) if !v.trim().is_empty() => v.trim().to_string(),
        _ => std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.0.0".into()),
    };
    println!("cargo:rustc-env=P2_PRODUCT_VERSION={version}");
}
