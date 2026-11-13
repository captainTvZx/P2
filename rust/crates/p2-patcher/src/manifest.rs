//! Kütüphane manifesti — electron/patcherManifest.cjs ve server/src/releaseManifest.ts ile
//! aynı kurallar: güvenli göreli yol, parça aralıkları (`lengths` varsa içerik tanımlı, yoksa
//! eski sabit 4 MB), proje klasör adı ve kurulum planı (yerelde olan parçalar kopyalanır,
//! yalnız eksikler indirilir).

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const CHUNK_SIZE: u64 = 4 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ManifestFile {
    pub path: String,
    pub size: u64,
    pub chunks: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lengths: Option<Vec<u64>>,
}

/// Sunucunun manifesti; bilinmeyen alanlar (`chunkSize`, `platform`, `totalSize`…) korunur.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    #[serde(default)]
    pub version: Value,
    #[serde(default)]
    pub executable: String,
    #[serde(default, rename = "launchArgs")]
    pub launch_args: Option<String>,
    #[serde(default)]
    pub files: Vec<ManifestFile>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

pub fn is_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn reserved(name: &str) -> bool {
    let base = name.split('.').next().unwrap_or_default().to_ascii_lowercase();
    matches!(base.as_str(), "con" | "prn" | "aux" | "nul") || ((base.starts_with("com") || base.starts_with("lpt")) && base.len() == 4 && base.as_bytes()[3].is_ascii_digit() && base.as_bytes()[3] != b'0')
}

/// Göreli, güvenli yol: mutlak/sürücü/UNC yok, ".." yok, yasak karakter yok, parça sonunda
/// nokta/boşluk ve ayrılmış cihaz adı yok.
pub fn safe_relative_path(input: &str) -> Option<String> {
    if input.is_empty() || input.chars().count() > 1024 {
        return None;
    }
    if input.chars().any(|c| (c as u32) < 0x20 || "<>:\"|?*".contains(c)) {
        return None;
    }
    let value = input.replace('\\', "/");
    if value.starts_with('/') || (value.len() >= 2 && value.as_bytes()[0].is_ascii_alphabetic() && value.as_bytes()[1] == b':') {
        return None;
    }
    let parts: Vec<&str> = value.split('/').collect();
    if parts.len() > 64 {
        return None;
    }
    for part in &parts {
        if part.is_empty() || *part == "." || *part == ".." || part.ends_with('.') || part.ends_with(' ') || reserved(part) || part.chars().count() > 255 {
            return None;
        }
    }
    Some(parts.join("/"))
}

/// (offset, length) aralıkları.
pub fn chunk_ranges(file: &ManifestFile) -> Vec<(u64, u64)> {
    match &file.lengths {
        Some(lengths) => {
            let mut offset = 0;
            lengths
                .iter()
                .map(|l| {
                    let r = (offset, *l);
                    offset += l;
                    r
                })
                .collect()
        }
        None => (0..file.size.div_ceil(CHUNK_SIZE)).map(|i| (i * CHUNK_SIZE, CHUNK_SIZE.min(file.size - i * CHUNK_SIZE))).collect(),
    }
}

/// Proje adından güvenli klasör adı (Windows'un yasak karakterleri atılır).
pub fn project_folder_name(name: &str, suffix: Option<&str>) -> String {
    let mut cleaned: String = name.chars().map(|c| if (c as u32) < 0x20 || "<>:\"/\\|?*".contains(c) { ' ' } else { c }).collect();
    cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    cleaned = cleaned.trim_end_matches(['.', ' ']).to_string();
    if reserved(&cleaned) {
        cleaned = format!("_{cleaned}");
    }
    let mut cleaned: String = cleaned.chars().take(80).collect::<String>().trim().to_string();
    if cleaned.is_empty() {
        cleaned = "Proje".into();
    }
    match suffix {
        Some(s) => format!("{cleaned}-{s}"),
        None => cleaned,
    }
}

pub fn same_file(a: Option<&ManifestFile>, b: &ManifestFile) -> bool {
    a.is_some_and(|a| a.size == b.size && a.chunks == b.chunks)
}

