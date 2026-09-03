use super::hash::sha1_hex;
use super::manifest::{AssetIndex, Library, VersionDetail, VersionManifest, VersionManifestEntry};
use super::rules::{current_arch, current_os_name, rules_allow};
use crate::state::AppState;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::Emitter;

const VERSION_MANIFEST_URL: &str = "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub instance_id: String,
    pub stage: String,
    pub message: String,
    pub current: u64,
    pub total: u64,
}

fn emit_progress(app: &tauri::AppHandle, instance_id: &str, stage: &str, message: &str, current: u64, total: u64) {
    let _ = app.emit(
        "launch-progress",
        DownloadProgress {
            instance_id: instance_id.to_string(),
            stage: stage.to_string(),
            message: message.to_string(),
            current,
            total,
        },
    );
}

pub async fn fetch_version_manifest(state: &AppState) -> anyhow::Result<VersionManifest> {
    let cache_path = state.versions_dir().join("version_manifest_v2.json");
    match state.http.get(VERSION_MANIFEST_URL).send().await {
        Ok(resp) if resp.status().is_success() => {
            let bytes = resp.bytes().await?;
            std::fs::create_dir_all(state.versions_dir())?;
            std::fs::write(&cache_path, &bytes)?;
            Ok(serde_json::from_slice(&bytes)?)
        }
        _ => {
            let data = std::fs::read(&cache_path)
                .map_err(|_| anyhow::anyhow!("no network and no cached version manifest"))?;
            Ok(serde_json::from_slice(&data)?)
        }
    }
}

