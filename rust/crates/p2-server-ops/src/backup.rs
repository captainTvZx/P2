//! `.p2backup` — `server/ops/backup.cjs` karşılığı ve onunla karşılıklı uyumlu.
//!
//! Biçim: gzip'li tar; `manifest.json` (format 1, product "P2 Server", databaseFormat),
//! `database.dump` (pg_dump `--format=custom` ya da pg_dump bulunamazsa `p2-postgres-json-v1`
//! taşınabilir JSON dökümü), isteğe bağlı `uploads/` ve `releases/` klasörleri.
//! Node sürümü sistem `tar` aracını çağırıyordu; burada `tar` + `flate2` kullanılır, arşivler iki yönde
//! de okunur (bsdtar/GNU tar → Rust, Rust → bsdtar/GNU tar).

use std::collections::HashSet;
use std::io::{BufWriter, Write as _};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, anyhow, bail};
use serde_json::{Value, json};
use sqlx::{Connection as _, PgConnection, Row as _};

use crate::config::clean_database_url;
use crate::ops::Outcome;

/// `run(file, args)`: başarısızsa ileti + stderr.
fn run(file: &Path, args: &[&str]) -> anyhow::Result<(String, String)> {
    let output = crate::hide(Command::new(file).args(args)).output().with_context(|| format!("{} çalıştırılamadı", file.display()))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        let mut message = format!("Command failed: {} {}", file.display(), args.join(" "));
        if !stderr.trim().is_empty() {
            message.push('\n');
            message.push_str(stderr.trim());
        }
        bail!(message);
    }
    Ok((stdout, stderr))
}

/// JS `localeCompare(..., { numeric: true })` yaklaşığı: rakam dizileri sayı olarak karşılaştırılır.
fn numeric_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    fn split(s: &str) -> Vec<(bool, String)> {
        let mut parts: Vec<(bool, String)> = Vec::new();
        for ch in s.chars() {
            let digit = ch.is_ascii_digit();
            match parts.last_mut() {
                Some((d, text)) if *d == digit => text.push(ch),
                _ => parts.push((digit, ch.to_string())),
            }
        }
        parts
    }
    let (pa, pb) = (split(a), split(b));
    for (x, y) in pa.iter().zip(pb.iter()) {
        let ord = if x.0 && y.0 {
            let (nx, ny) = (x.1.trim_start_matches('0'), y.1.trim_start_matches('0'));
            nx.len().cmp(&ny.len()).then_with(|| nx.cmp(ny))
        } else {
            x.1.to_lowercase().cmp(&y.1.to_lowercase())
        };
        if ord != std::cmp::Ordering::Equal {
            return ord;
        }
    }
    pa.len().cmp(&pb.len())
}

/// Kayıt defterindeki PostgreSQL kurulumları (`HKLM\SOFTWARE\PostgreSQL\Installations` → Base Directory).
fn windows_install_roots() -> Vec<PathBuf> {
    let mut roots: Vec<String> = Vec::new();
    if !cfg!(windows) {
        return Vec::new();
    }
    let re = regex::Regex::new(r"(?i)^\s*Base Directory\s+REG_SZ\s+(.+?)\s*$").unwrap();
    for view in ["/reg:64", "/reg:32"] {
        let args = ["query", r"HKLM\SOFTWARE\PostgreSQL\Installations", "/s", "/v", "Base Directory", view];
        if let Ok((stdout, _)) = run(Path::new("reg.exe"), &args) {
            for line in stdout.lines() {
                if let Some(found) = re.captures(line) {
                    let value = found[1].to_string();
                    if !roots.contains(&value) {
                        roots.push(value);
                    }
                }
            }
        }
    }
    roots.sort_by(|a, b| numeric_cmp(b, a));
    roots.into_iter().map(PathBuf::from).collect()
}

