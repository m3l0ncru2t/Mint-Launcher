//! The admin-side half of the remote admin feature (see `remote_api.rs` for
//! the host side) - proves the currently signed-in Minecraft account's
//! identity to a host machine via the same `joinServer`/`hasJoined`
//! handshake every vanilla client already performs when joining any server,
//! then remembers the resulting link so it can show up in the sidebar like
//! any other server. Everything after this initial connect (mods, console,
//! start/stop/restart) is called directly from the frontend via `fetch`/
//! `WebSocket` against the host's API - this module only covers the one step
//! that needs the signed-in account's real access token, which never leaves
//! this process except straight to Mojang.

use crate::auth::GameProfile;
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::State;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteServerLink {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub token: String,
    pub version_id: String,
    pub loader: crate::instance::ModLoader,
    pub has_icon: bool,
    /// Set for a host linked through Mint Connect (see `tunnel.rs`) instead
    /// of a plain address - its `host`/`port` are then only placeholders on
    /// disk, replaced at load time with the live loopback proxy (see
    /// `resolve`) since that port changes every launch.
    #[serde(default)]
    pub node_id: Option<String>,
}

/// Points a Mint Connect link's host/port at its (lazily started) loopback
/// proxy. A no-op for an ordinary address link.
async fn resolve(state: &AppState, link: &mut RemoteServerLink) {
    if let Some(code) = link.node_id.clone() {
        if let Ok(port) = state.tunnel_client.proxy_port(&code).await {
            link.host = "127.0.0.1".to_string();
            link.port = port;
        }
    }
}

fn remote_servers_path(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join("remote_servers.json")
}

