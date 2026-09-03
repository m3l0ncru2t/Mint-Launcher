use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub offline_username: Option<String>,
    /// Azure AD "Application (client) ID" needed to enable real Microsoft
    /// account login. See src-tauri/src/msa.rs for the auth flow this powers.
    pub microsoft_client_id: Option<String>,
    /// The saved account (see accounts.rs) to silently re-authenticate as on
    /// startup, so signing in with Microsoft persists across app restarts.
    pub last_account_id: Option<String>,
}

pub fn load(path: &Path) -> Settings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, settings: &Settings) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(settings)?)
}
