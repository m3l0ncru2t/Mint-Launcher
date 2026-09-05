use super::download::emit_progress;
use super::manifest::VersionDetail;
use crate::state::AppState;
use std::path::{Path, PathBuf};

/// Very old version manifests predate the `javaVersion` field entirely -
/// they're all from the Java 8 era, so that's the safe fallback.
const DEFAULT_JAVA_MAJOR: u32 = 8;

/// Finds a Java runtime that satisfies `detail.javaVersion`, in order of
/// preference: a runtime this app already auto-downloaded, then a matching
/// system install, then (if neither exists) downloads a matching Eclipse
/// Temurin JRE from Adoptium and caches it for next time - the same approach
/// Prism/MultiMC use, since most people installing a random launcher won't
/// already have the right JDK on PATH. Returns the java/javaw binary to
/// launch with, either a bare command name (resolved via PATH, for the
/// system case) or an absolute path into the cached runtime.
pub async fn ensure_java(
    app: &tauri::AppHandle,
    state: &AppState,
    instance_id: &str,
    detail: &VersionDetail,
) -> anyhow::Result<PathBuf> {
    let major = detail.java_version.as_ref().map(|v| v.major_version).unwrap_or(DEFAULT_JAVA_MAJOR);

    let managed_dir = state.java_dir().join(major.to_string());
    if let Some(home) = find_cached_home(&managed_dir) {
        return Ok(managed_launch_binary(&home));
    }

    if system_java_matches(major).await {
        return Ok(PathBuf::from(system_launch_command()));
    }

    download_and_extract_jre(app, state, instance_id, major, &managed_dir).await
}

fn managed_launch_binary(home: &Path) -> PathBuf {
    home.join("bin").join(if cfg!(target_os = "windows") { "javaw.exe" } else { "java" })
}

fn system_launch_command() -> &'static str {
    if cfg!(target_os = "windows") {
        "javaw"
    } else {
        "java"
    }
}

/// `javaw` deliberately has no console output, so version probing always
/// goes through the plain `java` binary even though launching prefers
/// `javaw` (no console flash) where available.
async fn system_java_matches(required_major: u32) -> bool {
    let Ok(output) = tokio::process::Command::new("java").arg("-version").output().await else {
        return false;
    };
    let combined = format!("{}{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    parse_java_major(&combined) == Some(required_major)
}

/// Pulls the major version out of `java -version`'s output, e.g.
/// `openjdk version "17.0.9" 2023-10-17` -> 17, or the pre-Java-9 scheme
/// `java version "1.8.0_392"` -> 8.
fn parse_java_major(output: &str) -> Option<u32> {
    let start = output.find('"')? + 1;
    let end = start + output[start..].find('"')?;
    let version = &output[start..end];
    let mut parts = version.split('.');
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}

/// A previously auto-downloaded runtime's actual top-level folder name
/// varies per exact Adoptium build, so it's recorded once (see
/// `locate_and_record_home`) instead of re-discovered by guessing.
fn find_cached_home(managed_dir: &Path) -> Option<PathBuf> {
    let rel = std::fs::read_to_string(managed_dir.join("home.txt")).ok()?;
    let home = managed_dir.join(rel.trim());
    managed_launch_binary(&home).is_file().then_some(home)
}

fn adoptium_os() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "mac"
    } else {
        "linux"
    }
}

fn adoptium_arch() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else if cfg!(target_arch = "x86") {
        "x86"
    } else {
        "x64"
    }
}

async fn download_and_extract_jre(
    app: &tauri::AppHandle,
    state: &AppState,
    instance_id: &str,
    major: u32,
    managed_dir: &Path,
) -> anyhow::Result<PathBuf> {
    emit_progress(app, instance_id, "java", &format!("Downloading Java {major} runtime"), 0, 1);

    let os = adoptium_os();
    let arch = adoptium_arch();
    let url =
        format!("https://api.adoptium.net/v3/binary/latest/{major}/ga/{os}/{arch}/jre/hotspot/normal/eclipse");

    let resp = state
        .http
        .get(&url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Couldn't reach Adoptium to download a Java {major} runtime: {e}"))?;
    let resp = resp.error_for_status().map_err(|_| {
        anyhow::anyhow!(
            "No Java {major} build is available for this system ({os}/{arch}) - install a JDK manually and make sure it's on PATH."
        )
    })?;
    let bytes = resp.bytes().await?;

    if managed_dir.exists() {
        std::fs::remove_dir_all(managed_dir)?;
    }
    std::fs::create_dir_all(managed_dir)?;

    emit_progress(app, instance_id, "java", &format!("Extracting Java {major} runtime"), 0, 1);
    if cfg!(target_os = "windows") {
        extract_zip(&bytes, managed_dir)?;
    } else {
        extract_tar_gz(&bytes, managed_dir)?;
    }

    let home = locate_and_record_home(managed_dir)?;
    emit_progress(app, instance_id, "java", "Java runtime ready", 1, 1);
    Ok(managed_launch_binary(&home))
}

fn extract_zip(bytes: &[u8], dest: &Path) -> anyhow::Result<()> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(rel_path) = entry.enclosed_name() else {
            continue;
        };
        let out_path = dest.join(rel_path);
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out_file = std::fs::File::create(&out_path)?;
            std::io::copy(&mut entry, &mut out_file)?;
        }
    }
    Ok(())
}

fn extract_tar_gz(bytes: &[u8], dest: &Path) -> anyhow::Result<()> {
    let gz = flate2::read::GzDecoder::new(std::io::Cursor::new(bytes));
    tar::Archive::new(gz).unpack(dest)?;
    Ok(())
}

/// Recursively hunts down the extracted archive's `bin/java` (macOS builds
/// nest it under `Contents/Home/bin/java`) and records the home directory
/// relative to `managed_dir` so future launches can find it in one read
/// instead of re-searching.
fn locate_and_record_home(managed_dir: &Path) -> anyhow::Result<PathBuf> {
    fn search(dir: &Path, depth: u32) -> Option<PathBuf> {
        if depth > 5 {
            return None;
        }
        let probe = dir.join("bin").join(if cfg!(target_os = "windows") { "java.exe" } else { "java" });
        if probe.is_file() {
            return Some(dir.to_path_buf());
        }
        for entry in std::fs::read_dir(dir).ok()?.flatten() {
            if entry.file_type().ok()?.is_dir() {
                if let Some(found) = search(&entry.path(), depth + 1) {
                    return Some(found);
                }
            }
        }
        None
    }

    let home = search(managed_dir, 0)
        .ok_or_else(|| anyhow::anyhow!("downloaded Java runtime doesn't contain a bin/java executable"))?;
    let rel = home.strip_prefix(managed_dir).unwrap_or(&home);
    std::fs::write(managed_dir.join("home.txt"), rel.to_string_lossy().as_bytes())?;
    Ok(home)
}