pub async fn fetch_version_detail(
    state: &AppState,
    entry: &VersionManifestEntry,
) -> anyhow::Result<VersionDetail> {
    let dir = state.versions_dir().join(&entry.id);
    let cache_path = dir.join(format!("{}.json", entry.id));
    std::fs::create_dir_all(&dir)?;

    if let Ok(cached) = std::fs::read(&cache_path) {
        if let Ok(detail) = serde_json::from_slice::<VersionDetail>(&cached) {
            return Ok(detail);
        }
    }

    let bytes = state.http.get(&entry.url).send().await?.bytes().await?;
    std::fs::write(&cache_path, &bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}

async fn download_verified(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    expected_sha1: Option<&str>,
) -> anyhow::Result<()> {
    if dest.exists() {
        if let Some(expected) = expected_sha1 {
            if let Ok(existing) = std::fs::read(dest) {
                if sha1_hex(&existing) == expected {
                    return Ok(());
                }
            }
        } else {
            return Ok(());
        }
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = client.get(url).send().await?.error_for_status()?.bytes().await?;
    if let Some(expected) = expected_sha1 {
        let actual = sha1_hex(&bytes);
        if actual != expected {
            anyhow::bail!("sha1 mismatch for {url}: expected {expected}, got {actual}");
        }
    }
    std::fs::write(dest, &bytes)?;
    Ok(())
}

pub async fn download_client_jar(
    app: &tauri::AppHandle,
    state: &AppState,
    instance_id: &str,
    detail: &VersionDetail,
) -> anyhow::Result<PathBuf> {
    emit_progress(app, instance_id, "client", "Downloading client jar", 0, 1);
    let dest = state
        .versions_dir()
        .join(&detail.id)
        .join(format!("{}.jar", detail.id));
    download_verified(&state.http, &detail.downloads.client.url, &dest, Some(&detail.downloads.client.sha1))
        .await?;
    emit_progress(app, instance_id, "client", "Client jar ready", 1, 1);
    Ok(dest)
}

/// Returns (classpath entries, native jar paths to extract).
pub async fn download_libraries(
    app: &tauri::AppHandle,
    state: &AppState,
    instance_id: &str,
    libraries: &[Library],
) -> anyhow::Result<(Vec<PathBuf>, Vec<PathBuf>)> {
    let features = HashMap::new();
    let applicable: Vec<&Library> = libraries
        .iter()
        .filter(|lib| rules_allow(lib.rules.as_deref(), &features))
        .collect();

    let total = applicable.len() as u64;
    let mut classpath = Vec::new();
    let mut natives = Vec::new();

    for (i, lib) in applicable.iter().enumerate() {
        emit_progress(
            app,
            instance_id,
            "libraries",
            &format!("Library {}", lib.name),
            i as u64,
            total,
        );

        let mut resolved = false;

        if let Some(downloads) = &lib.downloads {
            if let Some(artifact) = &downloads.artifact {
                let dest = state.libraries_dir().join(&artifact.path);
                download_verified(&state.http, &artifact.url, &dest, Some(&artifact.sha1)).await?;
                classpath.push(dest);
                resolved = true;
            }

            if let Some(natives_map) = &lib.natives {
                if let Some(classifier_key) = natives_map.get(current_os_name()) {
                    let classifier_key = classifier_key.replace("${arch}", arch_bits());
                    if let Some(classifiers) = &downloads.classifiers {
                        if let Some(artifact) = classifiers.get(&classifier_key) {
                            let dest = state.libraries_dir().join(&artifact.path);
                            download_verified(&state.http, &artifact.url, &dest, Some(&artifact.sha1))
                                .await?;
                            natives.push(dest);
                        }
                    }
                }
            }
        }

        // Fabric/Forge-style libraries: just a Maven coordinate + a
        // repository base URL, no pre-resolved per-artifact download info.
        if !resolved {
            if let Some(repo_url) = &lib.url {
                if let Some(rel_path) = maven_coordinates_to_path(&lib.name) {
                    let dest = state.libraries_dir().join(&rel_path);
                    let full_url = format!("{}/{}", repo_url.trim_end_matches('/'), rel_path);
                    // No published sha1 for these - skip if already present,
                    // otherwise download and trust the (versioned) artifact.
                    download_verified(&state.http, &full_url, &dest, None).await?;
                    classpath.push(dest);
                }
            }
        }
    }

    emit_progress(app, instance_id, "libraries", "Libraries ready", total, total);
    Ok((classpath, natives))
}

/// Resolves a Maven coordinate (`group:artifact:version[:classifier][@ext]`)
/// to the relative path Maven repositories serve it at, e.g.
/// `net.fabricmc:fabric-loader:0.15.7` ->
/// `net/fabricmc/fabric-loader/0.15.7/fabric-loader-0.15.7.jar`.
fn maven_coordinates_to_path(name: &str) -> Option<String> {
    let (coords, ext) = name.split_once('@').unwrap_or((name, "jar"));
    let parts: Vec<&str> = coords.split(':').collect();
    if parts.len() < 3 {
        return None;
    }
    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let filename = match parts.get(3) {
        Some(classifier) => format!("{artifact}-{version}-{classifier}.{ext}"),
        None => format!("{artifact}-{version}.{ext}"),
    };
    Some(format!("{group}/{artifact}/{version}/{filename}"))
}

fn arch_bits() -> &'static str {
    match current_arch() {
        "x64" | "arm64" => "64",
        _ => "32",
    }
}

pub fn extract_natives(natives_jars: &[PathBuf], dest_dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dest_dir)?;
    for jar_path in natives_jars {
        let file = std::fs::File::open(jar_path)?;
        let mut archive = zip::ZipArchive::new(file)?;
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let name = entry.name().to_string();
            if name.starts_with("META-INF/") || name.ends_with('/') {
                continue;
            }
            let Some(file_name) = Path::new(&name).file_name() else {
                continue;
            };
            let out_path = dest_dir.join(file_name);
            let mut out_file = std::fs::File::create(&out_path)?;
            std::io::copy(&mut entry, &mut out_file)?;
        }
    }
    Ok(())
}

pub async fn download_assets(
    app: &tauri::AppHandle,
    state: &AppState,
    instance_id: &str,
    detail: &VersionDetail,
) -> anyhow::Result<()> {
    emit_progress(app, instance_id, "assets", "Fetching asset index", 0, 1);

    let index_dest = state
        .assets_dir()
        .join("indexes")
        .join(format!("{}.json", detail.asset_index.id));
    download_verified(
        &state.http,
        &detail.asset_index.url,
        &index_dest,
        Some(&detail.asset_index.sha1),
    )
    .await?;

    let index_bytes = std::fs::read(&index_dest)?;
    let asset_index: AssetIndex = serde_json::from_slice(&index_bytes)?;

    let total = asset_index.objects.len() as u64;
    let objects_dir = state.assets_dir().join("objects");

    for (i, (name, object)) in asset_index.objects.iter().enumerate() {
        if i % 25 == 0 {
            emit_progress(app, instance_id, "assets", &format!("Asset {name}"), i as u64, total);
        }
        let prefix = &object.hash[0..2];
        let dest = objects_dir.join(prefix).join(&object.hash);
        let url = format!("https://resources.download.minecraft.net/{prefix}/{}", object.hash);
        download_verified(&state.http, &url, &dest, Some(&object.hash)).await?;
    }

    emit_progress(app, instance_id, "assets", "Assets ready", total, total);
    Ok(())
}
