use crate::accounts;
use crate::auth::GameProfile;
use crate::commands::auth::persist_account;
use crate::instance::{self, ModLoader};
use crate::minecraft::download;
use crate::minecraft::fabric;
use crate::minecraft::launch::{self as mc_launch, LaunchContext};
use crate::msa;
use crate::state::AppState;
use tauri::{Emitter, State};

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

/// Force-kills an instance's running game process, identified only by its
/// pid (recorded by `spawn_and_stream`) - not by holding onto the `Child`
/// handle itself, since that's already tied up in the long-running
/// `child.wait()` for the same launch.
#[tauri::command]
pub async fn stop_instance(state: State<'_, AppState>, instance_id: String) -> Result<(), String> {
    let pid = state.running_pids.lock().await.get(&instance_id).copied();
    let pid = pid.ok_or_else(|| "This instance isn't running".to_string())?;
    kill_process(pid).map_err(|e| e.to_string())
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
    let exit_code =
        mc_launch::spawn_and_stream(app, state, instance_id, inst.memory_mb, &extra_jvm_args, args, &game_dir)
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
