use crate::state::AppState;
use serde::{Deserialize, Serialize};
#[cfg(target_os = "windows")]
use std::io::Read;
use tauri::{AppHandle, State};

const REPO: &str = "m3l0ncru2t/Mint-Launcher";
/// Must match the asset name produced by the "Package portable Windows
/// build" step in .github/workflows/build.yml, and the exe name it zips up.
const PORTABLE_ASSET_NAME: &str = "Mint-Launcher-portable-windows.zip";
const PORTABLE_EXE_NAME: &str = "Mint Launcher.exe";

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    body: Option<String>,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortableUpdateInfo {
    pub version: String,
    pub notes: String,
    pub download_url: String,
}

/// The Tauri auto-updater (see UpdateBanner.tsx) works by downloading and
/// silently running the NSIS/MSI installer, which assumes an installed app
/// living at a fixed location - not a relocatable exe someone's carrying
/// around on a USB stick. Portable mode instead checks GitHub releases
/// directly and swaps the exe in place; the frontend picks between the two
/// flows based on this flag.
#[tauri::command]
pub fn is_portable() -> bool {
    crate::portable_root().is_some()
}

fn version_after_last_v(tag: &str) -> &str {
    tag.rsplit_once('v').map(|(_, v)| v).unwrap_or(tag)
}

fn version_tuple(v: &str) -> Vec<u64> {
    v.split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

#[tauri::command]
pub async fn check_portable_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<PortableUpdateInfo>, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let release: GithubRelease = state
        .http
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;

    let Some(asset) = release.assets.iter().find(|a| a.name == PORTABLE_ASSET_NAME) else {
        return Ok(None);
    };

    let remote_version = version_after_last_v(&release.tag_name).to_string();
    let current_version = app.package_info().version.to_string();
    if version_tuple(&remote_version) <= version_tuple(&current_version) {
        return Ok(None);
    }

    Ok(Some(PortableUpdateInfo {
        version: remote_version,
        notes: release.body.unwrap_or_default(),
        download_url: asset.browser_download_url.clone(),
    }))
}

#[cfg(not(target_os = "windows"))]
#[tauri::command]
pub async fn install_portable_update(
    _app: AppHandle,
    _state: State<'_, AppState>,
    _download_url: String,
) -> Result<(), String> {
    Err("Portable updates are only supported on Windows".to_string())
}

/// Downloads the new portable zip, drops the new exe next to the running
/// one, and hands off to a detached PowerShell script that waits for this
/// process to exit before swapping the files and relaunching - a normal
/// Windows exe can't overwrite (or delete) its own running file, so the
/// swap has to happen from a separate process after this one is gone.
#[cfg(target_os = "windows")]
#[tauri::command]
pub async fn install_portable_update(
    app: AppHandle,
    state: State<'_, AppState>,
    download_url: String,
) -> Result<(), String> {
    if crate::portable_root().is_none() {
        return Err("Not running a portable build".to_string());
    }

    let bytes = state
        .http
        .get(&download_url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;

    let exe_path = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_dir = exe_path
        .parent()
        .ok_or_else(|| "could not determine executable directory".to_string())?;

    let mut archive =
        zip::ZipArchive::new(std::io::Cursor::new(bytes.as_ref())).map_err(|e| e.to_string())?;
    let mut new_exe_bytes = Vec::new();
    archive
        .by_name(PORTABLE_EXE_NAME)
        .map_err(|_| format!("update archive is missing {PORTABLE_EXE_NAME}"))?
        .read_to_end(&mut new_exe_bytes)
        .map_err(|e| e.to_string())?;

    let new_exe_path = exe_dir.join("Mint Launcher.update.exe");
    std::fs::write(&new_exe_path, &new_exe_bytes).map_err(|e| e.to_string())?;

    let script_path = exe_dir.join("mint-launcher-update.ps1");
    let script = format!(
        r#"$ErrorActionPreference = 'SilentlyContinue'
Wait-Process -Id {pid} -Timeout 30
for ($i = 0; $i -lt 20; $i++) {{
    try {{
        Move-Item -Force -LiteralPath '{new_exe}' -Destination '{current_exe}'
        break
    }} catch {{
        Start-Sleep -Milliseconds 500
    }}
}}
Start-Process -FilePath '{current_exe}'
Remove-Item -Force -LiteralPath $PSCommandPath
"#,
        pid = std::process::id(),
        new_exe = new_exe_path.display(),
        current_exe = exe_path.display(),
    );
    std::fs::write(&script_path, script).map_err(|e| e.to_string())?;

    std::process::Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script_path)
        .spawn()
        .map_err(|e| e.to_string())?;

    app.exit(0);
    Ok(())
}
