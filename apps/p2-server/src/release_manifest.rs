//! Kütüphane sürüm manifesti — `server/src/releaseManifest.ts` karşılığı: saf doğrulama,
//! normalleştirme, iki paket arasındaki fark ve "aynı manifest mi" kararı.
//! Aynı kurallar istemcide (electron/patcherManifest.cjs, p2-patcher) de uygulanır.

use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};

pub const CHUNK_SIZE: i64 = 4 * 1024 * 1024;
pub const MANIFEST_VERSION: i64 = 1;
pub const DIFF_LIST_LIMIT: usize = 300;
const MAX_SAFE: f64 = 9_007_199_254_740_991.0;

#[derive(Clone, Debug)]
pub struct ManifestFile {
    pub path: String,
    pub size: i64,
    pub chunks: Vec<String>,
    /// İçerik tanımlı (değişken boylu) parçaların uzunlukları; yoksa sabit 4 MB düzen.
    pub lengths: Option<Vec<i64>>,
}

#[derive(Clone, Debug)]
pub struct Manifest {
    pub platform: String,
    pub executable: String,
    pub launch_args: Option<String>,
    pub files: Vec<ManifestFile>,
    pub total_size: i64,
    pub file_count: i64,
    pub chunk_count: i64,
}

pub struct Limits {
    pub files: usize,
    pub chunks: usize,
    pub bytes: i64,
}

pub const DEFAULT_LIMITS: Limits = Limits { files: 200_000, chunks: 400_000, bytes: 200 * 1024 * 1024 * 1024 };

pub fn chunk_length(file: &ManifestFile, index: usize) -> i64 {
    if let Some(lengths) = &file.lengths {
        return lengths.get(index).copied().unwrap_or(0);
    }
    (file.size - index as i64 * CHUNK_SIZE).clamp(0, CHUNK_SIZE)
}

pub fn is_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn is_hash_value(value: &Value) -> bool {
    value.as_str().is_some_and(is_hash)
}

fn safe_int(value: &Value) -> Option<i64> {
    value.as_f64().filter(|f| f.is_finite() && f.fract() == 0.0 && f.abs() <= MAX_SAFE).map(|f| f as i64)
}

/// Göreli, güvenli bir yol: mutlak/sürücü/UNC yok, ".." yok, Windows'un yasak karakterleri
/// ve ayrılmış cihaz adları yok, parça sonunda nokta/boşluk yok.
pub fn safe_relative_path(input: Option<&Value>) -> Option<String> {
    static RESERVED: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let reserved = RESERVED.get_or_init(|| regex::Regex::new(r"(?i)^(con|prn|aux|nul|com[1-9]|lpt[1-9])(\..*)?$").unwrap());
    let input = input?.as_str()?;
    let units = input.encode_utf16().count();
    if units == 0 || units > 1024 {
        return None;
    }
    if input.chars().any(|c| (c as u32) <= 0x1f || matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*')) {
        return None;
    }
    let value = input.replace('\\', "/");
    let bytes = value.as_bytes();
    if value.starts_with('/') || (bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':') {
        return None;
    }
    let parts: Vec<&str> = value.split('/').collect();
    if parts.len() > 64 {
        return None;
    }
    for part in &parts {
        if part.is_empty() || *part == "." || *part == ".." {
            return None;
        }
        if part.ends_with('.') || part.ends_with(' ') {
            return None;
        }
        if reserved.is_match(part) {
            return None;
        }
        if part.encode_utf16().count() > 255 {
            return None;
        }
    }
    Some(parts.join("/"))
}

pub fn chunk_count_for(size: i64) -> i64 {
    (size + CHUNK_SIZE - 1) / CHUNK_SIZE
}

/// JS `String(x)`; alan hiç yoksa "undefined".
fn js_string_opt(value: Option<&Value>) -> String {
    match value {
        None => "undefined".into(),
        Some(v) => crate::tasks::js_string(v),
    }
}

pub struct Validated {
    pub manifest: Manifest,
    pub unique_hashes: Vec<String>,
}