/// `postgresTool(name)`: P2_PG_BIN → kayıt defteri → Program Files\PostgreSQL\<sürüm>\bin → PATH.
pub fn postgres_tool(name: &str) -> anyhow::Result<PathBuf> {
    let executable = if cfg!(windows) { format!("{name}.exe") } else { name.to_string() };
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(bin) = std::env::var_os("P2_PG_BIN").filter(|v| !v.is_empty()) {
        candidates.push(PathBuf::from(bin).join(&executable));
    }
    if cfg!(windows) {
        for base in windows_install_roots() {
            candidates.push(base.join("bin").join(&executable));
        }
        for var in ["ProgramFiles", "ProgramFiles(x86)"] {
            let Some(root) = std::env::var_os(var).filter(|v| !v.is_empty()) else { continue };
            let base = PathBuf::from(root).join("PostgreSQL");
            if let Ok(read) = std::fs::read_dir(&base) {
                let mut versions: Vec<String> =
                    read.filter_map(|e| e.ok()).filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false)).map(|e| e.file_name().to_string_lossy().into_owned()).collect();
                versions.sort_by(|a, b| numeric_cmp(b, a));
                for version in versions {
                    candidates.push(base.join(version).join("bin").join(&executable));
                }
            }
        }
    }
    candidates.push(PathBuf::from(&executable));
    for candidate in candidates {
        if run(&candidate, &["--version"]).is_ok() {
            return Ok(candidate);
        }
    }
    bail!("{name} bulunamadı. PostgreSQL istemci araçlarını kur veya P2_PG_BIN ile bin klasörünü belirt.")
}

/// `stamp()`: `2026-11-13_10-05-00-123`.
pub fn stamp() -> String {
    crate::iso_now().replace([':', '.'], "-").replace('T', "_").replace('Z', "")
}

fn identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

/// `fs.mkdtemp(path.join(os.tmpdir(), prefix))`.
fn temp_dir(prefix: &str) -> anyhow::Result<PathBuf> {
    for _ in 0..16 {
        let mut bytes = [0u8; 4];
        getrandom::fill(&mut bytes).map_err(|e| anyhow!("{e}"))?;
        let dir = std::env::temp_dir().join(format!("{prefix}{}", hex::encode(bytes)));
        if std::fs::create_dir(&dir).is_ok() {
            return Ok(dir);
        }
    }
    bail!("geçici klasör oluşturulamadı")
}

// ------------------------------------------------------------------ taşınabilir JSON dökümü

