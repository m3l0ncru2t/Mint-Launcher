//! The host-side remote admin API - lets a trusted admin's *own* Mint
//! Launcher installation connect over the network (a private tunnel like
//! Tailscale, never meant to be port-forwarded to the public internet) and
//! see/manage the one server instance shared via Settings, exactly the same
//! way this app's own local UI does.
//!
//! Authorization is tied to the server's own `ops.json` rather than a
//! separate login system: an admin proves who they are via the same
//! `joinServer`/`hasJoined` handshake every vanilla client already performs
//! when joining any server (see `commands::remote_client` for the half of
//! this that runs on the *admin's* Mint). Their own Minecraft access token
//! never reaches this machine - only Mojang ever sees it.

use crate::commands;
use crate::instance::{self, Instance};
use crate::state::{AppState, RemoteSession};
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Multipart, Path, Query, State, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::{AppHandle, Listener, Manager};
use tower_http::cors::CorsLayer;

type ApiError = (StatusCode, String);

fn build_router(app: AppHandle) -> Router {
    Router::new()
        .route("/auth/login", post(login))
        .route("/instance", get(get_instance))
        .route("/icon", get(icon))
        .route("/mods", get(list_mods).post(upload_mod))
        .route("/mods/{file_name}", delete(delete_mod))
        .route("/mods/{file_name}/info", get(mod_info))
        .route("/mods/{file_name}/toggle", post(toggle_mod))
        .route("/mods/updates", get(mod_updates))
        .route("/mods/updates/apply", post(apply_mod_update))
        .route("/mods/search", get(search_mods))
        .route("/mods/install", post(install_mod))
        .route("/resourcepacks", get(list_resourcepacks).post(upload_resourcepack))
        .route("/resourcepacks/{file_name}", delete(delete_resourcepack))
        .route("/resourcepacks/{file_name}/toggle", post(toggle_resourcepack))
        .route("/resourcepacks/updates", get(resourcepack_updates))
        .route("/resourcepacks/updates/apply", post(apply_resourcepack_update))
        .route("/resourcepacks/search", get(search_resourcepacks))
        .route("/resourcepacks/install", post(install_resourcepack))
        .route("/console", get(console_tail))
        .route("/console/ws", get(console_ws))
        .route("/status", get(status))
        .route("/stats", get(stats))
        .route("/players", get(players))
        .route("/command", post(send_command))
        .route("/ops", get(list_ops).post(add_op))
        .route("/ops/{name}", delete(remove_op))
        .route("/whitelist", get(list_whitelist).post(add_whitelist))
        .route("/whitelist/{name}", delete(remove_whitelist))
        .route("/bans", get(list_bans).post(add_ban))
        .route("/bans/{name}", delete(remove_ban))
        .route("/start", post(start))
        .route("/stop", post(stop))
        .route("/restart", post(restart))
        .route("/kill", post(kill))
        .layer(CorsLayer::permissive())
        .with_state(app)
}

