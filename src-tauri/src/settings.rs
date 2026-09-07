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
    /// Unlocks the "+ New Server"/"Import Server" entry points in the
    /// sidebar - see `instance::InstanceKind::Server`. Off by default since
    /// server support is still experimental.
    #[serde(default)]
    pub experimental_server_instances: bool,
    /// Unlocks the Configs/Logs tabs in `InstanceFilesPanel`. Off by default
    /// alongside the other experimental options, since editing a config file
    /// or clearing a stale one wrong can break an instance in ways the rest
    /// of the UI doesn't guard against.
    #[serde(default)]
    pub experimental_configs_logs_tabs: bool,
    /// Whether `remote_api`'s admin API server is running at all - off by
    /// default since it's a new network-facing surface (meant to be reached
    /// only over a private tunnel like Tailscale, never port-forwarded to the
    /// public internet).
    #[serde(default)]
    pub remote_admin_enabled: bool,
    #[serde(default = "default_remote_admin_port")]
    pub remote_admin_port: u16,
    /// The one server instance currently shared over the remote admin API -
    /// v1 only supports sharing a single instance at a time.
    #[serde(default)]
    pub remote_admin_instance_id: Option<String>,
    /// Which layout `InstanceDetail`/`RemoteInstanceDetail` use for the area
    /// below the header - `false` (default) is the padded/boxed look local
    /// instances have always had; `true` removes that padding for a more
    /// spacious layout (how a remote server's view originally looked, before
    /// this became a deliberate choice rather than an oversight). Applies to
    /// both local and remote instances alike, not just remote ones.
    #[serde(default)]
    pub spacious_instance_view: bool,
}

fn default_remote_admin_port() -> u16 {
    25580
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
            experimental_server_instances: false,
            experimental_configs_logs_tabs: false,
            remote_admin_enabled: false,
            remote_admin_port: default_remote_admin_port(),
            remote_admin_instance_id: None,
            spacious_instance_view: false,
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
