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
use tauri::{Emitter, State};
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
/// console (see `send_instance_command`) and waits up to 15s for it to exit
/// on its own before falling back to a forceful kill - abrupt termination
/// risks world corruption on a server far more than it does a client.
#[tauri::command]
pub async fn stop_instance(app: tauri::AppHandle, state: State<'_, AppState>, instance_id: String) -> Result<(), String> {
    let pid = state.running_instances.lock().await.get(&instance_id).map(|r| r.pid);
    let pid = pid.ok_or_else(|| "This instance isn't running".to_string())?;

    let is_server = instance::get_instance(&state.instances_dir(), &instance_id)
        .ok()
        .flatten()
        .is_some_and(|inst| inst.kind == InstanceKind::Server);

    if is_server {
        let stdin = state.instance_stdins.lock().await.get(&instance_id).cloned();
        if let Some(stdin) = stdin {
            let sent = {
                let mut stdin = stdin.lock().await;
                stdin.write_all(b"stop\n").await.is_ok() && stdin.flush().await.is_ok()
            };
            if sent {
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

/// Writes a line to a running server instance's console (its stdin) - the
/// same mechanism `stop_instance` uses to send a graceful `stop`, exposed
/// directly so the UI can offer a free-form server console input.
#[tauri::command]
pub async fn send_instance_command(state: State<'_, AppState>, instance_id: String, command: String) -> Result<(), String> {
    let stdin = state
        .instance_stdins
        .lock()
        .await
        .get(&instance_id)
        .cloned()
        .ok_or_else(|| "This instance isn't a running server".to_string())?;
    let mut stdin = stdin.lock().await;
    stdin.write_all(command.as_bytes()).await.map_err(|e| e.to_string())?;
    stdin.write_all(b"\n").await.map_err(|e| e.to_string())?;
    stdin.flush().await.map_err(|e| e.to_string())?;
    Ok(())
}

/// Lets the UI show a "running" badge (and which account) for every
/// instance, not just the currently-selected one.
#[tauri::command]
pub async fn list_running_instances(
    state: State<'_, AppState>,
) -> Result<HashMap<String, RunningInstance>, String> {
    Ok(state.running_instances.lock().await.clone())
}

#[cfg(unix)]
fn kill_process(pid: u32) -> std::io::Result<()> {
    std::process::Command::new("kill").arg("-KILL").arg(pid.to_string()).status()?;
    Ok(())
}

#[cfg(windows)]
fn kill_process(pid: u32) -> std::io::Result<()> {
    std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F", "/T"])
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

    let detail = match inst.loader {
        ModLoader::Vanilla => detail,
        ModLoader::Fabric => {
            let loader_version = inst
                .loader_version
                .clone()
                .ok_or_else(|| anyhow::anyhow!("This instance has no Fabric loader version set"))?;
            fabric::apply_server_loader(&state.http, detail, &inst.version_id, &loader_version).await?
        }
        ModLoader::Forge | ModLoader::Quilt => {
            anyhow::bail!("{:?} servers aren't supported yet", inst.loader);
        }
    };

    let java_path = java::ensure_java(app, state, instance_id, &detail).await?;

    let server_jar = download::download_server_jar(app, state, instance_id, &detail).await?;
    let (mut classpath, _native_jars) =
        download::download_libraries(app, state, instance_id, &detail.libraries).await?;
    classpath.push(server_jar);

    let game_dir = inst.game_dir(&state.instances_dir());
    std::fs::create_dir_all(&game_dir)?;
    // Only ever written here, right before launch - never inferred at import
    // time - so this always reflects the explicit EULA checkbox the user
    // already had to check to get this far.
    std::fs::write(game_dir.join("eula.txt"), "eula=true\n")?;

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
        &classpath,
        &detail.main_class,
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
