//! "Mint Connect" - a built-in private tunnel so remote admin works without
//! installing Tailscale/WireGuard or touching a router. Built on iroh: every
//! Mint install has a keypair, its public key is its address, and iroh
//! finds the other side (NAT hole-punching, falling back to public relay
//! servers) and encrypts everything end to end.
//!
//! It deliberately adds no new API - it only carries the *existing* remote
//! admin HTTP/WebSocket API (see `remote_api.rs`) across the internet as a
//! plain byte stream, so authentication stays exactly what it was (the
//! Mojang join/hasJoined handshake + the server's ops list):
//!
//!   host:   iroh connection -> stream -> TCP to 127.0.0.1:<remote admin port>
//!   admin:  a loopback TCP port -> stream -> iroh connection to the host
//!
//! The admin's frontend keeps talking plain `http://127.0.0.1:<port>` and
//! never knows a tunnel is involved.

use iroh::endpoint::{presets, Connection};
use iroh::{Endpoint, EndpointId, SecretKey};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, OnceCell, Semaphore};

const ALPN: &[u8] = b"mint-remote/1";
const DIAL_TIMEOUT: Duration = Duration::from_secs(30);
/// Cap on simultaneous tunneled streams the host will serve - the API is
/// reachable by anyone who has the connection code, so a runaway (or
/// hostile) client can't open unlimited sockets on the host.
const MAX_HOST_STREAMS: usize = 128;

