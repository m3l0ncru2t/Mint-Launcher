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

/// Retrofits RCON onto `server.properties` if it isn't already configured -
/// called right before every server launch (see `commands::launch::
/// do_launch_server`). RCON survives Mint restarting in a way the piped
/// stdin `spawn_and_stream_server` normally holds does not (see
/// `minecraft::rcon`), so this is what lets player count/TPS/console
/// commands keep working after the launcher is reopened with the server
/// already running, instead of only for as long as the Mint process that
/// launched it stays alive. Only ever adds keys that are missing, so an
/// operator who's already configured RCON themselves (a chosen port,
/// a password they know) - or explicitly turned it off (`enable-rcon=false`)
/// - is left untouched. A server that's already running when this runs
/// doesn't pick up the change until its next restart, same as any other
/// `server.properties` edit.
pub fn ensure_rcon_enabled(game_dir: &Path) -> std::io::Result<()> {
    let props = read_properties(game_dir);
    if props.get("enable-rcon").is_some_and(|v| v != "true") {
        return Ok(());
    }
    if props.contains_key("enable-rcon") && props.contains_key("rcon.port") && props.contains_key("rcon.password") {
        return Ok(());
    }

    let mut updates = HashMap::new();
    if !props.contains_key("enable-rcon") {
        updates.insert("enable-rcon".to_string(), "true".to_string());
    }
    if !props.contains_key("rcon.port") {
        let base_port = props.get("server-port").and_then(|p| p.parse::<u16>().ok()).unwrap_or(25565);
        updates.insert("rcon.port".to_string(), pick_free_port(base_port.wrapping_add(10)).to_string());
    }
    if !props.contains_key("rcon.password") {
        updates.insert("rcon.password".to_string(), uuid::Uuid::new_v4().simple().to_string());
    }
    write_properties(game_dir, &updates)
}

/// Best-effort free-port search starting at `start`, wrapping past 65535
/// back to 1024 rather than overflowing - a small race exists between this
/// check and the JVM's own bind moments later (the same tradeoff any
/// "find a free port" helper has), acceptable since RCON is loopback-only
/// admin tooling nothing else on the machine is likely competing for.
fn pick_free_port(start: u16) -> u16 {
    let start = start.max(1024);
    for offset in 0..50u16 {
        let port = 1024 + (start - 1024 + offset) % (u16::MAX - 1024);
        if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return port;
        }
    }
    start
}