/// Sütunun döküm ifadesi: node-pg'nin satırı JS'e çevirip `JSON.stringify` ile yazdığı biçim.
/// timestamp → ISO (`...Z`, ms), int8/numeric → metin, bytea → `{ $p2: 'buffer', value: base64 }`,
/// enum dizileri → ham dizi metni (node-pg bunları ayrıştırmaz), geri kalanı row_to_json'daki hali.
fn column_expr(name: &str, typname: &str, category: &str, element_kind: Option<&str>) -> String {
    let col = identifier(name);
    const ISO: &str = r#"'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'"#;
    match typname {
        "timestamp" => format!("to_char({col}, {ISO})"),
        "timestamptz" => format!("to_char({col} AT TIME ZONE 'UTC', {ISO})"),
        "date" => format!(r#"to_char({col}, 'YYYY-MM-DD"T00:00:00.000Z"')"#),
        "int8" | "numeric" | "money" => format!("{col}::text"),
        "bytea" => format!("CASE WHEN {col} IS NULL THEN NULL ELSE json_build_object('$p2', 'buffer', 'value', translate(encode({col}, 'base64'), E'\\n', '')) END"),
        _ if category == "A" && element_kind == Some("e") => format!("{col}::text"),
        _ => col,
    }
}

/// `createPortableDump`: `{"format":"p2-postgres-json-v1","tables":[{"name","rows":[...]}]}`.
pub async fn create_portable_dump(database_url: &str, destination: &Path) -> anyhow::Result<()> {
    let mut db = PgConnection::connect(&clean_database_url(database_url)?).await?;
    let result = async {
        let names: Vec<String> = sqlx::query_scalar("SELECT tablename::text FROM pg_tables WHERE schemaname = 'public' ORDER BY tablename").fetch_all(&mut db).await?;
        let file = std::fs::File::create(destination)?;
        let mut out = BufWriter::new(file);
        out.write_all(br#"{"format":"p2-postgres-json-v1","tables":["#)?;
        for (index, name) in names.iter().enumerate() {
            if index > 0 {
                out.write_all(b",")?;
            }
            let columns = sqlx::query(
                "SELECT a.attname::text, t.typname::text, t.typcategory::text, et.typtype::text \
                 FROM pg_attribute a JOIN pg_type t ON t.oid = a.atttypid LEFT JOIN pg_type et ON et.oid = t.typelem AND t.typelem <> 0 \
                 WHERE a.attrelid = ('public.' || quote_ident($1))::regclass AND a.attnum > 0 AND NOT a.attisdropped ORDER BY a.attnum",
            )
            .bind(name)
            .fetch_all(&mut db)
            .await?;
            let exprs: Vec<String> = columns
                .iter()
                .map(|row| {
                    let (column, typname, category, element): (String, String, String, Option<String>) = (row.get(0), row.get(1), row.get(2), row.get(3));
                    format!("{} AS {}", column_expr(&column, &typname, &category, element.as_deref()), identifier(&column))
                })
                .collect();
            write!(out, r#"{{"name":{},"rows":["#, serde_json::to_string(name)?)?;
            if !exprs.is_empty() {
                let query = format!("SELECT row_to_json(x)::text FROM (SELECT {} FROM {}) x", exprs.join(", "), identifier(name));
                let rows: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(query)).fetch_all(&mut db).await?;
                for (i, row) in rows.iter().enumerate() {
                    if i > 0 {
                        out.write_all(b",")?;
                    }
                    // row_to_json boşluk bırakmaz; yine de JSON.stringify ile aynı sıkı biçime çevrilir.
                    let value: Value = serde_json::from_str(row)?;
                    serde_json::to_writer(&mut out, &value)?;
                }
            } else {
                // Sütunsuz tablo: JS'te her satır `{}`.
                let count: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {}", identifier(name)))).fetch_one(&mut db).await?;
                for i in 0..count {
                    out.write_all(if i > 0 { b",{}" } else { b"{}" })?;
                }
            }
            out.write_all(b"]}")?;
        }
        out.write_all(b"]}")?;
        out.flush()?;
        anyhow::Ok(())
    }
    .await;
    let _ = db.close().await;
    result
}

/// `restoredValue`: `$p2` işaretlerini çözer. bytea için PostgreSQL'in `\x..` metin girdisi kullanılır
/// (değerler `json_populate_record` ile sütun tipine çevrilir).
fn restored_value(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(restored_value).collect()),
        Value::Object(map) => {
            match map.get("$p2").and_then(Value::as_str) {
                Some("bigint") => return Value::String(js_string(map.get("value").unwrap_or(&Value::Null))),
                Some("buffer") => {
                    let text = js_string(map.get("value").unwrap_or(&Value::Null));
                    let bytes = base64_decode(&text);
                    return Value::String(format!("\\x{}", hex::encode(bytes)));
                }
                _ => {}
            }
            Value::Object(map.iter().map(|(k, v)| (k.clone(), restored_value(v))).collect())
        }
        other => other.clone(),
    }
}

fn js_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Null => "null".into(),
        other => other.to_string(),
    }
}

/// Node `Buffer.from(s, 'base64')`: geçersiz karakterleri atlayan hoşgörülü çözücü.
fn base64_decode(text: &str) -> Vec<u8> {
    fn val(c: u8) -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return None,
        } as u32)
    }
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut acc, mut bits) = (0u32, 0u32);
    for c in text.bytes() {
        if c == b'=' {
            break;
        }
        let Some(v) = val(c) else { continue };
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    out
}

