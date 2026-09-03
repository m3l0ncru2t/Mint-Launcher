//! Persists Microsoft accounts across app restarts by storing each one's
//! OAuth refresh token (not the short-lived Minecraft session itself) - see
//! msa.rs, which exchanges these for a fresh session on demand.

use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedAccount {
    /// The Minecraft profile UUID - stable across re-logins, unlike the name.
    pub id: String,
    pub username: String,
    pub refresh_token: String,
    /// Whichever Azure app issued this refresh token - it must be reused for
    /// refreshes even if the user later changes their client ID override.
    pub client_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSummary {
    pub id: String,
    pub username: String,
}

fn accounts_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("accounts.json")
}

pub fn load(data_dir: &Path) -> Vec<SavedAccount> {
    std::fs::read_to_string(accounts_path(data_dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save(data_dir: &Path, accounts: &[SavedAccount]) -> std::io::Result<()> {
    std::fs::write(accounts_path(data_dir), serde_json::to_string_pretty(accounts)?)
}

/// Adds a new saved account, or updates an existing one's username/tokens
/// (Microsoft may rotate the refresh token on each use).
pub fn upsert(data_dir: &Path, account: SavedAccount) -> std::io::Result<()> {
    let mut accounts = load(data_dir);
    match accounts.iter_mut().find(|a| a.id == account.id) {
        Some(existing) => *existing = account,
        None => accounts.push(account),
    }
    save(data_dir, &accounts)
}

pub fn remove(data_dir: &Path, id: &str) -> std::io::Result<()> {
    let mut accounts = load(data_dir);
    accounts.retain(|a| a.id != id);
    save(data_dir, &accounts)
}