fn load(data_dir: &Path) -> Vec<RemoteServerLink> {
    std::fs::read_to_string(remote_servers_path(data_dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save(data_dir: &Path, links: &[RemoteServerLink]) -> std::io::Result<()> {
    std::fs::write(remote_servers_path(data_dir), serde_json::to_string_pretty(links)?)
}

#[tauri::command]
pub async fn list_remote_servers(state: State<'_, AppState>) -> Result<Vec<RemoteServerLink>, String> {
    let mut links = load(&state.data_dir);
    for link in links.iter_mut() {
        resolve(&state, link).await;
    }
    Ok(links)
}

#[tauri::command]
pub async fn remove_remote_server(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let mut links = load(&state.data_dir);
    if let Some(code) = links.iter().find(|l| l.id == id).and_then(|l| l.node_id.clone()) {
        state.tunnel_client.forget(&code).await;
    }
    links.retain(|l| l.id != id);
    save(&state.data_dir, &links).map_err(|e| e.to_string())
}

/// Mojang's actual error shape here is `{"error": "SomeException", "path":
/// "...", "errorMessage": "..."}` - `errorMessage` isn't always present (a
/// bad access token comes back as just `{"error":
/// "ForbiddenOperationException", "path": "..."}`, confirmed by hand against
/// the live endpoint), so this falls back to the bare exception name instead
/// of a completely generic message when it's missing.
#[derive(Debug, Deserialize)]
struct MojangJoinError {
    error: Option<String>,
    #[serde(rename = "errorMessage")]
    error_message: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginResponse {
    token: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RemoteInstanceInfo {
    name: String,
    version_id: String,
    loader: crate::instance::ModLoader,
    has_icon: bool,
}

/// Runs the join/hasJoined handshake against a host and returns the bearer
/// token it hands back - shared by `remote_connect` (a brand new link) and
/// `remote_reconnect` (an existing one whose token stopped working, most
/// often because the host's own app restarted - `remote_sessions` there is
/// deliberately in-memory only, so that alone invalidates every admin's
/// saved token with no other warning).
async fn login_handshake(client: &reqwest::Client, profile: &GameProfile, host: &str, port: u16) -> Result<String, String> {
    if profile.user_type != "msa" {
        return Err(
            "An offline account can't prove ownership to a remote server - sign in with a real Microsoft account"
                .to_string(),
        );
    }

    // Mojang's join endpoint validates this against roughly the shape vanilla
    // itself sends - a random 64-bit value as hex (`Long.toHexString`, so up
    // to 16 hex digits) - and rejects anything longer with "Invalid
    // serverId", confirmed live against the real endpoint. Only needs to be
    // unpredictable and single-use, not any particular length beyond that, so
    // the first 16 hex characters of a fresh UUID satisfy both.
    let server_id = uuid::Uuid::new_v4().simple().to_string()[..16].to_string();
    join_server(client, profile, &server_id).await.map_err(|e| e.to_string())?;

    let login_resp = client
        .post(format!("http://{host}:{port}/auth/login"))
        .json(&serde_json::json!({ "username": profile.username, "serverId": server_id }))
        .send()
        .await
        .map_err(|e| format!("Couldn't reach {host}:{port} - {e}"))?;
    if !login_resp.status().is_success() {
        let message = login_resp.text().await.unwrap_or_default();
        return Err(if message.is_empty() { "Login was rejected by the host".to_string() } else { message });
    }
    let login: LoginResponse = login_resp.json().await.map_err(|e| e.to_string())?;
    Ok(login.token)
}

/// Connects to a host machine's remote admin API for the first time,
/// proving the currently signed-in account's identity via Mojang's own
/// `joinServer`/`hasJoined` handshake - the real access token goes to Mojang
/// only, the host machine never sees it, only a one-time random `serverId`
/// and the username.
#[tauri::command]
pub async fn remote_connect(state: State<'_, AppState>, host: String, port: u16) -> Result<RemoteServerLink, String> {
    connect_common(&state, host, port, None).await
}

/// Links a host through Mint Connect using its connection code - no address,
/// VPN or port forwarding involved. Dials the host first so a wrong code or
/// an offline host is reported clearly, then runs the same handshake as
/// `remote_connect` over the loopback proxy.
#[tauri::command]
pub async fn remote_connect_code(state: State<'_, AppState>, code: String) -> Result<RemoteServerLink, String> {
    let code = crate::tunnel::normalize_code(&code)?;
    state.tunnel_client.probe(&code).await?;
    let port = state.tunnel_client.proxy_port(&code).await?;
    connect_common(&state, "127.0.0.1".to_string(), port, Some(code)).await
}

async fn connect_common(
    state: &AppState,
    host: String,
    port: u16,
    node_id: Option<String>,
) -> Result<RemoteServerLink, String> {
    let profile = state
        .active_profile
        .lock()
        .await
        .clone()
        .ok_or_else(|| "Sign in with a Microsoft account first".to_string())?;

    let token = login_handshake(&state.http, &profile, &host, port).await?;

    let instance_resp = state
        .http
        .get(format!("http://{host}:{port}/instance"))
        .bearer_auth(&token)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !instance_resp.status().is_success() {
        let message = instance_resp.text().await.unwrap_or_default();
        return Err(if message.is_empty() { "Couldn't load the shared server's details".to_string() } else { message });
    }
    let info: RemoteInstanceInfo = instance_resp.json().await.map_err(|e| e.to_string())?;

    let link = RemoteServerLink {
        id: uuid::Uuid::new_v4().to_string(),
        name: info.name,
        host,
        port,
        token,
        version_id: info.version_id,
        loader: info.loader,
        has_icon: info.has_icon,
        node_id,
    };

    // A Mint Connect link's loopback port is only good for this launch, so
    // what gets saved has placeholders instead (see `resolve`).
    let stored = match &link.node_id {
        Some(_) => RemoteServerLink { host: String::new(), port: 0, ..link.clone() },
        None => link.clone(),
    };
    let mut links = load(&state.data_dir);
    links.retain(|l| match (&l.node_id, &stored.node_id) {
        (Some(a), Some(b)) => a != b,
        (None, None) => !(l.host == stored.host && l.port == stored.port),
        _ => true,
    });
    links.push(stored);
    save(&state.data_dir, &links).map_err(|e| e.to_string())?;

    Ok(link)
}

/// Re-runs the same handshake for an *existing* saved link, replacing just
/// its token in place - what the frontend calls automatically the moment any
/// call to a host comes back 401 (see `remoteApi.ts`), so a saved connection
/// silently keeps working across the host restarting instead of forcing the
/// admin to disconnect and re-add it.
#[tauri::command]
pub async fn remote_reconnect(state: State<'_, AppState>, id: String) -> Result<RemoteServerLink, String> {
    let mut links = load(&state.data_dir);
    let existing = links
        .iter()
        .find(|l| l.id == id)
        .cloned()
        .ok_or_else(|| "That remote server was removed".to_string())?;

    let profile = state
        .active_profile
        .lock()
        .await
        .clone()
        .ok_or_else(|| "Sign in with a Microsoft account first".to_string())?;

    let mut reachable = existing.clone();
    resolve(&state, &mut reachable).await;
    let token = login_handshake(&state.http, &profile, &reachable.host, reachable.port).await?;

    for link in links.iter_mut() {
        if link.id == id {
            link.token = token.clone();
        }
    }
    save(&state.data_dir, &links).map_err(|e| e.to_string())?;

    Ok(RemoteServerLink { token, ..reachable })
}

async fn join_server(client: &reqwest::Client, profile: &GameProfile, server_id: &str) -> anyhow::Result<()> {
    let resp = client
        .post("https://sessionserver.mojang.com/session/minecraft/join")
        .json(&serde_json::json!({
            "accessToken": profile.access_token,
            "selectedProfile": profile.uuid.replace('-', ""),
            "serverId": server_id,
        }))
        .send()
        .await?;

    if resp.status().is_success() {
        return Ok(());
    }
    let err: Option<MojangJoinError> = resp.json().await.ok();
    let message = err.and_then(|e| e.error_message.or(e.error));
    anyhow::bail!(message.unwrap_or_else(|| "Mojang rejected this login".to_string()))
}