/// `restorePortableDump`: tek işlemde TRUNCATE + yabancı anahtar sırasına göre INSERT.
pub async fn restore_portable_dump(database_url: &str, source: &Path) -> anyhow::Result<()> {
    let text = std::fs::read_to_string(source)?;
    let dump: Value = serde_json::from_str(&text)?;
    let Some(all_tables) = dump.get("tables").and_then(Value::as_array).filter(|_| dump.get("format").and_then(Value::as_str) == Some("p2-postgres-json-v1")) else {
        bail!("Taşınabilir veritabanı yedeği geçersiz.");
    };
    let mut db = PgConnection::connect(&clean_database_url(database_url)?).await?;
    let result = async {
        let mut tx = db.begin().await?;
        let existing: HashSet<String> = sqlx::query_scalar::<_, String>("SELECT tablename::text FROM pg_tables WHERE schemaname = 'public'").fetch_all(&mut *tx).await?.into_iter().collect();
        let tables: Vec<(&str, &Vec<Value>)> = all_tables
            .iter()
            .filter_map(|t| Some((t.get("name")?.as_str()?, t.get("rows")?.as_array()?)))
            .filter(|(name, _)| existing.contains(*name))
            .collect();
        if !tables.is_empty() {
            let list: Vec<String> = tables.iter().map(|(name, _)| identifier(name)).collect();
            sqlx::raw_sql(sqlx::AssertSqlSafe(format!("TRUNCATE {} RESTART IDENTITY CASCADE", list.join(", ")))).execute(&mut *tx).await?;
        }
        let relations: Vec<(String, String)> = sqlx::query_as(
            "SELECT child.relname::text AS child, parent.relname::text AS parent FROM pg_constraint fk JOIN pg_class child ON child.oid = fk.conrelid \
             JOIN pg_class parent ON parent.oid = fk.confrelid JOIN pg_namespace ns ON ns.oid = child.relnamespace WHERE fk.contype = 'f' AND ns.nspname = 'public'",
        )
        .fetch_all(&mut *tx)
        .await?;
        // Node'daki sıralama: bekleyenler arasında ebeveyni kalmamış tablolar (kendine başvuru hariç) önce.
        let mut pending: Vec<(&str, &Vec<Value>)> = Vec::new();
        for table in &tables {
            if let Some(slot) = pending.iter_mut().find(|(n, _)| n == &table.0) {
                *slot = *table; // Map.set: aynı ad sonradan gelirse değer güncellenir, yeri korunur
            } else {
                pending.push(*table);
            }
        }
        let mut ordered: Vec<(&str, &Vec<Value>)> = Vec::new();
        while !pending.is_empty() {
            let names: HashSet<&str> = pending.iter().map(|(n, _)| *n).collect();
            let ready: Vec<(&str, &Vec<Value>)> = pending
                .iter()
                .copied()
                .filter(|(name, _)| !relations.iter().any(|(child, parent)| child == name && names.contains(parent.as_str()) && parent != name))
                .collect();
            if ready.is_empty() {
                ordered.append(&mut pending);
                break;
            }
            for table in ready {
                ordered.push(table);
                pending.retain(|(n, _)| *n != table.0);
            }
        }
        for (name, rows) in ordered {
            // Aynı sütun kümesine sahip ardışık satırlar tek INSERT ile (json_populate_recordset) yazılır.
            let mut batch: Vec<Value> = Vec::new();
            let mut batch_columns: Vec<String> = Vec::new();
            for raw in rows {
                let row = restored_value(raw);
                let Some(object) = row.as_object() else { continue };
                let columns: Vec<String> = object.keys().cloned().collect();
                if columns.is_empty() {
                    continue;
                }
                if columns != batch_columns || batch.len() >= 500 {
                    insert_batch(&mut tx, name, &batch_columns, &batch).await?;
                    batch.clear();
                    batch_columns = columns;
                }
                batch.push(row);
            }
            insert_batch(&mut tx, name, &batch_columns, &batch).await?;
        }
        tx.commit().await?;
        anyhow::Ok(())
    }
    .await;
    let _ = db.close().await;
    result
}

async fn insert_batch(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, table: &str, columns: &[String], rows: &[Value]) -> anyhow::Result<()> {
    if rows.is_empty() || columns.is_empty() {
        return Ok(());
    }
    let list: Vec<String> = columns.iter().map(|c| identifier(c)).collect();
    let list = list.join(", ");
    let query = format!("INSERT INTO {t} ({list}) SELECT {list} FROM json_populate_recordset(NULL::{t}, $1::json)", t = identifier(table));
    let payload = Value::Array(rows.to_vec()).to_string();
    sqlx::query(sqlx::AssertSqlSafe(query)).bind(payload).execute(&mut **tx).await?;
    Ok(())
}

// ------------------------------------------------------------------ yedek alma

pub struct CreateOptions<'a> {
    pub database_url: &'a str,
    pub data_dir: &'a Path,
    pub destination: &'a Path,
    pub version: &'a str,
    pub include_releases: bool,
}

#[derive(Clone, Debug)]
pub struct CreateResult {
    pub path: PathBuf,
    pub size: u64,
    pub manifest: Value,
}