/// İstemcinin gönderdiği dosya listesini doğrular; toplamlar sunucuda yeniden hesaplanır.
/// Hata: (kod, mesaj).
pub fn validate_manifest_input(executable: Option<&Value>, launch_args: Option<&Value>, platform: Option<&Value>, files: Option<&Value>, limits: &Limits) -> Result<Validated, (&'static str, String)> {
    let files = match files {
        Some(Value::Array(items)) if !items.is_empty() => items,
        _ => return Err(("MANIFEST_EMPTY", "Sürümde en az bir dosya olmalı.".into())),
    };
    if files.len() > limits.files {
        return Err(("MANIFEST_TOO_MANY_FILES", format!("Dosya sayısı {} sınırını aşıyor.", limits.files)));
    }
    let executable = match safe_relative_path(executable) {
        Some(e) if e.to_ascii_lowercase().ends_with(".exe") => e,
        _ => return Err(("MANIFEST_EXECUTABLE", "Başlatılacak dosya .exe uzantılı geçerli bir yol olmalı.".into())),
    };
    let launch_args = launch_args.and_then(Value::as_str).map(|a| crate::validate::js_slice(crate::tasks::js_trim(a), 500)).unwrap_or_default();
    let platform = platform
        .and_then(Value::as_str)
        .filter(|p| (3..=20).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'))
        .unwrap_or("win-x64")
        .to_string();
    let mut seen: HashSet<String> = HashSet::new();
    let mut unique: Vec<String> = Vec::new();
    let mut unique_set: HashSet<String> = HashSet::new();
    let mut out: Vec<ManifestFile> = Vec::new();
    let mut total_size: i64 = 0;
    let mut has_executable = false;
    let empty = serde_json::Map::new();
    for row in files {
        let item = match row {
            Value::Object(map) => map,
            // Dizi de JS'te nesnedir: alanları "undefined" olur.
            Value::Array(_) => &empty,
            _ => return Err(("MANIFEST_FILE", "Dosya kaydı geçersiz.".into())),
        };
        let Some(path) = safe_relative_path(item.get("path")) else {
            let shown = crate::validate::js_slice(&js_string_opt(item.get("path")), 120);
            return Err(("MANIFEST_PATH", format!("Geçersiz dosya yolu: {shown}")));
        };
        // JS toLowerCase: Unicode küçük harf.
        let key = path.to_lowercase();
        if !seen.insert(key) {
            return Err(("MANIFEST_DUPLICATE", format!("Aynı yol iki kez geçiyor: {path}")));
        }
        let Some(size) = item.get("size").and_then(safe_int).filter(|s| *s >= 0) else {
            return Err(("MANIFEST_SIZE", format!("Dosya boyutu geçersiz: {path}")));
        };
        let Some(Value::Array(chunks)) = item.get("chunks") else {
            return Err(("MANIFEST_CHUNKS", format!("Parça listesi geçersiz: {path}")));
        };
        let mut lengths: Option<Vec<i64>> = None;
        if let Some(raw) = item.get("lengths") {
            // İçerik tanımlı düzen: her parça 1..CHUNK_SIZE bayt, toplam dosya boyutuna eşit.
            let Value::Array(raw) = raw else {
                return Err(("MANIFEST_CHUNKS", format!("Parça uzunlukları parça sayısıyla uyuşmuyor: {path}")));
            };
            if raw.len() != chunks.len() {
                return Err(("MANIFEST_CHUNKS", format!("Parça uzunlukları parça sayısıyla uyuşmuyor: {path}")));
            }
            let mut sum: i64 = 0;
            let mut list = Vec::with_capacity(raw.len());
            for length in raw {
                let Some(length) = safe_int(length).filter(|l| (1..=CHUNK_SIZE).contains(l)) else {
                    return Err(("MANIFEST_CHUNKS", format!("Parça uzunluğu geçersiz: {path}")));
                };
                sum += length;
                list.push(length);
            }
            if sum != size {
                return Err(("MANIFEST_CHUNKS", format!("Parça uzunlukları boyutla uyuşmuyor: {path}")));
            }
            lengths = Some(list);
        } else if chunks.len() as i64 != chunk_count_for(size) {
            return Err(("MANIFEST_CHUNKS", format!("Parça sayısı boyutla uyuşmuyor: {path}")));
        }
        let mut hashes = Vec::with_capacity(chunks.len());
        for hash in chunks {
            if !is_hash_value(hash) {
                return Err(("MANIFEST_HASH", format!("Parça özeti geçersiz: {path}")));
            }
            let hash = hash.as_str().unwrap().to_string();
            if unique_set.insert(hash.clone()) {
                unique.push(hash.clone());
            }
            if unique_set.len() > limits.chunks {
                return Err(("MANIFEST_TOO_MANY_CHUNKS", format!("Parça sayısı {} sınırını aşıyor.", limits.chunks)));
            }
            hashes.push(hash);
        }
        total_size += size;
        if total_size > limits.bytes {
            return Err(("MANIFEST_TOO_LARGE", "Sürüm boyutu sınırı aşıyor.".into()));
        }
        if path == executable {
            has_executable = true;
        }
        out.push(ManifestFile { path, size, chunks: hashes, lengths });
    }
    if !has_executable {
        return Err(("MANIFEST_EXECUTABLE_MISSING", "Başlatılacak dosya listede yok.".into()));
    }
    let file_count = out.len() as i64;
    let chunk_count = unique.len() as i64;
    Ok(Validated {
        manifest: Manifest { platform, executable, launch_args: Some(launch_args).filter(|a| !a.is_empty()), files: out, total_size, file_count, chunk_count },
        unique_hashes: unique,
    })
}

impl Manifest {
    pub fn to_json(&self) -> Value {
        let mut map = serde_json::Map::new();
        map.insert("version".into(), json!(MANIFEST_VERSION));
        map.insert("chunkSize".into(), json!(CHUNK_SIZE));
        map.insert("platform".into(), json!(self.platform));
        map.insert("executable".into(), json!(self.executable));
        if let Some(args) = &self.launch_args {
            map.insert("launchArgs".into(), json!(args));
        }
        let files: Vec<Value> = self
            .files
            .iter()
            .map(|f| {
                let mut file = json!({ "path": f.path, "size": f.size, "chunks": f.chunks });
                if let Some(lengths) = &f.lengths {
                    file.as_object_mut().unwrap().insert("lengths".into(), json!(lengths));
                }
                file
            })
            .collect();
        map.insert("files".into(), Value::Array(files));
        map.insert("totalSize".into(), json!(self.total_size));
        map.insert("fileCount".into(), json!(self.file_count));
        map.insert("chunkCount".into(), json!(self.chunk_count));
        Value::Object(map)
    }
}

/// Saklanmış manifestin dosya listesi (TS'te tip dönüşümü; alan yoksa None).
pub fn stored_files(manifest: &Value) -> Option<Vec<ManifestFile>> {
    let files = manifest.get("files")?.as_array()?;
    Some(
        files
            .iter()
            .map(|f| ManifestFile {
                path: f.get("path").and_then(Value::as_str).unwrap_or_default().to_string(),
                size: f.get("size").and_then(Value::as_f64).unwrap_or(0.0) as i64,
                chunks: f.get("chunks").and_then(Value::as_array).map(|c| c.iter().map(|h| h.as_str().unwrap_or_default().to_string()).collect()).unwrap_or_default(),
                lengths: f.get("lengths").and_then(Value::as_array).map(|l| l.iter().map(|n| n.as_f64().unwrap_or(0.0) as i64).collect()),
            })
            .collect(),
    )
}

/// İki paket arasındaki fark: dosya bazında eklendi / silindi / değişti ve önceki paketten
/// geçecek birinin gerçekten indireceği bayt (yalnız önceki pakette bulunmayan parçalar).
pub fn diff_manifests(previous: Option<&[ManifestFile]>, next: &[ManifestFile], limit: usize) -> Value {
    let previous = previous.unwrap_or(&[]);
    // JS Map: yinelenen anahtar ilk konumunda kalır, değeri sonuncusudur.
    let mut order: Vec<Option<&ManifestFile>> = Vec::new();
    let mut index: HashMap<&str, usize> = HashMap::new();
    for file in previous {
        match index.get(file.path.as_str()) {
            Some(&i) => order[i] = Some(file),
            None => {
                index.insert(&file.path, order.len());
                order.push(Some(file));
            }
        }
    }
    let had: HashSet<&str> = previous.iter().flat_map(|f| f.chunks.iter().map(String::as_str)).collect();
    let (mut added, mut removed, mut changed, mut unchanged) = (0i64, 0i64, 0i64, 0i64);
    let (mut added_bytes, mut removed_bytes, mut changed_bytes, mut download_bytes) = (0i64, 0i64, 0i64, 0i64);
    let (mut added_list, mut removed_list, mut changed_list) = (Vec::new(), Vec::new(), Vec::new());
    let mut truncated = false;
    let mut counted: HashSet<&str> = HashSet::new();
    let push = |list: &mut Vec<Value>, item: Value, truncated: &mut bool| {
        if list.len() < limit { list.push(item) } else { *truncated = true }
    };
    for file in next {
        for (i, hash) in file.chunks.iter().enumerate() {
            if !had.contains(hash.as_str()) && counted.insert(hash.as_str()) {
                download_bytes += chunk_length(file, i);
            }
        }
        let old = index.get(file.path.as_str()).and_then(|&i| order[i].take());
        let Some(old) = old else {
            added += 1;
            added_bytes += file.size;
            push(&mut added_list, json!({ "path": file.path, "size": file.size }), &mut truncated);
            continue;
        };
        let same = old.size == file.size && old.chunks == file.chunks;
        if same {
            unchanged += 1;
            continue;
        }
        changed += 1;
        changed_bytes += file.size;
        push(&mut changed_list, json!({ "path": file.path, "size": file.size, "previousSize": old.size }), &mut truncated);
    }
    for old in order.into_iter().flatten() {
        removed += 1;
        removed_bytes += old.size;
        push(&mut removed_list, json!({ "path": old.path, "size": old.size }), &mut truncated);
    }
    json!({
        "added": added, "removed": removed, "changed": changed, "unchanged": unchanged,
        "addedBytes": added_bytes, "removedBytes": removed_bytes, "changedBytes": changed_bytes, "downloadBytes": download_bytes,
        "files": { "added": added_list, "removed": removed_list, "changed": changed_list },
        "truncated": truncated,
    })
}

/// Saklı manifest (JSON) ile yeni manifest aynı içeriği mi tanımlıyor.
pub fn same_manifest(a: &Value, b: &Manifest) -> bool {
    let num = |k: &str| a.get(k).and_then(Value::as_f64);
    if a.get("executable").and_then(Value::as_str) != Some(b.executable.as_str())
        || num("totalSize") != Some(b.total_size as f64)
        || num("fileCount") != Some(b.file_count as f64)
        || num("chunkCount") != Some(b.chunk_count as f64)
    {
        return false;
    }
    let Some(files) = stored_files(a) else { return false };
    let index: HashMap<&str, &ManifestFile> = files.iter().map(|f| (f.path.as_str(), f)).collect();
    for file in &b.files {
        let Some(other) = index.get(file.path.as_str()) else { return false };
        if other.size != file.size || other.chunks != file.chunks {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const H1: &str = "1111111111111111111111111111111111111111111111111111111111111111";
    const H2: &str = "2222222222222222222222222222222222222222222222222222222222222222";

    fn validate(files: Value, exe: &str) -> Result<Validated, (&'static str, String)> {
        validate_manifest_input(Some(&json!(exe)), None, None, Some(&files), &DEFAULT_LIMITS)
    }

    #[test]
    fn yol_kurallari() {
        let p = |s: &str| safe_relative_path(Some(&json!(s)));
        assert_eq!(p("Game\\Bin\\Game.exe").as_deref(), Some("Game/Bin/Game.exe"));
        for bad in ["", "/abs", "C:/x", "a/../b", "a//b", "con", "CON.txt", "a/b.", "a/b ", "a:b", "x\u{1}y"] {
            assert!(p(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn dogrulama() {
        let ok = validate(json!([{ "path": "Game.exe", "size": 5, "chunks": [H1] }]), "Game.exe").unwrap();
        assert_eq!(ok.manifest.total_size, 5);
        assert_eq!(ok.unique_hashes, vec![H1.to_string()]);
        assert_eq!(validate(json!([]), "Game.exe").err().unwrap().0, "MANIFEST_EMPTY");
        assert_eq!(validate(json!([{ "path": "Game.exe", "size": 5, "chunks": [] }]), "Game.exe").err().unwrap().0, "MANIFEST_CHUNKS");
        assert_eq!(validate(json!([{ "path": "a.dat", "size": 5, "chunks": [H1] }]), "Game.exe").err().unwrap().0, "MANIFEST_EXECUTABLE_MISSING");
        assert_eq!(validate(json!([{ "path": "Game.exe", "size": 5, "chunks": [H1], "lengths": [4] }]), "Game.exe").err().unwrap().0, "MANIFEST_CHUNKS");
        assert!(validate(json!([{ "path": "Game.exe", "size": 5, "chunks": [H1, H2], "lengths": [2, 3] }]), "Game.exe").is_ok());
        assert_eq!(validate(json!([{ "path": 5, "size": 5, "chunks": [H1] }]), "Game.exe").err().unwrap().1, "Geçersiz dosya yolu: 5");
    }

    #[test]
    fn fark() {
        let a = vec![ManifestFile { path: "a".into(), size: 5, chunks: vec![H1.into()], lengths: None }];
        let b = vec![ManifestFile { path: "b".into(), size: 5, chunks: vec![H2.into()], lengths: None }];
        let d = diff_manifests(Some(&a), &b, DIFF_LIST_LIMIT);
        assert_eq!(d["added"], 1);
        assert_eq!(d["removed"], 1);
        assert_eq!(d["downloadBytes"], 5);
    }

    // ---- releaseManifest.spec.ts / releaseDiff.spec.ts karşılıkları
    fn h(n: u64) -> String {
        format!("{n:064x}")
    }
    fn mf(path: &str, size: i64, chunks: &[u64]) -> ManifestFile {
        ManifestFile { path: path.into(), size, chunks: chunks.iter().map(|n| h(*n)).collect(), lengths: None }
    }
    fn code(r: Result<Validated, (&'static str, String)>) -> &'static str {
        r.err().map(|e| e.0).unwrap_or("OK")
    }

    #[test]
    fn spec_guvenli_yol() {
        let p = |s: &str| safe_relative_path(Some(&json!(s)));
        for bad in ["", "/etc/passwd", "C:/Windows", "\\\\server\\share", "../secret", "a/../b", "./x", "a/b/", "con", "PRN.txt", "file.", "name ", "x\u{0}y", "a<b", "q?"] {
            assert!(p(bad).is_none(), "{bad:?}");
        }
        assert!(p(&vec!["a"; 66].join("/")).is_none());
        assert!(safe_relative_path(Some(&json!(5))).is_none());
    }

    #[test]
    fn spec_dogrulama() {
        let file = |path: &str, size: i64, chunks: &[u64]| json!({ "path": path, "size": size, "chunks": chunks.iter().map(|n| h(*n)).collect::<Vec<_>>() });
        let ok = validate(json!([file("Game.exe", 10, &[1]), file("data.pak", CHUNK_SIZE + 5, &[2, 3]), file("shared.bin", 3, &[1])]), "Game.exe").unwrap();
        assert_eq!((ok.manifest.file_count, ok.manifest.chunk_count), (3, 3));
        assert_eq!(ok.unique_hashes, vec![h(1), h(2), h(3)]);
        assert_eq!(code(validate(json!([file("Game.exe", CHUNK_SIZE + 1, &[1])]), "Game.exe")), "MANIFEST_CHUNKS");
        assert_eq!(code(validate(json!([file("Game.exe", 1, &[1])]), "Missing.exe")), "MANIFEST_EXECUTABLE_MISSING");
        assert_eq!(code(validate(json!([file("Game.bin", 1, &[1])]), "Game.bin")), "MANIFEST_EXECUTABLE");
        assert_eq!(code(validate(json!([file("Game.exe", 1, &[1]), file("game.EXE", 1, &[2])]), "Game.exe")), "MANIFEST_DUPLICATE");
        assert_eq!(code(validate(json!([{ "path": "Game.exe", "size": 1, "chunks": ["nothex"] }]), "Game.exe")), "MANIFEST_HASH");
        assert_eq!(code(validate(json!([file("Game.exe", 1, &[1]), file("empty.txt", 0, &[])]), "Game.exe")), "OK");
    }

    #[test]
    fn spec_icerik_tanimli() {
        let with = |size: i64, chunks: &[u64], lengths: Value| json!([{ "path": "Game.exe", "size": size, "chunks": chunks.iter().map(|n| h(*n)).collect::<Vec<_>>(), "lengths": lengths }]);
        let ok = validate(with(5_000_000, &[1, 2], json!([3_000_000, 2_000_000])), "Game.exe").unwrap();
        assert_eq!(ok.manifest.files[0].lengths, Some(vec![3_000_000, 2_000_000]));
        assert_eq!(code(validate(with(5_000_000, &[1, 2], json!([3_000_000, 1_000_000])), "Game.exe")), "MANIFEST_CHUNKS");
        assert_eq!(code(validate(with(5_000_000, &[1], json!([5_000_000])), "Game.exe")), "MANIFEST_CHUNKS");
        assert_eq!(code(validate(with(5_000_000, &[1, 2], json!([5_000_000])), "Game.exe")), "MANIFEST_CHUNKS");
        let plain = validate(json!([{ "path": "Game.exe", "size": 5_000_000, "chunks": [h(1), h(2)] }]), "Game.exe").unwrap();
        assert!(plain.manifest.files[0].lengths.is_none());
        let prev = validate(with(5_000_000, &[1, 2], json!([3_000_000, 2_000_000])), "Game.exe").unwrap().manifest;
        let next = validate(with(5_000_100, &[9, 2], json!([3_000_100, 2_000_000])), "Game.exe").unwrap().manifest;
        let d = diff_manifests(Some(&prev.files), &next.files, DIFF_LIST_LIMIT);
        assert_eq!((d["downloadBytes"].as_i64(), d["changed"].as_i64()), (Some(3_000_100), Some(1)));
    }

    #[test]
    fn spec_fark() {
        let d = diff_manifests(None, &[mf("game.exe", 10, &[1]), mf("data.pak", 5, &[2])], DIFF_LIST_LIMIT);
        assert_eq!((d["added"].as_i64(), d["addedBytes"].as_i64(), d["downloadBytes"].as_i64(), d["truncated"].as_bool()), (Some(2), Some(15), Some(15), Some(false)));
        let previous = [mf("game.exe", 10, &[1]), mf("data.pak", 5, &[2]), mf("old.bin", 3, &[3])];
        let next = [mf("game.exe", 10, &[1]), mf("data.pak", 7, &[4]), mf("new.txt", 2, &[2])];
        let d = diff_manifests(Some(&previous), &next, DIFF_LIST_LIMIT);
        assert_eq!(
            json!([d["added"], d["removed"], d["changed"], d["unchanged"], d["addedBytes"], d["removedBytes"], d["changedBytes"], d["downloadBytes"]]),
            json!([1, 1, 1, 1, 2, 3, 7, 7])
        );
        assert_eq!(d["files"]["changed"], json!([{ "path": "data.pak", "size": 7, "previousSize": 5 }]));
        assert_eq!(d["files"]["removed"], json!([{ "path": "old.bin", "size": 3 }]));
        assert_eq!(diff_manifests(None, &[mf("a.bin", 4, &[9]), mf("b.bin", 4, &[9])], DIFF_LIST_LIMIT)["downloadBytes"], 4);
        let d = diff_manifests(None, &[mf("a", 1, &[1]), mf("b", 1, &[2]), mf("c", 1, &[3])], 1);
        assert_eq!((d["added"].as_i64(), d["files"]["added"].as_array().map(Vec::len), d["truncated"].as_bool()), (Some(3), Some(1), Some(true)));
    }
}
