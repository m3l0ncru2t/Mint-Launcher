//! Skin and cape management via the same `api.minecraftservices.com` API
//! the official launcher and minecraft.net's profile page use - separate
//! from msa.rs, which only handles the initial sign-in chain, not ongoing
//! profile changes.

use crate::http_util::ensure_success;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkinInfo {
    pub id: String,
    pub state: String,
    pub url: String,
    pub variant: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapeInfo {
    pub id: String,
    pub state: String,
    pub url: String,
    pub alias: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileDetails {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub skins: Vec<SkinInfo>,
    #[serde(default)]
    pub capes: Vec<CapeInfo>,
}

pub async fn get_profile_details(client: &reqwest::Client, access_token: &str) -> anyhow::Result<ProfileDetails> {
    let resp = client
        .get("https://api.minecraftservices.com/minecraft/profile")
        .bearer_auth(access_token)
        .send()
        .await?;
    let resp = ensure_success(resp, "Fetching profile").await?;
    Ok(resp.json().await?)
}

/// `variant` is `"classic"` (the default wide-arm model) or `"slim"`
/// (narrow arms, aka the "Alex" model). `image` must be a 64x64 PNG.
pub async fn upload_skin(
    client: &reqwest::Client,
    access_token: &str,
    variant: &str,
    image: Vec<u8>,
) -> anyhow::Result<ProfileDetails> {
    let part = reqwest::multipart::Part::bytes(image)
        .file_name("skin.png")
        .mime_str("image/png")?;
    let form = reqwest::multipart::Form::new()
        .text("variant", variant.to_string())
        .part("file", part);

    let resp = client
        .post("https://api.minecraftservices.com/minecraft/profile/skins")
        .bearer_auth(access_token)
        .multipart(form)
        .send()
        .await?;
    let resp = ensure_success(resp, "Uploading skin").await?;
    Ok(resp.json().await?)
}

/// Reverts to the account's default (Steve/Alex) skin.
pub async fn reset_skin(client: &reqwest::Client, access_token: &str) -> anyhow::Result<()> {
    let resp = client
        .delete("https://api.minecraftservices.com/minecraft/profile/skins/active")
        .bearer_auth(access_token)
        .send()
        .await?;
    ensure_success(resp, "Resetting skin").await?;
    Ok(())
}

/// Equips one of the capes this account already owns - capes can't be
/// uploaded, only selected from ones granted by Mojang promotions/events.
pub async fn set_cape(client: &reqwest::Client, access_token: &str, cape_id: &str) -> anyhow::Result<ProfileDetails> {
    let resp = client
        .put("https://api.minecraftservices.com/minecraft/profile/capes/active")
        .bearer_auth(access_token)
        .json(&serde_json::json!({ "capeId": cape_id }))
        .send()
        .await?;
    let resp = ensure_success(resp, "Equipping cape").await?;
    Ok(resp.json().await?)
}

pub async fn remove_cape(client: &reqwest::Client, access_token: &str) -> anyhow::Result<()> {
    let resp = client
        .delete("https://api.minecraftservices.com/minecraft/profile/capes/active")
        .bearer_auth(access_token)
        .send()
        .await?;
    ensure_success(resp, "Removing cape").await?;
    Ok(())
}

/// Looks up any player's current skin texture URL from Mojang's public
/// session server - no access token needed, works for any real Minecraft
/// account by UUID. Used to show account-switcher avatars for saved accounts
/// that aren't the currently signed-in one (so there's no token on hand for
/// them). Returns `None` for offline/legacy accounts (fake local UUIDs that
/// don't exist server-side) or on any lookup failure - callers fall back to
/// a plain initial-letter avatar in that case.
pub async fn fetch_public_skin_url(client: &reqwest::Client, uuid: &str) -> Option<String> {
    let compact = uuid.replace('-', "");
    let resp = client
        .get(format!("https://sessionserver.mojang.com/session/minecraft/profile/{compact}"))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let json: serde_json::Value = resp.json().await.ok()?;
    let properties = json.get("properties")?.as_array()?;
    let textures_prop = properties
        .iter()
        .find(|p| p.get("name").and_then(|n| n.as_str()) == Some("textures"))?;
    let textures_b64 = textures_prop.get("value")?.as_str()?;
    let decoded = STANDARD.decode(textures_b64).ok()?;
    let textures_json: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    textures_json.get("textures")?.get("SKIN")?.get("url")?.as_str().map(String::from)
}