/// Runs forever, polling `Settings` every couple of seconds and (re)starting
/// or stopping the actual listener to match - the same "background task
/// watches settings" shape `AppState::watch_for_dead_instances` already uses
/// for a different concern. Simpler than wiring a start/stop signal through
/// every settings-setter command, and means toggling the port while it's
/// already running just works.
pub async fn run_supervisor(app: AppHandle) {
    let mut current: Option<(tokio::task::JoinHandle<()>, u16)> = None;
    loop {
        let (enabled, port) = {
            let state = app.state::<AppState>();
            let settings = state.settings.lock().await;
            (settings.remote_admin_enabled, settings.remote_admin_port)
        };

        let needs_restart = match &current {
            Some((_, running_port)) => enabled && *running_port != port,
            None => false,
        };

        if (!enabled || needs_restart) && current.is_some() {
            if let Some((handle, _)) = current.take() {
                handle.abort();
            }
        }

        if enabled && current.is_none() {
            let router = build_router(app.clone());
            match tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await {
                Ok(listener) => {
                    let handle = tokio::spawn(async move {
                        let _ = axum::serve(listener, router).await;
                    });
                    current = Some((handle, port));
                }
                Err(_) => {
                    // Port already in use, or no permission to bind it -
                    // just try again next tick rather than crashing the app.
                }
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
}

fn generate_token() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

async fn require_session(app: &AppHandle, headers: &HeaderMap) -> Result<RemoteSession, ApiError> {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or((StatusCode::UNAUTHORIZED, "Missing bearer token".to_string()))?;

    let state = app.state::<AppState>();
    let mut sessions = state.remote_sessions.lock().await;
    let Some(session) = sessions.get(token).cloned() else {
        return Err((StatusCode::UNAUTHORIZED, "Invalid or expired session - reconnect".to_string()));
    };
    if session.expires_at < std::time::Instant::now() {
        sessions.remove(token);
        return Err((StatusCode::UNAUTHORIZED, "Session expired - reconnect".to_string()));
    }
    Ok(session)
}

async fn require_session_token(app: &AppHandle, token: &str) -> Result<RemoteSession, ApiError> {
    let state = app.state::<AppState>();
    let sessions = state.remote_sessions.lock().await;
    let Some(session) = sessions.get(token).cloned() else {
        return Err((StatusCode::UNAUTHORIZED, "Invalid or expired session".to_string()));
    };
    if session.expires_at < std::time::Instant::now() {
        return Err((StatusCode::UNAUTHORIZED, "Session expired".to_string()));
    }
    Ok(session)
}

/// The one server instance currently shared over this API - v1 only ever
/// supports sharing a single instance at a time (see `Settings.
/// remote_admin_instance_id`).
async fn shared_instance(app: &AppHandle) -> Result<(Instance, String), ApiError> {
    let state = app.state::<AppState>();
    let instance_id = state
        .settings
        .lock()
        .await
        .remote_admin_instance_id
        .clone()
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "No server is shared for remote admin".to_string()))?;
    let inst = instance::get_instance(&state.instances_dir(), &instance_id)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "The shared instance no longer exists".to_string()))?;
    Ok((inst, instance_id))
}

