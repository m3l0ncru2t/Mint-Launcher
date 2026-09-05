//! Microsoft account login for real Minecraft accounts, using the OAuth2
//! authorization code + PKCE flow with a local loopback redirect (the
//! system browser opens to Microsoft's sign-in page and redirects back to
//! a one-shot local HTTP listener - no embedded browser needed).
//!
//! Chain: Microsoft identity platform -> Xbox Live -> XSTS -> Minecraft
//! services -> game profile, per
//! https://wiki.vg/Microsoft_Authentication_Scheme.
//!
//! `DEFAULT_CLIENT_ID` below is Prism Launcher's own public Azure AD app
//! registration, not one registered for this project. Freshly-registered
//! apps (personal-account, public-client, settings all textbook-correct)
//! consistently got a 403 "Invalid app registration" from
//! `login_with_xbox`, while this well-established client ID worked
//! immediately for the same Microsoft account - Microsoft's Xbox Live sign-in
//! scope appears to apply extra anti-abuse scrutiny to brand-new app
//! registrations that no amount of correct configuration works around.
//! Reusing a known-good public client ID (not a secret) is standard
//! practice among small/hobby Minecraft launchers for exactly this reason.

use crate::auth::GameProfile;
use crate::http_util::ensure_success;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use tauri::Emitter;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use uuid::Uuid;

const SCOPE: &str = "XboxLive.signin offline_access";

/// See the module-level doc comment for why this is Prism Launcher's client
/// ID rather than one registered for Mint Launcher. Overridable in Settings
/// for anyone who wants to use their own registration instead.
pub const DEFAULT_CLIENT_ID: &str = "c36a9fb6-4f2a-41ff-90bd-ae7cc92031eb";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginUrlInfo {
    pub url: String,
}

#[derive(Debug, Deserialize)]
struct MsTokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
}

/// Result of a successful sign-in or session refresh. `refresh_token` is
/// `Some` whenever Microsoft issued one (Microsoft may rotate it on every
/// use), for the caller to persist via accounts.rs.
pub struct LoginResult {
    pub profile: GameProfile,
    pub refresh_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct XblResponse {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: XblDisplayClaims,
}

#[derive(Debug, Deserialize)]
struct XblDisplayClaims {
    xui: Vec<HashMap<String, String>>,
}

#[derive(Debug, Deserialize)]
struct XstsErrorResponse {
    #[serde(rename = "XErr")]
    x_err: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct MinecraftAuthResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct MinecraftProfileResponse {
    id: String,
    name: String,
}

pub async fn login(
    app: &tauri::AppHandle,
    client: &reqwest::Client,
    client_id: &str,
) -> anyhow::Result<LoginResult> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    // The redirect URI below uses the hostname "localhost" rather than the
    // literal "127.0.0.1" because Microsoft's identity platform only grants
    // its any-port leniency to an app registration's exact "http://localhost"
    // redirect URI - a literal loopback IP wouldn't match it. But that means
    // whichever address family the OS/browser resolves "localhost" to has to
    // have a listener waiting on it, and on some systems (IPv6 preferred in
    // resolution order, an "::1 localhost" hosts entry, etc.) that's not the
    // IPv4 socket above. Binding the IPv6 loopback on the same port too (best
    // effort - older systems without IPv6 simply won't get this listener)
    // covers both without changing the redirect URI at all.
    let listener_v6 = tokio::net::TcpListener::bind(format!("[::1]:{port}")).await.ok();
    let redirect_uri = format!("http://localhost:{port}");

    let verifier = pkce_verifier();
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = Uuid::new_v4().simple().to_string();

    let auth_url = format!(
        "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize?client_id={}&response_type=code&redirect_uri={}&response_mode=query&scope={}&state={}&code_challenge={}&code_challenge_method=S256&prompt=select_account",
        percent_encode(client_id),
        percent_encode(&redirect_uri),
        percent_encode(SCOPE),
        state,
        challenge,
    );

    let _ = app.emit("microsoft-login-open", LoginUrlInfo { url: auth_url });

    let code = wait_for_redirect(listener, listener_v6, &state).await?;
    let (ms_access_token, refresh_token) =
        exchange_code(client, client_id, &code, &verifier, &redirect_uri).await?;
    let profile = finish_login(client, &ms_access_token).await?;

