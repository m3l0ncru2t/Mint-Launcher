//! Tracks which installed resource packs are actually active, the same way
//! the game itself does: via the `resourcePacks` list in `options.txt`
//! (entries formatted as `file/<name>.zip`) - there's no rename-based
//! enable/disable convention for resource packs the way loaders use
//! `.disabled` for mods.

use std::collections::HashSet;
use std::path::Path;

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

/// The set of installed resource pack file names currently active in-game.
pub fn enabled_resourcepacks(game_dir: &Path) -> HashSet<String> {
    let lines = read_lines(game_dir);
    let Some(line) = lines.iter().find(|l| l.starts_with("resourcePacks:")) else {
        return HashSet::new();
    };
    parse_resource_packs_line(line)
        .into_iter()
        .filter_map(|entry| entry.strip_prefix("file/").map(str::to_string))
        .collect()
}

/// Adds or removes one resource pack from the active list in `options.txt`,
/// preserving every other line/setting untouched. Creates the file (and the
/// `resourcePacks` key) if either doesn't exist yet.
pub fn set_resourcepack_enabled(game_dir: &Path, file_name: &str, enabled: bool) -> std::io::Result<()> {
    std::fs::create_dir_all(game_dir)?;
    let mut lines = read_lines(game_dir);
    let entry = format!("file/{file_name}");

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
