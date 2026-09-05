mod accounts;
mod auth;
mod commands;
mod http_util;
mod importer;
mod instance;
mod minecraft;
mod mod_meta;
mod msa;
mod settings;
mod state;

use state::AppState;
use std::path::PathBuf;
use tauri::Manager;

/// The folder next to the running executable, if it's carrying a
/// `portable.txt` marker - dropped there by the portable release zip (see
/// "Package portable Windows build" in .github/workflows/build.yml). `None`
/// means this is a normal installed build. Also used by the portable
/// self-updater in `commands::updater`.
pub fn portable_root() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?.to_path_buf();
    exe_dir.join("portable.txt").is_file().then_some(exe_dir)
}

/// Picks where the app stores its data (instances, settings, accounts,
/// backgrounds - everything under `AppState::data_dir`). Portable mode
/// (see `portable_root`) keeps data in a `data` folder next to the exe
/// instead of the normal per-OS app data directory, so moving/copying that
/// folder brings everything with it and leaves nothing on the host machine.
fn resolve_data_dir(app: &tauri::App) -> PathBuf {
    match portable_root() {
        Some(root) => root.join("data"),
        None => app.path().app_data_dir().expect("failed to resolve app data dir"),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let data_dir = resolve_data_dir(app);
            std::fs::create_dir_all(&data_dir)?;
            app.manage(AppState::new(data_dir));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::instances::list_instances,
            commands::instances::reorder_instances,
            commands::instances::create_instance,
            commands::instances::delete_instance,
            commands::instances::get_instance,
            commands::instances::list_mods,
            commands::instances::delete_mod,
            commands::instances::get_mods_dir,
            commands::instances::check_mod_updates,
            commands::instances::apply_mod_update,
            commands::instances::search_mods,
            commands::instances::install_mod,
            commands::instances::get_mod_info,
            commands::instances::list_resourcepacks,
            commands::instances::delete_resourcepack,
            commands::instances::get_resourcepacks_dir,
            commands::instances::search_resourcepacks,
            commands::instances::install_resourcepack,
            commands::instances::toggle_resourcepack,
            commands::instances::get_resourcepack_info,
            commands::instances::get_resourcepack_project_info,
            commands::instances::check_resourcepack_updates,
            commands::instances::apply_resourcepack_update,
            commands::instances::list_servers,
            commands::instances::save_servers,
            commands::instances::ping_server,
            commands::instances::update_instance_settings,
            commands::instances::upgrade_instance_loader,
            commands::instances::update_instance_version,
            commands::instances::set_instance_icon,
            commands::instances::remove_instance_icon,
            commands::instances::get_instance_icon,
            commands::instances::export_instance,
            commands::instances::import_instance,
            commands::import_launcher::suggest_launcher_paths,
            commands::import_launcher::scan_external_launcher,
            commands::import_launcher::import_external_instance,
            commands::instances::get_project_info,
            commands::instances::toggle_mod,
            commands::versions::get_minecraft_versions,
            commands::versions::get_fabric_loader_versions,
            commands::auth::get_settings,
            commands::auth::set_microsoft_client_id,
            commands::appearance::set_background_theme,
            commands::appearance::set_theme_opacity,
            commands::appearance::add_custom_background,
            commands::appearance::rename_custom_background,
            commands::appearance::list_custom_backgrounds,
            commands::appearance::get_custom_background,
            commands::appearance::remove_custom_background,
            commands::auth::get_active_profile,
            commands::auth::sign_out,
            commands::auth::login_offline,
            commands::auth::login_microsoft,
            commands::auth::list_accounts,
            commands::auth::switch_account,
            commands::auth::remove_account,
            commands::auth::get_profile_details,
            commands::auth::upload_skin,
            commands::auth::reset_skin,
            commands::auth::set_cape,
            commands::auth::remove_cape,
            commands::auth::get_player_skin_url,
            commands::launch::launch_instance,
            commands::launch::stop_instance,
            commands::launch::list_running_instances,
            commands::updater::is_portable,
            commands::updater::check_portable_update,
            commands::updater::install_portable_update,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
