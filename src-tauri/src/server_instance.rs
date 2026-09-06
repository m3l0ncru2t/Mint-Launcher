//! Importing an existing dedicated server folder as a server `Instance`.
//! Creation from scratch is handled directly by
//! `commands::instances::create_server_instance` - this module only covers
//! the "point at an existing folder" path, which needs its own file-copying
//! logic since a bare server folder carries none of the launcher-specific
//! metadata `importer::scan` relies on to auto-detect version/loader.

use crate::importer::{copy_dir_recursive, is_excluded_dir, walk_stats};
use crate::instance::{self, Instance, InstanceKind, ModLoader};
use std::fs;
use std::path::Path;

/// Loose root files worth carrying over from an existing server folder.
/// Deliberately curated (not "every loose file") for the same reason
/// `importer::CONTENT_FILES` is - avoids sweeping up anything unexpected
/// sitting at the root of a folder the user picked by hand.
const SERVER_CONTENT_FILES: &[&str] =
    &["server.properties", "eula.txt", "whitelist.json", "ops.json", "banned-players.json", "banned-ips.json"];

fn server_content_stats(source_dir: &Path) -> (u64, u64) {
    let mut count = 0u64;
    let mut bytes = 0u64;
    if let Ok(entries) = fs::read_dir(source_dir) {
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else { continue };
            if file_type.is_dir() && !is_excluded_dir(&entry.file_name().to_string_lossy()) {
                walk_stats(&entry.path(), &mut count, &mut bytes);
            }
        }
    }
    for file_name in SERVER_CONTENT_FILES {
        if let Ok(meta) = fs::metadata(source_dir.join(file_name)) {
            count += 1;
            bytes += meta.len();
        }
    }
    (count, bytes)
}

/// Imports an existing dedicated server folder (e.g. one previously run by
/// hand or managed by another panel) as a new server `Instance`.
/// `version_id`/`loader`/`loader_version` come straight from the Import
/// Server dialog's manual entry fields rather than being auto-detected -
/// unlike `importer::scan`, there's no known launcher metadata file to read
/// them from here.
pub fn import_server_folder(
    instances_root: &Path,
    source_dir: &Path,
    name: String,
    version_id: String,
    loader: ModLoader,
    loader_version: Option<String>,
    eula_accepted: bool,
    mut on_progress: impl FnMut(u64, u64, &str),
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

    let mut inst =
        instance::create_instance(instances_root, name, version_id, loader, loader_version, InstanceKind::Server)?;
    let game_dir = inst.game_dir(instances_root);

    let (total_files, _) = server_content_stats(source_dir);
    let mut copied: u64 = 0;
    on_progress(0, total_files, "Starting import…");

    for entry in fs::read_dir(source_dir)?.flatten() {
        let Ok(file_type) = entry.file_type() else { continue };
        if !file_type.is_dir() {
            continue;
        }
        let dir_name = entry.file_name();
        if is_excluded_dir(&dir_name.to_string_lossy()) {
            continue;
        }
        copy_dir_recursive(&entry.path(), &game_dir.join(&dir_name), &mut copied, total_files, &mut on_progress)?;
    }
    for file_name in SERVER_CONTENT_FILES {
        let src = source_dir.join(file_name);
        if src.is_file() {
            let _ = fs::copy(&src, game_dir.join(file_name));
            copied += 1;
            on_progress(copied, total_files, file_name);
        }
    }

    inst.eula_accepted = true;
    inst.save(instances_root)?;
    Ok(inst)
}
