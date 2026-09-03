//! Tracks which installed resource packs are actually active, the same way
//! the game itself does: via the `resourcePacks` list in `options.txt` -
//! zipped packs are listed as `file/<name>.zip`, unzipped/folder packs just
//! by their bare folder name. There's no rename-based enable/disable
//! convention for resource packs the way loaders use `.disabled` for mods.

use std::collections::HashSet;
use std::path::Path;

/// Total size of a folder-style resource pack (an unzipped pack directly
/// under `resourcepacks/`, as opposed to a `.zip`) - Minecraft treats both
/// as equally valid, so Mint's resource pack list needs to as well.
pub fn dir_size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut total = 0u64;
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            total += dir_size(&entry.path());
        } else if file_type.is_file() {
            total += entry.metadata().map(|m| m.len()).unwrap_or(0);
        }
    }
    total
}

fn options_path(game_dir: &Path) -> std::path::PathBuf {
    game_dir.join("options.txt")
}

fn read_lines(game_dir: &Path) -> Vec<String> {
    std::fs::read_to_string(options_path(game_dir))
        .map(|s| s.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

fn parse_resource_packs_line(line: &str) -> Vec<String> {
    let Some(value) = line.strip_prefix("resourcePacks:") else {
        return Vec::new();
    };
    serde_json::from_str(value).unwrap_or_default()
}

/// The set of installed resource pack file/folder names currently active
/// in-game - normalizes away the `file/` prefix zipped packs use, since
/// folder packs are listed bare and Mint's list identifies both the same
/// way (by name).
pub fn enabled_resourcepacks(game_dir: &Path) -> HashSet<String> {
    let lines = read_lines(game_dir);
    let Some(line) = lines.iter().find(|l| l.starts_with("resourcePacks:")) else {
        return HashSet::new();
    };
    parse_resource_packs_line(line)
        .into_iter()
        .map(|entry| entry.strip_prefix("file/").map(str::to_string).unwrap_or(entry))
        .collect()
}

/// Adds or removes one resource pack from the active list in `options.txt`,
/// preserving every other line/setting untouched. Creates the file (and the
/// `resourcePacks` key) if either doesn't exist yet.
pub fn set_resourcepack_enabled(game_dir: &Path, file_name: &str, enabled: bool) -> std::io::Result<()> {
    std::fs::create_dir_all(game_dir)?;
    let mut lines = read_lines(game_dir);
    let is_folder = game_dir.join("resourcepacks").join(file_name).is_dir();
    let entry = if is_folder { file_name.to_string() } else { format!("file/{file_name}") };

    let mut list = lines
        .iter()
        .find(|l| l.starts_with("resourcePacks:"))
        .map(|l| parse_resource_packs_line(l))
        .unwrap_or_default();

    if enabled {
        if !list.iter().any(|e| e == &entry) {
            list.push(entry);
        }
    } else {
        list.retain(|e| e != &entry);
    }

    let new_line = format!("resourcePacks:{}", serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_string()));
    match lines.iter().position(|l| l.starts_with("resourcePacks:")) {
        Some(idx) => lines[idx] = new_line,
        None => lines.push(new_line),
    }

    std::fs::write(options_path(game_dir), lines.join("\n") + "\n")
}