/// This install's permanent identity - created on first use and kept in the
/// data folder, so the connection code an owner hands out stays valid across
/// restarts. Not sensitive to *see* (it's derived into the public code), but
/// it's the private half, so it's written owner-readable only.
pub fn load_or_create_secret(data_dir: &Path) -> std::io::Result<SecretKey> {
    let path = data_dir.join("tunnel_key");
    if let Ok(bytes) = std::fs::read(&path) {
        if let Ok(arr) = <[u8; 32]>::try_from(bytes.as_slice()) {
            return Ok(SecretKey::from_bytes(&arr));
        }
    }
    let key = SecretKey::generate();
    std::fs::write(&path, key.to_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(key)
}

/// The shareable connection code for this install (hex of the public key).
pub fn connection_code(data_dir: &Path) -> std::io::Result<String> {
    Ok(load_or_create_secret(data_dir)?.public().to_string())
}

// --- host side ---------------------------------------------------------

/// Starts accepting tunneled connections and forwards each stream to the
/// local remote admin API on `port` (a shared value, so the supervisor can
/// keep it current if the owner changes the port without restarting this).
/// Returns the endpoint so the caller (the remote admin supervisor) can
/// `close()` it when the feature gets switched off.
pub async fn start_host(data_dir: &Path, port: Arc<AtomicU16>) -> Result<Endpoint, String> {
    let secret = load_or_create_secret(data_dir).map_err(|e| e.to_string())?;
    let endpoint = Endpoint::builder(presets::N0)
        .secret_key(secret)
        .alpns(vec![ALPN.to_vec()])
        .bind()
        .await
        .map_err(|e| format!("{e:?}"))?;

    let accepting = endpoint.clone();
    let limit = Arc::new(Semaphore::new(MAX_HOST_STREAMS));
    tokio::spawn(async move {
        while let Some(incoming) = accepting.accept().await {
            let port = port.clone();
            let limit = limit.clone();
            tokio::spawn(async move {
                let Ok(conn) = incoming.await else { return };
                while let Ok((send, recv)) = conn.accept_bi().await {
                    let Ok(permit) = limit.clone().try_acquire_owned() else {
                        // Over the cap - dropping the stream resets it.
                        continue;
                    };
                    let port = port.clone();
                    tokio::spawn(async move {
                        let _permit = permit;
                        let Ok(tcp) = TcpStream::connect(("127.0.0.1", port.load(Ordering::Relaxed))).await else {
                            return;
                        };
                        pipe(tcp, send, recv).await;
                    });
                }
            });
        }
    });
    Ok(endpoint)
}

/// Shuttles bytes both ways between a TCP socket and one QUIC stream until
/// each direction ends.
async fn pipe(tcp: TcpStream, mut send: iroh::endpoint::SendStream, mut recv: iroh::endpoint::RecvStream) {
    let (mut tcp_read, mut tcp_write) = tcp.into_split();
    let upstream = async {
        let _ = tokio::io::copy(&mut tcp_read, &mut send).await;
        let _ = send.finish();
    };
    let downstream = async {
        let _ = tokio::io::copy(&mut recv, &mut tcp_write).await;
        let _ = tcp_write.shutdown().await;
    };
    tokio::join!(upstream, downstream);
}

// --- admin (client) side ----------------------------------------------

struct Proxy {
    port: u16,
    task: tokio::task::JoinHandle<()>,
}

/// One shared iroh endpoint plus a loopback proxy per host the admin has
/// linked - created lazily, so an install that never uses Mint Connect
/// never opens a socket for it.
#[derive(Default)]
pub struct ClientTunnel {
    endpoint: OnceCell<Endpoint>,
    proxies: Mutex<HashMap<String, Proxy>>,
    conns: Mutex<HashMap<String, Arc<Mutex<Option<Connection>>>>>,
}

fn parse_code(code: &str) -> Result<EndpointId, String> {
    code.trim()
        .parse::<EndpointId>()
        .map_err(|_| "That doesn't look like a valid Mint Connect code".to_string())
}

/// Canonical form of a connection code (trimmed, validated) - what gets
/// stored on a link and compared for "already linked" checks.
pub fn normalize_code(code: &str) -> Result<String, String> {
    parse_code(code).map(|id| id.to_string())
}

impl ClientTunnel {
    async fn endpoint(&self) -> Result<Endpoint, String> {
        self.endpoint
            .get_or_try_init(|| async {
                Endpoint::bind(presets::N0).await.map_err(|e| format!("Couldn't start Mint Connect: {e:?}"))
            })
            .await
            .cloned()
    }

    async fn conn_slot(&self, key: &str) -> Arc<Mutex<Option<Connection>>> {
        self.conns.lock().await.entry(key.to_string()).or_default().clone()
    }

    /// Opens one stream to the host, (re)dialing the underlying connection
    /// if there isn't a live one - a host restarting, or a network change,
    /// just costs the next request a redial rather than breaking the link.
    async fn open_stream(
        &self,
        id: EndpointId,
    ) -> Result<(iroh::endpoint::SendStream, iroh::endpoint::RecvStream), String> {
        let endpoint = self.endpoint().await?;
        let slot = self.conn_slot(&id.to_string()).await;
        let mut guard = slot.lock().await;
        for _ in 0..2 {
            if let Some(conn) = guard.as_ref() {
                if conn.close_reason().is_none() {
                    if let Ok(stream) = conn.open_bi().await {
                        return Ok(stream);
                    }
                }
            }
            *guard = None;
            let conn = tokio::time::timeout(DIAL_TIMEOUT, endpoint.connect(id, ALPN))
                .await
                .map_err(|_| {
                    "Couldn't reach that server through Mint Connect - check the code, and that Mint is open on \
                     the host with Mint Connect switched on"
                        .to_string()
                })?
                .map_err(|e| format!("Couldn't reach that server through Mint Connect: {e:?}"))?;
            *guard = Some(conn);
        }
        Err("Lost the connection to the host".to_string())
    }

    /// Dials the host now so a bad code/offline host is reported precisely
    /// when linking, instead of as a vague "connection refused" later.
    pub async fn probe(&self, code: &str) -> Result<(), String> {
        let id = parse_code(code)?;
        let (mut send, _recv) = self.open_stream(id).await?;
        let _ = send.finish();
        Ok(())
    }

    /// The loopback port that tunnels to this host, starting its proxy on
    /// first use. Ports are ephemeral, so callers resolve a link's address
    /// through this each launch rather than persisting it.
    pub async fn proxy_port(self: &Arc<Self>, code: &str) -> Result<u16, String> {
        let id = parse_code(code)?;
        let key = id.to_string();
        let mut proxies = self.proxies.lock().await;
        if let Some(p) = proxies.get(&key) {
            return Ok(p.port);
        }
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let this = self.clone();
        let task = tokio::spawn(async move {
            loop {
                let Ok((tcp, _)) = listener.accept().await else { break };
                let this = this.clone();
                tokio::spawn(async move {
                    let Ok((send, recv)) = this.open_stream(id).await else { return };
                    pipe(tcp, send, recv).await;
                });
            }
        });
        proxies.insert(key, Proxy { port, task });
        Ok(port)
    }

    /// Stops a host's proxy (when its link is removed).
    pub async fn forget(&self, code: &str) {
        let Ok(id) = parse_code(code) else { return };
        let key = id.to_string();
        if let Some(p) = self.proxies.lock().await.remove(&key) {
            p.task.abort();
        }
        self.conns.lock().await.remove(&key);
    }
}
