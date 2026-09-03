//! Tracks which installed mod files were pulled in as someone else's
//! dependency versus installed directly, so the UI can group them. Only
//! covers mods installed through the launcher's Modrinth browser - a jar
//! dropped in manually has no record and is treated as directly installed.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModMeta {
    pub project_id: String,
    pub is_dependency: bool,
}

pub type ModMetaMap = HashMap<String, ModMeta>;

fn meta_path(mods_dir: &Path) -> std::path::PathBuf {
    mods_dir.join(".mint_mod_meta.json")
}

pub fn load(mods_dir: &Path) -> ModMetaMap {
    std::fs::read_to_string(meta_path(mods_dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save(mods_dir: &Path, map: &ModMetaMap) -> std::io::Result<()> {
    std::fs::write(meta_path(mods_dir), serde_json::to_string_pretty(map)?)
}

pub fn remove_entry(mods_dir: &Path, file_name: &str) {
    let mut map = load(mods_dir);
    if map.remove(file_name).is_some() {
        let _ = save(mods_dir, &map);
    }
}

pub fn rename_entry(mods_dir: &Path, old_name: &str, new_name: &str) {
    let mut map = load(mods_dir);
    if let Some(entry) = map.remove(old_name) {
        map.insert(new_name.to_string(), entry);
        let _ = save(mods_dir, &map);
    }
}