fn absolute(path: &Path) -> PathBuf {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

/// `createBackup`. Bloklayan; tokio çalışma zamanı dışında çağrılmalı.
pub fn create_backup(options: CreateOptions<'_>) -> anyhow::Result<CreateResult> {
    if options.database_url.is_empty() {
        bail!("PostgreSQL yapılandırması bulunamadı.");
    }
    let target = absolute(options.destination);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let work = temp_dir("p2-backup-")?;
    let result = (|| {
        let dump = work.join("database.dump");
        let mut database_format = "postgres-custom";
        match postgres_tool("pg_dump") {
            Ok(pg_dump) => {
                let url = clean_database_url(options.database_url)?;
                let dump_s = dump.display().to_string();
                run(&pg_dump, &["--dbname", &url, "--format=custom", "--no-owner", "--no-acl", "--file", &dump_s])?;
            }
            Err(_) => {
                database_format = "p2-postgres-json-v1";
                crate::block_on(create_portable_dump(options.database_url, &dump))?;
            }
        }
        let uploads = options.data_dir.join("uploads");
        let releases = options.data_dir.join("releases");
        let manifest = json!({
            "format": 1,
            "product": "P2 Server",
            "version": options.version,
            "createdAt": crate::iso_now(),
            "databaseFormat": database_format,
            "includes": { "database": true, "uploads": uploads.exists(), "releases": options.include_releases && releases.exists() },
        });
        std::fs::write(work.join("manifest.json"), serde_json::to_string_pretty(&manifest)?)?;
        write_archive(&target, &work, options.data_dir, &manifest)?;
        let size = std::fs::metadata(&target)?.len();
        Ok(CreateResult { path: target.clone(), size, manifest })
    })();
    let _ = std::fs::remove_dir_all(&work);
    result
}

/// `tar -czf target -C work manifest.json database.dump [-C dataDir uploads] [-C dataDir releases]`.
fn write_archive(target: &Path, work: &Path, data_dir: &Path, manifest: &Value) -> anyhow::Result<()> {
    let file = std::fs::File::create(target)?;
    let gz = flate2::write::GzEncoder::new(BufWriter::new(file), flate2::Compression::default());
    let mut tar = tar::Builder::new(gz);
    tar.follow_symlinks(true);
    tar.append_path_with_name(work.join("manifest.json"), "manifest.json")?;
    tar.append_path_with_name(work.join("database.dump"), "database.dump")?;
    for folder in ["uploads", "releases"] {
        if manifest["includes"][folder].as_bool() == Some(true) {
            append_tree(&mut tar, folder, &data_dir.join(folder))?;
        }
    }
    let gz = tar.into_inner()?;
    let mut inner = gz.finish()?;
    inner.flush()?;
    inner.into_inner().map_err(|e| anyhow!("{}", e.error()))?.sync_all()?;
    Ok(())
}

/// PAX `path` kaydı: ASCII dışı adlar bsdtar/GNU tar'da UTF-8 olarak okunsun (bsdtar başlıktaki
/// ham adı Windows'ta kod sayfasıyla çözer). bsdtar'ın kendi arşivleri de böyle yazar.
fn pax_path<W: std::io::Write>(tar: &mut tar::Builder<W>, name: &str) -> std::io::Result<()> {
    if name.is_ascii() {
        return Ok(());
    }
    let body = format!("path={name}\n");
    let mut len = body.len() + 2;
    loop {
        let total = len.to_string().len() + 1 + body.len();
        if total == len {
            break;
        }
        len = total;
    }
    let record = format!("{len} {body}");
    let mut header = tar::Header::new_ustar();
    header.set_path("PaxHeader/p2")?;
    header.set_size(record.len() as u64);
    header.set_entry_type(tar::EntryType::XHeader);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append(&header, record.as_bytes())
}

/// `tar -C dataDir <klasör>` gibi: klasör girdisi ve içeriği (alfabetik), bağlantılar izlenir.
fn append_tree<W: std::io::Write>(tar: &mut tar::Builder<W>, name: &str, dir: &Path) -> std::io::Result<()> {
    pax_path(tar, &format!("{name}/"))?;
    tar.append_dir(name, dir)?;
    let mut children: Vec<_> = std::fs::read_dir(dir)?.filter_map(|e| e.ok()).collect();
    children.sort_by_key(|e| e.file_name());
    for child in children {
        let child_name = format!("{name}/{}", child.file_name().to_string_lossy());
        let path = child.path();
        if std::fs::metadata(&path)?.is_dir() {
            append_tree(tar, &child_name, &path)?;
        } else {
            pax_path(tar, &child_name)?;
            tar.append_path_with_name(&path, &child_name)?;
        }
    }
    Ok(())
}

// ------------------------------------------------------------------ doğrulama ve yükleme

fn open_archive(file: &Path) -> anyhow::Result<tar::Archive<flate2::read::GzDecoder<std::io::BufReader<std::fs::File>>>> {
    let handle = std::fs::File::open(file)?;
    Ok(tar::Archive::new(flate2::read::GzDecoder::new(std::io::BufReader::new(handle))))
}

/// `validateArchive`: boyut, girdi sayısı, güvenli yollar, izinli içerik, bağlantı yasağı, zorunlu dosyalar.
pub fn validate_archive(file: &Path) -> anyhow::Result<Vec<String>> {
    let info = std::fs::metadata(file)?;
    if !info.is_file() || info.len() < 64 || info.len() > 250 * 1024 * 1024 * 1024 {
        bail!("Yedek dosyası geçersiz veya desteklenen boyut sınırını aşıyor.");
    }
    let mut entries: Vec<String> = Vec::new();
    let mut has_link = false;
    let mut archive = open_archive(file)?;
    for entry in archive.entries()? {
        let entry = entry?;
        let kind = entry.header().entry_type();
        if kind.is_pax_global_extensions() || kind.is_pax_local_extensions() || kind.is_gnu_longname() || kind.is_gnu_longlink() {
            continue;
        }
        if kind.is_symlink() || kind.is_hard_link() {
            has_link = true;
        }
        let name = String::from_utf8_lossy(&entry.path_bytes()).trim().to_string();
        if !name.is_empty() {
            entries.push(name);
        }
        if entries.len() > 2_000_000 {
            break;
        }
    }
    if entries.is_empty() || entries.len() > 2_000_000 {
        bail!("Yedek içeriği geçersiz veya çok fazla dosya içeriyor.");
    }
    let drive = regex::Regex::new(r"^[A-Za-z]:").unwrap();
    for entry in &entries {
        let normalized = entry.replace('\\', "/");
        if normalized.starts_with('/') || drive.is_match(&normalized) || normalized.split('/').any(|part| part == "..") {
            bail!("Yedek güvenli olmayan bir dosya yolu içeriyor.");
        }
        let allowed = normalized == "manifest.json"
            || normalized == "database.dump"
            || normalized == "uploads"
            || normalized.starts_with("uploads/")
            || normalized == "releases"
            || normalized.starts_with("releases/");
        if !allowed {
            bail!("Yedekte beklenmeyen içerik var: {normalized}");
        }
    }
    if has_link {
        bail!("Sembolik bağlantı içeren yedekler güvenlik nedeniyle yüklenemez.");
    }
    if !entries.iter().any(|e| e == "manifest.json") || !entries.iter().any(|e| e == "database.dump") {
        bail!("Yedekte manifest veya veritabanı dökümü eksik.");
    }
    Ok(entries)
}

fn extract(file: &Path, work: &Path) -> anyhow::Result<()> {
    let mut archive = open_archive(file)?;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let kind = entry.header().entry_type();
        if kind.is_pax_global_extensions() {
            continue;
        }
        entry.unpack_in(work)?;
    }
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

pub struct RestoreOptions<'a> {
    pub database_url: &'a str,
    pub data_dir: &'a Path,
    pub source: &'a Path,
    /// Sunucuyu durdurur (`ok: false` → yükleme iptal, ileti hata olur).
    pub stop: Option<&'a dyn Fn() -> Outcome>,
    /// Durdurulduysa sonunda (başarılı ya da değil) yeniden başlatır.
    pub start: Option<&'a dyn Fn() -> Outcome>,
}

