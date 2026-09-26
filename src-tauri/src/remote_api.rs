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
        .route("/mods", get(list_mods).post(upload_mod).layer(upload_body_limit()))
        .route("/mods/{file_name}", delete(delete_mod))
        .route("/mods/{file_name}/info", get(mod_info))
        .route("/mods/{file_name}/toggle", post(toggle_mod))
        .route("/mods/updates", get(mod_updates))
        .route("/mods/updates/apply", post(apply_mod_update))
        .route("/mods/search", get(search_mods))
        .route("/mods/install", post(install_mod))
        .route("/resourcepacks", get(list_resourcepacks).post(upload_resourcepack).layer(upload_body_limit()))
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
        .route("/files", get(list_files))
        .route(
            "/files/content",
            get(read_file)
                .put(write_file)
                .layer(axum::extract::DefaultBodyLimit::max(4 * 1024 * 1024)),
        )
        .route("/ops", get(list_ops).post(add_op))
        .route("/ops/{name}", delete(remove_op))
        .route("/whitelist", get(list_whitelist).post(add_whitelist))
        .route("/whitelist/{name}", delete(remove_whitelist))
        .route("/whitelist-state", get(whitelist_state).post(set_whitelist_state))
        .route("/server-info", get(server_info))
        .route("/bans", get(list_bans).post(add_ban))
        .route("/bans/{name}", delete(remove_ban))
        .route("/start", post(start))
        .route("/stop", post(stop))
        .route("/restart", post(restart))
        .route("/restart/cancel", post(cancel_restart))
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
    // The Mint Connect tunnel endpoint (see tunnel.rs), while it's switched
    // on - independent of the port listener above, since it just forwards
    // into it.
    let mut tunnel: Option<iroh::Endpoint> = None;
    let tunnel_port = std::sync::Arc::new(std::sync::atomic::AtomicU16::new(0));
    loop {
        let (enabled, port, tunnel_wanted) = {
            let state = app.state::<AppState>();
            let settings = state.settings.lock().await;
            (
                settings.remote_admin_enabled,
                settings.remote_admin_port,
                settings.remote_admin_enabled && settings.remote_admin_tunnel_enabled,
            )
        };

        tunnel_port.store(port, std::sync::atomic::Ordering::Relaxed);
        if tunnel_wanted && tunnel.is_none() {
            // A failure (no network yet, say) just retries on the next tick.
            let data_dir = app.state::<AppState>().data_dir.clone();
            tunnel = crate::tunnel::start_host(&data_dir, tunnel_port.clone()).await.ok();
        } else if !tunnel_wanted {
            if let Some(endpoint) = tunnel.take() {
                endpoint.close().await;
            }
        }

        // Expired sessions were only ever dropped when that same token was
        // presented again, so ones never reused (an admin who just closed
        // their Mint) piled up in memory for the life of the process.
        {
            let state = app.state::<AppState>();
            let now = std::time::Instant::now();
            state.remote_sessions.lock().await.retain(|_, s| s.expires_at > now);
        }

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
    commands::instances::check_mod_updates(app.clone(), state, id)
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

/// Axum caps every request body at 2MB by default, *before* a handler ever
/// runs - so the 300MB check inside the upload handlers never got a chance,
/// and any jar over 2MB failed with a 413. Raised only on the two upload
/// routes (with a little headroom for the multipart framing), so every other
/// route - `/auth/login` included, which is reachable before any auth - keeps
/// the small default.
fn upload_body_limit() -> axum::extract::DefaultBodyLimit {
    axum::extract::DefaultBodyLimit::max(MAX_MOD_UPLOAD_BYTES + 1024 * 1024)
}

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
    commands::instances::check_resourcepack_updates(app.clone(), state, id)
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

/// Same two patterns the local console filters (see src/lib/consoleFilter.ts):
/// "...of a max of N players online..." and "...time is N".
fn is_mint_poll_line(line: &str) -> bool {
    let players = line
        .split_once("of a max of ")
        .is_some_and(|(_, rest)| rest.trim_start().starts_with(|c: char| c.is_ascii_digit()) && rest.contains("players online"));
    let time = line
        .split_once("time is ")
        .is_some_and(|(_, rest)| rest.trim_start().starts_with(|c: char| c.is_ascii_digit()));
    players || time
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
            // Mint's own `list`/`time query gametime` polling responses are
            // noise to a remote admin too (their console hides them as well) -
            // dropping them here also saves sending them over the wire every
            // few seconds, and covers admins on an older build.
            if is_mint_poll_line(line) {
                return;
            }
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
    /// Whether the owner gave *this* admin full access (see `Settings.
    /// remote_admin_full_access_uuids`) - lets their UI show the raw console
    /// input and Files tab only when they'd actually work, rather than
    /// offering controls that just answer 403.
    full_access: bool,
}

/// The authoritative "is it running" signal `running_instances` already is
/// for the local UI - kept accurate regardless of console/stdin access by
/// `AppState::watch_for_dead_instances`. Deliberately separate from
/// `/players`, which only works while Mint holds this instance's console and
/// would otherwise wrongly report "stopped" for a server that's genuinely
/// running but lost its console link to an app restart.
async fn status(State(app): State<AppHandle>, headers: HeaderMap) -> Result<Json<StatusInfo>, ApiError> {
    let session = require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    let running = state.running_instances.lock().await.contains_key(&id);
    let full_access = has_full_access(&app, &session).await;
    Ok(Json(StatusInfo { running, full_access }))
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

/// What the remote Players/Chat tabs send on their own - always allowed for
/// any op admin. See `send_command`.
const MODERATION_COMMANDS: &[&str] = &["kick", "ban", "ban-ip", "pardon", "pardon-ip", "op", "deop", "whitelist"];

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
    let session = require_session(&app, &headers).await?;
    // Every admin gets the moderation commands the Players/Chat tabs use;
    // anything else typed into a raw console (`stop`, `gamerule`, `execute`,
    // `save-off`, ...) needs the owner's full-access switch.
    let verb = body.command.trim_start().trim_start_matches('/').split_whitespace().next().unwrap_or("").to_lowercase();
    if !MODERATION_COMMANDS.contains(&verb.as_str()) {
        require_full_access(&app, &session).await?;
    }
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::launch::send_instance_command(state, id, body.command)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    Ok(StatusCode::NO_CONTENT)
}

/// Whether the owner handed *this* admin full access (see `Settings.
/// remote_admin_full_access_uuids`) and they're still an op on the shared
/// server - checked per request rather than baked into the session, so
/// revoking either one takes effect immediately for an already-connected
/// admin without restarting the API or waiting out their session.
async fn has_full_access(app: &AppHandle, session: &RemoteSession) -> bool {
    let normalize = |u: &str| u.replace('-', "").to_lowercase();
    let state = app.state::<AppState>();
    let granted = state
        .settings
        .lock()
        .await
        .remote_admin_full_access_uuids
        .iter()
        .any(|u| normalize(u) == normalize(&session.uuid));
    if !granted {
        return false;
    }
    let Ok((inst, _)) = shared_instance(app).await else {
        return false;
    };
    crate::minecraft::server_admin::read_ops(&inst.game_dir(&state.instances_dir()))
        .iter()
        .any(|o| normalize(&o.uuid) == normalize(&session.uuid))
}

async fn require_full_access(app: &AppHandle, session: &RemoteSession) -> Result<(), ApiError> {
    if has_full_access(app, session).await {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            "The server owner hasn't given you full access".to_string(),
        ))
    }
}

