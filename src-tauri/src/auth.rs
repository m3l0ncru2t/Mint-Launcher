use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameProfile {
    pub username: String,
    pub uuid: String,
    pub access_token: String,
    /// "legacy" for offline accounts, "msa" for real Microsoft accounts.
    pub user_type: String,
}

/// Generates a deterministic offline profile the same way vanilla's offline
/// mode does: UUID = MD5("OfflinePlayer:<name>") with version/variant bits
/// patched in (Java's `UUID.nameUUIDFromBytes`). This only works for
/// singleplayer/LAN - it is not a real, server-verifiable Microsoft account.
pub fn offline_profile(username: &str) -> GameProfile {
    GameProfile {
        username: username.to_string(),
        uuid: offline_uuid(username),
        access_token: "0".to_string(),
        user_type: "legacy".to_string(),
    }
}

fn offline_uuid(username: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(format!("OfflinePlayer:{username}").as_bytes());
    let mut bytes: [u8; 16] = hasher.finalize().into();
    bytes[6] = (bytes[6] & 0x0f) | 0x30;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format_uuid(&bytes)
}

fn format_uuid(bytes: &[u8; 16]) -> String {
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15],
    )
}
