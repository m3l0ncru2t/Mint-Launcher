//! A record of what remote admins did on this host (see `remote_api.rs`),
//! plus the server automation's own actions (scheduled restarts, crash
//! auto-restarts) - so an owner can trace who kicked someone, edited a
//! config, or restarted the server, and when. Kept as JSON lines in the
//! data folder; trimmed to its newest entries once it grows past a cap.

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const MAX_BYTES: u64 = 2 * 1024 * 1024;
/// How many entries survive a trim.
const KEEP_AFTER_TRIM: usize = 5000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    /// RFC 3339, UTC.
    pub time: String,
    /// Who did it - an admin's Minecraft name, or "Mint" for automation.
    pub actor: String,
    /// The admin's UUID; empty for automation.
    #[serde(default)]
    pub uuid: String,
    /// Short action name ("Kicked", "Edited file", "Restarted server"...).
    pub action: String,
    /// What it applied to - a player, file name, command text...
    #[serde(default)]
    pub detail: String,
}

// One writer at a time, so concurrent requests can't interleave lines.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

fn log_path(data_dir: &Path) -> PathBuf {
    data_dir.join("admin_log.jsonl")
}

/// Appends one entry. Best-effort: a failed write never fails the action
/// it's recording.
pub fn record(data_dir: &Path, actor: &str, uuid: &str, action: &str, detail: &str) {
    let entry = LogEntry {
        time: chrono::Utc::now().to_rfc3339(),
        actor: actor.to_string(),
        uuid: uuid.to_string(),
        action: action.to_string(),
        // Long free text (a pasted command, say) is cut so one entry can't
        // bloat the file.
        detail: detail.chars().take(500).collect(),
    };
    let Ok(line) = serde_json::to_string(&entry) else { return };
    let _guard = WRITE_LOCK.lock();
    let path = log_path(data_dir);
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(file, "{line}");
    }
    if std::fs::metadata(&path).map(|m| m.len() > MAX_BYTES).unwrap_or(false) {
        if let Ok(text) = std::fs::read_to_string(&path) {
            let lines: Vec<&str> = text.lines().collect();
            let keep = &lines[lines.len().saturating_sub(KEEP_AFTER_TRIM)..];
            let _ = std::fs::write(&path, keep.join("\n") + "\n");
        }
    }
}

/// Newest first, at most `limit` entries.
pub fn read(data_dir: &Path, limit: usize) -> Vec<LogEntry> {
    let Ok(text) = std::fs::read_to_string(log_path(data_dir)) else { return Vec::new() };
    text.lines()
        .rev()
        .filter_map(|l| serde_json::from_str::<LogEntry>(l).ok())
        .take(limit)
        .collect()
}

pub fn clear(data_dir: &Path) -> std::io::Result<()> {
    let _guard = WRITE_LOCK.lock();
    match std::fs::remove_file(log_path(data_dir)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}