#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// Aynı dosyanın kurulu hâlinde (yeri kaymış olabilir).
    SelfFile { src_offset: u64 },
    /// Başka bir kurulu dosyada.
    Local { src_path: String, src_offset: u64 },
    /// Ağdan indirilecek.
    Net,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlannedChunk {
    pub hash: String,
    pub offset: u64,
    pub length: u64,
    pub from: Source,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Build {
    pub path: String,
    pub size: u64,
    pub chunks: Vec<PlannedChunk>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub builds: Vec<Build>,
    pub deletes: Vec<String>,
    pub downloads: Vec<String>,
    pub bytes_to_download: u64,
    pub bytes_to_write: u64,
}

/// patcherManifest.cjs `planInstall`.
pub fn plan_install(installed: Option<&Manifest>, target: &Manifest) -> Plan {
    let empty = Vec::new();
    let installed_files = installed.map(|m| &m.files).unwrap_or(&empty);
    let by_path: HashMap<&str, &ManifestFile> = installed_files.iter().map(|f| (f.path.as_str(), f)).collect();
    let mut local_by_hash: HashMap<&str, (&str, u64, u64)> = HashMap::new();
    for f in installed_files {
        for (hash, (offset, length)) in f.chunks.iter().zip(chunk_ranges(f)) {
            local_by_hash.entry(hash.as_str()).or_insert((f.path.as_str(), offset, length));
        }
    }
    let mut plan = Plan::default();
    let mut downloads: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut keep: HashSet<&str> = HashSet::new();
    for file in &target.files {
        keep.insert(file.path.as_str());
        let current = by_path.get(file.path.as_str()).copied();
        if same_file(current, file) {
            continue;
        }
        let mut current_by_hash: HashMap<&str, (u64, u64)> = HashMap::new();
        if let Some(c) = current {
            for (hash, r) in c.chunks.iter().zip(chunk_ranges(c)) {
                current_by_hash.entry(hash.as_str()).or_insert(r);
            }
        }
        let chunks = file
            .chunks
            .iter()
            .zip(chunk_ranges(file))
            .map(|(hash, (offset, length))| {
                let from = if let Some((src, len)) = current_by_hash.get(hash.as_str()).filter(|r| r.1 == length) {
                    let _ = len;
                    Source::SelfFile { src_offset: *src }
                } else if let Some((path, src, _)) = local_by_hash.get(hash.as_str()).filter(|r| r.2 == length) {
                    Source::Local { src_path: path.to_string(), src_offset: *src }
                } else {
                    if seen.insert(hash.clone()) {
                        downloads.push(hash.clone());
                        plan.bytes_to_download += length;
                    } else {
                        // Aynı parça iki kez indirilmez (patcherManifest.cjs: Set) ama bayt yine sayılır.
                        plan.bytes_to_download += length;
                    }
                    Source::Net
                };
                PlannedChunk { hash: hash.clone(), offset, length, from }
            })
            .collect();
        plan.bytes_to_write += file.size;
        plan.builds.push(Build { path: file.path.clone(), size: file.size, chunks });
    }
    plan.deletes = installed_files.iter().map(|f| f.path.clone()).filter(|p| !keep.contains(p.as_str())).collect();
    plan.downloads = downloads;
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(c: char) -> String {
        std::iter::repeat_n(c, 64).collect()
    }

    #[test]
    fn safe_paths() {
        assert_eq!(safe_relative_path("Game\\Bin\\game.exe").as_deref(), Some("Game/Bin/game.exe"));
        for bad in ["/etc/x", "C:/x", "a/../b", "a/./b", "con.txt", "x/LPT1", "a /b", "a./b", "a<b", ""] {
            assert_eq!(safe_relative_path(bad), None, "{bad}");
        }
        assert!(safe_relative_path("com0/x").is_some());
    }

    #[test]
    fn folder_names() {
        assert_eq!(project_folder_name("Spike: 2 / Oyun?", None), "Spike 2 Oyun");
        assert_eq!(project_folder_name("aux", None), "_aux");
        assert_eq!(project_folder_name("   ", Some("ab12")), "Proje-ab12");
    }

    #[test]
    fn ranges_fixed_and_variable() {
        let fixed = ManifestFile { path: "a".into(), size: CHUNK_SIZE * 2 + 5, chunks: vec![h('a'), h('b'), h('c')], lengths: None };
        assert_eq!(chunk_ranges(&fixed), vec![(0, CHUNK_SIZE), (CHUNK_SIZE, CHUNK_SIZE), (CHUNK_SIZE * 2, 5)]);
        let var = ManifestFile { path: "a".into(), size: 30, chunks: vec![h('a'), h('b')], lengths: Some(vec![10, 20]) };
        assert_eq!(chunk_ranges(&var), vec![(0, 10), (10, 20)]);
    }

    #[test]
    fn plan_reuses_local_chunks() {
        let installed = Manifest {
            version: Value::from("1.0"),
            executable: "game.exe".into(),
            launch_args: None,
            files: vec![
                ManifestFile { path: "game.exe".into(), size: 30, chunks: vec![h('a'), h('b')], lengths: Some(vec![10, 20]) },
                ManifestFile { path: "old.dat".into(), size: 5, chunks: vec![h('e')], lengths: Some(vec![5]) },
                ManifestFile { path: "same.pak".into(), size: 7, chunks: vec![h('s')], lengths: Some(vec![7]) },
            ],
            extra: Map::new(),
        };
        let target = Manifest {
            files: vec![
                // 'b' aynı dosyada (yeri kaydı), 'c' yeni
                ManifestFile { path: "game.exe".into(), size: 35, chunks: vec![h('c'), h('b')], lengths: Some(vec![15, 20]) },
                // 'e' başka dosyadan kopyalanır
                ManifestFile { path: "new.dat".into(), size: 5, chunks: vec![h('e')], lengths: Some(vec![5]) },
                ManifestFile { path: "same.pak".into(), size: 7, chunks: vec![h('s')], lengths: Some(vec![7]) },
            ],
            ..installed.clone()
        };
        let plan = plan_install(Some(&installed), &target);
        assert_eq!(plan.downloads, vec![h('c')]);
        assert_eq!(plan.bytes_to_download, 15);
        assert_eq!(plan.builds.len(), 2, "aynı dosya yeniden yazılmaz");
        assert_eq!(plan.builds[0].chunks[1].from, Source::SelfFile { src_offset: 10 });
        assert_eq!(plan.builds[1].chunks[0].from, Source::Local { src_path: "old.dat".into(), src_offset: 0 });
        assert_eq!(plan.deletes, vec!["old.dat".to_string()]);
    }

    #[test]
    fn manifest_keeps_unknown_fields() {
        let v: Value = serde_json::json!({ "version": 1, "chunkSize": 4194304, "platform": "windows", "executable": "g.exe", "launchArgs": "-x", "totalSize": 3, "files": [{ "path": "g.exe", "size": 3, "chunks": [h('a')], "lengths": [3] }] });
        let m: Manifest = serde_json::from_value(v.clone()).unwrap();
        assert_eq!(serde_json::to_value(&m).unwrap(), v);
    }
}
