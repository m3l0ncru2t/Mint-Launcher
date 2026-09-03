use crate::auth::GameProfile;
use crate::settings::{self, Settings};
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
