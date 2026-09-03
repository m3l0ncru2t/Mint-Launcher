use crate::instance::{self, Instance, ModLoader};
use crate::mod_meta;
use crate::state::AppState;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use tauri::State;

const MAX_ICON_BYTES: usize = 5 * 1024 * 1024;

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

fn resolve_resourcepacks_dir(state: &AppState, id: &str) -> Result<std::path::PathBuf, String> {
    Ok(resolve_instance(state, id)?.resourcepacks_dir(&state.instances_dir()))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcePackFile {
    pub file_name: String,
    pub size: u64,
    pub enabled: bool,
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
pub fn update_instance_settings(
    state: State<AppState>,
    id: String,
    name: String,
    memory_mb: u32,
    java_args: Option<String>,
    account_id: Option<String>,
) -> Result<Instance, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Instance name can't be empty".to_string());
    }
    let mut inst = resolve_instance(&state, &id)?;
    inst.name = name;
    inst.memory_mb = memory_mb.clamp(512, 32768);
    inst.java_args = java_args.filter(|s| !s.trim().is_empty());
    inst.account_id = account_id.filter(|s| !s.is_empty());
    inst.save(&state.instances_dir()).map_err(|e| e.to_string())?;
    Ok(inst)
}

#[tauri::command]
pub fn set_instance_icon(state: State<AppState>, id: String, data_base64: String) -> Result<Instance, String> {
    let data = STANDARD.decode(data_base64.as_bytes()).map_err(|e| e.to_string())?;
    if data.len() > MAX_ICON_BYTES {
        return Err("Image is too large (max 5MB)".to_string());
    }
    if instance::sniff_image_mime(&data).is_none() {
        return Err("Unrecognized image format - use PNG, JPEG, GIF, or WebP".to_string());
    }

    let mut inst = resolve_instance(&state, &id)?;
    std::fs::write(inst.icon_path(&state.instances_dir()), &data).map_err(|e| e.to_string())?;
    inst.has_icon = true;
    inst.save(&state.instances_dir()).map_err(|e| e.to_string())?;
    Ok(inst)
}

#[tauri::command]
pub fn remove_instance_icon(state: State<AppState>, id: String) -> Result<Instance, String> {
    let mut inst = resolve_instance(&state, &id)?;
    let _ = std::fs::remove_file(inst.icon_path(&state.instances_dir()));
    inst.has_icon = false;
    inst.save(&state.instances_dir()).map_err(|e| e.to_string())?;
    Ok(inst)
}

#[tauri::command]
pub fn get_instance_icon(state: State<AppState>, id: String) -> Result<Option<String>, String> {
    let inst = resolve_instance(&state, &id)?;
    let Ok(data) = std::fs::read(inst.icon_path(&state.instances_dir())) else {
        return Ok(None);
    };
    let Some(mime) = instance::sniff_image_mime(&data) else {
        return Ok(None);
    };
    Ok(Some(format!("data:{mime};base64,{}", STANDARD.encode(&data))))
}

