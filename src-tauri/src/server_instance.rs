//! Importing an existing dedicated server folder as a server `Instance`.
//! Creation from scratch is handled directly by
//! `commands::instances::create_server_instance` - this module only covers
//! the "point at an existing folder" path.
//!
//! An imported server is kept fully in place (see `Instance::external_dir`):
//! nothing is copied, and Mint launches it against the *original* folder -
//! copying world saves/mods/configs into Mint's own instances tree would
//! duplicate what can easily be gigabytes of data for no benefit, and the
//! user already has this folder set up exactly how they want it. Version and
//! mod loader are auto-detected from the folder's own files (see
//! `detect_server`) rather than asked for manually, since they're already
//! implied by whatever's actually installed there - a user pointing Mint at
//! a real, working server shouldn't have to re-tell it what that server is.

use crate::instance::{self, Instance, InstanceKind, ModLoader};
use std::fs;
use std::io::Read;
use std::path::Path;

pub struct ServerDetection {
    pub version_id: String,
    pub loader: ModLoader,
    pub loader_version: Option<String>,
}

/// The name of one subdirectory under `dir`, if any exist - used to read a
/// version/loader straight off a Maven-style layout
/// (`libraries/net/fabricmc/fabric-loader/<version>/...`) without parsing
/// any file contents.
fn first_subdir_name(dir: &Path) -> Option<String> {
    fs::read_dir(dir).ok()?.flatten().find_map(|entry| {
        entry
            .file_type()
            .ok()
            .filter(|t| t.is_dir())
            .map(|_| entry.file_name().to_string_lossy().into_owned())
    })
}

/// A Fabric server install always has
/// `libraries/net/fabricmc/fabric-loader/<loader-version>/` - reading that
/// folder name is exact and doesn't need to touch any file contents.
/// Forge/Quilt aren't launchable yet (see `do_launch_server`), so those are
/// detected just to fail with a clear message instead of silently being
/// treated as Vanilla.
fn detect_loader(source_dir: &Path) -> anyhow::Result<(ModLoader, Option<String>)> {
    if let Some(version) = first_subdir_name(&source_dir.join("libraries/net/fabricmc/fabric-loader")) {
        return Ok((ModLoader::Fabric, Some(version)));
    }
    if first_subdir_name(&source_dir.join("libraries/net/minecraftforge/forge")).is_some() {
        anyhow::bail!("This looks like a Forge server - Mint can't launch Forge servers yet");
    }
    if first_subdir_name(&source_dir.join("libraries/org/quiltmc/quilt-loader")).is_some() {
        anyhow::bail!("This looks like a Quilt server - Mint can't launch Quilt servers yet");
    }
    Ok((ModLoader::Vanilla, None))
}

/// Reads the exact Minecraft version straight from Fabric's own
/// `intermediary` mappings folder, present on every Fabric server - the only
/// place that names the game version directly (`fabric-loader`'s own folder
/// name is the *loader* version instead).
fn detect_version_from_intermediary(source_dir: &Path) -> Option<String> {
    first_subdir_name(&source_dir.join("libraries/net/fabricmc/intermediary"))
}

/// Modern (1.18+) server jars are a small "bundler" wrapper whose
/// `META-INF/versions.list` names the real game version - reads that
/// straight out of any root-level jar without ever running it.
fn detect_version_from_bundler_jar(source_dir: &Path) -> Option<String> {
    for entry in fs::read_dir(source_dir).ok()?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jar") {
            continue;
        }
        if let Some(version) = read_versions_list(&path) {
            return Some(version);
        }
    }
    None
}

fn read_versions_list(jar_path: &Path) -> Option<String> {
    let file = fs::File::open(jar_path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    let mut entry = archive.by_name("META-INF/versions.list").ok()?;
    let mut contents = String::new();
    entry.read_to_string(&mut contents).ok()?;
    // Tab-separated `<sha1>\t<version id>\t<jar path inside the bundler>`.
    contents.lines().next()?.split('\t').nth(1).map(str::to_string)
}

/// Last resort for a pre-bundler (pre-1.18) vanilla jar, which has no
/// embedded version metadata at all: the server prints its own version on
/// every boot, so a folder that's been run at least once has it in the log.
fn detect_version_from_log(source_dir: &Path) -> Option<String> {
    const MARKER: &str = "Starting minecraft server version ";
    let contents = fs::read_to_string(source_dir.join("logs/latest.log")).ok()?;
    for line in contents.lines() {
        if let Some(rest) = line.split(MARKER).nth(1) {
            if let Some(version) = rest.split_whitespace().next() {
                return Some(version.to_string());
            }
        }
    }
    None
}

fn detect_version(source_dir: &Path) -> Option<String> {
    detect_version_from_intermediary(source_dir)
        .or_else(|| detect_version_from_bundler_jar(source_dir))
        .or_else(|| detect_version_from_log(source_dir))
}

/// Auto-detects everything needed to launch an existing server folder - see
/// the module doc for why this is detected rather than asked for. Used both
/// as a live preview while the Import Server dialog is open and as the
/// authoritative source of truth inside `import_server_folder` itself.
pub fn detect_server(source_dir: &Path) -> anyhow::Result<ServerDetection> {
    let (loader, loader_version) = detect_loader(source_dir)?;
    let version_id = detect_version(source_dir).ok_or_else(|| {
        anyhow::anyhow!(
            "Couldn't automatically detect this server's Minecraft version. Start the server at least once \
             outside Mint (so it writes logs/latest.log) and try importing again."
        )
    })?;
    Ok(ServerDetection { version_id, loader, loader_version })
}

/// Imports an existing dedicated server folder in place - see the module doc.
pub fn import_server_folder(
    instances_root: &Path,
    source_dir: &Path,
    name: String,
    eula_accepted: bool,
) -> anyhow::Result<Instance> {
    if !source_dir.is_dir() {
        anyhow::bail!("That folder doesn't exist");
    }
    // A very common case: the folder belongs to a panel's own dedicated
    // system user (PufferPanel, Pterodactyl, etc. all run as their own
    // account, not the desktop user Mint runs as) - surface something
    // actionable instead of a bare "Permission denied (os error 13)".
    if let Err(e) = fs::read_dir(source_dir) {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            anyhow::bail!(
                "Mint can't read {} - permission denied. This usually means the folder belongs to another \
                 system user (e.g. a server panel like PufferPanel running as its own account). Copy it \
                 somewhere you own first, e.g.:\n\
                 sudo cp -r {} ~/server-import && sudo chown -R $USER ~/server-import\n\
                 then import that copy instead.",
                source_dir.display(),
                source_dir.display(),
            );
        }
        anyhow::bail!("Couldn't read {}: {e}", source_dir.display());
    }
    if !eula_accepted {
        anyhow::bail!("You must accept the Minecraft EULA to import a server");
    }

    let detection = detect_server(source_dir)?;

    let mut inst = instance::create_instance(
        instances_root,
        name,
        detection.version_id,
        detection.loader,
        detection.loader_version,
        InstanceKind::Server,
    )?;
    inst.external_dir = Some(source_dir.to_string_lossy().into_owned());
    inst.eula_accepted = true;
    inst.save(instances_root)?;
    Ok(inst)
}
