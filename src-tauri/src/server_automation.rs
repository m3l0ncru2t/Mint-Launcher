//! Scheduled restarts and auto-restart-after-crash for server instances.
//! Both act through the normal launch/restart paths, so a scheduled restart
//! still gives players the in-game countdown (and can be canceled), and an
//! auto-restart shows up in the UI exactly like pressing Start.

use crate::commands;
use crate::instance::{self, Instance, InstanceKind, RestartScheduleMode};
use crate::state::AppState;
use std::time::{Duration, Instant, SystemTime};
use tauri::{AppHandle, Manager};

/// Crash-loop guard: after this many auto-restarts within `CRASH_WINDOW`,
/// stop trying - a server that dies on boot shouldn't restart forever.
const MAX_AUTO_RESTARTS: usize = 3;
const CRASH_WINDOW: Duration = Duration::from_secs(15 * 60);
const RESTART_DELAY: Duration = Duration::from_secs(10);
/// A daily time only counts if Mint notices within this long after it -
/// opening the launcher at 14:00 mustn't fire a missed 05:00 restart.
const DAILY_GRACE_MINUTES: i64 = 3;

fn log(state: &AppState, action: &str, detail: &str) {
    crate::admin_log::record(&state.data_dir, "Mint", "", action, detail);
}

/// Records that a server is being stopped on purpose, so its exit isn't
/// treated as a crash.
pub async fn mark_stop_requested(state: &AppState, instance_id: &str) {
    state.stop_requested.lock().await.insert(instance_id.to_string());
}

/// Called whenever a server process ends - by the launch path that started
/// it (with its exit code), or by the dead-process watcher for a server Mint
/// only adopted. Decides whether to auto-restart and, if so, schedules it.
/// Returns a note to add to the "server stopped" message, if any.
pub async fn handle_exit(app: &AppHandle, inst: &Instance, crashed: bool) -> Option<String> {
    let state = app.state::<AppState>();
    let deliberate = state.stop_requested.lock().await.remove(&inst.id);
    if deliberate || !crashed || inst.kind != InstanceKind::Server || !inst.auto_restart_on_crash {
        return None;
    }

    let now = Instant::now();
    {
        let mut history = state.auto_restart_history.lock().await;
        let recent = history.entry(inst.id.clone()).or_default();
        recent.retain(|t| now.duration_since(*t) < CRASH_WINDOW);
        // The launch path and the dead-process watcher can both see the same
        // exit - only one relaunch, or there'd be two copies of the server.
        if recent.last().is_some_and(|t| now.duration_since(*t) < Duration::from_secs(30)) {
            return None;
        }
        if recent.len() >= MAX_AUTO_RESTARTS {
            log(&state, "Auto-restart stopped", &format!("{} crashed {} times in 15 minutes", inst.name, recent.len() + 1));
            return Some(format!(
                " - auto-restart gave up after {MAX_AUTO_RESTARTS} crashes in 15 minutes; fix the cause, then start it again"
            ));
        }
        recent.push(now);
    }

    log(&state, "Auto-restarted after crash", &inst.name);
    let app = app.clone();
    let id = inst.id.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(RESTART_DELAY).await;
        let state = app.state::<AppState>();
        // Someone may have started it by hand during the delay.
        if state.running_instances.lock().await.contains_key(&id) {
            return;
        }
        relaunch(app.clone(), id).await;
    });
    Some(format!(" - restarting automatically in {} seconds", RESTART_DELAY.as_secs()))
}

/// Boxed so the launch path (which calls `handle_exit`, which calls this)
/// isn't a recursive future type - that cycle stops the compiler proving
/// it's `Send`.
fn relaunch(app: AppHandle, id: String) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    Box::pin(async move {
        let _ = commands::launch::launch_instance(app.clone(), app.state::<AppState>(), id, None).await;
    })
}

/// For a server whose process vanished without Mint holding its exit code
/// (one adopted after Mint restarted): a crash if the game wrote a crash
/// report recently, or if its log doesn't end with a normal shutdown.
pub fn looks_like_crash(game_dir: &std::path::Path) -> bool {
    let recently = SystemTime::now() - Duration::from_secs(5 * 60);
    if crate::minecraft::exit_info::crashed_since(game_dir, recently) {
        return true;
    }
    let tail = commands::launch::tail_lines(&game_dir.join("logs").join("latest.log"), 16 * 1024);
    !tail.contains("Stopping server") && !tail.contains("All dimensions are saved")
}

/// Runs forever: every 20s, restarts any running server whose schedule is
/// due. Restarts go through `restart_instance` (countdown, cancelable).
pub async fn run_scheduler(app: AppHandle) {
    loop {
        tokio::time::sleep(Duration::from_secs(20)).await;
        let state = app.state::<AppState>();
        let instances_dir = state.instances_dir();
        let Ok(instances) =
            tauri::async_runtime::spawn_blocking(move || instance::list_instances(&instances_dir)).await
        else {
            continue;
        };
        let Ok(instances) = instances else { continue };
        let now = chrono::Local::now();

        for inst in instances {
            let schedule = &inst.restart_schedule;
            if inst.kind != InstanceKind::Server || !schedule.enabled {
                continue;
            }
            if !state.running_instances.lock().await.contains_key(&inst.id) {
                continue;
            }
            // A countdown is already running (manual restart, or ours).
            if state.restart_cancellations.lock().await.contains_key(&inst.id) {
                continue;
            }
            let last = state.schedule_last_fired.lock().await.get(&inst.id).copied();

            let due = match schedule.mode {
                RestartScheduleMode::Daily => schedule.times.iter().filter_map(|t| instance::parse_hhmm(t)).any(|mins| {
                    let Some(target) = now
                        .date_naive()
                        .and_hms_opt(mins / 60, mins % 60, 0)
                        .and_then(|t| t.and_local_timezone(chrono::Local).earliest())
                    else {
                        return false;
                    };
                    let since = now.signed_duration_since(target).num_minutes();
                    (0..DAILY_GRACE_MINUTES).contains(&since) && last.is_none_or(|l| l < target)
                }),
                RestartScheduleMode::Interval => {
                    let Some(started) = commands::launch::instance_start_time(&state, &inst.id).await else {
                        continue;
                    };
                    let interval_secs = u64::from(schedule.interval_hours.max(1)) * 3600;
                    let due_at = started + interval_secs;
                    let now_secs = now.timestamp().max(0) as u64;
                    // Fires once per run of the server - canceling the
                    // countdown means "not this time", not "ask again soon".
                    now_secs >= due_at && last.is_none_or(|l| (l.timestamp().max(0) as u64) < due_at)
                }
            };
            if !due {
                continue;
            }

            state.schedule_last_fired.lock().await.insert(inst.id.clone(), now);
            log(&state, "Scheduled restart", &inst.name);
            let app = app.clone();
            let id = inst.id.clone();
            tauri::async_runtime::spawn(async move {
                let _ = commands::launch::restart_instance(app.clone(), app.state::<AppState>(), id).await;
            });
        }
    }
}
