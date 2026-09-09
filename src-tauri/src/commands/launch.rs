use crate::accounts;
use crate::auth::GameProfile;
use crate::commands::auth::persist_account;
use crate::instance::{self, InstanceKind, ModLoader};
use crate::minecraft::download;
use crate::minecraft::fabric;
use crate::minecraft::java;
use crate::minecraft::launch::{self as mc_launch, LaunchContext};
use crate::minecraft::server_launch;
use crate::msa;
use crate::state::{AppState, RunningInstance};
use std::collections::HashMap;
use tauri::{Emitter, Manager, State};
use tokio::io::AsyncWriteExt;

#[tauri::command]
pub async fn launch_instance(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    server_address: Option<String>,
) -> Result<i32, String> {
    let result = do_launch(&app, &state, &instance_id, server_address.as_deref()).await;

    if let Err(e) = &result {
        let _ = app.emit(
            "launch-progress",
            download::DownloadProgress {
                instance_id: instance_id.clone(),
                stage: "error".to_string(),
                message: e.to_string(),
                current: 0,
                total: 0,
            },
        );
    }

    result.map_err(|e| e.to_string())
}

/// For a client instance, force-kills the game immediately (unchanged
/// behavior - a client autosaves constantly, no graceful-shutdown need). For
/// a server instance, tries a graceful shutdown first: writes `stop` to its
/// console (see `write_console_line`) and waits up to 15s for it to exit
/// on its own before falling back to a forceful kill - abrupt termination
/// risks world corruption on a server far more than it does a client.
#[tauri::command]
pub async fn stop_instance(app: tauri::AppHandle, state: State<'_, AppState>, instance_id: String) -> Result<(), String> {
    let pid = state.running_instances.lock().await.get(&instance_id).map(|r| r.pid);
    let pid = pid.ok_or_else(|| "This instance isn't running".to_string())?;

    let inst = instance::get_instance(&state.instances_dir(), &instance_id).ok().flatten();
    if let Some(inst) = &inst {
        if inst.kind == InstanceKind::Server {
            let game_dir = inst.game_dir(&state.instances_dir());
            if write_console_line(&state, &game_dir, &instance_id, "stop").await.is_ok() {
                for _ in 0..30 {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    if !state.running_instances.lock().await.contains_key(&instance_id) {
                        // spawn_and_stream_server's own wait()-completion
                        // cleanup already removed/persisted/emitted this.
                        return Ok(());
                    }
                }
            }
        }
    }

    force_kill(&app, &state, &instance_id, pid).await
}

/// Writes one line to a running server's console - prefers the live stdin
/// pipe this Mint process itself is holding (lowest latency, and its output
/// shows up in the already-tailed log same as always), falling back to RCON
/// (see `minecraft::rcon`) when there's no such pipe, e.g. for an instance
/// only adopted after Mint itself restarted (see `minecraft::
/// server_properties::ensure_rcon_enabled` for why RCON survives that when
/// stdin can't). Errs only when neither path is available.
async fn write_console_line(
    state: &AppState,
    game_dir: &std::path::Path,
    instance_id: &str,
    line: &str,
) -> Result<(), String> {
    let stdin = state.instance_stdins.lock().await.get(instance_id).cloned();
    if let Some(stdin) = stdin {
        let mut stdin = stdin.lock().await;
        let sent = stdin.write_all(line.as_bytes()).await.is_ok()
            && stdin.write_all(b"\n").await.is_ok()
            && stdin.flush().await.is_ok();
        if sent {
            return Ok(());
        }
    }
    rcon_execute(game_dir, line)
        .await
        .map(|_| ())
        .map_err(|_| "Console access isn't available for this server".to_string())
}

