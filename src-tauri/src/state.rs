use crate::auth::GameProfile;
use crate::settings::{self, Settings};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::Mutex;

/// Which account launched a currently-running instance, and its process id
/// (see `AppState::running_instances`) - lets the UI show who's playing each
/// instance, and lets `stop_instance` find the right pid to kill.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningInstance {
    pub pid: u32,
    pub account_uuid: String,
    pub account_username: String,
}

pub struct AppState {
    pub data_dir: PathBuf,
    pub http: reqwest::Client,
    pub settings: Mutex<Settings>,
    /// The signed-in profile for this launch of the app. Deliberately not
    /// persisted to disk: offline profiles carry no real credential, and
    /// Microsoft access tokens are short-lived, so re-authenticating each
    /// session avoids stashing a bearer token in plaintext on disk.
    pub active_profile: Mutex<Option<GameProfile>>,
    /// Each instance currently running a game, if any - tracked by pid
    /// rather than holding onto the `Child` handle itself, so a
    /// `stop_instance` call can request termination without fighting the
    /// long-running `child.wait()` already in progress for the same process.
    /// Also used to block launching an instance that's already running (see
    /// `do_launch`) - two accounts (or the same one twice) sharing one
    /// instance's game directory at once risks corrupting its world saves.
    pub running_instances: Mutex<HashMap<String, RunningInstance>>,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> Self {
        let settings = settings::load(&data_dir.join("settings.json"));
        Self {
            data_dir,
            http: reqwest::Client::builder()
                .user_agent("mint-launcher/0.1.0")
                .build()
                .expect("failed to build http client"),
            settings: Mutex::new(settings),
            active_profile: Mutex::new(None),
            running_instances: Mutex::new(HashMap::new()),
        }
    }

    pub fn settings_path(&self) -> PathBuf {
        self.data_dir.join("settings.json")
    }

    pub fn instances_dir(&self) -> PathBuf {
        self.data_dir.join("instances")
    }

    pub fn versions_dir(&self) -> PathBuf {
        self.data_dir.join("versions")
    }

    pub fn libraries_dir(&self) -> PathBuf {
        self.data_dir.join("libraries")
    }

    pub fn assets_dir(&self) -> PathBuf {
        self.data_dir.join("assets")
    }

    /// Where auto-downloaded Java runtimes are cached, one subfolder per
    /// major version (see `minecraft::java::ensure_java`) - shared across
    /// every instance that needs that version, downloaded once.
    pub fn java_dir(&self) -> PathBuf {
        self.data_dir.join("java")
    }
}
