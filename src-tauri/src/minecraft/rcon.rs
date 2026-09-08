//! A minimal client for Minecraft's RCON protocol (https://wiki.vg/RCON) -
//! a normal TCP connection authenticated by a password stored in
//! `server.properties`, rather than the piped stdin handle
//! `spawn_and_stream_server` normally holds in `AppState.instance_stdins`.
//! That handle only exists in the Mint process that actually spawned the
//! server, so it's gone the moment Mint restarts and re-adopts an
//! already-running instance; RCON has no such limitation; any Mint process
//! (this one, a later one after a relaunch, or a remote admin's own request
//! routed through the host) can use it as long as it knows the port/password
//! - see `commands::launch::rcon_execute` and `minecraft::server_properties::
//! ensure_rcon_enabled`. One connection per call: call volume here is at
//! most a few times a minute (polling), so a persistent connection pool
//! isn't worth the complexity.

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

const TYPE_AUTH: i32 = 3;
const TYPE_EXEC_COMMAND: i32 = 2;

async fn write_packet(stream: &mut TcpStream, id: i32, packet_type: i32, body: &str) -> std::io::Result<()> {
    let mut payload = Vec::with_capacity(body.len() + 10);
    payload.extend_from_slice(&id.to_le_bytes());
    payload.extend_from_slice(&packet_type.to_le_bytes());
    payload.extend_from_slice(body.as_bytes());
    payload.push(0); // null-terminates the body string
    payload.push(0); // empty second string, also null-terminated
    stream.write_all(&(payload.len() as i32).to_le_bytes()).await?;
    stream.write_all(&payload).await
}

/// Reads one packet and returns (id, body) - the type field isn't needed by
/// any caller here (a failed auth is signaled entirely via `id == -1`).
async fn read_packet(stream: &mut TcpStream) -> std::io::Result<(i32, String)> {
    let mut len_buf = [0u8; 4];
    stream.read_exact(&mut len_buf).await?;
    let len = i32::from_le_bytes(len_buf).max(0) as usize;
    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;
    let id = i32::from_le_bytes(buf.get(0..4).and_then(|s| s.try_into().ok()).unwrap_or([0; 4]));
    // Body sits between the 8-byte id+type header and the two trailing nulls.
    let body_end = len.saturating_sub(2).max(8);
    let body = String::from_utf8_lossy(buf.get(8..body_end).unwrap_or(&[])).into_owned();
    Ok((id, body))
}

/// Connects to `127.0.0.1:<port>`, authenticates with `password`, runs
/// `command`, and returns its text response (the same text the console/log
/// would show for that command).
pub async fn execute(port: u16, password: &str, command: &str) -> anyhow::Result<String> {
    let mut stream =
        tokio::time::timeout(std::time::Duration::from_secs(3), TcpStream::connect(("127.0.0.1", port))).await??;

    write_packet(&mut stream, 1, TYPE_AUTH, password).await?;
    let (auth_id, _) = read_packet(&mut stream).await?;
    if auth_id == -1 {
        anyhow::bail!("RCON authentication failed");
    }

    write_packet(&mut stream, 2, TYPE_EXEC_COMMAND, command).await?;
    let (_, body) = read_packet(&mut stream).await?;
    Ok(body)
}
