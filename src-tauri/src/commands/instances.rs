use crate::instance::{self, Instance, ModLoader};
use crate::mod_meta;
use crate::state::AppState;
use serde::Serialize;
use tauri::State;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModFile {
    pub file_name: String,
    pub size: u64,
    pub enabled: bool,
    pub is_dependency: bool,
}

fn resolve_instance(state: &AppState, id: &str) -> Result<Instance, String> {
    instance::get_instance(&state.instances_dir(), id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Instance not found".to_string())
}

fn resolve_mods_dir(state: &AppState, id: &str) -> Result<std::path::PathBuf, String> {
    Ok(resolve_instance(state, id)?.mods_dir(&state.instances_dir()))
}

#[tauri::command]
pub fn list_instances(state: State<AppState>) -> Result<Vec<Instance>, String> {
    instance::list_instances(&state.instances_dir()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn create_instance(
    state: State<AppState>,
    name: String,
    version_id: String,
    loader: ModLoader,
    loader_version: Option<String>,
) -> Result<Instance, String> {
    if name.trim().is_empty() {
        return Err("Instance name can't be empty".to_string());
    }
    instance::create_instance(&state.instances_dir(), name, version_id, loader, loader_version)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_instance(state: State<AppState>, id: String) -> Result<(), String> {
    instance::delete_instance(&state.instances_dir(), &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_instance(state: State<AppState>, id: String) -> Result<Option<Instance>, String> {
    instance::get_instance(&state.instances_dir(), &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_mods(state: State<AppState>, id: String) -> Result<Vec<ModFile>, String> {
    let dir = resolve_mods_dir(&state, &id)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let meta = mod_meta::load(&dir);

    let mut mods = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
            continue;
        }
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let lower = file_name.to_lowercase();
        if !lower.ends_with(".jar") && !lower.ends_with(".jar.disabled") {
            continue;
        }
        let size = entry.metadata().map_err(|e| e.to_string())?.len();
        let is_dependency = meta.get(&file_name).is_some_and(|m| m.is_dependency);
        mods.push(ModFile {
            enabled: !lower.ends_with(".disabled"),
            size,
            file_name,
            is_dependency,
        });
    }
    mods.sort_by(|a, b| a.file_name.to_lowercase().cmp(&b.file_name.to_lowercase()));
    Ok(mods)
}

#[tauri::command]
pub fn delete_mod(state: State<AppState>, id: String, file_name: String) -> Result<(), String> {
    let dir = resolve_mods_dir(&state, &id)?;
    let path = dir.join(&file_name);
    if path.parent() != Some(dir.as_path()) {
        return Err("Invalid mod file name".to_string());
    }
    std::fs::remove_file(path).map_err(|e| e.to_string())?;
    mod_meta::remove_entry(&dir, &file_name);
    Ok(())
}

#[tauri::command]
pub fn toggle_mod(
    state: State<AppState>,
    id: String,
    file_name: String,
    enabled: bool,
) -> Result<String, String> {
    let dir = resolve_mods_dir(&state, &id)?;
    let old_path = dir.join(&file_name);
    if old_path.parent() != Some(dir.as_path()) {
        return Err("Invalid mod file name".to_string());
    }

    let currently_enabled = !file_name.to_lowercase().ends_with(".disabled");
    if currently_enabled == enabled {
        return Ok(file_name);
    }

    let new_name = if enabled {
        file_name
            .strip_suffix(".disabled")
            .ok_or_else(|| "Invalid mod file name".to_string())?
            .to_string()
    } else {
        format!("{file_name}.disabled")
    };

    let new_path = dir.join(&new_name);
    std::fs::rename(&old_path, &new_path).map_err(|e| e.to_string())?;
    mod_meta::rename_entry(&dir, &file_name, &new_name);
    Ok(new_name)
}

#[tauri::command]
pub fn save_servers(
    state: State<AppState>,
    id: String,
    servers: Vec<crate::instance::ServerEntry>,
) -> Result<Instance, String> {
    let mut inst = resolve_instance(&state, &id)?;
    inst.servers = servers;
    inst.save(&state.instances_dir()).map_err(|e| e.to_string())?;
    Ok(inst)
}

#[tauri::command]
pub fn get_mods_dir(state: State<AppState>, id: String) -> Result<String, String> {
    let dir = resolve_mods_dir(&state, &id)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn check_mod_updates(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<crate::minecraft::modrinth::ModUpdateInfo>, String> {
    let inst = resolve_instance(&state, &id)?;
    let dir = inst.mods_dir(&state.instances_dir());
    crate::minecraft::modrinth::check_updates(
        &state.http,
        &dir,
        &inst.version_id,
        inst.loader.modrinth_loader(),
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn apply_mod_update(
    state: State<'_, AppState>,
    id: String,
    old_file_name: String,
    download_url: String,
) -> Result<(), String> {
    let dir = resolve_mods_dir(&state, &id)?;
    crate::minecraft::modrinth::apply_update(&state.http, &dir, &old_file_name, &download_url)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn search_mods(
    state: State<'_, AppState>,
    id: String,
    query: String,
    offset: u32,
) -> Result<crate::minecraft::modrinth::ModSearchPage, String> {
    let inst = resolve_instance(&state, &id)?;
    crate::minecraft::modrinth::search_mods(
        &state.http,
        &query,
        &inst.version_id,
        inst.loader.modrinth_loader(),
        offset,
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_mod_info(
    state: State<'_, AppState>,
    id: String,
    file_name: String,
) -> Result<crate::minecraft::modrinth::ModDetails, String> {
    let dir = resolve_mods_dir(&state, &id)?;
    crate::minecraft::modrinth::fetch_mod_details(&state.http, &dir, &file_name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_project_info(
    state: State<'_, AppState>,
    id: String,
    project_id: String,
) -> Result<crate::minecraft::modrinth::ModProjectDetails, String> {
    let inst = resolve_instance(&state, &id)?;
    crate::minecraft::modrinth::fetch_project_details(
        &state.http,
        &project_id,
        &inst.version_id,
        inst.loader.modrinth_loader(),
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn install_mod(
    state: State<'_, AppState>,
    id: String,
    project_id: String,
) -> Result<crate::minecraft::modrinth::InstallSummary, String> {
    let inst = resolve_instance(&state, &id)?;
    let dir = inst.mods_dir(&state.instances_dir());
    crate::minecraft::modrinth::install_mod(
        &state.http,
        &dir,
        &inst.version_id,
        inst.loader.modrinth_loader(),
        &project_id,
    )
    .await
    .map_err(|e| e.to_string())
}
