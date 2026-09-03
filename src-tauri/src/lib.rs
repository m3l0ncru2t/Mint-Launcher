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
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("failed to resolve app data dir");
            std::fs::create_dir_all(&data_dir)?;
            app.manage(AppState::new(data_dir));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::instances::list_instances,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