/// `restoreBackup`. Bloklayan; tokio çalışma zamanı dışında çağrılmalı. Sonuç: manifest.
pub fn restore_backup(options: RestoreOptions<'_>) -> anyhow::Result<Value> {
    if options.database_url.is_empty() {
        bail!("Önce hedef PostgreSQL bağlantısını yapılandır.");
    }
    let file = absolute(options.source);
    validate_archive(&file)?;
    let work = temp_dir("p2-restore-")?;
    let mut stopped = false;
    let result = (|| {
        extract(&file, &work)?;
        let manifest: Value = serde_json::from_str(&std::fs::read_to_string(work.join("manifest.json"))?)?;
        if manifest.get("format").and_then(Value::as_f64) != Some(1.) || manifest.get("product").and_then(Value::as_str) != Some("P2 Server") {
            bail!("Bu dosya desteklenen bir P2 Server yedeği değil.");
        }
        if let Some(stop) = options.stop {
            let result = stop();
            if !result.ok {
                bail!(result.message.unwrap_or_else(|| "Sunucu durdurulamadı.".into()));
            }
            stopped = true;
        }
        let dump = work.join("database.dump");
        if manifest.get("databaseFormat").and_then(Value::as_str) == Some("p2-postgres-json-v1") {
            crate::block_on(restore_portable_dump(options.database_url, &dump))?;
        } else {
            let pg_restore = postgres_tool("pg_restore")?;
            let url = clean_database_url(options.database_url)?;
            let dump_s = dump.display().to_string();
            run(&pg_restore, &["--dbname", &url, "--clean", "--if-exists", "--no-owner", "--no-acl", "--exit-on-error", &dump_s])?;
        }
        // uploads ve releases aynı biçimde geri yüklenir: eskisi tarih damgasıyla kenara alınır.
        for folder in ["uploads", "releases"] {
            let restored = work.join(folder);
            if !restored.exists() {
                continue;
            }
            std::fs::create_dir_all(options.data_dir)?;
            let incoming = options.data_dir.join(format!(".{folder}-restored-{}", stamp()));
            if std::fs::rename(&restored, &incoming).is_err() {
                copy_dir(&restored, &incoming)?;
            }
            let current = options.data_dir.join(folder);
            if current.exists() {
                std::fs::rename(&current, options.data_dir.join(format!("{folder}-before-restore-{}", stamp())))?;
            }
            std::fs::rename(&incoming, &current)?;
        }
        Ok(manifest)
    })();
    let _ = std::fs::remove_dir_all(&work);
    if stopped {
        if let Some(start) = options.start {
            let _ = start();
        }
    }
    result
}

