use crate::instance::{self, ModLoader};
use crate::minecraft::download;
use crate::minecraft::fabric;
use crate::minecraft::launch::{self as mc_launch, LaunchContext};
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

async fn do_launch(
    app: &tauri::AppHandle,
    state: &AppState,
    instance_id: &str,
    server_address: Option<&str>,
) -> anyhow::Result<i32> {
    let inst = instance::get_instance(&state.instances_dir(), instance_id)?
        .ok_or_else(|| anyhow::anyhow!("Instance not found"))?;

    let profile = state
        .active_profile
        .lock()
        .await
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Sign in before launching an instance"))?;

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

    let exit_code = mc_launch::spawn_and_stream(app, instance_id, inst.memory_mb, args, &game_dir).await?;

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