/// Broadcasts a `say` countdown (60s, 30s, 10s, 5s) to any connected players
/// before the actual stop+relaunch, so a restart doesn't just drop everyone
/// with no warning. Uses `write_console_line`, so this reaches players even
/// for an instance Mint only adopted after restarting, as long as RCON is
/// available for it; silently does nothing if neither console path is.
async fn broadcast_restart_countdown(state: &AppState, instance_id: &str) {
    let game_dir = instance::get_instance(&state.instances_dir(), instance_id)
        .ok()
        .flatten()
        .map(|inst| inst.game_dir(&state.instances_dir()));
    // The last step announces the restart itself rather than "in 5 seconds"
    // - by the time anyone reads it, it's already happening.
    const STEPS: [(&str, u64); 4] = [
        ("Server restarting in 60 seconds", 30),
        ("Server restarting in 30 seconds", 20),
        ("Server restarting in 10 seconds", 5),
        ("Server restarting", 5),
    ];
    for (message, wait_after) in STEPS {
        if let Some(game_dir) = &game_dir {
            let _ = write_console_line(state, game_dir, instance_id, &format!("say {message}")).await;
        }
        tokio::time::sleep(std::time::Duration::from_secs(wait_after)).await;
    }
}

/// Restart, with warning: if the instance is currently running, broadcasts
/// the countdown above before stopping it, then launches it again either
/// way. Used by both the local Restart button and the remote `/restart`
/// endpoint, so an admin restarting someone else's server gives the same
/// heads-up a local restart does.
#[tauri::command]
pub async fn restart_instance(app: tauri::AppHandle, state: State<'_, AppState>, instance_id: String) -> Result<(), String> {
    let running = state.running_instances.lock().await.contains_key(&instance_id);
    if running {
        broadcast_restart_countdown(&state, &instance_id).await;
        let _ = stop_instance(app.clone(), app.state::<AppState>(), instance_id.clone()).await;
    }
    launch_instance(app.clone(), app.state::<AppState>(), instance_id, None).await?;
    Ok(())
}

/// Always an immediate forceful kill, regardless of instance kind -
/// exposed in the UI as "Kill" for servers (redundant with Stop for
/// clients, so not surfaced there).
#[tauri::command]
pub async fn kill_instance(app: tauri::AppHandle, state: State<'_, AppState>, instance_id: String) -> Result<(), String> {
    let pid = state.running_instances.lock().await.get(&instance_id).map(|r| r.pid);
    let pid = pid.ok_or_else(|| "This instance isn't running".to_string())?;
    force_kill(&app, &state, &instance_id, pid).await
}

/// Shared by `stop_instance`'s forceful fallback and `kill_instance`. Clears
/// `running_instances`/`instance_stdins` (and persists/emits that) itself
/// instead of waiting for `spawn_and_stream`'s own `child.wait()` to notice
/// the exit: an entry restored by `reconcile_running_instances` after a
/// relaunch has no such task running in *this* process to ever notice, so
/// without this a killed-but-restored instance would keep showing as
/// running until the next full restart. Harmless if `spawn_and_stream` also
/// does its own (redundant) removal moments later for a normal, non-restored
/// kill.
async fn force_kill(app: &tauri::AppHandle, state: &AppState, instance_id: &str, pid: u32) -> Result<(), String> {
    kill_process(pid).map_err(|e| e.to_string())?;

    state.running_instances.lock().await.remove(instance_id);
    state.instance_stdins.lock().await.remove(instance_id);
    state.persist_running_instances().await;
    let _ = app.emit(
        "instance-running-changed",
        serde_json::json!({ "instanceId": instance_id, "running": false }),
    );
    Ok(())
}

