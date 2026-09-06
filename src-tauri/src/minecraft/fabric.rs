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
    // Some importers (see `importer::scan_curseforge_instance`) have been
    // seen storing a compound "<loaderVersion>-<gameVersion>" string as the
    // loader version. Fabric's meta API treats the two as separate path
    // segments and 400s ("no loader version found for ...") if either one
    // drags the other along, so this strips a stray trailing "-<game_version>"
    // before building the request regardless of where the value came from -
    // fixes launching an instance that was already imported with the bad
    // value stored, not just newly-imported ones.
    let loader_version = loader_version.strip_suffix(&format!("-{game_version}")).unwrap_or(loader_version);

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

/// Fetches Fabric's dedicated-server profile for `game_version`+`loader_version`
/// and layers it onto the vanilla `base` version detail, the server-side
/// counterpart to `apply_loader`. Simpler than the client version: the
/// server endpoint's `mainClass` is `net.fabricmc.loader.impl.launch.knot.KnotServer`
/// and it carries no `jvm` arguments (a dedicated server's launch args are
/// just `-jar <jar> nogui`, not the client's argument-template system), so
/// only `main_class`/`id`/`libraries` are relevant here.
pub async fn apply_server_loader(
    client: &reqwest::Client,
    mut base: VersionDetail,
    game_version: &str,
    loader_version: &str,
) -> anyhow::Result<VersionDetail> {
    let loader_version = loader_version.strip_suffix(&format!("-{game_version}")).unwrap_or(loader_version);

    let url = format!("{FABRIC_META_BASE}/{game_version}/{loader_version}/server/json");
    let resp = client.get(&url).send().await?;
    let resp = ensure_success(resp, "Fetching Fabric server loader profile").await?;
    let profile: LoaderProfile = resp.json().await?;

    base.id = profile.id;
    base.main_class = profile.main_class;
    base.libraries.extend(profile.libraries);

    Ok(base)
}
