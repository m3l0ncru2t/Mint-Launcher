use crate::minecraft::download;
use crate::minecraft::fabric::{self, FabricLoaderInfo};
use crate::minecraft::manifest::VersionManifestEntry;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_minecraft_versions(
    state: State<'_, AppState>,
) -> Result<Vec<VersionManifestEntry>, String> {
    let manifest = download::fetch_version_manifest(&state).await.map_err(|e| e.to_string())?;
    Ok(manifest.versions)
}

#[tauri::command]
pub async fn get_fabric_loader_versions(
    state: State<'_, AppState>,
    game_version: String,
) -> Result<Vec<FabricLoaderInfo>, String> {
    fabric::list_loader_versions(&state.http, &game_version)
        .await
        .map_err(|e| e.to_string())
}