/// Confirms an admin's Mint really does control the Minecraft account it
/// claims, the same way any Minecraft server confirms a joining client does:
/// the admin's own Mint already called Mojang's `joinServer` with this exact
/// `server_id` and their own access token (see `commands::remote_client::
/// connect`) - if Mojang's `hasJoined` now agrees, this genuinely is that
/// account, and this machine never had to see the access token itself.
async fn has_joined(client: &reqwest::Client, username: &str, server_id: &str) -> anyhow::Result<String> {
    let resp = client
        .get("https://sessionserver.mojang.com/session/minecraft/hasJoined")
        .query(&[("username", username), ("serverId", server_id)])
        .send()
        .await?;
    let text = resp.text().await?;
    if text.trim().is_empty() || text.trim() == "null" {
        anyhow::bail!("Mojang didn't confirm this account joined - try signing in again.");
    }
    let json: serde_json::Value = serde_json::from_str(&text)?;
    let id = json
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Unexpected response from Mojang"))?;
    Ok(crate::minecraft::server_admin::dash_uuid(id))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginRequest {
    username: String,
    server_id: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LoginResponse {
    token: String,
    uuid: String,
    username: String,
}

async fn login(State(app): State<AppHandle>, Json(body): Json<LoginRequest>) -> Result<Json<LoginResponse>, ApiError> {
    let (inst, _) = shared_instance(&app).await?;
    let state = app.state::<AppState>();

    let uuid = has_joined(&state.http, &body.username, &body.server_id)
        .await
        .map_err(|e| (StatusCode::UNAUTHORIZED, e.to_string()))?;

    let ops = crate::minecraft::server_admin::read_ops(&inst.game_dir(&state.instances_dir()));
    if !ops.iter().any(|o| o.uuid.eq_ignore_ascii_case(&uuid)) {
        return Err((
            StatusCode::FORBIDDEN,
            "This account isn't an operator on the shared server".to_string(),
        ));
    }

    let token = generate_token();
    state.remote_sessions.lock().await.insert(
        token.clone(),
        RemoteSession {
            uuid: uuid.clone(),
            username: body.username.clone(),
            expires_at: std::time::Instant::now() + std::time::Duration::from_secs(6 * 3600),
        },
    );

    Ok(Json(LoginResponse { token, uuid, username: body.username }))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteInstanceInfo {
    id: String,
    name: String,
    version_id: String,
    loader: instance::ModLoader,
    has_icon: bool,
}

async fn get_instance(
    State(app): State<AppHandle>,
    headers: HeaderMap,
) -> Result<Json<RemoteInstanceInfo>, ApiError> {
    require_session(&app, &headers).await?;
    let (inst, id) = shared_instance(&app).await?;
    Ok(Json(RemoteInstanceInfo {
        id,
        name: inst.name,
        version_id: inst.version_id,
        loader: inst.loader,
        has_icon: inst.has_icon,
    }))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct IconInfo {
    icon_url: Option<String>,
}

async fn icon(State(app): State<AppHandle>, headers: HeaderMap) -> Result<Json<IconInfo>, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    let icon_url = commands::instances::get_server_icon(state, id).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    Ok(Json(IconInfo { icon_url }))
}

async fn list_mods(
    State(app): State<AppHandle>,
    headers: HeaderMap,
) -> Result<Json<Vec<commands::instances::ModFile>>, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::instances::list_mods(state, id)
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

async fn delete_mod(
    State(app): State<AppHandle>,
    headers: HeaderMap,
    Path(file_name): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::instances::delete_mod(state, id, file_name).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct ToggleRequest {
    enabled: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ToggleResponse {
    file_name: String,
}

async fn toggle_mod(
    State(app): State<AppHandle>,
    headers: HeaderMap,
    Path(file_name): Path<String>,
    Json(body): Json<ToggleRequest>,
) -> Result<Json<ToggleResponse>, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    let new_file_name =
        commands::instances::toggle_mod(state, id, file_name, body.enabled).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(Json(ToggleResponse { file_name: new_file_name }))
}

#[derive(Debug, Deserialize)]
struct SearchParams {
    query: String,
    #[serde(default)]
    offset: u32,
}

async fn search_mods(
    State(app): State<AppHandle>,
    headers: HeaderMap,
    Query(params): Query<SearchParams>,
) -> Result<Json<crate::minecraft::modrinth::ModSearchPage>, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::instances::search_mods(state, id, params.query, params.offset)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallRequest {
    project_id: String,
}

async fn install_mod(
    State(app): State<AppHandle>,
    headers: HeaderMap,
    Json(body): Json<InstallRequest>,
) -> Result<Json<crate::minecraft::modrinth::InstallSummary>, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::instances::install_mod(state, id, body.project_id)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

async fn mod_info(
    State(app): State<AppHandle>,
    headers: HeaderMap,
    Path(file_name): Path<String>,
) -> Result<Json<crate::minecraft::modrinth::ModDetails>, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::instances::get_mod_info(state, id, file_name)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

async fn mod_updates(
    State(app): State<AppHandle>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::minecraft::modrinth::ModUpdateInfo>>, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::instances::check_mod_updates(state, id)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ApplyModUpdateRequest {
    old_file_name: String,
    download_url: String,
}

async fn apply_mod_update(
    State(app): State<AppHandle>,
    headers: HeaderMap,
    Json(body): Json<ApplyModUpdateRequest>,
) -> Result<StatusCode, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::instances::apply_mod_update(state, id, body.old_file_name, body.download_url)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(StatusCode::NO_CONTENT)
}

/// 300MB - generous for a fat mod jar, but still a hard ceiling so a
/// misbehaving/malicious upload can't fill the disk.
const MAX_MOD_UPLOAD_BYTES: usize = 300 * 1024 * 1024;

async fn upload_mod(
    State(app): State<AppHandle>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<StatusCode, ApiError> {
    require_session(&app, &headers).await?;
    let (inst, _) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    let mods_dir = inst.mods_dir(&state.instances_dir());
    std::fs::create_dir_all(&mods_dir).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut wrote_one = false;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
    {
        let Some(file_name) = field.file_name().map(str::to_string) else {
            continue;
        };
        if !file_name.to_lowercase().ends_with(".jar") {
            return Err((StatusCode::BAD_REQUEST, "Only .jar files are allowed".to_string()));
        }
        let dest = commands::instances::safe_child(&mods_dir, &file_name).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
        let data = field.bytes().await.map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
        if data.len() > MAX_MOD_UPLOAD_BYTES {
            return Err((StatusCode::PAYLOAD_TOO_LARGE, "Mod file is too large".to_string()));
        }
        std::fs::write(&dest, &data).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        wrote_one = true;
    }
    if !wrote_one {
        return Err((StatusCode::BAD_REQUEST, "No file in the upload".to_string()));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn list_resourcepacks(
    State(app): State<AppHandle>,
    headers: HeaderMap,
) -> Result<Json<Vec<commands::instances::ResourcePackFile>>, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::instances::list_resourcepacks(state, id)
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

async fn delete_resourcepack(
    State(app): State<AppHandle>,
    headers: HeaderMap,
    Path(file_name): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::instances::delete_resourcepack(state, id, file_name).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn toggle_resourcepack(
    State(app): State<AppHandle>,
    headers: HeaderMap,
    Path(file_name): Path<String>,
    Json(body): Json<ToggleRequest>,
) -> Result<StatusCode, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::instances::toggle_resourcepack(state, id, file_name, body.enabled)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn search_resourcepacks(
    State(app): State<AppHandle>,
    headers: HeaderMap,
    Query(params): Query<SearchParams>,
) -> Result<Json<crate::minecraft::modrinth::ModSearchPage>, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::instances::search_resourcepacks(state, id, params.query, params.offset)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

async fn install_resourcepack(
    State(app): State<AppHandle>,
    headers: HeaderMap,
    Json(body): Json<InstallRequest>,
) -> Result<Json<crate::minecraft::modrinth::InstalledModInfo>, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::instances::install_resourcepack(state, id, body.project_id)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

async fn resourcepack_updates(
    State(app): State<AppHandle>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::minecraft::modrinth::ModUpdateInfo>>, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::instances::check_resourcepack_updates(state, id)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

async fn apply_resourcepack_update(
    State(app): State<AppHandle>,
    headers: HeaderMap,
    Json(body): Json<ApplyModUpdateRequest>,
) -> Result<StatusCode, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::instances::apply_resourcepack_update(state, id, body.old_file_name, body.download_url)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn upload_resourcepack(
    State(app): State<AppHandle>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<StatusCode, ApiError> {
    require_session(&app, &headers).await?;
    let (inst, _) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    let dir = inst.resourcepacks_dir(&state.instances_dir());
    std::fs::create_dir_all(&dir).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut wrote_one = false;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?
    {
        let Some(file_name) = field.file_name().map(str::to_string) else {
            continue;
        };
        if !file_name.to_lowercase().ends_with(".zip") {
            return Err((StatusCode::BAD_REQUEST, "Only .zip files are allowed".to_string()));
        }
        let dest = commands::instances::safe_child(&dir, &file_name).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
        let data = field.bytes().await.map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
        if data.len() > MAX_MOD_UPLOAD_BYTES {
            return Err((StatusCode::PAYLOAD_TOO_LARGE, "Resource pack file is too large".to_string()));
        }
        std::fs::write(&dest, &data).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        wrote_one = true;
    }
    if !wrote_one {
        return Err((StatusCode::BAD_REQUEST, "No file in the upload".to_string()));
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn console_tail(State(app): State<AppHandle>, headers: HeaderMap) -> Result<String, ApiError> {
    require_session(&app, &headers).await?;
    let (inst, _) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    let log_path = inst.game_dir(&state.instances_dir()).join("logs").join("latest.log");
    Ok(commands::launch::tail_lines(&log_path, 65536))
}

async fn console_ws(
    State(app): State<AppHandle>,
    Query(params): Query<HashMap<String, String>>,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let token = params
        .get("token")
        .cloned()
        .ok_or((StatusCode::UNAUTHORIZED, "Missing token".to_string()))?;
    require_session_token(&app, &token).await?;
    let (_, instance_id) = shared_instance(&app).await?;
    Ok(ws.on_upgrade(move |socket| stream_console(app, instance_id, socket)))
}

/// Bridges the same `instance-log` event Tauri already emits internally (for
/// the local console tab) onto a WebSocket - `app.listen_any` works for any
/// Rust-side subscriber, not just the frontend, so no separate log-tailing
/// mechanism is needed here.
async fn stream_console(app: AppHandle, instance_id: String, mut socket: WebSocket) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let target_id = instance_id.clone();
    let listener_id = app.listen_any("instance-log", move |event| {
        let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) else {
            return;
        };
        if payload.get("instanceId").and_then(|v| v.as_str()) != Some(target_id.as_str()) {
            return;
        }
        if let Some(line) = payload.get("line").and_then(|v| v.as_str()) {
            let _ = tx.send(line.to_string());
        }
    });

    while let Some(line) = rx.recv().await {
        if socket.send(Message::Text(line.into())).await.is_err() {
            break;
        }
    }
    app.unlisten(listener_id);
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusInfo {
    running: bool,
}

/// The authoritative "is it running" signal `running_instances` already is
/// for the local UI - kept accurate regardless of console/stdin access by
/// `AppState::watch_for_dead_instances`. Deliberately separate from
/// `/players`, which only works while Mint holds this instance's console and
/// would otherwise wrongly report "stopped" for a server that's genuinely
/// running but lost its console link to an app restart.
async fn status(State(app): State<AppHandle>, headers: HeaderMap) -> Result<Json<StatusInfo>, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    let running = state.running_instances.lock().await.contains_key(&id);
    Ok(Json(StatusInfo { running }))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteStats {
    cpu_percent: f32,
    memory_mb: u64,
    memory_limit_mb: u32,
    system_used_memory_mb: u64,
    system_total_memory_mb: u64,
    /// `None` until a second `time query gametime` sample has been taken
    /// (see `commands::launch::get_server_tps`) - same "first read is empty"
    /// shape the local running card already handles.
    tps: Option<f64>,
    mspt: Option<f64>,
}

/// CPU/memory reuses `get_process_stats`, which just needs the tracked pid
/// (available cross-user without any special permission, unlike the console
/// access TPS/players need) - `memory_limit_mb` is included alongside so the
/// client doesn't need a separate `/instance` round-trip just to show
/// "used / limit".
async fn stats(State(app): State<AppHandle>, headers: HeaderMap) -> Result<Json<RemoteStats>, ApiError> {
    require_session(&app, &headers).await?;
    let (inst, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    let pid = state
        .running_instances
        .lock()
        .await
        .get(&id)
        .map(|r| r.pid)
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "Server isn't running".to_string()))?;

    let process = commands::launch::get_process_stats(app.state::<AppState>(), pid)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let tps_info = commands::launch::get_server_tps(app.state::<AppState>(), id).await.ok().flatten();

    Ok(Json(RemoteStats {
        cpu_percent: process.cpu_percent,
        memory_mb: process.memory_mb,
        memory_limit_mb: inst.memory_mb,
        system_used_memory_mb: process.system_used_memory_mb,
        system_total_memory_mb: process.system_total_memory_mb,
        tps: tps_info.as_ref().map(|t| t.tps),
        mspt: tps_info.as_ref().map(|t| t.mspt),
    }))
}

async fn players(
    State(app): State<AppHandle>,
    headers: HeaderMap,
) -> Result<Json<crate::minecraft::server_ping::ServerStatus>, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::launch::list_online_players(state, id)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))
}

#[derive(Debug, Deserialize)]
struct CommandRequest {
    command: String,
}

/// Runs a raw console command - what the kick/ban/op/deop/whitelist buttons
/// in the Players tab send while the server is running, exactly like the
/// local `PlayersPanel` does via `sendInstanceCommand`. Already
/// stdin-or-RCON (see `commands::launch::write_console_line`), so this works
/// the same whether or not the host's own console link is currently alive.
async fn send_command(
    State(app): State<AppHandle>,
    headers: HeaderMap,
    Json(body): Json<CommandRequest>,
) -> Result<StatusCode, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::launch::send_instance_command(state, id, body.command)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct UsernameRequest {
    username: String,
}

/// The four list endpoints below mirror `PlayersPanel`'s "while stopped,
/// edit ops.json/whitelist.json/banned-players.json directly" path (see
/// `commands::instances::ensure_not_running`) - for managing the whitelist
/// before ever starting the server, say. While it's running, the frontend
/// uses `send_command` above instead, same as the local UI does.
async fn list_ops(
    State(app): State<AppHandle>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::minecraft::server_admin::OpEntry>>, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::instances::get_ops(state, id).map(Json).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

async fn add_op(
    State(app): State<AppHandle>,
    headers: HeaderMap,
    Json(body): Json<UsernameRequest>,
) -> Result<StatusCode, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::instances::add_op_entry(state, id, body.username).await.map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_op(
    State(app): State<AppHandle>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::instances::remove_op_entry(state, id, name).await.map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_whitelist(
    State(app): State<AppHandle>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::minecraft::server_admin::WhitelistEntry>>, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::instances::get_whitelist(state, id).map(Json).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

async fn add_whitelist(
    State(app): State<AppHandle>,
    headers: HeaderMap,
    Json(body): Json<UsernameRequest>,
) -> Result<StatusCode, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::instances::add_whitelist_entry(state, id, body.username)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_whitelist(
    State(app): State<AppHandle>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::instances::remove_whitelist_entry(state, id, name).await.map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_bans(
    State(app): State<AppHandle>,
    headers: HeaderMap,
) -> Result<Json<Vec<crate::minecraft::server_admin::BannedPlayerEntry>>, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::instances::get_banned_players(state, id).map(Json).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

async fn add_ban(
    State(app): State<AppHandle>,
    headers: HeaderMap,
    Json(body): Json<UsernameRequest>,
) -> Result<StatusCode, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::instances::add_ban_entry(state, id, body.username, String::new())
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_ban(
    State(app): State<AppHandle>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Result<StatusCode, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::instances::unban_player_entry(state, id, name).await.map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn start(State(app): State<AppHandle>, headers: HeaderMap) -> Result<StatusCode, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::launch::launch_instance(app.clone(), state, id, None)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn stop(State(app): State<AppHandle>, headers: HeaderMap) -> Result<StatusCode, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::launch::stop_instance(app.clone(), state, id)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn restart(State(app): State<AppHandle>, headers: HeaderMap) -> Result<StatusCode, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    // Same countdown-then-relaunch `restart_instance` the local Restart
    // button calls, so a remote admin's restart gives players the same
    // in-game heads-up a local one does - this request simply stays open
    // for the ~60s countdown before responding.
    commands::launch::restart_instance(app.clone(), app.state::<AppState>(), id)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn kill(State(app): State<AppHandle>, headers: HeaderMap) -> Result<StatusCode, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::launch::kill_instance(app.clone(), state, id)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(StatusCode::NO_CONTENT)
}
