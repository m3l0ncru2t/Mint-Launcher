use crate::auth::GameProfile;
use crate::settings::{self, Settings};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex;

/// Which account launched a currently-running instance, and its process id
/// (see `AppState::running_instances`) - lets the UI show who's playing each
/// instance, and lets `stop_instance` find the right pid to kill. The
/// account fields are `None` for a running *server* instance - there's no
/// account involved in launching one - in which case the UI just shows a
/// plain "Running" instead of "Running as {username}".
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningInstance {
    pub pid: u32,
    #[serde(default)]
    pub account_uuid: Option<String>,
    #[serde(default)]
    pub account_username: Option<String>,
}

/// One admin's proven identity for the remote admin API (see
/// `remote_api.rs`) - created after a successful `joinServer`/`hasJoined`
/// handshake confirms they own the account and it's currently an op on the
/// shared instance. Opaque bearer tokens rather than anything JWT-like since
/// there's nothing that needs to be self-describing here - the session is
/// only ever looked up in `AppState.remote_sessions`, never parsed.
#[derive(Debug, Clone)]
pub struct RemoteSession {
    pub uuid: String,
    pub username: String,
    pub expires_at: std::time::Instant,
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
    /// A running *server* instance's stdin, kept open so console commands
    /// (including a graceful `stop`) can be written to it - see
    /// `minecraft::server_launch`. Deliberately separate from
    /// `running_instances`: a raw pipe handle can't be serialized/cloned, so
    /// it can't live in a map that gets sent wholesale to the frontend, and
    /// it can't survive a relaunch the way a bare pid can (see
    /// `reconcile_running_instances`) - there's simply nothing to reconstruct
    /// it from after this process restarts.
    pub instance_stdins: Mutex<HashMap<String, Arc<Mutex<tokio::process::ChildStdin>>>>,
    /// Kept persistently (rather than a fresh `System` per call) since
    /// `sysinfo` computes a process's CPU% from the delta between two
    /// refreshes of the *same* `System` - a one-shot query would always read
    /// 0%. `commands::launch::get_process_stats` refreshes just the one pid
    /// being polled, not the whole system, on each call.
    pub process_stats: Mutex<sysinfo::System>,
    /// Username -> real Mojang UUID, resolved on demand by
    /// `commands::launch::list_online_players` so a console-derived player
    /// list (which only ever gives back names, never UUIDs) can still show
    /// real avatars. Cached indefinitely per launcher run rather than
    /// re-resolved on every few-second poll, since that would burn through
    /// Mojang's lookup API's rate limit fast with even a handful of players
    /// online.
    pub uuid_cache: Mutex<HashMap<String, String>>,
    /// The last (real time, in-game tick count) sample `commands::launch::
    /// get_server_tps` took for each instance - TPS/MSPT are a rate, so
    /// computing them needs two samples spaced apart in real time. Kept here
    /// rather than returned to the frontend to average client-side, since
    /// the actual elapsed wall-clock time between polls can drift a bit and
    /// this way the math always uses the real measured interval.
    pub tps_samples: Mutex<HashMap<String, (std::time::Instant, i64)>>,
    /// Bearer token -> the admin it belongs to, for the remote admin API
    /// (`remote_api.rs`). Deliberately in-memory only, like `active_profile`
    /// - a restart of this app (or of the machine) just means every admin's
    /// Mint has to redo the join/hasJoined handshake, which is quick and
    /// keeps no long-lived secret on disk.
    pub remote_sessions: Mutex<HashMap<String, RemoteSession>>,
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
            instance_stdins: Mutex::new(HashMap::new()),
            process_stats: Mutex::new(sysinfo::System::new()),
            uuid_cache: Mutex::new(HashMap::new()),
            tps_samples: Mutex::new(HashMap::new()),
            remote_sessions: Mutex::new(HashMap::new()),
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

    /// Runs forever, periodically clearing any tracked-as-running instance
    /// whose pid has actually died. Nothing else would ever notice this for
    /// an instance this same app process didn't itself just launch: a fresh
    /// `spawn_and_stream`/`spawn_and_stream_server` call's own `child.wait()`
    /// task is what normally notices an exit and cleans up, but that task
    /// only exists in the process that made the call - an entry restored by
    /// `reconcile_running_instances` at startup, or adopted later by
    /// `commands::launch::detect_running_server`, has no such task. Without
    /// this, the "Running" badge for one of those would stick around forever
    /// once the real process dies, until the whole app restarts again or the
    /// user manually hits Stop/Kill.
    pub async fn watch_for_dead_instances(app: AppHandle) {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            let state = app.state::<AppState>();
            let dead: Vec<String> = {
                let running = state.running_instances.lock().await;
                running.iter().filter(|(_, r)| !pid_is_alive(r.pid)).map(|(id, _)| id.clone()).collect()
            };
            for instance_id in dead {
                state.running_instances.lock().await.remove(&instance_id);
                state.instance_stdins.lock().await.remove(&instance_id);
                state.persist_running_instances().await;
                let _ = app.emit(
                    "instance-running-changed",
                    serde_json::json!({ "instanceId": instance_id, "running": false }),
                );
            }
        }
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
