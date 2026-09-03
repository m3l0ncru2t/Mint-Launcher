use crate::auth::GameProfile;
use crate::settings::{self, Settings};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::Mutex;

pub struct AppState {
    pub data_dir: PathBuf,
    pub http: reqwest::Client,
    pub settings: Mutex<Settings>,
    /// The signed-in profile for this launch of the app. Deliberately not
    /// persisted to disk: offline profiles carry no real credential, and
    /// Microsoft access tokens are short-lived, so re-authenticating each
    /// session avoids stashing a bearer token in plaintext on disk.
    pub active_profile: Mutex<Option<GameProfile>>,
    /// The OS process id of each instance's currently-running game, if any -
    /// tracked by pid rather than holding onto the `Child` handle itself, so
    /// a `stop_instance` call can request termination without fighting the
    /// long-running `child.wait()` already in progress for the same process.
    pub running_pids: Mutex<HashMap<String, u32>>,
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
            running_pids: Mutex::new(HashMap::new()),
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
}