/// Writes a line to a running server instance's console via
/// `write_console_line` - the same mechanism `stop_instance` uses to send a
/// graceful `stop`, exposed directly so the UI can offer a free-form server
/// console input and the kick/ban/op/whitelist actions in `PlayersPanel`.
#[tauri::command]
pub async fn send_instance_command(state: State<'_, AppState>, instance_id: String, command: String) -> Result<(), String> {
    let inst = instance::get_instance(&state.instances_dir(), &instance_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Instance not found".to_string())?;
    let game_dir = inst.game_dir(&state.instances_dir());
    write_console_line(&state, &game_dir, &instance_id, &command).await
}

/// Reads a server's RCON port/password out of its `server.properties`, if
/// it's enabled there - `None` if the server predates `ensure_rcon_enabled`
/// and hasn't been restarted since, or a user has since disabled it.
fn rcon_creds(game_dir: &std::path::Path) -> Option<(u16, String)> {
    let props = crate::minecraft::server_properties::read_properties(game_dir);
    if props.get("enable-rcon").map(String::as_str) != Some("true") {
        return None;
    }
    let port = props.get("rcon.port")?.parse().ok()?;
    Some((port, props.get("rcon.password")?.clone()))
}

async fn rcon_execute(game_dir: &std::path::Path, command: &str) -> Result<String, String> {
    let (port, password) =
        rcon_creds(game_dir).ok_or_else(|| "RCON isn't available for this server".to_string())?;
    crate::minecraft::rcon::execute(port, &password, command).await.map_err(|e| e.to_string())
}

/// Reads the last `max_bytes` of a file as (possibly lossily-decoded) text -
/// good enough for scanning console output for a known ASCII pattern, where
/// a mangled multi-byte character right at the truncation point (if any)
/// can't land inside the pattern itself.
pub(crate) fn tail_lines(path: &std::path::Path, max_bytes: usize) -> String {
    let Ok(data) = std::fs::read(path) else {
        return String::new();
    };
    let start = data.len().saturating_sub(max_bytes);
    String::from_utf8_lossy(&data[start..]).into_owned()
}

/// Parses vanilla's own response to the `list` command - `There are X of a
/// max of Y players online: a, b, c` (still present, just with nothing after
/// the colon, when nobody's on) - out of a chunk of console output. Scans
/// for the *last* match so a stale response sitting further up the tail from
/// an earlier poll isn't picked over this poll's fresh one.
fn parse_list_response(text: &str) -> Option<(u32, u32, Vec<String>)> {
    let mut result = None;
    for line in text.lines() {
        let Some(after_there_are) = line.split_once("There are").map(|(_, rest)| rest) else {
            continue;
        };
        let Some((count_str, rest)) = after_there_are.split_once("of a max of") else {
            continue;
        };
        let Some((max_str, rest)) = rest.split_once("players online") else {
            continue;
        };
        let Some(count) = count_str.trim().parse::<u32>().ok() else {
            continue;
        };
        let Some(max) = max_str.trim().parse::<u32>().ok() else {
            continue;
        };
        let names = rest
            .trim_start_matches(':')
            .trim()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        result = Some((count, max, names));
    }
    result
}

/// The network-ping-based player count/list (`ping_server`) doesn't work at
/// all for a server sitting behind a proxy-only protection mod like
/// TCPShield, which rejects any direct connection to the game port -
/// including Mint's own status ping - as unauthorized. Sending the server's
/// own `list` command through its console and reading the response back off
/// its log (or, if there's no live stdin, over RCON - see `rcon_execute`)
/// sidesteps the network entirely, so it works regardless of
/// firewalling/proxying in front of the port. `PlayersPanel`/`InstanceDetail`
/// fall back to `ping_server` only if both of these are unavailable.
async fn list_via_stdin(
    state: &AppState,
    game_dir: &std::path::Path,
    instance_id: &str,
) -> Option<(u32, u32, Vec<String>)> {
    let stdin = state.instance_stdins.lock().await.get(instance_id).cloned()?;
    {
        let mut stdin = stdin.lock().await;
        stdin.write_all(b"list\n").await.ok()?;
        stdin.flush().await.ok()?;
    }
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;
    let log_path = game_dir.join("logs").join("latest.log");
    parse_list_response(&tail_lines(&log_path, 8192))
}

async fn list_via_rcon(game_dir: &std::path::Path) -> Option<(u32, u32, Vec<String>)> {
    let response = rcon_execute(game_dir, "list").await.ok()?;
    parse_list_response(&response)
}

#[tauri::command]
pub async fn list_online_players(
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<crate::minecraft::server_ping::ServerStatus, String> {
    let inst = instance::get_instance(&state.instances_dir(), &instance_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Instance not found".to_string())?;
    let game_dir = inst.game_dir(&state.instances_dir());

    let (online, max, names) = match list_via_stdin(&state, &game_dir, &instance_id).await {
        Some(v) => v,
        None => list_via_rcon(&game_dir)
            .await
            .ok_or_else(|| "Console access isn't available for this server".to_string())?,
    };

    let mut sample = Vec::with_capacity(names.len());
    for name in names {
        let cached = state.uuid_cache.lock().await.get(&name).cloned();
        let id = match cached {
            Some(id) => id,
            None => {
                match crate::minecraft::server_admin::lookup_uuid(&state.http, &name).await {
                    Ok((uuid, _)) => {
                        state.uuid_cache.lock().await.insert(name.clone(), uuid.clone());
                        uuid
                    }
                    // Still show the player even if the lookup fails (rate
                    // limited, offline, or just not a real Mojang account) -
                    // `PlayerAvatar` already degrades to a plain initial when
                    // it can't resolve a skin for whatever id it's given.
                    Err(_) => format!("offline-{name}"),
                }
            }
        };
        sample.push(crate::minecraft::server_ping::PlayerSample { id, name });
    }

    Ok(crate::minecraft::server_ping::ServerStatus {
        motd: Vec::new(),
        online: Some(online),
        max: Some(max),
        favicon: None,
        sample,
    })
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TpsInfo {
    pub tps: f64,
    pub mspt: f64,
}

/// Parses vanilla's response to `time query gametime` out of a chunk of
/// console output, taking the *last* match for the same stale-vs-fresh
/// reason `parse_list_response` does. Deliberately just looks for `time is`
/// and then reads off the run of digits after it, rather than matching a
/// full fixed sentence - the exact phrasing has changed across versions
/// ("The time is N" vs. "The game time is N tick(s)", seen live on this
/// version), and this way it survives another rewording without needing to
/// know about it in advance.
fn parse_gametime_response(text: &str) -> Option<i64> {
    let mut result = None;
    for line in text.lines() {
        let Some(idx) = line.find("time is") else { continue };
        let rest = line[idx + "time is".len()..].trim_start();
        let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(tick) = digits.parse::<i64>() {
            result = Some(tick);
        }
    }
    result
}

async fn gametime_via_stdin(state: &AppState, game_dir: &std::path::Path, instance_id: &str) -> Option<i64> {
    let stdin = state.instance_stdins.lock().await.get(instance_id).cloned()?;
    {
        let mut stdin = stdin.lock().await;
        stdin.write_all(b"time query gametime\n").await.ok()?;
        stdin.flush().await.ok()?;
    }
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    // 8KB, matching `list_via_stdin` - a busy modded server can log a lot in
    // the ~400ms between sending the command and reading this back, and a
    // too-small window risks the response having already scrolled past it.
    let log_path = game_dir.join("logs").join("latest.log");
    parse_gametime_response(&tail_lines(&log_path, 8192))
}

async fn gametime_via_rcon(game_dir: &std::path::Path) -> Option<i64> {
    let response = rcon_execute(game_dir, "time query gametime").await.ok()?;
    parse_gametime_response(&response)
}

/// TPS/MSPT aren't exposed by any vanilla command directly, but the world's
/// own tick counter is - `time query gametime` (a completely vanilla command,
/// no profiler mod like spark needed) reports how many ticks have ever
/// elapsed. Comparing that against a previous sample and the real time
/// between them gives an honest tick rate/tick duration for any server this
/// launcher has console access to, over stdin or RCON (see `rcon_execute`).
/// Returns `None` on the first call for a given instance (and after nothing
/// changed, e.g. an empty server some packs pause ticking on) since there's
/// no prior sample yet to diff against - the same "first call reads nothing
/// meaningful" shape `get_process_stats` already has for CPU%.
#[tauri::command]
pub async fn get_server_tps(state: State<'_, AppState>, instance_id: String) -> Result<Option<TpsInfo>, String> {
    let inst = instance::get_instance(&state.instances_dir(), &instance_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Instance not found".to_string())?;
    let game_dir = inst.game_dir(&state.instances_dir());

    let tick = match gametime_via_stdin(&state, &game_dir, &instance_id).await {
        Some(t) => t,
        None => gametime_via_rcon(&game_dir)
            .await
            .ok_or_else(|| "Console access isn't available for this server".to_string())?,
    };
    let now = std::time::Instant::now();

    let prev = state.tps_samples.lock().await.insert(instance_id, (now, tick));
    let Some((prev_instant, prev_tick)) = prev else {
        return Ok(None);
    };

    let dt_ticks = tick - prev_tick;
    let dt_secs = now.duration_since(prev_instant).as_secs_f64();
    if dt_ticks <= 0 || dt_secs <= 0.0 {
        return Ok(None);
    }

    Ok(Some(TpsInfo {
        tps: (dt_ticks as f64 / dt_secs).min(20.0),
        mspt: (dt_secs * 1000.0) / dt_ticks as f64,
    }))
}

/// Lets the UI show a "running" badge (and which account) for every
/// instance, not just the currently-selected one.
#[tauri::command]
pub async fn list_running_instances(
    state: State<'_, AppState>,
) -> Result<HashMap<String, RunningInstance>, String> {
    Ok(state.running_instances.lock().await.clone())
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessStats {
    pub cpu_percent: f32,
    pub memory_mb: u64,
    /// Whole-machine memory usage, not just this process's - shown alongside
    /// the process's own figure so it's clear whether a server that feels
    /// slow is actually memory-starved at the system level, not just relative
    /// to its own `-Xmx`.
    pub system_used_memory_mb: u64,
    pub system_total_memory_mb: u64,
}

/// Live CPU/memory for a running instance's process - meant to be polled
/// (e.g. every couple of seconds) while a server instance is running. CPU%
/// only becomes meaningful after the *second* call for a given pid, since
/// `sysinfo` derives it from the delta between two refreshes of the same
/// `System` (see `AppState::process_stats`) - the first call after a fresh
/// launch reads 0%, same as any other process monitor's first sample.
#[tauri::command]
pub async fn get_process_stats(state: State<'_, AppState>, pid: u32) -> Result<ProcessStats, String> {
    let mut sys = state.process_stats.lock().await;
    let sys_pid = sysinfo::Pid::from_u32(pid);
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[sys_pid]), true);
    sys.refresh_memory();
    let process = sys.process(sys_pid).ok_or_else(|| "That process isn't running".to_string())?;
    Ok(ProcessStats {
        cpu_percent: process.cpu_usage(),
        memory_mb: process.memory() / (1024 * 1024),
        system_used_memory_mb: sys.used_memory() / (1024 * 1024),
        system_total_memory_mb: sys.total_memory() / (1024 * 1024),
    })
}

/// Looks for a Java process whose working directory matches this server
/// instance's game folder and, if found, adopts it into `running_instances`
/// exactly as if Mint had launched it itself - so a server already started
/// outside Mint (by hand, by another panel, before Mint was even open) shows
/// up as running instead of looking stopped forever. Cheap no-op if the
/// instance is already tracked as running. Returns whether it's now known to
/// be running, either way.
///
/// There's no stdin for an adopted server (Mint never spawned it, so it
/// never piped one) - `stop_instance` already falls back to a forceful kill
/// when no stdin is on file, so Stop/Restart still work, just without the
/// graceful `stop`-command shutdown a Mint-launched server gets. That
/// forceful kill itself still needs the OS to allow signaling the process,
/// though - for the cross-user case below, it generally won't (see there).
#[tauri::command]
pub async fn detect_running_server(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
) -> Result<bool, String> {
    if state.running_instances.lock().await.contains_key(&instance_id) {
        return Ok(true);
    }
    let inst = instance::get_instance(&state.instances_dir(), &instance_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Instance not found".to_string())?;
    if inst.kind != InstanceKind::Server {
        return Ok(false);
    }

    let game_dir = inst.game_dir(&state.instances_dir());
    let target_dir = std::fs::canonicalize(&game_dir).unwrap_or(game_dir);

    let (exact_match, unreadable_cwd_candidates) = {
        let mut sys = state.process_stats.lock().await;
        sys.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::All,
            true,
            sysinfo::ProcessRefreshKind::nothing().with_cwd(sysinfo::UpdateKind::Always),
        );
        let mut exact_match = None;
        let mut unreadable = Vec::new();
        for (pid, process) in sys.processes() {
            if !process.name().to_string_lossy().to_lowercase().contains("java") {
                continue;
            }
            match process.cwd() {
                Some(cwd) if cwd == target_dir => exact_match = Some(pid.as_u32()),
                Some(_) => {}
                // `cwd()` reads `/proc/<pid>/cwd`, which the kernel only
                // allows for a process with the *same* uid as the target (or
                // root/CAP_SYS_PTRACE) - a Linux group membership like being
                // in a PufferPanel service's group doesn't count, even though
                // it's plenty to read that service's actual files. A java
                // process this launcher can see in the process list at all
                // but can't read the cwd of is kept as a fallback candidate
                // below instead of being dismissed outright.
                None => unreadable.push(pid.as_u32()),
            }
        }
        (exact_match, unreadable)
    };

    let found_pid = if exact_match.is_some() {
        exact_match
    } else if let [only_candidate] = unreadable_cwd_candidates[..] {
        // Can't confirm that candidate's cwd, so confirm the *server*
        // itself is what's listening instead: a live Server List Ping
        // response on this instance's own configured port needs no special
        // permissions at all, and is solid enough evidence to attribute it
        // to this instance now that there's exactly one otherwise-unplaceable
        // java process to explain it - e.g. a server another OS user's
        // service (PufferPanel, most notably) launched.
        let port = crate::minecraft::server_properties::read_properties(&target_dir)
            .get("server-port")
            .cloned()
            .unwrap_or_else(|| "25565".to_string());
        let responded = crate::minecraft::server_ping::ping(&format!("localhost:{port}")).await.is_ok();
        responded.then_some(only_candidate)
    } else {
        None
    };

    let Some(pid) = found_pid else {
        return Ok(false);
    };

    state
        .running_instances
        .lock()
        .await
        .insert(instance_id.clone(), RunningInstance { pid, account_uuid: None, account_username: None });
    state.persist_running_instances().await;
    let _ = app.emit(
        "instance-running-changed",
        serde_json::json!({ "instanceId": instance_id, "running": true, "pid": pid }),
    );
    Ok(true)
}

#[cfg(unix)]
fn kill_process(pid: u32) -> std::io::Result<()> {
    std::process::Command::new("kill").arg("-KILL").arg(pid.to_string()).status()?;
    Ok(())
}

#[cfg(windows)]
fn kill_process(pid: u32) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F", "/T"])
        .creation_flags(CREATE_NO_WINDOW)
        .status()?;
    Ok(())
}

/// An instance bound to a specific saved account (via its settings) always
/// launches as that account, silently refreshing its session, regardless of
/// whichever account is currently active - this is what lets different
/// instances run under different Microsoft accounts. Instances with no
/// binding fall back to the currently signed-in profile, as before.
async fn resolve_launch_profile(state: &AppState, inst: &instance::Instance) -> anyhow::Result<GameProfile> {
    let Some(account_id) = &inst.account_id else {
        return state
            .active_profile
            .lock()
            .await
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Sign in before launching an instance"));
    };

    let account = accounts::load(&state.data_dir)
        .into_iter()
        .find(|a| &a.id == account_id)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "This instance's account is no longer saved - open its settings and pick another."
            )
        })?;

    let result = msa::refresh(&state.http, &account.client_id, &account.refresh_token)
        .await
        .map_err(|e| {
            anyhow::anyhow!("This instance's account needs to be signed in again ({e}) - open its settings and reselect it.")
        })?;

    persist_account(state, account_id, &account.client_id, &result).await;
    Ok(result.profile)
}

