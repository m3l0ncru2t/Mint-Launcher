use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// A theme's saved sidebar/addon-list transparency - each theme (a built-in
/// preset id or a custom background id) remembers its own look rather than
/// sharing one global transparency setting, since a busy custom image and a
/// plain gradient usually want different amounts of blend.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeOpacity {
    pub sidebar: f32,
    pub mods_panel: f32,
}

impl Default for ThemeOpacity {
    fn default() -> Self {
        Self {
            sidebar: 0.82,
            mods_panel: 0.82,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub offline_username: Option<String>,
    /// Azure AD "Application (client) ID" needed to enable real Microsoft
    /// account login. See src-tauri/src/msa.rs for the auth flow this powers.
    pub microsoft_client_id: Option<String>,
    /// The saved account (see accounts.rs) to silently re-authenticate as on
    /// startup, so signing in with Microsoft persists across app restarts.
    pub last_account_id: Option<String>,
    /// Which background theme is active: a built-in preset id (the frontend
    /// owns that list), the id of a previously-added custom background (see
    /// `commands::appearance::add_custom_background`), or `None` for the
    /// plain default look with no background image at all.
    #[serde(default)]
    pub background_theme: Option<String>,
    /// Per-theme sidebar/addon-list transparency, keyed by theme id (a
    /// built-in preset id, or a custom background id) - a theme with no
    /// entry here just uses `ThemeOpacity::default()`.
    #[serde(default)]
    pub theme_opacity: HashMap<String, ThemeOpacity>,
    /// Display names for custom backgrounds, keyed by their id - a custom
    /// background with no entry here shows as plain "Custom".
    #[serde(default)]
    pub custom_background_names: HashMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            offline_username: None,
            microsoft_client_id: None,
            last_account_id: None,
            background_theme: None,
            theme_opacity: HashMap::new(),
            custom_background_names: HashMap::new(),
        }
    }
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