/// Resolves a client-supplied relative path against the server folder,
/// refusing anything that could land outside it: `..`, absolute paths and
/// drive prefixes are rejected outright, and the final canonicalized path
/// (symlinks resolved) must still sit under the canonicalized root.
fn resolve_server_path(root: &std::path::Path, rel: &str) -> Result<std::path::PathBuf, ApiError> {
    use std::path::Component;
    let root = root
        .canonicalize()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let mut path = root.clone();
    for component in std::path::Path::new(rel).components() {
        match component {
            Component::Normal(part) => path.push(part),
            Component::CurDir => {}
            _ => return Err((StatusCode::BAD_REQUEST, "Invalid path".to_string())),
        }
    }
    let resolved = path
        .canonicalize()
        .map_err(|_| (StatusCode::NOT_FOUND, "No such file or folder".to_string()))?;
    if !resolved.starts_with(&root) {
        return Err((StatusCode::FORBIDDEN, "That path is outside the server folder".to_string()));
    }
    Ok(resolved)
}

#[derive(Debug, Deserialize)]
struct FilesQuery {
    #[serde(default)]
    path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FileEntry {
    name: String,
    is_dir: bool,
    size: u64,
}

const MAX_LISTED_ENTRIES: usize = 5000;
/// Larger files are cut down to their *last* this-many bytes (the useful end
/// of a growing log) rather than refused or streamed whole to a text viewer.
const MAX_VIEW_BYTES: u64 = 2 * 1024 * 1024;

async fn list_files(
    State(app): State<AppHandle>,
    headers: HeaderMap,
    Query(q): Query<FilesQuery>,
) -> Result<Json<Vec<FileEntry>>, ApiError> {
    let session = require_session(&app, &headers).await?;
    require_full_access(&app, &session).await?;
    let (inst, _) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    let dir = resolve_server_path(&inst.game_dir(&state.instances_dir()), &q.path)?;
    if !dir.is_dir() {
        return Err((StatusCode::BAD_REQUEST, "Not a folder".to_string()));
    }
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(&dir)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .flatten()
        .take(MAX_LISTED_ENTRIES)
    {
        // Follows symlinks (metadata, not file_type) so a link to a folder
        // inside the server shows as a folder - anything it points outside
        // the server folder is still refused when actually opened.
        let Ok(meta) = std::fs::metadata(entry.path()) else {
            continue;
        };
        entries.push(FileEntry {
            name: entry.file_name().to_string_lossy().into_owned(),
            is_dir: meta.is_dir(),
            size: if meta.is_dir() { 0 } else { meta.len() },
        });
    }
    entries.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    Ok(Json(entries))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FileContent {
    content: String,
    size: u64,
    /// True when `content` is only the tail of a file bigger than
    /// `MAX_VIEW_BYTES`.
    truncated: bool,
    /// Whether saving an edit back would be lossless: the whole file was
    /// read and it's valid UTF-8 (a lossy decode would silently mangle
    /// other bytes on write).
    editable: bool,
}

async fn read_file(
    State(app): State<AppHandle>,
    headers: HeaderMap,
    Query(q): Query<FilesQuery>,
) -> Result<Json<FileContent>, ApiError> {
    use std::io::{Read, Seek, SeekFrom};
    let session = require_session(&app, &headers).await?;
    require_full_access(&app, &session).await?;
    let (inst, _) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    let path = resolve_server_path(&inst.game_dir(&state.instances_dir()), &q.path)?;
    if !path.is_file() {
        return Err((StatusCode::BAD_REQUEST, "Not a file".to_string()));
    }
    let io_err = |e: std::io::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    let mut file = std::fs::File::open(&path).map_err(io_err)?;
    let size = file.metadata().map_err(io_err)?.len();
    let truncated = size > MAX_VIEW_BYTES;
    if truncated {
        file.seek(SeekFrom::Start(size - MAX_VIEW_BYTES)).map_err(io_err)?;
    }
    let mut data = Vec::new();
    file.take(MAX_VIEW_BYTES).read_to_end(&mut data).map_err(io_err)?;
    // A NUL byte early on is a reliable "this isn't text" signal (jars, world
    // region files, images) - nothing useful to show in a text viewer.
    if data.iter().take(8192).any(|b| *b == 0) {
        return Err((StatusCode::UNSUPPORTED_MEDIA_TYPE, "This looks like a binary file".to_string()));
    }
    let editable = !truncated && std::str::from_utf8(&data).is_ok();
    Ok(Json(FileContent {
        content: String::from_utf8_lossy(&data).into_owned(),
        size,
        truncated,
        editable,
    }))
}

#[derive(Debug, Deserialize)]
struct WriteFileRequest {
    path: String,
    content: String,
}

/// Overwrites an *existing* text file - configs, `server.properties`,
/// ops/whitelist json and the like. Deliberately can't create files or touch
/// anything that isn't plain UTF-8 text (jars, world data), so a mistaken
/// save can't corrupt something the text viewer couldn't have shown anyway.
async fn write_file(
    State(app): State<AppHandle>,
    headers: HeaderMap,
    Json(body): Json<WriteFileRequest>,
) -> Result<StatusCode, ApiError> {
    let session = require_session(&app, &headers).await?;
    require_full_access(&app, &session).await?;
    let (inst, _) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    let path = resolve_server_path(&inst.game_dir(&state.instances_dir()), &body.path)?;
    if !path.is_file() {
        return Err((StatusCode::BAD_REQUEST, "Not an existing file".to_string()));
    }
    if body.content.len() as u64 > MAX_VIEW_BYTES {
        return Err((StatusCode::PAYLOAD_TOO_LARGE, "File is too large to edit here".to_string()));
    }
    let io_err = |e: std::io::Error| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string());
    let existing = std::fs::read(&path).map_err(io_err)?;
    if existing.len() as u64 > MAX_VIEW_BYTES || std::str::from_utf8(&existing).is_err() || existing.contains(&0) {
        return Err((StatusCode::UNSUPPORTED_MEDIA_TYPE, "Only plain text files can be edited".to_string()));
    }
    std::fs::write(&path, body.content.as_bytes()).map_err(io_err)?;
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

async fn cancel_restart(State(app): State<AppHandle>, headers: HeaderMap) -> Result<StatusCode, ApiError> {
    require_session(&app, &headers).await?;
    let (_, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    commands::launch::cancel_restart(state, id)
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteServerInfo {
    #[serde(flatten)]
    info: commands::instances::ServerInfo,
    public_ip: Option<String>,
    /// True when this admin isn't allowed the host's addresses (see below).
    ips_hidden: bool,
}

/// The Server tab's numbers. The host's IP addresses are only included for
/// full-access admins - a server behind a proxy (TCPShield and the like) may
/// be deliberately keeping its real address private even from its ops.
async fn server_info(State(app): State<AppHandle>, headers: HeaderMap) -> Result<Json<RemoteServerInfo>, ApiError> {
    let session = require_session(&app, &headers).await?;
    let (inst, _) = shared_instance(&app).await?;
    let full = has_full_access(&app, &session).await;
    let state = app.state::<AppState>();
    let instances_dir = state.instances_dir();
    let mut info = tauri::async_runtime::spawn_blocking(move || {
        commands::instances::collect_server_info(&inst, &instances_dir)
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let public_ip = if full {
        commands::instances::lookup_public_ip(&state.http).await.ok()
    } else {
        info.local_ip = None;
        None
    };
    Ok(Json(RemoteServerInfo { info, public_ip, ips_hidden: !full }))
}

#[derive(Debug, Serialize, Deserialize)]
struct WhitelistState {
    enabled: bool,
}

async fn whitelist_state(State(app): State<AppHandle>, headers: HeaderMap) -> Result<Json<WhitelistState>, ApiError> {
    require_session(&app, &headers).await?;
    let (inst, _) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    let props = crate::minecraft::server_properties::read_properties(&inst.game_dir(&state.instances_dir()));
    Ok(Json(WhitelistState { enabled: props.get("white-list").map(String::as_str) == Some("true") }))
}

/// Same as the local Players tab's toggle: saved to server.properties (so
/// it holds across restarts and works while stopped), plus `whitelist
/// on/off` through the console when running so it applies immediately. Any
/// op may do this - it's the same `whitelist` command they could run in game.
async fn set_whitelist_state(
    State(app): State<AppHandle>,
    headers: HeaderMap,
    Json(body): Json<WhitelistState>,
) -> Result<StatusCode, ApiError> {
    require_session(&app, &headers).await?;
    let (inst, id) = shared_instance(&app).await?;
    let state = app.state::<AppState>();
    let updates = HashMap::from([("white-list".to_string(), body.enabled.to_string())]);
    crate::minecraft::server_properties::write_properties(&inst.game_dir(&state.instances_dir()), &updates)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let running = state.running_instances.lock().await.contains_key(&id);
    if running {
        let command = if body.enabled { "whitelist on" } else { "whitelist off" };
        commands::launch::send_instance_command(app.state::<AppState>(), id, command.to_string())
            .await
            .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    }
    Ok(StatusCode::NO_CONTENT)
}
