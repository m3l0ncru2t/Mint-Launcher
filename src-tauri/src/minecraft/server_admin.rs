//! Direct read/write access to a vanilla server's `ops.json`/`whitelist.json`/
//! `banned-players.json` - the same files the `op`/`whitelist`/`ban` console
//! commands themselves maintain. Only used while the server *isn't* running:
//! while it's up, its own in-memory copy of these lists is authoritative and
//! would clobber a direct file edit the moment anything next touches them, so
//! the equivalent console command is sent instead in that case (see
//! `commands::instances::ensure_not_running` and `PlayersPanel.tsx`).

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpEntry {
    pub uuid: String,
    pub name: String,
    #[serde(default = "default_op_level")]
    pub level: u8,
    #[serde(default)]
    pub bypasses_player_limit: bool,
}

fn default_op_level() -> u8 {
    4
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WhitelistEntry {
    pub uuid: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BannedPlayerEntry {
    pub uuid: String,
    pub name: String,
    #[serde(default)]
    pub created: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub expires: String,
    #[serde(default)]
    pub reason: String,
}

fn read_json_list<T: serde::de::DeserializeOwned>(path: &Path) -> Vec<T> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_json_list<T: Serialize>(path: &Path, list: &[T]) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(list).expect("Vec<T> of these types always serializes");
    std::fs::write(path, json)
}

pub fn read_ops(game_dir: &Path) -> Vec<OpEntry> {
    read_json_list(&game_dir.join("ops.json"))
}

pub fn read_whitelist(game_dir: &Path) -> Vec<WhitelistEntry> {
    read_json_list(&game_dir.join("whitelist.json"))
}

pub fn read_banned_players(game_dir: &Path) -> Vec<BannedPlayerEntry> {
    read_json_list(&game_dir.join("banned-players.json"))
}

pub fn remove_op(game_dir: &Path, name: &str) -> std::io::Result<()> {
    let mut list = read_ops(game_dir);
    list.retain(|e| !e.name.eq_ignore_ascii_case(name));
    write_json_list(&game_dir.join("ops.json"), &list)
}

pub fn remove_whitelist(game_dir: &Path, name: &str) -> std::io::Result<()> {
    let mut list = read_whitelist(game_dir);
    list.retain(|e| !e.name.eq_ignore_ascii_case(name));
    write_json_list(&game_dir.join("whitelist.json"), &list)
}

pub fn unban_player(game_dir: &Path, name: &str) -> std::io::Result<()> {
    let mut list = read_banned_players(game_dir);
    list.retain(|e| !e.name.eq_ignore_ascii_case(name));
    write_json_list(&game_dir.join("banned-players.json"), &list)
}

pub fn add_whitelist(game_dir: &Path, uuid: String, name: String) -> std::io::Result<()> {
    let mut list = read_whitelist(game_dir);
    list.retain(|e| !e.name.eq_ignore_ascii_case(&name));
    list.push(WhitelistEntry { uuid, name });
    write_json_list(&game_dir.join("whitelist.json"), &list)
}

pub fn add_ban(game_dir: &Path, uuid: String, name: String, reason: String) -> std::io::Result<()> {
    let mut list = read_banned_players(game_dir);
    list.retain(|e| !e.name.eq_ignore_ascii_case(&name));
    list.push(BannedPlayerEntry {
        uuid,
        name,
        created: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S %z").to_string(),
        source: "Mint Launcher".to_string(),
        expires: "forever".to_string(),
        reason: if reason.trim().is_empty() { "Banned by an operator".to_string() } else { reason },
    });
    write_json_list(&game_dir.join("banned-players.json"), &list)
}

pub fn add_op(game_dir: &Path, uuid: String, name: String) -> std::io::Result<()> {
    let mut list = read_ops(game_dir);
    list.retain(|e| !e.name.eq_ignore_ascii_case(&name));
    list.push(OpEntry { uuid, name, level: 4, bypasses_player_limit: false });
    write_json_list(&game_dir.join("ops.json"), &list)
}

/// Resolves a username to its real Mojang account UUID - needed to add a
/// fresh entry to `ops.json`/`whitelist.json` while the server isn't running
/// to resolve and write it itself the way the `op`/`whitelist add` console
/// commands do.
pub async fn lookup_uuid(client: &reqwest::Client, username: &str) -> anyhow::Result<(String, String)> {
    let resp = client
        .get(format!("https://api.mojang.com/users/profiles/minecraft/{username}"))
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("No such player \"{username}\"");
    }
    let json: serde_json::Value = resp.json().await?;
    let raw_id = json
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Unexpected response from Mojang"))?;
    let name = json.get("name").and_then(|v| v.as_str()).unwrap_or(username).to_string();
    Ok((dash_uuid(raw_id), name))
}

/// Mojang's lookup API returns a compact 32-hex-digit UUID with no dashes;
/// every other file/protocol in the game (and `ops.json`/`whitelist.json`
/// themselves) expects the standard dashed form.
pub(crate) fn dash_uuid(compact: &str) -> String {
    if compact.len() != 32 {
        return compact.to_string();
    }
    format!(
        "{}-{}-{}-{}-{}",
        &compact[0..8],
        &compact[8..12],
        &compact[12..16],
        &compact[16..20],
        &compact[20..32]
    )
}