async fn do_launch(
    app: &tauri::AppHandle,
    state: &AppState,
    instance_id: &str,
    server_address: Option<&str>,
) -> anyhow::Result<i32> {
    let inst = instance::get_instance(&state.instances_dir(), instance_id)?
        .ok_or_else(|| anyhow::anyhow!("Instance not found"))?;

    // Two accounts (or the same one twice) launching the same instance at
    // once would have both JVMs writing to the same world saves and config
    // at the same time - a real corruption risk, not just a UI quirk. Same
    // concern applies to a server instance, just without a second "account"
    // in the picture - it's still one game directory two processes could
    // write to at once.
    if let Some(running) = state.running_instances.lock().await.get(instance_id) {
        match &running.account_username {
            Some(name) => anyhow::bail!("This instance is already running as {name}"),
            None => anyhow::bail!("This instance is already running"),
        }
    }

    if inst.kind == InstanceKind::Server {
        return do_launch_server(app, state, &inst).await;
    }

    let profile = resolve_launch_profile(state, &inst).await?;

    let manifest = download::fetch_version_manifest(state).await?;
    let entry = manifest
        .versions
        .iter()
        .find(|v| v.id == inst.version_id)
        .ok_or_else(|| anyhow::anyhow!("Unknown Minecraft version {}", inst.version_id))?;
    let detail = download::fetch_version_detail(state, entry).await?;

    let detail = match inst.loader {
        ModLoader::Vanilla => detail,
        ModLoader::Fabric => {
            let loader_version = inst
                .loader_version
                .clone()
                .ok_or_else(|| anyhow::anyhow!("This instance has no Fabric loader version set"))?;
            fabric::apply_loader(&state.http, detail, &inst.version_id, &loader_version).await?
        }
        ModLoader::Forge | ModLoader::Quilt => {
            anyhow::bail!("{:?} isn't supported yet", inst.loader);
        }
    };

    let java_path = java::ensure_java(app, state, instance_id, &detail).await?;

    download::download_client_jar(app, state, instance_id, &detail).await?;
    let (mut classpath, native_jars) =
        download::download_libraries(app, state, instance_id, &detail.libraries).await?;

    let client_jar = state
        .versions_dir()
        .join(&detail.id)
        .join(format!("{}.jar", detail.id));
    classpath.push(client_jar);

    let natives_dir = inst.natives_dir(&state.instances_dir());
    download::extract_natives(&native_jars, &natives_dir)?;

    download::download_assets(app, state, instance_id, &detail).await?;

    let game_dir = inst.game_dir(&state.instances_dir());
    let assets_dir = state.assets_dir();
    let ctx = LaunchContext {
        detail: &detail,
        classpath: &classpath,
        natives_dir: &natives_dir,
        game_dir: &game_dir,
        assets_dir: &assets_dir,
        profile: &profile,
        quick_play_server: server_address,
    };
    let args = mc_launch::build_command_args(&ctx);

    instance::touch_last_played(&state.instances_dir(), instance_id)?;

    let _ = app.emit(
        "launch-progress",
        download::DownloadProgress {
            instance_id: instance_id.to_string(),
            stage: "launching".to_string(),
            message: "Starting Minecraft".to_string(),
            current: 1,
            total: 1,
        },
    );

    let extra_jvm_args: Vec<String> = inst
        .java_args
        .as_deref()
        .unwrap_or("")
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let exit_code = mc_launch::spawn_and_stream(
        app,
        state,
        instance_id,
        &profile,
        &java_path,
        inst.memory_mb,
        &extra_jvm_args,
        args,
        &game_dir,
    )
    .await?;

    let _ = app.emit(
        "launch-progress",
        download::DownloadProgress {
            instance_id: instance_id.to_string(),
            stage: "exited".to_string(),
            message: format!("Minecraft exited with code {exit_code}"),
            current: 1,
            total: 1,
        },
    );

    Ok(exit_code)
}

