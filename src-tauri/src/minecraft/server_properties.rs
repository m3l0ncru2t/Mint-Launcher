//! Reads and writes a curated subset of `server.properties` - the actual
//! Minecraft settings for a dedicated server (difficulty, max players, PvP,
//! whitelist, ports, ...), as opposed to the launcher-level settings
//! (memory, JVM args) `Instance` itself already covers. A real
//! `server.properties` commonly has ~40 keys and hand-written comments; only
//! known keys this module manages are ever touched, and only their value is
//! replaced in place - every other line (unknown keys, comments, blank
//! lines) passes through untouched, so importing an existing, already
//! fine-tuned server never loses anything Mint doesn't have a UI for.

use std::collections::HashMap;
use std::path::Path;

pub fn read_properties(game_dir: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(contents) = std::fs::read_to_string(game_dir.join("server.properties")) else {
        return map;
    };
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            map.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    map
}

/// Merges `updates` into `server.properties`, preserving every other line
/// (see the module doc). Keys not already present are appended at the end -
/// this is exactly how a fresh server.jar first-run generates the file, so
/// nothing here needs a "create from template" path of its own; the file is
/// created outright if it doesn't exist yet.
pub fn write_properties(game_dir: &Path, updates: &HashMap<String, String>) -> std::io::Result<()> {
    let path = game_dir.join("server.properties");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();

    let mut written: Vec<&str> = Vec::new();
    let mut out_lines: Vec<String> = Vec::new();
    for line in existing.lines() {
        let trimmed = line.trim();
        let key = (!trimmed.is_empty() && !trimmed.starts_with('#'))
            .then(|| trimmed.split_once('=').map(|(k, _)| k.trim()))
            .flatten();
        match key.and_then(|k| updates.get_key_value(k)) {
            Some((k, v)) => {
                out_lines.push(format!("{k}={v}"));
                written.push(k.as_str());
            }
            None => out_lines.push(line.to_string()),
        }
    }
    for (key, value) in updates {
        if !written.contains(&key.as_str()) {
            out_lines.push(format!("{key}={value}"));
        }
    }

    std::fs::create_dir_all(game_dir)?;
    std::fs::write(path, out_lines.join("\n") + "\n")
}
