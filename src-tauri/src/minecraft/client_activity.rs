//! What a running client is doing - which server it joined, or which
//! singleplayer world it's in - read back off its own log, since the game
//! offers no other way to ask. Vanilla logs joining a server ("Connecting to
//! host, port", then "Loaded N advancements" once in), being disconnected
//! (with the reason) and starting a singleplayer world, but not leaving a
//! server on purpose - so a joined server stays the answer until the next
//! join, world or disconnect.

use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ClientState {
    #[default]
    Menu,
    Joining,
    Server,
    /// Joining failed (wrong version, login failed, ...).
    Failed,
    /// Kicked, timed out, or the server went away.
    Disconnected,
    Singleplayer,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientActivity {
    pub state: ClientState,
    /// The signed-in player ("Setting user: ...").
    pub player: Option<String>,
    /// The address as typed, port left off when it's the default.
    pub server: Option<String>,
    /// That address's name in the instance's server list, if it's there.
    pub server_name: Option<String>,
    pub world: Option<String>,
    /// Time of day `state` began, as the log has it ("22:34").
    pub since: Option<String>,
    pub reason: Option<String>,
}

/// Replays `log` (a client's latest.log) and returns where it ended up.
pub fn parse(log: &str) -> ClientActivity {
    let mut a = ClientActivity::default();
    for line in log.lines() {
        let Some((head, msg)) = line.split_once("]: ") else { continue };
        let time = head.get(1..6).filter(|t| t.as_bytes().get(2) == Some(&b':')).map(str::to_string);
        let server_thread = head.contains("[Server thread/");

        if let Some(name) = msg.strip_prefix("Setting user: ") {
            a.player = Some(name.trim().to_string());
        } else if let Some(target) = msg.strip_prefix("Connecting to ") {
            let Some((host, port)) = target.trim().rsplit_once(", ") else { continue };
            let server = if port == "25565" { host.to_string() } else { format!("{host}:{port}") };
            a = ClientActivity { state: ClientState::Joining, server: Some(server), since: time, player: a.player, ..Default::default() };
        } else if msg.starts_with("Loaded ") && msg.ends_with(" advancements") {
            if a.state == ClientState::Joining {
                a.state = ClientState::Server;
                a.since = time;
            }
        } else if let Some(explained) = msg.strip_prefix("Disconnect explained: ") {
            // "The server closed the connection -> The server": what, then whose side.
            let reason = explained.split(" -> ").next().unwrap_or(explained).trim();
            if let Some(state) = ended(a.state) {
                a.state = state;
                a.reason = Some(reason.to_string());
                a.since = time;
            }
        } else if let Some(reason) = msg.strip_prefix("Client disconnected with reason: ") {
            // Logged after "Disconnect explained" (which has the better
            // reason), or alone - "Disconnected" alone is leaving on purpose.
            if let Some(state) = ended(a.state) {
                if reason.trim() == "Disconnected" {
                    a = ClientActivity { player: a.player, since: time, ..Default::default() };
                } else {
                    a.state = state;
                    a.reason = Some(reason.trim().to_string());
                    a.since = time;
                }
            }
        } else if server_thread && msg.starts_with("Starting integrated minecraft server") {
            a = ClientActivity { state: ClientState::Singleplayer, since: time, player: a.player, ..Default::default() };
        } else if server_thread && msg == "Stopping server" && a.state == ClientState::Singleplayer {
            a = ClientActivity { player: a.player, since: time, ..Default::default() };
        }
    }
    a
}

/// What a disconnect turns `state` into - `None` if there was no connection.
fn ended(state: ClientState) -> Option<ClientState> {
    match state {
        ClientState::Joining => Some(ClientState::Failed),
        ClientState::Server => Some(ClientState::Disconnected),
        _ => None,
    }
}

/// The singleplayer world open now: opening one rewrites its session.lock.
pub fn current_world(game_dir: &Path) -> Option<String> {
    std::fs::read_dir(game_dir.join("saves"))
        .ok()?
        .flatten()
        .filter_map(|e| {
            let modified = e.path().join("session.lock").metadata().ok()?.modified().ok()?;
            Some((modified, e.file_name().to_string_lossy().into_owned()))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, name)| name)
}

/// `address`'s name in the instance's server list (servers.dat), matched
/// case-insensitively and ignoring a default `:25565`.
pub fn server_list_name(game_dir: &Path, address: &str) -> Option<String> {
    let norm = |s: &str| s.trim().trim_end_matches(":25565").to_ascii_lowercase();
    let wanted = norm(address);
    super::servers_dat::read_servers(game_dir)
        .into_iter()
        .find(|s| norm(&s.address) == wanted)
        .map(|s| s.name)
        .filter(|n| !n.trim().is_empty())
}