/// The server-instance counterpart to `do_launch` - no account to resolve,
/// no client assets/natives to fetch, and a far simpler command line (just
/// `-jar <server jar> nogui`, not the client's argument-template system).
async fn do_launch_server(app: &tauri::AppHandle, state: &AppState, inst: &instance::Instance) -> anyhow::Result<i32> {
    let instance_id = &inst.id;

    if !inst.eula_accepted {
        anyhow::bail!("Accept the Minecraft EULA in this server's settings before launching it");
    }

    let manifest = download::fetch_version_manifest(state).await?;
    let entry = manifest
        .versions
        .iter()
        .find(|v| v.id == inst.version_id)
        .ok_or_else(|| anyhow::anyhow!("Unknown Minecraft version {}", inst.version_id))?;
    let detail = download::fetch_version_detail(state, entry).await?;

    let java_path = java::ensure_java(app, state, instance_id, &detail).await?;

    let game_dir = inst.game_dir(&state.instances_dir());
    std::fs::create_dir_all(&game_dir)?;
    // Only ever written here, right before launch - never inferred at import
    // time - so this always reflects the explicit EULA checkbox the user
    // already had to check to get this far.
    std::fs::write(game_dir.join("eula.txt"), "eula=true\n")?;
    crate::minecraft::server_properties::ensure_rcon_enabled(&game_dir)?;

    // Vanilla and Fabric need genuinely different invocations, not just a
    // different classpath - see `server_launch::ServerJvmTarget`.
    let target = match inst.loader {
        ModLoader::Vanilla => {
            let server_jar = download::download_server_jar(app, state, instance_id, &detail).await?;
            server_launch::ServerJvmTarget::Jar(server_jar)
        }
        ModLoader::Fabric => {
            let loader_version = inst
                .loader_version
                .clone()
                .ok_or_else(|| anyhow::anyhow!("This instance has no Fabric loader version set"))?;
            let detail = fabric::apply_server_loader(&state.http, detail, &inst.version_id, &loader_version).await?;
            let server_jar = download::download_server_jar(app, state, instance_id, &detail).await?;
            let (mut classpath, _native_jars) =
                download::download_libraries(app, state, instance_id, &detail.libraries).await?;
            classpath.push(server_jar);
            server_launch::ServerJvmTarget::Classpath { classpath, main_class: detail.main_class.clone() }
        }
        ModLoader::Forge | ModLoader::Quilt => {
            anyhow::bail!("{:?} servers aren't supported yet", inst.loader);
        }
    };

    instance::touch_last_played(&state.instances_dir(), instance_id)?;

    let _ = app.emit(
        "launch-progress",
        download::DownloadProgress {
            instance_id: instance_id.to_string(),
            stage: "launching".to_string(),
            message: "Starting server".to_string(),
            current: 1,
            total: 1,
        },
    );

    let extra_jvm_args: Vec<String> = inst
        .java_args
        .as_deref()
        .unwrap_or("")
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let exit_code = server_launch::spawn_and_stream_server(
        app,
        state,
        instance_id,
        &java_path,
        inst.memory_mb,
        &extra_jvm_args,
        target,
        &game_dir,
    )
    .await?;

    let _ = app.emit(
        "launch-progress",
        download::DownloadProgress {
            instance_id: instance_id.to_string(),
            stage: "exited".to_string(),
            message: format!("Server exited with code {exit_code}"),
            current: 1,
            total: 1,
        },
    );

    Ok(exit_code)
}
