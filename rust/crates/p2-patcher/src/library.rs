//! `library.json` — Electron'un `userData/library/library.json` biçimiyle aynı (Faz 8'de kurulu
//! oyunlar doğrudan devralınabilsin). Bilinmeyen alanlar korunur.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InstalledProject {
    pub folder: String,
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub release_id: String,
    #[serde(default)]
    pub version: Value,
    #[serde(default)]
    pub installed_at: u64,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub executable: String,
    #[serde(default)]
    pub launch_args: String,
    #[serde(default)]
    pub user_args: String,
    #[serde(default)]
    pub play_seconds: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_played_at: Option<u64>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LibraryState {
    #[serde(default = "one")]
    pub version: u32,
    #[serde(default)]
    pub device_id: String,
    /// Oyunların kurulduğu kök klasör ("" = seçilmedi).
    #[serde(default)]
    pub root: String,
    #[serde(default)]
    pub projects: BTreeMap<String, InstalledProject>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn one() -> u32 {
    1
}

impl Default for LibraryState {
    fn default() -> Self {
        Self { version: 1, device_id: String::new(), root: String::new(), projects: BTreeMap::new(), extra: Map::new() }
    }
}

impl LibraryState {
    pub fn load(file: &Path) -> Self {
        let mut state: Self = std::fs::read(file).ok().and_then(|b| serde_json::from_slice(b.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&b)).ok()).unwrap_or_default();
        if state.device_id.is_empty() {
            state.device_id = uuid::Uuid::new_v4().to_string();
            let _ = state.save(file);
        }
        state
    }

    /// Atomik yazım (geçici dosya + yeniden adlandırma).
    pub fn save(&self, file: &Path) -> std::io::Result<()> {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = file.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec(self).unwrap_or_default())?;
        std::fs::rename(&tmp, file)
    }

    pub fn project_dir(&self, folder: &str) -> PathBuf {
        PathBuf::from(&self.root).join(folder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn electron_library_json_roundtrips() {
        let v = serde_json::json!({ "version": 1, "deviceId": "d", "root": "D:\\Oyunlar", "projects": { "p": { "folder": "Spike 2", "channel": "alpha", "releaseId": "r", "version": "1.0.3", "installedAt": 1, "pinned": false, "executable": "Game.exe", "launchArgs": "", "userArgs": "-windowed", "playSeconds": 120, "lastPlayedAt": 5, "gelecekAlan": true } } });
        let s: LibraryState = serde_json::from_value(v.clone()).unwrap();
        assert_eq!(s.projects["p"].user_args, "-windowed");
        assert_eq!(serde_json::to_value(&s).unwrap(), v);
    }
}
