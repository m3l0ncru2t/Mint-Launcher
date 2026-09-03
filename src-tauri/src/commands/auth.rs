use crate::accounts::{self, AccountSummary, SavedAccount};
use crate::auth::{self, GameProfile};
use crate::minecraft::profile::{self, ProfileDetails};
use crate::msa;
use crate::settings::{self, Settings};
use crate::state::AppState;
use base64::Engine;
use tauri::State;

async fn active_access_token(state: &AppState) -> Result<String, String> {
    state
        .active_profile
        .lock()
        .await
        .clone()
        .map(|p| p.access_token)
        .ok_or_else(|| "Sign in with a Microsoft account first".to_string())
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    Ok(state.settings.lock().await.clone())
}

#[tauri::command]
pub async fn set_microsoft_client_id(
    state: State<'_, AppState>,
    client_id: Option<String>,
) -> Result<(), String> {
    let mut current = state.settings.lock().await;
    current.microsoft_client_id = client_id;
    settings::save(&state.settings_path(), &current).map_err(|e| e.to_string())
}

/// Returns the current session, silently re-authenticating the last-used
/// saved account first if there isn't one yet - this is what makes signing
/// in with Microsoft persist across app restarts.
#[tauri::command]
pub async fn get_active_profile(state: State<'_, AppState>) -> Result<Option<GameProfile>, String> {
    {
        let existing = state.active_profile.lock().await;
        if existing.is_some() {
            return Ok(existing.clone());
        }
    }

    let Some(id) = state.settings.lock().await.last_account_id.clone() else {
        return Ok(None);
    };
    let Some(account) = accounts::load(&state.data_dir).into_iter().find(|a| a.id == id) else {
        return Ok(None);
    };

    match msa::refresh(&state.http, &account.client_id, &account.refresh_token).await {
        Ok(result) => {
            persist_account(&state, &id, &account.client_id, &result).await;
            *state.active_profile.lock().await = Some(result.profile.clone());
            Ok(Some(result.profile))
        }
        // The saved session no longer works (revoked, expired) - fall back
        // to the login screen instead of failing app startup.
        Err(_) => Ok(None),
    }
}

#[tauri::command]
pub async fn sign_out(state: State<'_, AppState>) -> Result<(), String> {
    *state.active_profile.lock().await = None;
    let mut current = state.settings.lock().await;
    current.last_account_id = None;
    settings::save(&state.settings_path(), &current).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn login_offline(state: State<'_, AppState>, username: String) -> Result<GameProfile, String> {
    let username = username.trim().to_string();
    if username.is_empty() || username.len() > 16 {
        return Err("Username must be 1-16 characters".to_string());
    }
    let profile = auth::offline_profile(&username);

    let mut current = state.settings.lock().await;
    current.offline_username = Some(username);
    settings::save(&state.settings_path(), &current).map_err(|e| e.to_string())?;
    drop(current);

    *state.active_profile.lock().await = Some(profile.clone());
    Ok(profile)
}

#[tauri::command]
pub async fn login_microsoft(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<GameProfile, String> {
    let client_id = state
        .settings
        .lock()
        .await
        .microsoft_client_id
        .clone()
        .unwrap_or_else(|| msa::DEFAULT_CLIENT_ID.to_string());

    let result = msa::login(&app, &state.http, &client_id)
        .await
        .map_err(|e| e.to_string())?;

    let id = result.profile.uuid.clone();
    persist_account(&state, &id, &client_id, &result).await;

    let mut current = state.settings.lock().await;
    current.last_account_id = Some(id);
    settings::save(&state.settings_path(), &current).map_err(|e| e.to_string())?;
    drop(current);

    *state.active_profile.lock().await = Some(result.profile.clone());
    Ok(result.profile)
}

/// Saved Microsoft accounts available to switch to, for the account
/// dropdown - does not include the current offline profile, if any.
#[tauri::command]
pub async fn list_accounts(state: State<'_, AppState>) -> Result<Vec<AccountSummary>, String> {
    Ok(accounts::load(&state.data_dir)
        .into_iter()
        .map(|a| AccountSummary {
            id: a.id,
            username: a.username,
        })
        .collect())
}

/// Switches the active session to a previously saved Microsoft account,
/// silently refreshing its session rather than opening a browser.
#[tauri::command]
pub async fn switch_account(state: State<'_, AppState>, id: String) -> Result<GameProfile, String> {
    let account = accounts::load(&state.data_dir)
        .into_iter()
        .find(|a| a.id == id)
        .ok_or_else(|| "This account is no longer saved".to_string())?;

    let result = msa::refresh(&state.http, &account.client_id, &account.refresh_token)
        .await
        .map_err(|e| e.to_string())?;

    persist_account(&state, &id, &account.client_id, &result).await;

    let mut current = state.settings.lock().await;
    current.last_account_id = Some(id);
    settings::save(&state.settings_path(), &current).map_err(|e| e.to_string())?;
    drop(current);

    *state.active_profile.lock().await = Some(result.profile.clone());
    Ok(result.profile)
}

#[tauri::command]
pub async fn remove_account(state: State<'_, AppState>, id: String) -> Result<(), String> {
    accounts::remove(&state.data_dir, &id).map_err(|e| e.to_string())?;

    let mut profile = state.active_profile.lock().await;
    if profile.as_ref().is_some_and(|p| p.uuid == id) {
        *profile = None;
        drop(profile);
        let mut current = state.settings.lock().await;
        current.last_account_id = None;
        settings::save(&state.settings_path(), &current).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn get_profile_details(state: State<'_, AppState>) -> Result<ProfileDetails, String> {
    let token = active_access_token(&state).await?;
    profile::get_profile_details(&state.http, &token).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn upload_skin(
    state: State<'_, AppState>,
    variant: String,
    data_base64: String,
) -> Result<ProfileDetails, String> {
    let token = active_access_token(&state).await?;
    let data = base64::engine::general_purpose::STANDARD
        .decode(data_base64.as_bytes())
        .map_err(|e| e.to_string())?;
    profile::upload_skin(&state.http, &token, &variant, data)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn reset_skin(state: State<'_, AppState>) -> Result<(), String> {
    let token = active_access_token(&state).await?;
    profile::reset_skin(&state.http, &token).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_cape(state: State<'_, AppState>, cape_id: String) -> Result<ProfileDetails, String> {
    let token = active_access_token(&state).await?;
    profile::set_cape(&state.http, &token, &cape_id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_cape(state: State<'_, AppState>) -> Result<(), String> {
    let token = active_access_token(&state).await?;
    profile::remove_cape(&state.http, &token).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_player_skin_url(state: State<'_, AppState>, uuid: String) -> Result<Option<String>, String> {
    Ok(profile::fetch_public_skin_url(&state.http, &uuid).await)
}

/// Saves (or updates) a Microsoft account's refresh token after a
/// successful login/refresh, if one was issued. Microsoft may omit a fresh
/// refresh token from a `grant_type=refresh_token` response, in which case
/// the previously saved one keeps working and is left as-is.
pub(crate) async fn persist_account(state: &AppState, id: &str, client_id: &str, result: &msa::LoginResult) {
    let Some(refresh_token) = &result.refresh_token else {
        return;
    };
    let _ = accounts::upsert(
        &state.data_dir,
        SavedAccount {
            id: id.to_string(),
            username: result.profile.username.clone(),
            refresh_token: refresh_token.clone(),
            client_id: client_id.to_string(),
        },
    );
}