/// A large instance's worlds/mods can take a while to zip/unzip - `async`
/// plus `spawn_blocking` keeps that file I/O off the main thread, so the
/// window stays responsive instead of the OS reporting Mint as "not
/// responding" mid-export/import.
#[tauri::command]
pub async fn export_instance(state: State<'_, AppState>, id: String, dest_path: String) -> Result<(), String> {
    let instances_dir = state.instances_dir();
    tauri::async_runtime::spawn_blocking(move || {
        instance::export_instance(&instances_dir, &id, std::path::Path::new(&dest_path))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn import_instance(state: State<'_, AppState>, source_path: String) -> Result<Instance, String> {
    let instances_dir = state.instances_dir();
    tauri::async_runtime::spawn_blocking(move || {
        instance::import_instance(&instances_dir, std::path::Path::new(&source_path))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ping_server(address: String) -> Result<crate::minecraft::server_ping::ServerStatus, String> {
    crate::minecraft::server_ping::ping(&address).await.map_err(|e| e.to_string())
}

/// Reads the server list live from the instance's own `servers.dat`, the
/// same file Minecraft itself reads/writes - so a server added in-game shows
/// up here too, not just ones added through Mint.
#[tauri::command]
pub fn list_servers(state: State<AppState>, id: String) -> Result<Vec<crate::instance::ServerEntry>, String> {
    let inst = resolve_instance(&state, &id)?;
    Ok(crate::minecraft::servers_dat::read_servers(&inst.game_dir(&state.instances_dir())))
}

#[tauri::command]
pub fn save_servers(
    state: State<AppState>,
    id: String,
    servers: Vec<crate::instance::ServerEntry>,
) -> Result<Vec<crate::instance::ServerEntry>, String> {
    let inst = resolve_instance(&state, &id)?;
    crate::minecraft::servers_dat::write_servers(&inst.game_dir(&state.instances_dir()), &servers)
        .map_err(|e| e.to_string())?;
    Ok(servers)
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
        "mod",
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_resourcepack_project_info(
    state: State<'_, AppState>,
    id: String,
    project_id: String,
) -> Result<crate::minecraft::modrinth::ModProjectDetails, String> {
    let inst = resolve_instance(&state, &id)?;
    crate::minecraft::modrinth::fetch_project_details(&state.http, &project_id, &inst.version_id, None, "resourcepack")
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_resourcepacks(state: State<AppState>, id: String) -> Result<Vec<ResourcePackFile>, String> {
    let inst = resolve_instance(&state, &id)?;
    let dir = inst.resourcepacks_dir(&state.instances_dir());
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let enabled_set = crate::minecraft::resourcepacks::enabled_resourcepacks(&inst.game_dir(&state.instances_dir()));

    let mut packs = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        let file_name = entry.file_name().to_string_lossy().into_owned();

        // A resource pack is either a `.zip` (the only form Modrinth
        // distributes, and what Mint's own installer writes) or a plain
        // unzipped folder - both are equally valid to Minecraft itself, and
        // folder packs are common enough (e.g. carried over by the launcher
        // importer) that skipping them would silently hide real packs.
        let size = if file_type.is_dir() {
            crate::minecraft::resourcepacks::dir_size(&entry.path())
        } else if file_type.is_file() && file_name.to_lowercase().ends_with(".zip") {
            entry.metadata().map_err(|e| e.to_string())?.len()
        } else {
            continue;
        };
        let enabled = enabled_set.contains(&file_name);
        packs.push(ResourcePackFile { file_name, size, enabled });
    }
    packs.sort_by(|a, b| a.file_name.to_lowercase().cmp(&b.file_name.to_lowercase()));
    Ok(packs)
}

#[tauri::command]
pub fn toggle_resourcepack(state: State<AppState>, id: String, file_name: String, enabled: bool) -> Result<(), String> {
    let inst = resolve_instance(&state, &id)?;
    let game_dir = inst.game_dir(&state.instances_dir());
    crate::minecraft::resourcepacks::set_resourcepack_enabled(&game_dir, &file_name, enabled)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_resourcepack_info(
    state: State<'_, AppState>,
    id: String,
    file_name: String,
) -> Result<crate::minecraft::modrinth::ResourcePackDetails, String> {
    let dir = resolve_resourcepacks_dir(&state, &id)?;
    crate::minecraft::modrinth::fetch_resourcepack_details(&state.http, &dir, &file_name)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn check_resourcepack_updates(
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<crate::minecraft::modrinth::ModUpdateInfo>, String> {
    let inst = resolve_instance(&state, &id)?;
    let dir = inst.resourcepacks_dir(&state.instances_dir());
    crate::minecraft::modrinth::check_resourcepack_updates(&state.http, &dir, &inst.version_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn apply_resourcepack_update(
    state: State<'_, AppState>,
    id: String,
    old_file_name: String,
    download_url: String,
) -> Result<(), String> {
    let dir = resolve_resourcepacks_dir(&state, &id)?;
    crate::minecraft::modrinth::apply_update(&state.http, &dir, &old_file_name, &download_url)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_resourcepack(state: State<AppState>, id: String, file_name: String) -> Result<(), String> {
    let dir = resolve_resourcepacks_dir(&state, &id)?;
    let path = dir.join(&file_name);
    if path.parent() != Some(dir.as_path()) {
        return Err("Invalid resource pack file name".to_string());
    }
    if path.is_dir() {
        std::fs::remove_dir_all(path).map_err(|e| e.to_string())
    } else {
        std::fs::remove_file(path).map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub fn get_resourcepacks_dir(state: State<AppState>, id: String) -> Result<String, String> {
    let dir = resolve_resourcepacks_dir(&state, &id)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn search_resourcepacks(
    state: State<'_, AppState>,
    id: String,
    query: String,
    offset: u32,
) -> Result<crate::minecraft::modrinth::ModSearchPage, String> {
    let inst = resolve_instance(&state, &id)?;
    crate::minecraft::modrinth::search_resourcepacks(&state.http, &query, &inst.version_id, offset)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn install_resourcepack(
    state: State<'_, AppState>,
    id: String,
    project_id: String,
) -> Result<crate::minecraft::modrinth::InstalledModInfo, String> {
    let inst = resolve_instance(&state, &id)?;
    let dir = inst.resourcepacks_dir(&state.instances_dir());
    crate::minecraft::modrinth::install_resourcepack(&state.http, &dir, &inst.version_id, &project_id)
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