/// `backupName()` (konsolun kaydet penceresindeki öneri): `P2-Server-Backup-2026-11-13-1005.p2backup`.
pub fn console_backup_name() -> String {
    let iso = crate::iso_now();
    let date = iso[..16].replacen('T', "-", 1).replacen(':', "", 1);
    format!("P2-Server-Backup-{date}.p2backup")
}

/// backup-cli.cjs dosya adı: `P2-Server-Backup-2026-11-13_10-05-00-123.p2backup`.
pub fn cli_backup_name() -> String {
    format!("P2-Server-Backup-{}.p2backup", stamp())
}

/// `relations` sıralamasının saf hali (testler için dışa açık değil).
#[allow(dead_code)]
fn order_names(names: &[&str], relations: &[(&str, &str)]) -> Vec<String> {
    let mut pending: Vec<&str> = names.to_vec();
    let mut ordered = Vec::new();
    while !pending.is_empty() {
        let set: HashSet<&str> = pending.iter().copied().collect();
        let ready: Vec<&str> = pending.iter().copied().filter(|n| !relations.iter().any(|(c, p)| c == n && set.contains(p) && p != n)).collect();
        if ready.is_empty() {
            ordered.extend(pending.drain(..).map(String::from));
            break;
        }
        for r in ready {
            ordered.push(r.to_string());
            pending.retain(|n| *n != r);
        }
    }
    ordered
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert!(regex::Regex::new(r"^P2-Server-Backup-\d{4}-\d\d-\d\d-\d{4}\.p2backup$").unwrap().is_match(&console_backup_name()));
        assert!(regex::Regex::new(r"^P2-Server-Backup-\d{4}-\d\d-\d\d_\d\d-\d\d-\d\d-\d{3}\.p2backup$").unwrap().is_match(&cli_backup_name()));
    }

    #[test]
    fn numeric_sort() {
        let mut v = vec!["9", "17", "10.2", "10.10"];
        v.sort_by(|a, b| numeric_cmp(b, a));
        assert_eq!(v, ["17", "10.10", "10.2", "9"]);
    }

    #[test]
    fn values() {
        assert_eq!(restored_value(&json!({"$p2": "bigint", "value": "12"})), json!("12"));
        assert_eq!(restored_value(&json!({"$p2": "buffer", "value": "AAEC/w=="})), json!("\\x000102ff"));
        assert_eq!(restored_value(&json!([{"a": {"$p2": "bigint", "value": 1}}])), json!([{"a": "1"}]));
    }

    #[test]
    fn ordering_like_node() {
        let order = order_names(&["a", "b", "c", "d"], &[("a", "b"), ("b", "c"), ("d", "d")]);
        assert_eq!(order, ["c", "d", "b", "a"]);
    }
}
