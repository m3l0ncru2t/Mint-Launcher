use crate::instance::{self, Instance, InstanceKind, ModLoader};
use crate::mod_meta;
use crate::server_instance;
use crate::state::AppState;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use std::io::Read;
use tauri::{Emitter, State};

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

fn resolve_config_dir(state: &AppState, id: &str) -> Result<std::path::PathBuf, String> {
    Ok(resolve_instance(state, id)?.game_dir(&state.instances_dir()).join("config"))
}

fn resolve_logs_dir(state: &AppState, id: &str) -> Result<std::path::PathBuf, String> {
    Ok(resolve_instance(state, id)?.game_dir(&state.instances_dir()).join("logs"))
}

/// Joins `file_name` onto `dir` and rejects anything that would escape it
/// (e.g. a `../` component) - the same guard `delete_mod`/`delete_resourcepack`
/// already apply inline, pulled out here since configs/logs need it in more
/// than one place each.
pub(crate) fn safe_child(dir: &std::path::Path, file_name: &str) -> Result<std::path::PathBuf, String> {
    let path = dir.join(file_name);
    if path.parent() != Some(dir) {
        return Err("Invalid file name".to_string());
    }
    Ok(path)
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
pub fn reorder_instances(state: State<AppState>, ordered_ids: Vec<String>) -> Result<Vec<Instance>, String> {
    instance::reorder_instances(&state.instances_dir(), &ordered_ids).map_err(|e| e.to_string())
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
    instance::create_instance(&state.instances_dir(), name, version_id, loader, loader_version, InstanceKind::Client)
        .map_err(|e| e.to_string())
}

/// A dedicated server instance - kept as its own command rather than an
/// extra `kind` param on `create_instance` so the EULA gate stays explicit
/// and localized to server creation, and so the client creation path (used
/// by every existing instance) stays untouched. `eula_accepted` must already
/// be `true` here - `CreateServerDialog` only calls this after the user has
/// checked the EULA box, never the other way around.
#[tauri::command]
pub fn create_server_instance(
    state: State<AppState>,
    name: String,
    version_id: String,
    loader: ModLoader,
    loader_version: Option<String>,
    eula_accepted: bool,
) -> Result<Instance, String> {
    if name.trim().is_empty() {
        return Err("Instance name can't be empty".to_string());
    }
    if !eula_accepted {
        return Err("You must accept the Minecraft EULA to create a server".to_string());
    }
    let mut inst = instance::create_instance(
        &state.instances_dir(),
        name,
        version_id,
        loader,
        loader_version,
        InstanceKind::Server,
    )
    .map_err(|e| e.to_string())?;
    inst.eula_accepted = true;
    inst.save(&state.instances_dir()).map_err(|e| e.to_string())?;
    Ok(inst)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerDetection {
    pub version_id: String,
    pub loader: ModLoader,
    pub loader_version: Option<String>,
}

/// Live preview for `ImportServerDialog`: runs the same auto-detection
/// `import_server_folder` itself relies on, so the dialog can show what was
/// found (or why detection failed) before the user commits to importing.
#[tauri::command(async)]
pub fn detect_server_instance(source_path: String) -> Result<ServerDetection, String> {
    let detection =
        server_instance::detect_server(std::path::Path::new(&source_path)).map_err(|e| e.to_string())?;
    Ok(ServerDetection {
        version_id: detection.version_id,
        loader: detection.loader,
        loader_version: detection.loader_version,
    })
}

/// Imports an existing dedicated server folder in place (see
/// `server_instance::import_server_folder`) - version/loader are
/// auto-detected from the folder itself, and nothing is copied, so this
/// finishes near-instantly with no progress to report.
#[tauri::command]
pub async fn import_server_folder(
    state: State<'_, AppState>,
    source_path: String,
    name: String,
    eula_accepted: bool,
) -> Result<Instance, String> {
    if name.trim().is_empty() {
        return Err("Instance name can't be empty".to_string());
    }
    let instances_dir = state.instances_dir();
    tauri::async_runtime::spawn_blocking(move || {
        server_instance::import_server_folder(&instances_dir, std::path::Path::new(&source_path), name, eula_accepted)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn delete_instance(state: State<AppState>, id: String) -> Result<(), String> {
    instance::delete_instance(&state.instances_dir(), &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_instance(state: State<AppState>, id: String) -> Result<Option<Instance>, String> {
    instance::get_instance(&state.instances_dir(), &id).map_err(|e| e.to_string())
}

#[tauri::command(async)]
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

/// One-way: only a Vanilla instance can be upgraded, and only to a loader
/// that's actually launchable (see `do_launch`) - Forge/Quilt are modeled
/// but not wired up yet. No files change hands here; the loader jar/
/// libraries are resolved and downloaded lazily on the instance's next
/// launch, same as a Fabric instance created from scratch.
#[tauri::command]
pub fn upgrade_instance_loader(
    state: State<AppState>,
    id: String,
    loader: ModLoader,
    loader_version: String,
) -> Result<Instance, String> {
    if loader != ModLoader::Fabric {
        return Err(format!("{loader:?} isn't supported yet"));
    }
    let mut inst = resolve_instance(&state, &id)?;
    if inst.loader != ModLoader::Vanilla {
        return Err("This instance already uses a mod loader".to_string());
    }
    inst.loader = loader;
    inst.loader_version = Some(loader_version);
    inst.save(&state.instances_dir()).map_err(|e| e.to_string())?;
    Ok(inst)
}

/// For an instance already on Fabric - moves it to a different Fabric
/// loader build (a newer one, typically, but nothing here actually
/// enforces that) without touching the Minecraft version, mods, worlds, or
/// config. Same lazy-resolution story as `upgrade_instance_loader`: nothing
/// is downloaded here, the new loader's libraries/profile are simply
/// re-resolved on the instance's next launch.
#[tauri::command]
pub fn update_fabric_loader_version(state: State<AppState>, id: String, loader_version: String) -> Result<Instance, String> {
    let mut inst = resolve_instance(&state, &id)?;
    if inst.loader != ModLoader::Fabric {
        return Err("This instance isn't using Fabric".to_string());
    }
    inst.loader_version = Some(loader_version);
    inst.save(&state.instances_dir()).map_err(|e| e.to_string())?;
    Ok(inst)
}

/// Moves an instance to a newer Minecraft version without recreating it -
/// mods, worlds, and config are untouched. Libraries and (for Fabric) the
/// loader profile are simply re-resolved against the new version on the
/// instance's next launch, the same lazy resolution a freshly created
/// instance goes through. Restricted to strictly newer versions (by release
/// date) so this can't be used to quietly downgrade an instance, which
/// `do_launch` never expects and isn't tested against.
#[tauri::command]
pub async fn update_instance_version(
    state: State<'_, AppState>,
    id: String,
    version_id: String,
) -> Result<Instance, String> {
    let manifest = crate::minecraft::download::fetch_version_manifest(&state)
        .await
        .map_err(|e| e.to_string())?;
    let target = manifest
        .versions
        .iter()
        .find(|v| v.id == version_id)
        .ok_or_else(|| "Unknown Minecraft version".to_string())?;

    let mut inst = resolve_instance(&state, &id)?;
    if let Some(current) = manifest.versions.iter().find(|v| v.id == inst.version_id) {
        if target.release_time <= current.release_time {
            return Err("Pick a version newer than the instance's current one".to_string());
        }
    }
    inst.version_id = version_id;
    inst.save(&state.instances_dir()).map_err(|e| e.to_string())?;
    Ok(inst)
}

/// The curated `server.properties` settings surfaced in the UI - see
/// `minecraft::server_properties` for how these are read/written without
/// disturbing the rest of the file.
#[tauri::command(async)]
pub fn get_server_properties(state: State<AppState>, id: String) -> Result<std::collections::HashMap<String, String>, String> {
    let inst = resolve_instance(&state, &id)?;
    if inst.kind != InstanceKind::Server {
        return Err("This isn't a server instance".to_string());
    }
    Ok(crate::minecraft::server_properties::read_properties(&inst.game_dir(&state.instances_dir())))
}

#[tauri::command]
pub fn save_server_properties(
    state: State<AppState>,
    id: String,
    values: std::collections::HashMap<String, String>,
) -> Result<(), String> {
    let inst = resolve_instance(&state, &id)?;
    if inst.kind != InstanceKind::Server {
        return Err("This isn't a server instance".to_string());
    }
    crate::minecraft::server_properties::write_properties(&inst.game_dir(&state.instances_dir()), &values)
        .map_err(|e| e.to_string())
}

#[tauri::command(async)]
pub fn get_ops(state: State<AppState>, id: String) -> Result<Vec<crate::minecraft::server_admin::OpEntry>, String> {
    let inst = resolve_instance(&state, &id)?;
    Ok(crate::minecraft::server_admin::read_ops(&inst.game_dir(&state.instances_dir())))
}

#[tauri::command(async)]
pub fn get_whitelist(state: State<AppState>, id: String) -> Result<Vec<crate::minecraft::server_admin::WhitelistEntry>, String> {
    let inst = resolve_instance(&state, &id)?;
    Ok(crate::minecraft::server_admin::read_whitelist(&inst.game_dir(&state.instances_dir())))
}

#[tauri::command(async)]
pub fn get_banned_players(
    state: State<AppState>,
    id: String,
) -> Result<Vec<crate::minecraft::server_admin::BannedPlayerEntry>, String> {
    let inst = resolve_instance(&state, &id)?;
    Ok(crate::minecraft::server_admin::read_banned_players(&inst.game_dir(&state.instances_dir())))
}

/// Guards the direct-file-edit admin commands below: while the server is
/// actually running, its own in-memory copy of ops/whitelist/bans is
/// authoritative and can silently overwrite a direct file edit the next time
/// anything touches them, so those cases are expected to go through the
/// equivalent console command (`op`/`whitelist add`/`ban`/...) instead -
/// `PlayersPanel` already picks the right path based on whether the server
/// is running.
async fn ensure_not_running(state: &AppState, id: &str) -> Result<(), String> {
    if state.running_instances.lock().await.contains_key(id) {
        return Err("Use the console/online players list for this while the server is running".to_string());
    }
    Ok(())
}

#[tauri::command]
pub async fn remove_op_entry(state: State<'_, AppState>, id: String, name: String) -> Result<(), String> {
    ensure_not_running(&state, &id).await?;
    let inst = resolve_instance(&state, &id)?;
    crate::minecraft::server_admin::remove_op(&inst.game_dir(&state.instances_dir()), &name).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_whitelist_entry(state: State<'_, AppState>, id: String, name: String) -> Result<(), String> {
    ensure_not_running(&state, &id).await?;
    let inst = resolve_instance(&state, &id)?;
    crate::minecraft::server_admin::remove_whitelist(&inst.game_dir(&state.instances_dir()), &name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn unban_player_entry(state: State<'_, AppState>, id: String, name: String) -> Result<(), String> {
    ensure_not_running(&state, &id).await?;
    let inst = resolve_instance(&state, &id)?;
    crate::minecraft::server_admin::unban_player(&inst.game_dir(&state.instances_dir()), &name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn add_op_entry(state: State<'_, AppState>, id: String, username: String) -> Result<(), String> {
    ensure_not_running(&state, &id).await?;
    let inst = resolve_instance(&state, &id)?;
    let (uuid, name) = crate::minecraft::server_admin::lookup_uuid(&state.http, &username)
        .await
        .map_err(|e| e.to_string())?;
    crate::minecraft::server_admin::add_op(&inst.game_dir(&state.instances_dir()), uuid, name).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn add_whitelist_entry(state: State<'_, AppState>, id: String, username: String) -> Result<(), String> {
    ensure_not_running(&state, &id).await?;
    let inst = resolve_instance(&state, &id)?;
    let (uuid, name) = crate::minecraft::server_admin::lookup_uuid(&state.http, &username)
        .await
        .map_err(|e| e.to_string())?;
    crate::minecraft::server_admin::add_whitelist(&inst.game_dir(&state.instances_dir()), uuid, name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn add_ban_entry(
    state: State<'_, AppState>,
    id: String,
    username: String,
    reason: String,
) -> Result<(), String> {
    ensure_not_running(&state, &id).await?;
    let inst = resolve_instance(&state, &id)?;
    let (uuid, name) = crate::minecraft::server_admin::lookup_uuid(&state.http, &username)
        .await
        .map_err(|e| e.to_string())?;
    crate::minecraft::server_admin::add_ban(&inst.game_dir(&state.instances_dir()), uuid, name, reason)
        .map_err(|e| e.to_string())
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

#[tauri::command(async)]
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

/// A dedicated server's own `server-icon.png` (the same 64x64 favicon a
/// vanilla client shows for it in the multiplayer list) - used as the
/// instance's icon in the sidebar when the user hasn't set a custom one, so
/// a server instance shows the icon it was actually configured with instead
/// of a generic letter avatar.
#[tauri::command(async)]
pub fn get_server_icon(state: State<AppState>, id: String) -> Result<Option<String>, String> {
    let inst = resolve_instance(&state, &id)?;
    let Ok(data) = std::fs::read(inst.game_dir(&state.instances_dir()).join("server-icon.png")) else {
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

/// Refused while the source is running: copying a world (or a server's
/// files) that's being written to can capture a half-saved, corrupt state.
#[tauri::command]
pub async fn duplicate_instance(state: State<'_, AppState>, id: String) -> Result<Instance, String> {
    if state.running_instances.lock().await.contains_key(&id) {
        return Err("Stop this instance before duplicating it".to_string());
    }
    let instances_dir = state.instances_dir();
    tauri::async_runtime::spawn_blocking(move || instance::duplicate_instance(&instances_dir, &id))
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

/// Reveals `path` in the OS file manager. A hand-rolled command rather than
/// `tauri-plugin-opener`'s `openPath` - that plugin gates every call through
/// an ACL path-scope the frontend has no reliable way to pre-authorize for a
/// folder the user only picks at runtime (an imported instance's linked
/// folder can be anywhere on disk, not just under Mint's own app-data dir),
/// so it kept rejecting perfectly valid folders. This command is one of
/// Mint's own `#[tauri::command]`s, which - like every other filesystem
/// command here - the frontend can already call freely without a separate
/// permission grant.
#[tauri::command]
pub fn open_folder(path: String) -> Result<(), String> {
    let path = std::path::PathBuf::from(path);
    if !path.is_dir() {
        return Err("That folder doesn't exist".to_string());
    }
    #[cfg(target_os = "linux")]
    let mut cmd = std::process::Command::new("xdg-open");
    #[cfg(target_os = "macos")]
    let mut cmd = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut cmd = std::process::Command::new("explorer");

    cmd.arg(&path).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Emits `mod-update-checked` as each mod's check finishes (see
/// `modrinth::check_updates_matching`'s `on_result`) so `ModsPanel` can show
/// results appearing one by one instead of a long wait followed by all of
/// them at once - still returns the complete list too, both as the return
/// value every caller already expects and as a safety net for a listener
/// that missed an event (e.g. one set up slightly after this started).
#[tauri::command]
pub async fn check_mod_updates(
    app: tauri::AppHandle,
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
        |info| {
            let _ = app.emit("mod-update-checked", serde_json::json!({ "instanceId": id, "info": info }));
        },
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
    let dir = inst.mods_dir(&state.instances_dir());
    crate::minecraft::modrinth::search_mods(
        &state.http,
        &query,
        &inst.version_id,
        inst.loader.modrinth_loader(),
        offset,
        &dir,
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

// `async` so the folder-pack size walk below runs off the main thread - it
// recurses through every unzipped pack, and this runs on every window focus.
#[tauri::command(async)]
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

/// See `check_mod_updates` - same "emit as each one finishes, still return
/// the full list too" shape, on its own event name.
#[tauri::command]
pub async fn check_resourcepack_updates(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<Vec<crate::minecraft::modrinth::ModUpdateInfo>, String> {
    let inst = resolve_instance(&state, &id)?;
    let dir = inst.resourcepacks_dir(&state.instances_dir());
    crate::minecraft::modrinth::check_resourcepack_updates(&state.http, &dir, &inst.version_id, |info| {
        let _ = app.emit("resourcepack-update-checked", serde_json::json!({ "instanceId": id, "info": info }));
    })
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
    let dir = inst.resourcepacks_dir(&state.instances_dir());
    crate::minecraft::modrinth::search_resourcepacks(&state.http, &query, &inst.version_id, offset, &dir)
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
    let game_dir = inst.game_dir(&state.instances_dir());
    crate::minecraft::modrinth::install_resourcepack(&state.http, &dir, &game_dir, &inst.version_id, &project_id)
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

const MAX_TEXT_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_LOG_TEXT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigEntry {
    pub file_name: String,
    pub size: u64,
    pub is_dir: bool,
}

/// Top-level entries only (mirrors `list_mods`/`list_resourcepacks`, not a
/// recursive tree view) - most mod configs sit directly in `config/`, and a
/// per-mod subfolder still shows up (with its total size), just not
/// browsable into yet.
#[tauri::command(async)]
pub fn list_config_files(state: State<AppState>, id: String) -> Result<Vec<ConfigEntry>, String> {
    let dir = resolve_config_dir(&state, &id)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let file_type = entry.file_type().map_err(|e| e.to_string())?;
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let (size, is_dir) = if file_type.is_dir() {
            (crate::minecraft::resourcepacks::dir_size(&entry.path()), true)
        } else {
            (entry.metadata().map_err(|e| e.to_string())?.len(), false)
        };
        entries.push(ConfigEntry { file_name, size, is_dir });
    }
    entries.sort_by(|a, b| a.file_name.to_lowercase().cmp(&b.file_name.to_lowercase()));
    Ok(entries)
}

#[tauri::command(async)]
pub fn read_config_file(state: State<AppState>, id: String, file_name: String) -> Result<String, String> {
    let dir = resolve_config_dir(&state, &id)?;
    let path = safe_child(&dir, &file_name)?;
    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if meta.len() > MAX_TEXT_FILE_BYTES {
        return Err("This file is too large to edit here - open it in a text editor instead".to_string());
    }
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    String::from_utf8(bytes).map_err(|_| "This file isn't plain text".to_string())
}

#[tauri::command(async)]
pub fn write_config_file(
    state: State<AppState>,
    id: String,
    file_name: String,
    content: String,
) -> Result<(), String> {
    let dir = resolve_config_dir(&state, &id)?;
    let path = safe_child(&dir, &file_name)?;
    std::fs::write(path, content).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_config_file(state: State<AppState>, id: String, file_name: String) -> Result<(), String> {
    let dir = resolve_config_dir(&state, &id)?;
    let path = safe_child(&dir, &file_name)?;
    if path.is_dir() {
        std::fs::remove_dir_all(path).map_err(|e| e.to_string())
    } else {
        std::fs::remove_file(path).map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub fn get_config_dir(state: State<AppState>, id: String) -> Result<String, String> {
    let dir = resolve_config_dir(&state, &id)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.to_string_lossy().into_owned())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub file_name: String,
    pub size: u64,
    pub modified_at: Option<String>,
}

#[tauri::command(async)]
pub fn list_log_files(state: State<AppState>, id: String) -> Result<Vec<LogEntry>, String> {
    let dir = resolve_logs_dir(&state, &id)?;
    let mut entries = Vec::new();
    if dir.is_dir() {
        for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
                continue;
            }
            let file_name = entry.file_name().to_string_lossy().into_owned();
            let lower = file_name.to_lowercase();
            if !lower.ends_with(".log") && !lower.ends_with(".log.gz") {
                continue;
            }
            let meta = entry.metadata().map_err(|e| e.to_string())?;
            let modified_at = meta.modified().ok().map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339());
            entries.push(LogEntry { file_name, size: meta.len(), modified_at });
        }
    }
    entries.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
    Ok(entries)
}

/// Transparently decompresses a rotated `.log.gz` - Minecraft compresses
/// every log but the current session's `latest.log` once it rotates out.
///
/// `async` + `spawn_blocking` rather than a plain sync command: those run on
/// the main thread, and this is polled every couple of seconds (see the
/// console/chat fallback for a server Mint doesn't hold the console of) - and
/// a plain `latest.log` used to be read *whole* (then decoded and copied
/// again) just to show its last few MB, so on a server that's been up for
/// days every poll froze the window. A live `latest.log` now only reads its
/// tail; a rotated `.gz` still has to be decompressed in full to find its end.
#[tauri::command]
pub async fn read_log_file(state: State<'_, AppState>, id: String, file_name: String) -> Result<String, String> {
    let dir = resolve_logs_dir(&state, &id)?;
    let path = safe_child(&dir, &file_name)?;
    let is_gz = file_name.to_lowercase().ends_with(".gz");
    tauri::async_runtime::spawn_blocking(move || read_log_text(&path, is_gz))
        .await
        .map_err(|e| e.to_string())?
}

fn read_log_text(path: &std::path::Path, is_gz: bool) -> Result<String, String> {
    use std::io::{Seek, SeekFrom};
    let (bytes, truncated) = if is_gz {
        let raw = std::fs::read(path).map_err(|e| e.to_string())?;
        let mut decoder = flate2::read::GzDecoder::new(std::io::Cursor::new(raw));
        let mut out = Vec::new();
        decoder.read_to_end(&mut out).map_err(|e| e.to_string())?;
        let truncated = out.len() > MAX_LOG_TEXT_BYTES;
        if truncated {
            out.drain(..out.len() - MAX_LOG_TEXT_BYTES);
        }
        (out, truncated)
    } else {
        let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let len = file.metadata().map_err(|e| e.to_string())?.len();
        let start = len.saturating_sub(MAX_LOG_TEXT_BYTES as u64);
        file.seek(SeekFrom::Start(start)).map_err(|e| e.to_string())?;
        let mut out = Vec::with_capacity((len - start) as usize);
        file.read_to_end(&mut out).map_err(|e| e.to_string())?;
        (out, start > 0)
    };
    let text = String::from_utf8_lossy(&bytes);
    if truncated {
        // The cut usually lands mid-line (or mid-character) - drop that
        // partial first line rather than showing a mangled fragment.
        let tail = text.split_once('\n').map_or(text.as_ref(), |(_, rest)| rest);
        Ok(format!("(showing only the last {}MB of this log)\n…\n{tail}", MAX_LOG_TEXT_BYTES / (1024 * 1024)))
    } else {
        Ok(text.into_owned())
    }
}

#[tauri::command]
pub fn get_logs_dir(state: State<AppState>, id: String) -> Result<String, String> {
    let dir = resolve_logs_dir(&state, &id)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.to_string_lossy().into_owned())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopPlayer {
    pub name: String,
    pub playtime_seconds: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub local_ip: Option<String>,
    pub port: u16,
    pub level_name: String,
    /// From server.properties (so it shows while stopped too), `§` codes
    /// already turned into styled runs.
    pub motd: Vec<crate::minecraft::server_ping::TextRun>,
    /// Unix seconds the server process started, while it's running.
    pub started_at: Option<u64>,
    /// When this instance was created in (or imported into) Mint.
    pub added_to_mint: String,
    /// Last time Mint started it.
    pub last_started: Option<String>,
    /// When the world folder was created on disk - the closest thing to a
    /// "server created" date, since it predates Mint for an imported server.
    /// `None` where the filesystem doesn't record creation times.
    pub world_created: Option<String>,
    /// Total time the server has had this world running, from the world's
    /// own tick counter - includes time from before Mint managed it.
    pub world_run_seconds: Option<u64>,
    /// Players who have ever joined - one stats file per player in the world.
    pub total_players_joined: usize,
    pub total_playtime_seconds: u64,
    pub top_players: Vec<TopPlayer>,
    pub world_size_bytes: u64,
    pub mod_count: usize,
    pub op_count: usize,
    pub whitelist_count: usize,
    pub ban_count: usize,
}

/// The LAN address other machines on the network would use. Connecting a UDP
/// socket only asks the OS which interface it would route through - nothing
/// is sent.
fn local_ip() -> Option<String> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}

fn dir_size(dir: &std::path::Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else { return 0 };
    entries
        .flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => dir_size(&e.path()),
            Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}

/// `Data.Time` from level.dat: ticks the world has ever run (it only
/// advances while the server is up, and survives restarts). Rather than a
/// full NBT parser, this finds the one Long tag named exactly "Time" - the
/// 2-byte length prefix makes the name match exact, so `DayTime` etc. don't
/// collide.
fn world_ticks(world: &std::path::Path) -> Option<u64> {
    let file = std::fs::File::open(world.join("level.dat")).ok()?;
    let mut data = Vec::new();
    flate2::read::GzDecoder::new(file).take(16 * 1024 * 1024).read_to_end(&mut data).ok()?;
    const TAG: &[u8] = b"\x04\x00\x04Time";
    let at = data.windows(TAG.len()).position(|w| w == TAG)? + TAG.len();
    let value = i64::from_be_bytes(data.get(at..at + 8)?.try_into().ok()?);
    u64::try_from(value).ok()
}

pub(crate) fn collect_server_info(inst: &Instance, instances_dir: &std::path::Path) -> ServerInfo {
    let game_dir = inst.game_dir(instances_dir);
    let props = crate::minecraft::server_properties::read_properties(&game_dir);
    let level_name = props.get("level-name").cloned().unwrap_or_else(|| "world".to_string());
    let port = props.get("server-port").and_then(|p| p.parse().ok()).unwrap_or(25565);
    let world = game_dir.join(&level_name);

    let names: std::collections::HashMap<String, String> =
        std::fs::read_to_string(game_dir.join("usercache.json"))
            .ok()
            .and_then(|s| serde_json::from_str::<Vec<serde_json::Value>>(&s).ok())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|v| Some((v.get("uuid")?.as_str()?.to_lowercase(), v.get("name")?.as_str()?.to_string())))
            .collect();

    let mut players: Vec<TopPlayer> = Vec::new();
    let mut total_players_joined = 0;
    if let Ok(entries) = std::fs::read_dir(world.join("stats")) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            total_players_joined += 1;
            let uuid = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
            let ticks = std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .and_then(|v| {
                    let custom = v.get("stats")?.get("minecraft:custom")?;
                    // Renamed from play_one_minute in 1.21.2.
                    custom.get("minecraft:play_time").or_else(|| custom.get("minecraft:play_one_minute"))?.as_u64()
                })
                .unwrap_or(0);
            players.push(TopPlayer {
                name: names.get(&uuid).cloned().unwrap_or(uuid),
                playtime_seconds: ticks / 20,
            });
        }
    }
    let total_playtime_seconds = players.iter().map(|p| p.playtime_seconds).sum();
    players.sort_by(|a, b| b.playtime_seconds.cmp(&a.playtime_seconds));
    players.truncate(5);

    let motd = crate::minecraft::server_ping::parse_legacy_motd(&crate::minecraft::server_properties::unescape_value(
        props.get("motd").map(String::as_str).unwrap_or("A Minecraft Server"),
    ));

    ServerInfo {
        local_ip: local_ip(),
        port,
        level_name,
        motd,
        started_at: None,
        added_to_mint: inst.created_at.clone(),
        last_started: inst.last_played.clone(),
        world_run_seconds: world_ticks(&world).map(|t| t / 20),
        world_created: std::fs::metadata(&world)
            .and_then(|m| m.created())
            .ok()
            .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339()),
        total_players_joined,
        total_playtime_seconds,
        top_players: players,
        world_size_bytes: dir_size(&world),
        mod_count: std::fs::read_dir(inst.mods_dir(instances_dir))
            .map(|d| d.flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".jar")).count())
            .unwrap_or(0),
        op_count: crate::minecraft::server_admin::read_ops(&game_dir).len(),
        whitelist_count: crate::minecraft::server_admin::read_whitelist(&game_dir).len(),
        ban_count: crate::minecraft::server_admin::read_banned_players(&game_dir).len(),
    }
}

#[tauri::command]
pub async fn get_server_info(state: State<'_, AppState>, id: String) -> Result<ServerInfo, String> {
    let inst = resolve_instance(&state, &id)?;
    if inst.kind != InstanceKind::Server {
        return Err("This isn't a server instance".to_string());
    }
    let instances_dir = state.instances_dir();
    let mut info = tauri::async_runtime::spawn_blocking(move || collect_server_info(&inst, &instances_dir))
        .await
        .map_err(|e| e.to_string())?;
    info.started_at = crate::commands::launch::instance_start_time(&state, &id).await;
    Ok(info)
}

/// The machine's internet-facing address, as an outside service sees it.
pub(crate) async fn lookup_public_ip(http: &reqwest::Client) -> Result<String, String> {
    let text = http
        .get("https://api.ipify.org")
        .timeout(std::time::Duration::from_secs(8))
        .send()
        .await
        .map_err(|_| "Couldn't look up the public IP".to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    let ip = text.trim().to_string();
    if ip.parse::<std::net::IpAddr>().is_err() {
        return Err("Couldn't look up the public IP".to_string());
    }
    Ok(ip)
}

#[tauri::command]
pub async fn get_public_ip(state: State<'_, AppState>) -> Result<String, String> {
    lookup_public_ip(&state.http).await
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerAddresses {
    pub local_ip: Option<String>,
    pub port: u16,
    pub domain: Option<String>,
}

/// Just the connect details for the instance header - cheap, unlike
/// `get_server_info` (which walks the world folder).
#[tauri::command]
pub async fn get_server_addresses(state: State<'_, AppState>, id: String) -> Result<ServerAddresses, String> {
    let inst = resolve_instance(&state, &id)?;
    let props = crate::minecraft::server_properties::read_properties(&inst.game_dir(&state.instances_dir()));
    let port = props.get("server-port").and_then(|p| p.parse().ok()).unwrap_or(25565);
    let domain = state.settings.lock().await.server_domains.get(&id).cloned();
    Ok(ServerAddresses { local_ip: local_ip(), port, domain })
}
