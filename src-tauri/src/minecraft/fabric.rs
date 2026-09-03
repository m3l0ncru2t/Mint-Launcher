//! Fabric loader support via the official Fabric Meta API
//! (https://fabricmc.net/wiki/documentation:fabric_meta). A "profile" for a
//! given (game version, loader version) pair is shaped like a partial
//! version JSON - `mainClass`, extra `libraries`, extra `arguments` - meant
//! to be layered on top of the vanilla version it targets.

use super::manifest::{Arguments, Library, VersionDetail};
use crate::http_util::ensure_success;
use serde::{Deserialize, Serialize};

const FABRIC_META_BASE: &str = "https://meta.fabricmc.net/v2/versions/loader";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FabricLoaderInfo {
    pub version: String,
    pub stable: bool,
}

#[derive(Debug, Deserialize)]
struct FabricLoaderEntry {
    loader: FabricLoaderInfo,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoaderProfile {
    id: String,
    main_class: String,
    #[serde(default)]
    arguments: Option<Arguments>,
    #[serde(default)]
    libraries: Vec<Library>,
}

pub async fn list_loader_versions(
    client: &reqwest::Client,
    game_version: &str,
) -> anyhow::Result<Vec<FabricLoaderInfo>> {
    let url = format!("{FABRIC_META_BASE}/{game_version}");
    let resp = client.get(&url).send().await?;
    // Fabric's meta API answers a game version it has no builds for (e.g.
    // anything before 1.14, which predates Fabric's intermediary mappings)
    // with 400 Bad Request rather than an empty list - treat that as "no
    // builds" instead of a hard error.
    if resp.status() == reqwest::StatusCode::BAD_REQUEST {
        return Ok(Vec::new());
    }
    let resp = ensure_success(resp, "Fetching Fabric loader versions").await?;
    let entries: Vec<FabricLoaderEntry> = resp.json().await?;
    Ok(entries.into_iter().map(|e| e.loader).collect())
}

/// Fetches the Fabric profile for `game_version`+`loader_version` and layers
/// it onto the vanilla `base` version detail: extra libraries are appended,
/// `mainClass` and `id` are overridden, and JVM/game arguments are merged.
pub async fn apply_loader(
    client: &reqwest::Client,
    mut base: VersionDetail,
    game_version: &str,
    loader_version: &str,
) -> anyhow::Result<VersionDetail> {
    let url = format!("{FABRIC_META_BASE}/{game_version}/{loader_version}/profile/json");
    let resp = client.get(&url).send().await?;
    let resp = ensure_success(resp, "Fetching Fabric loader profile").await?;
    let profile: LoaderProfile = resp.json().await?;

    base.id = profile.id;
    base.main_class = profile.main_class;
    base.libraries.extend(profile.libraries);

    if let Some(overlay_args) = profile.arguments {
        match &mut base.arguments {
            Some(base_args) => {
                base_args.jvm.extend(overlay_args.jvm);
                base_args.game.extend(overlay_args.game);
            }
            None => base.arguments = Some(overlay_args),
        }
    }

    Ok(base)
}
