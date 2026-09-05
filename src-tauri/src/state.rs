use crate::auth::GameProfile;
use crate::settings::{self, Settings};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tokio::sync::Mutex;

/// Which account launched a currently-running instance, and its process id
/// (see `AppState::running_instances`) - lets the UI show who's playing each
/// instance, and lets `stop_instance` find the right pid to kill.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
        let running_instances = reconcile_running_instances(&data_dir);
        Self {
            data_dir,
            http: reqwest::Client::builder()
                .user_agent("mint-launcher/0.1.0")
                .build()
                .expect("failed to build http client"),
            settings: Mutex::new(settings),
            active_profile: Mutex::new(None),
            running_instances: Mutex::new(running_instances),
        }
    }

    fn running_instances_path(&self) -> PathBuf {
        self.data_dir.join("running_instances.json")
    }

    /// Best-effort snapshot of `running_instances` to disk - called after
    /// every insert/remove (see `minecraft::launch::spawn_and_stream` and
    /// `commands::launch::stop_instance`) so a relaunch (the in-app updater
    /// installing itself, most notably) doesn't leave the UI thinking a game
    /// that's still running in the background isn't - see
    /// `reconcile_running_instances`, which reads this back on startup.
    pub async fn persist_running_instances(&self) {
        let map = self.running_instances.lock().await.clone();
        let _ = save_running_instances(&self.running_instances_path(), &map);
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

fn save_running_instances(path: &Path, map: &HashMap<String, RunningInstance>) -> std::io::Result<()> {
    fs::write(path, serde_json::to_string_pretty(map)?)
}

/// Restores whichever instances were running the last time this data dir's
/// `running_instances.json` was written, keeping only the ones whose pid
/// still actually belongs to a live process - anything that already exited
/// (a normal game close while the launcher itself was mid-relaunch, not just
/// the "game survives an update-triggered relaunch" case this exists for) is
/// silently dropped instead of showing up as a phantom "running" instance
/// forever. The file is rewritten immediately with just the survivors, so a
/// stale entry doesn't linger around to be re-checked on every future
/// startup either.
fn reconcile_running_instances(data_dir: &Path) -> HashMap<String, RunningInstance> {
    let path = data_dir.join("running_instances.json");
    let Ok(data) = fs::read_to_string(&path) else {
        return HashMap::new();
    };
    let Ok(map) = serde_json::from_str::<HashMap<String, RunningInstance>>(&data) else {
        return HashMap::new();
    };
    let alive: HashMap<String, RunningInstance> = map.into_iter().filter(|(_, r)| pid_is_alive(r.pid)).collect();
    let _ = save_running_instances(&path, &alive);
    alive
}

#[cfg(unix)]
fn pid_is_alive(pid: u32) -> bool {
    // Signal 0 does nothing to the target process - it only checks whether
    // sending a real signal to it *would* succeed, i.e. whether it exists.
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn pid_is_alive(pid: u32) -> bool {
    std::process::Command::new("tasklist")
        .args(["/NH", "/FI", &format!("PID eq {pid}")])
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).contains(&pid.to_string()))
        .unwrap_or(false)
}