    Ok(LoginResult { profile, refresh_token })
}

/// Silently re-authenticates a previously saved account using its refresh
/// token, instead of opening a browser - what powers "stay logged in".
pub async fn refresh(
    client: &reqwest::Client,
    client_id: &str,
    refresh_token: &str,
) -> anyhow::Result<LoginResult> {
    let (ms_access_token, new_refresh_token) = refresh_token_exchange(client, client_id, refresh_token).await?;
    let profile = finish_login(client, &ms_access_token).await?;

    Ok(LoginResult {
        profile,
        refresh_token: new_refresh_token,
    })
}

async fn finish_login(client: &reqwest::Client, ms_access_token: &str) -> anyhow::Result<GameProfile> {
    let (xbl_token, uhs) = xbox_live_auth(client, ms_access_token).await?;
    let xsts_token = xsts_auth(client, &xbl_token).await?;
    let mc_access_token = minecraft_login(client, &uhs, &xsts_token).await?;
    let profile = fetch_minecraft_profile(client, &mc_access_token).await?;

    Ok(GameProfile {
        username: profile.name,
        uuid: format_dashed_uuid(&profile.id),
        access_token: mc_access_token,
        user_type: "msa".to_string(),
    })
}

/// A PKCE code verifier: 64 characters from the unreserved set, well within
/// RFC 7636's required 43-128 length.
fn pkce_verifier() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

/// Percent-encodes everything but RFC 3986 "unreserved" characters - enough
/// for the handful of query values (a UUID client ID, a scope string with a
/// space, and a `http://host:port` redirect URI) this module builds by hand.
fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Accepts exactly one connection on whichever loopback listener gets
/// Microsoft's redirect first (see `login` for why there can be two - an
/// IPv4 and an IPv6 one on the same port), pulls the `code`/`error`/`state`
/// query parameters out of the raw HTTP request line, and replies with a
/// small confirmation page. Closing the browser tab instead of finishing
/// sign-in never sends that redirect at all, so this gives up after a while
/// instead of hanging forever.
async fn wait_for_redirect(
    listener: TcpListener,
    listener_v6: Option<TcpListener>,
    expected_state: &str,
) -> anyhow::Result<String> {
    let accept = async {
        match &listener_v6 {
            Some(v6) => tokio::select! {
                r = listener.accept() => r,
                r = v6.accept() => r,
            },
            None => listener.accept().await,
        }
    };
    let (mut stream, _) = tokio::time::timeout(std::time::Duration::from_secs(300), accept)
        .await
        .map_err(|_| anyhow::anyhow!("Sign-in timed out - the browser window wasn't completed. Try again."))??;
    let mut buf = vec![0u8; 8192];
    let n = stream.read(&mut buf).await?;
    let request = String::from_utf8_lossy(&buf[..n]).into_owned();

    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("")
        .to_string();
    let query = path.splitn(2, '?').nth(1).unwrap_or("");
    let params = parse_query(query);

    let ok = params.contains_key("code") && params.get("error").is_none();
    let body = if ok {
        "<html><body>Signed in - you can close this tab and return to Mint Launcher.</body></html>"
    } else {
        "<html><body>Sign-in failed - you can close this tab and return to Mint Launcher.</body></html>"
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;

    if let Some(err) = params.get("error") {
        let desc = params.get("error_description").cloned().unwrap_or_default();
        anyhow::bail!("Microsoft sign-in failed: {err} {desc}");
    }

    match params.get("state") {
        Some(s) if s == expected_state => {}
        _ => anyhow::bail!("Microsoft sign-in response failed a security check - please try again."),
    }

    params
        .get("code")
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Microsoft didn't return an authorization code"))
}

fn parse_query(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .filter_map(|pair| {
            let mut it = pair.splitn(2, '=');
            let key = percent_decode(it.next()?);
            let value = percent_decode(it.next().unwrap_or(""));
            Some((key, value))
        })
        .collect()
}

/// Decodes a `application/x-www-form-urlencoded`-style query component -
/// `+` as space, `%XX` hex escapes as their byte value - the same convention
/// Microsoft's redirect (and `reqwest`'s own `Client::form`, used to build
/// the token exchange request below) use. Skipping this was a real bug: an
/// authorization `code` containing a `+`, `/`, or `=` arrives here still
/// percent-escaped from the URL, and `exchange_code`'s `.form(&params)`
/// percent-encodes it a *second* time on top of that, so Microsoft receives
/// a code that no longer matches the one it issued and rejects it with
/// `AADSTS70000: the provided value for the 'code' parameter is not valid` -
/// deterministically, every time, whenever a freshly issued code happens to
/// contain one of those characters (most don't, which is why this wasn't
/// caught immediately).
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let byte = std::str::from_utf8(&bytes[i + 1..i + 3])
                    .ok()
                    .and_then(|hex| u8::from_str_radix(hex, 16).ok());
                match byte {
                    Some(byte) => {
                        out.push(byte);
                        i += 3;
                    }
                    None => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

async fn exchange_code(
    client: &reqwest::Client,
    client_id: &str,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> anyhow::Result<(String, Option<String>)> {
    let params = [
        ("client_id", client_id),
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("code_verifier", verifier),
    ];
    let resp = client
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token")
        .form(&params)
        .send()
        .await?;
    let resp = ensure_success(resp, "Exchanging Microsoft authorization code").await?;
    let token: MsTokenResponse = resp.json().await?;
    Ok((token.access_token, token.refresh_token))
}

async fn refresh_token_exchange(
    client: &reqwest::Client,
    client_id: &str,
    refresh_token: &str,
) -> anyhow::Result<(String, Option<String>)> {
    let params = [
        ("client_id", client_id),
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("scope", SCOPE),
    ];
    let resp = client
        .post("https://login.microsoftonline.com/consumers/oauth2/v2.0/token")
        .form(&params)
        .send()
        .await?;
    let resp = ensure_success(resp, "Refreshing Microsoft session").await?;
    let token: MsTokenResponse = resp.json().await?;
    Ok((token.access_token, token.refresh_token))
}

async fn xbox_live_auth(client: &reqwest::Client, ms_access_token: &str) -> anyhow::Result<(String, String)> {
    let body = serde_json::json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            "RpsTicket": format!("d={ms_access_token}"),
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT",
    });
    let resp = client
        .post("https://user.auth.xboxlive.com/user/authenticate")
        .json(&body)
        .send()
        .await?;
    let resp = ensure_success(resp, "Xbox Live authentication").await?;
    let resp: XblResponse = resp.json().await?;

    let uhs = resp
        .display_claims
        .xui
        .first()
        .and_then(|c| c.get("uhs"))
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Xbox Live response missing user hash"))?;

    Ok((resp.token, uhs))
}

async fn xsts_auth(client: &reqwest::Client, xbl_token: &str) -> anyhow::Result<String> {
    let body = serde_json::json!({
        "Properties": {
            "SandboxId": "RETAIL",
            "UserTokens": [xbl_token],
        },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT",
    });
    let resp = client
        .post("https://xsts.auth.xboxlive.com/xsts/authorize")
        .json(&body)
        .send()
        .await?;

    if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
        let err: XstsErrorResponse = resp.json().await.unwrap_or(XstsErrorResponse { x_err: None });
        let message = match err.x_err {
            Some(2148916233) => {
                "This Microsoft account has no Xbox Live profile. Sign in at xbox.com once, then try again."
            }
            Some(2148916238) => "This account is a child account and needs to be added to a Family group first.",
            _ => "Xbox Live rejected this account.",
        };
        anyhow::bail!(message);
    }

    let resp = ensure_success(resp, "XSTS authorization").await?;
    let resp: XblResponse = resp.json().await?;
    Ok(resp.token)
}

async fn minecraft_login(client: &reqwest::Client, uhs: &str, xsts_token: &str) -> anyhow::Result<String> {
    let body = serde_json::json!({
        "identityToken": format!("XBL3.0 x={uhs};{xsts_token}"),
    });
    let resp = client
        .post("https://api.minecraftservices.com/authentication/login_with_xbox")
        .json(&body)
        .send()
        .await?;
    let resp = ensure_success(resp, "Minecraft login").await?;
    let resp: MinecraftAuthResponse = resp.json().await?;
    Ok(resp.access_token)
}

async fn fetch_minecraft_profile(
    client: &reqwest::Client,
    mc_access_token: &str,
) -> anyhow::Result<MinecraftProfileResponse> {
    let resp = client
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(mc_access_token)
        .send()
        .await?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        anyhow::bail!("This Microsoft account doesn't own Minecraft: Java Edition.");
    }

    let resp = ensure_success(resp, "Fetching Minecraft profile").await?;
    Ok(resp.json().await?)
}

fn format_dashed_uuid(raw: &str) -> String {
    if raw.contains('-') || raw.len() != 32 {
        return raw.to_string();
    }
    format!(
        "{}-{}-{}-{}-{}",
        &raw[0..8],
        &raw[8..12],
        &raw[12..16],
        &raw[16..20],
        &raw[20..32]
    )
}
