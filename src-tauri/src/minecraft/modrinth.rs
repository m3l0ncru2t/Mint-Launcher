//! Mod identification and update checking via the Modrinth API
//! (https://docs.modrinth.com/api/) - free, no API key required. Mods are
//! identified purely by the sha1 hash of the jar file, which Modrinth can
//! match against any file it hosts regardless of embedded metadata.

use super::hash::sha1_hex_file;
use crate::http_util::ensure_success;
use crate::mod_meta::{self, ModMeta};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

const MODRINTH_BASE: &str = "https://api.modrinth.com/v2";

#[derive(Debug, Clone, Deserialize)]
struct ModrinthVersion {
    id: String,
    project_id: String,
    version_number: String,
    files: Vec<ModrinthFile>,
    #[serde(default)]
    dependencies: Vec<ModrinthDependency>,
}

#[derive(Debug, Clone, Deserialize)]
struct ModrinthFile {
    url: String,
    filename: String,
    primary: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct ModrinthDependency {
    version_id: Option<String>,
    project_id: Option<String>,
    dependency_type: String,
}

#[derive(Debug, Deserialize)]
struct ModrinthProject {
    slug: String,
    title: String,
    description: String,
    icon_url: Option<String>,
    downloads: u64,
    client_side: String,
    server_side: String,
    categories: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ModrinthTeamMember {
    role: String,
    user: ModrinthUser,
}

#[derive(Debug, Deserialize)]
struct ModrinthUser {
    username: String,
}

#[derive(Debug, Deserialize)]
struct ModrinthSearchResponse {
    hits: Vec<ModrinthSearchHit>,
    total_hits: u64,
}

#[derive(Debug, Deserialize)]
struct ModrinthSearchHit {
    project_id: String,
    slug: String,
    title: String,
    description: String,
    author: String,
    downloads: u64,
    icon_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModSearchResult {
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub author: String,
    pub downloads: u64,
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModSearchPage {
    pub hits: Vec<ModSearchResult>,
    pub total_hits: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledModInfo {
    pub project_id: String,
    pub title: String,
    pub file_name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallSummary {
    pub installed: Vec<InstalledModInfo>,
    pub already_installed: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModUpdateInfo {
    pub file_name: String,
    pub project_id: Option<String>,
    pub title: Option<String>,
    pub icon_url: Option<String>,
    pub current_version: Option<String>,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub download_url: Option<String>,
}

pub async fn check_updates(
    client: &reqwest::Client,
    mods_dir: &Path,
    game_version: &str,
    loader: Option<&str>,
) -> anyhow::Result<Vec<ModUpdateInfo>> {
    check_updates_matching(client, mods_dir, game_version, loader, |lower| {
        lower.ends_with(".jar") || lower.ends_with(".jar.disabled")
    })
    .await
}

/// Resource packs have no loader and no ".disabled" rename convention
/// (enabling/disabling one is tracked separately, in `options.txt`) - every
/// `.zip` in the folder counts as installed.
pub async fn check_resourcepack_updates(
    client: &reqwest::Client,
    resourcepacks_dir: &Path,
    game_version: &str,
) -> anyhow::Result<Vec<ModUpdateInfo>> {
    check_updates_matching(client, resourcepacks_dir, game_version, None, |lower| lower.ends_with(".zip")).await
}

async fn check_updates_matching(
    client: &reqwest::Client,
    dir: &Path,
    game_version: &str,
    loader: Option<&str>,
    matches: impl Fn(&str) -> bool,
) -> anyhow::Result<Vec<ModUpdateInfo>> {
    let mut hash_to_file: HashMap<String, String> = HashMap::new();
    if dir.exists() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let file_name = entry.file_name().to_string_lossy().into_owned();
            let lower = file_name.to_lowercase();
            if !matches(&lower) {
                continue;
            }
            hash_to_file.insert(sha1_hex_file(&entry.path())?, file_name);
        }
    }

    if hash_to_file.is_empty() {
        return Ok(Vec::new());
    }

    let hashes: Vec<&String> = hash_to_file.keys().collect();
    let body = serde_json::json!({ "hashes": hashes, "algorithm": "sha1" });
    let resp = client
        .post(format!("{MODRINTH_BASE}/version_files"))
        .json(&body)
        .send()
        .await?;
    let resp = ensure_success(resp, "Looking up files on Modrinth").await?;
    let matched: HashMap<String, ModrinthVersion> = resp.json().await?;

    let mut results = Vec::new();
    let mut identified: HashSet<String> = HashSet::new();

    for (hash, current) in &matched {
        let Some(file_name) = hash_to_file.get(hash) else {
            continue;
        };
        identified.insert(file_name.clone());

        let latest = fetch_latest_compatible(client, &current.project_id, game_version, loader)
            .await
            .unwrap_or(None);

        let update_available = latest.as_ref().is_some_and(|l| l.id != current.id);
        let download_url = latest
            .as_ref()
            .filter(|_| update_available)
            .and_then(|l| l.files.iter().find(|f| f.primary).or(l.files.first()))
            .map(|f| f.url.clone());

        let mut project_info = fetch_project_info(client, &current.project_id).await.ok();
        if project_info.is_none() {
            // A single Modrinth request occasionally times out or hiccups -
            // retry once rather than permanently showing a blank icon/title.
            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
            project_info = fetch_project_info(client, &current.project_id).await.ok();
        }

        results.push(ModUpdateInfo {
            file_name: file_name.clone(),
            project_id: Some(current.project_id.clone()),
            title: project_info.as_ref().map(|p| p.title.clone()),
            icon_url: project_info.and_then(|p| p.icon_url),
            current_version: Some(current.version_number.clone()),
            latest_version: latest.map(|l| l.version_number),
            update_available,
            download_url,
        });
    }

    for file_name in hash_to_file.into_values() {
        if !identified.contains(&file_name) {
            results.push(ModUpdateInfo {
                file_name,
                project_id: None,
                title: None,
                icon_url: None,
                current_version: None,
                latest_version: None,
                update_available: false,
                download_url: None,
            });
        }
    }

    results.sort_by(|a, b| a.file_name.to_lowercase().cmp(&b.file_name.to_lowercase()));
    Ok(results)
}

async fn fetch_latest_compatible(
    client: &reqwest::Client,
    project_id: &str,
    game_version: &str,
    loader: Option<&str>,
) -> anyhow::Result<Option<ModrinthVersion>> {
    let game_versions_param = format!("[\"{game_version}\"]");
    let mut query = vec![("game_versions", game_versions_param.as_str())];
    let loaders_param = loader.map(|l| format!("[\"{l}\"]"));
    if let Some(p) = &loaders_param {
        query.push(("loaders", p.as_str()));
    }

    let resp = client
        .get(format!("{MODRINTH_BASE}/project/{project_id}/version"))
        .query(&query)
        .send()
        .await?;

    if !resp.status().is_success() {
        return Ok(None);
    }

    let versions: Vec<ModrinthVersion> = resp.json().await?;
    Ok(versions.into_iter().next())
}

/// Downloads a mod update and replaces the old file. Only accepts URLs on
/// Modrinth's own CDN, since the filename is derived from it.
pub async fn apply_update(
    client: &reqwest::Client,
    mods_dir: &Path,
    old_file_name: &str,
    download_url: &str,
) -> anyhow::Result<()> {
    if !download_url.starts_with("https://cdn.modrinth.com/") {
        anyhow::bail!("Refusing to download from an unexpected host");
    }

    let old_path = mods_dir.join(old_file_name);
    if old_path.parent() != Some(mods_dir) {
        anyhow::bail!("Invalid mod file name");
    }

    let new_file_name = download_url
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty() && *s != "." && *s != "..")
        .ok_or_else(|| anyhow::anyhow!("Couldn't determine a file name for the update"))?;

    let resp = client.get(download_url).send().await?;
    let resp = ensure_success(resp, "Downloading mod update").await?;
    let bytes = resp.bytes().await?;

    let new_path = mods_dir.join(new_file_name);
    std::fs::write(&new_path, &bytes)?;
    if new_path != old_path {
        let _ = std::fs::remove_file(&old_path);
    }
    Ok(())
}

async fn search_projects(
    client: &reqwest::Client,
    query: &str,
    game_version: &str,
    loader: Option<&str>,
    project_type: &str,
    offset: u32,
) -> anyhow::Result<ModSearchPage> {
    let mut facets = vec![format!("[\"versions:{game_version}\"]"), format!("[\"project_type:{project_type}\"]")];
    if let Some(l) = loader {
        facets.push(format!("[\"categories:{l}\"]"));
    }
    let facets_param = format!("[{}]", facets.join(","));
    let offset_param = offset.to_string();

    let resp = client
        .get(format!("{MODRINTH_BASE}/search"))
        .query(&[
            ("query", query),
            ("facets", facets_param.as_str()),
            ("limit", "20"),
            ("offset", offset_param.as_str()),
        ])
        .send()
        .await?;
    let resp = ensure_success(resp, "Searching Modrinth").await?;
    let parsed: ModrinthSearchResponse = resp.json().await?;

    Ok(ModSearchPage {
        total_hits: parsed.total_hits,
        hits: parsed
            .hits
            .into_iter()
            .map(|h| ModSearchResult {
                project_id: h.project_id,
                slug: h.slug,
                title: h.title,
                description: h.description,
                author: h.author,
                downloads: h.downloads,
                icon_url: h.icon_url,
            })
            .collect(),
    })
}

pub async fn search_mods(
    client: &reqwest::Client,
    query: &str,
    game_version: &str,
    loader: Option<&str>,
    offset: u32,
) -> anyhow::Result<ModSearchPage> {
    search_projects(client, query, game_version, loader, "mod", offset).await
}

/// Resource packs have no mod loader, so there's no `loader` parameter here -
/// only the game version narrows results.
pub async fn search_resourcepacks(
    client: &reqwest::Client,
    query: &str,
    game_version: &str,
    offset: u32,
) -> anyhow::Result<ModSearchPage> {
    search_projects(client, query, game_version, None, "resourcepack", offset).await
}

async fn fetch_project_info(client: &reqwest::Client, project_id: &str) -> anyhow::Result<ModrinthProject> {
    let resp = client
        .get(format!("{MODRINTH_BASE}/project/{project_id}"))
        .send()
        .await?;
    let resp = ensure_success(resp, "Fetching mod info").await?;
    Ok(resp.json().await?)
}

async fn fetch_project_author(client: &reqwest::Client, project_id: &str) -> Option<String> {
    let resp = client
        .get(format!("{MODRINTH_BASE}/project/{project_id}/members"))
        .send()
        .await
        .ok()?;
    let members: Vec<ModrinthTeamMember> = resp.json().await.ok()?;
    let owners: Vec<String> = members
        .iter()
        .filter(|m| m.role.eq_ignore_ascii_case("owner"))
        .map(|m| m.user.username.clone())
        .collect();
    let names = if !owners.is_empty() {
        owners
    } else {
        members.into_iter().map(|m| m.user.username).collect()
    };
    let joined = names.join(", ");
    (!joined.is_empty()).then_some(joined)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModProjectDetails {
    pub project_id: String,
    pub title: String,
    pub description: String,
    pub icon_url: Option<String>,
    pub author: Option<String>,
    pub downloads: u64,
    pub client_side: String,
    pub server_side: String,
    pub categories: Vec<String>,
    pub latest_version: Option<String>,
    pub project_url: String,
}

/// Full details for one Modrinth project, for the "more info" view when
/// browsing search results - beyond what the search endpoint itself returns.
pub async fn fetch_project_details(
    client: &reqwest::Client,
    project_id: &str,
    game_version: &str,
    loader: Option<&str>,
    project_type_path: &str,
) -> anyhow::Result<ModProjectDetails> {
    let project = fetch_project_info(client, project_id).await?;
    let author = fetch_project_author(client, project_id).await;
    let latest_version = fetch_latest_compatible(client, project_id, game_version, loader)
        .await
        .unwrap_or(None)
        .map(|v| v.version_number);

    Ok(ModProjectDetails {
        project_id: project_id.to_string(),
        title: project.title,
        description: project.description,
        icon_url: project.icon_url,
        author,
        downloads: project.downloads,
        client_side: project.client_side,
        server_side: project.server_side,
        categories: project.categories,
        latest_version,
        project_url: format!("https://modrinth.com/{project_type_path}/{}", project.slug),
    })
}

async fn fetch_version_by_id(client: &reqwest::Client, version_id: &str) -> anyhow::Result<ModrinthVersion> {
    let resp = client.get(format!("{MODRINTH_BASE}/version/{version_id}")).send().await?;
    let resp = ensure_success(resp, "Fetching mod version").await?;
    Ok(resp.json().await?)
}

/// The set of Modrinth project ids already present in `mods_dir`, identified
/// by hash - used to avoid re-installing a mod or one of its dependencies.
async fn installed_project_ids(client: &reqwest::Client, mods_dir: &Path) -> anyhow::Result<HashSet<String>> {
    let mut hashes = Vec::new();
    if mods_dir.exists() {
        for entry in std::fs::read_dir(mods_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let lower = entry.file_name().to_string_lossy().to_lowercase();
            if lower.ends_with(".jar") || lower.ends_with(".jar.disabled") {
                hashes.push(sha1_hex_file(&entry.path())?);
            }
        }
    }
    if hashes.is_empty() {
        return Ok(HashSet::new());
    }

    let body = serde_json::json!({ "hashes": hashes, "algorithm": "sha1" });
    let resp = client
        .post(format!("{MODRINTH_BASE}/version_files"))
        .json(&body)
        .send()
        .await?;
    let resp = ensure_success(resp, "Looking up installed mods on Modrinth").await?;
    let matched: HashMap<String, ModrinthVersion> = resp.json().await?;
    Ok(matched.into_values().map(|v| v.project_id).collect())
}

/// Installs a mod and, recursively, any of its "required" dependencies that
/// aren't already present - the same way Modrinth's own app does it.
pub async fn install_mod(
    client: &reqwest::Client,
    mods_dir: &Path,
    game_version: &str,
    loader: Option<&str>,
    project_id: &str,
) -> anyhow::Result<InstallSummary> {
    std::fs::create_dir_all(mods_dir)?;

    let already_present = installed_project_ids(client, mods_dir).await?;
    let mut meta = mod_meta::load(mods_dir);

    let mut installed = Vec::new();
    let mut already_installed = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: Vec<String> = vec![project_id.to_string()];

    while let Some(pid) = queue.pop() {
        if !visited.insert(pid.clone()) {
            continue;
        }
        if already_present.contains(&pid) {
            let title = fetch_project_info(client, &pid)
                .await
                .map(|p| p.title)
                .unwrap_or(pid);
            already_installed.push(title);
            continue;
        }

        let version = fetch_latest_compatible(client, &pid, game_version, loader)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!("No version of this mod is compatible with this Minecraft version/loader")
            })?;

        let file = version
            .files
            .iter()
            .find(|f| f.primary)
            .or(version.files.first())
            .ok_or_else(|| anyhow::anyhow!("This mod has no downloadable file"))?;

        let resp = client.get(&file.url).send().await?;
        let resp = ensure_success(resp, "Downloading mod").await?;
        let bytes = resp.bytes().await?;
        std::fs::write(mods_dir.join(&file.filename), &bytes)?;

        meta.insert(
            file.filename.clone(),
            ModMeta {
                project_id: pid.clone(),
                is_dependency: pid != project_id,
            },
        );

        let title = fetch_project_info(client, &pid)
            .await
            .map(|p| p.title)
            .unwrap_or_else(|_| pid.clone());
        installed.push(InstalledModInfo {
            project_id: pid,
            title,
            file_name: file.filename.clone(),
        });

        for dep in &version.dependencies {
            if dep.dependency_type != "required" {
                continue;
            }
            if let Some(dep_pid) = &dep.project_id {
                queue.push(dep_pid.clone());
            } else if let Some(dep_vid) = &dep.version_id {
                if let Ok(v) = fetch_version_by_id(client, dep_vid).await {
                    queue.push(v.project_id);
                }
            }
        }
    }

    let _ = mod_meta::save(mods_dir, &meta);

    Ok(InstallSummary {
        installed,
        already_installed,
    })
}

/// The set of Modrinth project ids already present in `resourcepacks_dir`,
/// identified by hash - same approach as `installed_project_ids`, but
/// resource packs have no ".disabled" convention to also match.
async fn installed_resourcepack_project_ids(
    client: &reqwest::Client,
    resourcepacks_dir: &Path,
) -> anyhow::Result<HashSet<String>> {
    let mut hashes = Vec::new();
    if resourcepacks_dir.exists() {
        for entry in std::fs::read_dir(resourcepacks_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            if entry.file_name().to_string_lossy().to_lowercase().ends_with(".zip") {
                hashes.push(sha1_hex_file(&entry.path())?);
            }
        }
    }
    if hashes.is_empty() {
        return Ok(HashSet::new());
    }

    let body = serde_json::json!({ "hashes": hashes, "algorithm": "sha1" });
    let resp = client
        .post(format!("{MODRINTH_BASE}/version_files"))
        .json(&body)
        .send()
        .await?;
    let resp = ensure_success(resp, "Looking up installed resource packs on Modrinth").await?;
    let matched: HashMap<String, ModrinthVersion> = resp.json().await?;
    Ok(matched.into_values().map(|v| v.project_id).collect())
}

/// Installs a resource pack. Unlike mods, resource packs have no loader and
/// (in practice) no dependency graph worth resolving, so this is a single
/// download rather than `install_mod`'s recursive walk.
pub async fn install_resourcepack(
    client: &reqwest::Client,
    resourcepacks_dir: &Path,
    game_version: &str,
    project_id: &str,
) -> anyhow::Result<InstalledModInfo> {
    std::fs::create_dir_all(resourcepacks_dir)?;

    let already_present = installed_resourcepack_project_ids(client, resourcepacks_dir).await?;
    if already_present.contains(project_id) {
        anyhow::bail!("This resource pack is already installed");
    }

    let version = fetch_latest_compatible(client, project_id, game_version, None)
        .await?
        .ok_or_else(|| anyhow::anyhow!("No version of this resource pack is compatible with this Minecraft version"))?;

    let file = version
        .files
        .iter()
        .find(|f| f.primary)
        .or(version.files.first())
        .ok_or_else(|| anyhow::anyhow!("This resource pack has no downloadable file"))?;

    let resp = client.get(&file.url).send().await?;
    let resp = ensure_success(resp, "Downloading resource pack").await?;
    let bytes = resp.bytes().await?;
    std::fs::write(resourcepacks_dir.join(&file.filename), &bytes)?;

    let title = fetch_project_info(client, project_id)
        .await
        .map(|p| p.title)
        .unwrap_or_else(|_| project_id.to_string());

    Ok(InstalledModInfo {
        project_id: project_id.to_string(),
        title,
        file_name: file.filename.clone(),
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcePackDetails {
    pub file_name: String,
    pub size: u64,
    pub found: bool,
    pub project_id: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub author: Option<String>,
    pub downloads: Option<u64>,
    pub current_version: Option<String>,
    pub categories: Vec<String>,
    pub project_url: Option<String>,
}

/// Looks up everything we can find about one installed resource pack file -
/// identified by its hash, same as `fetch_mod_details` - for the info panel.
pub async fn fetch_resourcepack_details(
    client: &reqwest::Client,
    resourcepacks_dir: &Path,
    file_name: &str,
) -> anyhow::Result<ResourcePackDetails> {
    let path = resourcepacks_dir.join(file_name);
    if path.parent() != Some(resourcepacks_dir) {
        anyhow::bail!("Invalid resource pack file name");
    }
    let size = std::fs::metadata(&path)?.len();

    let hash = sha1_hex_file(&path)?;
    let body = serde_json::json!({ "hashes": [hash], "algorithm": "sha1" });
    let resp = client
        .post(format!("{MODRINTH_BASE}/version_files"))
        .json(&body)
        .send()
        .await?;
    let resp = ensure_success(resp, "Looking up resource pack on Modrinth").await?;
    let matched: HashMap<String, ModrinthVersion> = resp.json().await?;

    let base = ResourcePackDetails {
        file_name: file_name.to_string(),
        size,
        found: false,
        project_id: None,
        title: None,
        description: None,
        icon_url: None,
        author: None,
        downloads: None,
        current_version: None,
        categories: Vec::new(),
        project_url: None,
    };

    let Some(version) = matched.into_values().next() else {
        return Ok(base);
    };

    let project = fetch_project_info(client, &version.project_id).await.ok();
    let author = fetch_project_author(client, &version.project_id).await;

    Ok(ResourcePackDetails {
        found: true,
        project_id: Some(version.project_id),
        title: project.as_ref().map(|p| p.title.clone()),
        description: project.as_ref().map(|p| p.description.clone()),
        icon_url: project.as_ref().and_then(|p| p.icon_url.clone()),
        author,
        downloads: project.as_ref().map(|p| p.downloads),
        current_version: Some(version.version_number),
        categories: project.as_ref().map(|p| p.categories.clone()).unwrap_or_default(),
        project_url: project.as_ref().map(|p| format!("https://modrinth.com/resourcepack/{}", p.slug)),
        ..base
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModDetails {
    pub file_name: String,
    pub size: u64,
    pub enabled: bool,
    pub is_dependency: bool,
    pub found: bool,
    pub project_id: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    pub author: Option<String>,
    pub downloads: Option<u64>,
    pub current_version: Option<String>,
    pub client_side: Option<String>,
    pub server_side: Option<String>,
    pub categories: Vec<String>,
    pub project_url: Option<String>,
}

/// Looks up everything we can find about one installed mod file - identified
/// by its hash, the same way updates are matched - for the info panel.
pub async fn fetch_mod_details(
    client: &reqwest::Client,
    mods_dir: &Path,
    file_name: &str,
) -> anyhow::Result<ModDetails> {
    let path = mods_dir.join(file_name);
    if path.parent() != Some(mods_dir) {
        anyhow::bail!("Invalid mod file name");
    }
    let size = std::fs::metadata(&path)?.len();
    let enabled = !file_name.to_lowercase().ends_with(".disabled");
    let is_dependency = mod_meta::load(mods_dir)
        .get(file_name)
        .is_some_and(|m| m.is_dependency);

    let hash = sha1_hex_file(&path)?;
    let body = serde_json::json!({ "hashes": [hash], "algorithm": "sha1" });
    let resp = client
        .post(format!("{MODRINTH_BASE}/version_files"))
        .json(&body)
        .send()
        .await?;
    let resp = ensure_success(resp, "Looking up mod on Modrinth").await?;
    let matched: HashMap<String, ModrinthVersion> = resp.json().await?;

    let base = ModDetails {
        file_name: file_name.to_string(),
        size,
        enabled,
        is_dependency,
        found: false,
        project_id: None,
        title: None,
        description: None,
        icon_url: None,
        author: None,
        downloads: None,
        current_version: None,
        client_side: None,
        server_side: None,
        categories: Vec::new(),
        project_url: None,
    };

    let Some(version) = matched.into_values().next() else {
        return Ok(base);
    };

    let project = fetch_project_info(client, &version.project_id).await.ok();
    let author = fetch_project_author(client, &version.project_id).await;

    Ok(ModDetails {
        found: true,
        project_id: Some(version.project_id),
        title: project.as_ref().map(|p| p.title.clone()),
        description: project.as_ref().map(|p| p.description.clone()),
        icon_url: project.as_ref().and_then(|p| p.icon_url.clone()),
        author,
        downloads: project.as_ref().map(|p| p.downloads),
        current_version: Some(version.version_number),
        client_side: project.as_ref().map(|p| p.client_side.clone()),
        server_side: project.as_ref().map(|p| p.server_side.clone()),
        categories: project.as_ref().map(|p| p.categories.clone()).unwrap_or_default(),
        project_url: project.as_ref().map(|p| format!("https://modrinth.com/mod/{}", p.slug)),
        ..base
    })
}
