use crate::instance;
use crate::settings::{self, ThemeOpacity};
use crate::state::AppState;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use tauri::State;
use uuid::Uuid;

const MAX_BACKGROUND_BYTES: usize = 8 * 1024 * 1024;

fn backgrounds_dir(state: &AppState) -> std::path::PathBuf {
    state.data_dir.join("backgrounds")
}

/// Selects a theme: a built-in preset id (the frontend owns that list), the
/// id of a previously-added custom background, or `None` for the plain
/// default look with no background image at all.
#[tauri::command]
pub async fn set_background_theme(state: State<'_, AppState>, theme: Option<String>) -> Result<(), String> {
    let mut current = state.settings.lock().await;
    current.background_theme = theme;
    settings::save(&state.settings_path(), &current).map_err(|e| e.to_string())
}

/// Saves the sidebar/addon-list transparency for one specific theme id, so
/// switching themes later recalls its own look instead of a shared global
/// setting.
#[tauri::command]
pub async fn set_theme_opacity(
    state: State<'_, AppState>,
    theme_id: String,
    sidebar: f32,
    mods_panel: f32,
) -> Result<(), String> {
    let mut current = state.settings.lock().await;
    current.theme_opacity.insert(
        theme_id,
        ThemeOpacity {
            sidebar: sidebar.clamp(0.0, 1.0),
            mods_panel: mods_panel.clamp(0.0, 1.0),
        },
    );
    settings::save(&state.settings_path(), &current).map_err(|e| e.to_string())
}

/// Adds a new custom background image under a freshly generated id and
/// immediately selects it as the active theme. Each upload gets its own
/// file rather than overwriting a single shared slot, so players can build
/// up a small gallery of their own images instead of losing the previous
/// one every time they pick a new one.
#[tauri::command]
pub async fn add_custom_background(
    state: State<'_, AppState>,
    data_base64: String,
    name: String,
) -> Result<String, String> {
    let data = STANDARD.decode(data_base64.as_bytes()).map_err(|e| e.to_string())?;
    if data.len() > MAX_BACKGROUND_BYTES {
        return Err("Image is too large (max 8MB)".to_string());
    }
    if instance::sniff_image_mime(&data).is_none() {
        return Err("Unrecognized image format - use PNG, JPEG, GIF, or WebP".to_string());
    }

    let dir = backgrounds_dir(&state);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let id = Uuid::new_v4().to_string();
    std::fs::write(dir.join(&id), &data).map_err(|e| e.to_string())?;

    let mut current = state.settings.lock().await;
    current.background_theme = Some(id.clone());
    let name = name.trim();
    if !name.is_empty() {
        current.custom_background_names.insert(id.clone(), name.to_string());
    }
    settings::save(&state.settings_path(), &current).map_err(|e| e.to_string())?;
    Ok(id)
}

#[tauri::command]
pub async fn rename_custom_background(state: State<'_, AppState>, id: String, name: String) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Name can't be empty".to_string());
    }
    let mut current = state.settings.lock().await;
    current.custom_background_names.insert(id, name.to_string());
    settings::save(&state.settings_path(), &current).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomBackgroundInfo {
    pub id: String,
    pub name: String,
}

#[tauri::command]
pub async fn list_custom_backgrounds(state: State<'_, AppState>) -> Result<Vec<CustomBackgroundInfo>, String> {
    let dir = backgrounds_dir(&state);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Ok(Vec::new());
    };
    let mut ids: Vec<String> = entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .collect();
    ids.sort();

    let current = state.settings.lock().await;
    Ok(ids
        .into_iter()
        .map(|id| {
            let name = current.custom_background_names.get(&id).cloned().unwrap_or_else(|| "Custom".to_string());
            CustomBackgroundInfo { id, name }
        })
        .collect())
}

#[tauri::command]
pub async fn get_custom_background(state: State<'_, AppState>, id: String) -> Result<Option<String>, String> {
    let dir = backgrounds_dir(&state);
    let path = dir.join(&id);
    if path.parent() != Some(dir.as_path()) {
        return Err("Invalid background id".to_string());
    }
    let Ok(data) = std::fs::read(path) else {
        return Ok(None);
    };
    let Some(mime) = instance::sniff_image_mime(&data) else {
        return Ok(None);
    };
    Ok(Some(format!("data:{mime};base64,{}", STANDARD.encode(&data))))
}

#[tauri::command]
pub async fn remove_custom_background(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let dir = backgrounds_dir(&state);
    let path = dir.join(&id);
    if path.parent() != Some(dir.as_path()) {
        return Err("Invalid background id".to_string());
    }
    let _ = std::fs::remove_file(path);

    let mut current = state.settings.lock().await;
    if current.background_theme.as_deref() == Some(id.as_str()) {
        current.background_theme = None;
    }
    current.custom_background_names.remove(&id);
    current.theme_opacity.remove(&id);
    settings::save(&state.settings_path(), &current).map_err(|e| e.to_string())
}
