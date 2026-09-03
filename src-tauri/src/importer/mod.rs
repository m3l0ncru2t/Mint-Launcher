use crate::instance::{self, Instance, ModLoader};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum LauncherKind {
    Official,
    MultiMc,
    CurseForge,
}

/// One importable "instance" found while scanning a folder the user picked.
/// For the official launcher this is a single installed version (its saves,
/// resourcepacks, etc. are shared across all versions there, so importing
/// several candidates from the same official `.minecraft` folder will copy
/// the same shared files into each resulting Mint instance - that mirrors
/// how the official launcher actually works, not a bug).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportCandidate {
    pub launcher: LauncherKind,
    pub name: String,
    /// The specific version/instance folder this candidate came from -
    /// carried through only for display/debugging, not used when copying.
    pub source_path: String,
    /// The `.minecraft`-equivalent content folder to copy saves/mods/etc
    /// out of.
    pub minecraft_dir: String,
    pub version_id: String,
    pub loader: ModLoader,
    pub loader_version: Option<String>,
    pub icon_base64: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedPath {
    pub label: String,
    pub path: String,
}

/// Common install locations for each supported launcher, per OS. Only
/// entries that actually exist on disk are returned, so the frontend can
/// offer them as one-click suggestions alongside a manual folder browse -
/// portable/custom installs won't show up here and need Browse instead.
pub fn suggest_paths() -> Vec<SuggestedPath> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let mut candidates: Vec<(&str, PathBuf)> = Vec::new();

    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = dirs::config_dir() {
            candidates.push(("Official Minecraft Launcher", appdata.join(".minecraft")));
            candidates.push(("Prism Launcher", appdata.join("PrismLauncher")));
            candidates.push(("PolyMC", appdata.join("PolyMC")));
            candidates.push(("MultiMC", appdata.join("MultiMC")));
        }
        candidates.push(("CurseForge", home.join("curseforge").join("minecraft").join("Instances")));
    }
    #[cfg(target_os = "macos")]
    {
        let support = home.join("Library").join("Application Support");
        candidates.push(("Official Minecraft Launcher", support.join("minecraft")));
        candidates.push(("Prism Launcher", support.join("PrismLauncher")));
        candidates.push(("PolyMC", support.join("PolyMC")));
        candidates.push((
            "CurseForge",
            home.join("Documents").join("curseforge").join("minecraft").join("Instances"),
        ));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        candidates.push(("Official Minecraft Launcher", home.join(".minecraft")));
        let local_share = home.join(".local").join("share");
        candidates.push(("Prism Launcher", local_share.join("PrismLauncher")));
        candidates.push(("PolyMC", local_share.join("PolyMC")));
        candidates.push(("MultiMC", local_share.join("multimc")));
    }

    candidates
        .into_iter()
        .filter(|(_, path)| path.is_dir())
        .map(|(label, path)| SuggestedPath {
            label: label.to_string(),
            path: path.to_string_lossy().into_owned(),
        })
        .collect()
}

/// Figures out what kind of launcher folder the user pointed at and returns
/// every importable instance found inside it. Accepts the launcher's root
/// folder (an official `.minecraft`, or a MultiMC-family/CurseForge
/// "instances" folder) as well as a single instance folder picked directly.
pub fn scan(path: &Path) -> anyhow::Result<Vec<ImportCandidate>> {
    if !path.is_dir() {
        anyhow::bail!("That folder doesn't exist");
    }

    if path.join("versions").is_dir() {
        return Ok(scan_official(path));
    }
    if path.join("mmc-pack.json").is_file() {
        return Ok(scan_multimc_instance(path).into_iter().collect());
    }
    if path.join("minecraftinstance.json").is_file() {
        return Ok(scan_curseforge_instance(path).into_iter().collect());
    }

    let instances_subdir = path.join("instances");
    if instances_subdir.is_dir() {
        let out = scan_children(&instances_subdir, scan_multimc_instance);
        if !out.is_empty() {
            return Ok(out);
        }
    }

    let out = scan_children(path, scan_curseforge_instance);
    if !out.is_empty() {
        return Ok(out);
    }

    anyhow::bail!(
        "Couldn't recognize a supported launcher here. Pick your .minecraft folder, a Prism/PolyMC/MultiMC \
         \"instances\" folder, or a CurseForge \"Instances\" folder."
    )
}

fn scan_children(
    dir: &Path,
    scan_one: impl Fn(&Path) -> Option<ImportCandidate>,
) -> Vec<ImportCandidate> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            if let Some(candidate) = scan_one(&entry.path()) {
                out.push(candidate);
            }
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

fn parse_version_json(json: &serde_json::Value, folder_id: &str) -> (String, ModLoader, Option<String>) {
    let base_version = json
        .get("inheritsFrom")
        .and_then(|v| v.as_str())
        .unwrap_or(folder_id)
        .to_string();

    let libs: Vec<&str> = json
        .get("libraries")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|l| l.get("name").and_then(|n| n.as_str())).collect())
        .unwrap_or_default();

    if let Some(lib) = libs.iter().find(|l| l.starts_with("net.fabricmc:fabric-loader:")) {
        return (base_version, ModLoader::Fabric, lib.rsplit(':').next().map(String::from));
    }
    if let Some(lib) = libs.iter().find(|l| l.starts_with("org.quiltmc:quilt-loader:")) {
        return (base_version, ModLoader::Quilt, lib.rsplit(':').next().map(String::from));
    }
    if let Some(lib) = libs
        .iter()
        .find(|l| l.starts_with("net.minecraftforge:forge:") || l.starts_with("net.minecraftforge:fmlloader:"))
    {
        let loader_version = lib.rsplit(':').next().map(|v| v.rsplit('-').next().unwrap_or(v).to_string());
        return (base_version, ModLoader::Forge, loader_version);
    }
    (base_version, ModLoader::Vanilla, None)
}

fn read_launcher_profile_names(root: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let Ok(data) = fs::read_to_string(root.join("launcher_profiles.json")) else {
        return map;
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&data) else {
        return map;
    };
    if let Some(profiles) = json.get("profiles").and_then(|p| p.as_object()) {
        for profile in profiles.values() {
            if let (Some(name), Some(last_version)) = (
                profile.get("name").and_then(|n| n.as_str()),
                profile.get("lastVersionId").and_then(|v| v.as_str()),
            ) {
                map.insert(last_version.to_string(), name.to_string());
            }
        }
    }
    map
}

fn scan_official(root: &Path) -> Vec<ImportCandidate> {
    let profile_names = read_launcher_profile_names(root);
    let mut out = scan_children(&root.join("versions"), |dir| {
        let folder_id = dir.file_name()?.to_str()?.to_string();
        let json_path = dir.join(format!("{folder_id}.json"));
        let data = fs::read_to_string(&json_path).ok()?;
        let json: serde_json::Value = serde_json::from_str(&data).ok()?;
        let (base_version, loader, loader_version) = parse_version_json(&json, &folder_id);
        let name = profile_names.get(&folder_id).cloned().unwrap_or_else(|| folder_id.clone());
        Some(ImportCandidate {
            launcher: LauncherKind::Official,
            name,
            source_path: dir.to_string_lossy().into_owned(),
            minecraft_dir: root.to_string_lossy().into_owned(),
            version_id: base_version,
            loader,
            loader_version,
            icon_base64: None,
        })
    });
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

fn read_ini_value(path: &Path, key: &str) -> Option<String> {
    let data = fs::read_to_string(path).ok()?;
    data.lines().find_map(|line| {
        let (k, v) = line.split_once('=')?;
        (k.trim() == key).then(|| v.trim().to_string())
    })
}

fn find_multimc_icon(instance_dir: &Path, icon_key: &str) -> Option<String> {
    let mut candidates = vec![instance_dir.join(format!("{icon_key}.png"))];
    if let Some(launcher_root) = instance_dir.parent().and_then(|p| p.parent()) {
        candidates.push(launcher_root.join("icons").join(format!("{icon_key}.png")));
    }
    for path in candidates {
        if let Ok(data) = fs::read(&path) {
            if instance::sniff_image_mime(&data).is_some() {
                return Some(STANDARD.encode(&data));
            }
        }
    }
    None
}

fn scan_multimc_instance(dir: &Path) -> Option<ImportCandidate> {
    let pack_data = fs::read_to_string(dir.join("mmc-pack.json")).ok()?;
    let pack: serde_json::Value = serde_json::from_str(&pack_data).ok()?;
    let components = pack.get("components").and_then(|c| c.as_array())?;

    let mut base_version = None;
    let mut loader = ModLoader::Vanilla;
    let mut loader_version = None;
    for c in components {
        let uid = c.get("uid").and_then(|u| u.as_str()).unwrap_or("");
        let version = c.get("version").and_then(|v| v.as_str()).map(String::from);
        match uid {
            "net.minecraft" => base_version = version,
            "net.fabricmc.fabric-loader" => {
                loader = ModLoader::Fabric;
                loader_version = version;
            }
            "org.quiltmc.quilt-loader" => {
                loader = ModLoader::Quilt;
                loader_version = version;
            }
            "net.minecraftforge" => {
                loader = ModLoader::Forge;
                loader_version = version;
            }
            _ => {}
        }
    }
    let base_version = base_version?;

    let content_dir = if dir.join(".minecraft").is_dir() {
        dir.join(".minecraft")
    } else {
        dir.join("minecraft")
    };
    if !content_dir.is_dir() {
        return None;
    }

    let name = read_ini_value(&dir.join("instance.cfg"), "name")
        .unwrap_or_else(|| dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
    let icon_base64 =
        read_ini_value(&dir.join("instance.cfg"), "iconKey").and_then(|key| find_multimc_icon(dir, &key));

    Some(ImportCandidate {
        launcher: LauncherKind::MultiMc,
        name,
        source_path: dir.to_string_lossy().into_owned(),
        minecraft_dir: content_dir.to_string_lossy().into_owned(),
        version_id: base_version,
        loader,
        loader_version,
        icon_base64,
    })
}

fn scan_curseforge_instance(dir: &Path) -> Option<ImportCandidate> {
    let data = fs::read_to_string(dir.join("minecraftinstance.json")).ok()?;
    let json: serde_json::Value = serde_json::from_str(&data).ok()?;

    let name = json
        .get("name")
        .and_then(|n| n.as_str())
        .map(String::from)
        .or_else(|| dir.file_name().map(|n| n.to_string_lossy().into_owned()))?;

    let base_version = json
        .get("baseModLoader")
        .and_then(|b| b.get("minecraftVersion"))
        .and_then(|v| v.as_str())
        .or_else(|| json.get("gameVersion").and_then(|v| v.as_str()))?
        .to_string();

    let loader_name = json.get("baseModLoader").and_then(|b| b.get("name")).and_then(|n| n.as_str());
    let (loader, loader_version) = match loader_name {
        Some(s) if s.starts_with("forge") => (ModLoader::Forge, s.strip_prefix("forge-").map(String::from)),
        Some(s) if s.starts_with("fabric") => (ModLoader::Fabric, s.strip_prefix("fabric-").map(String::from)),
        Some(s) if s.starts_with("quilt") => (ModLoader::Quilt, s.strip_prefix("quilt-").map(String::from)),
        _ => (ModLoader::Vanilla, None),
    };

    Some(ImportCandidate {
        launcher: LauncherKind::CurseForge,
        name,
        source_path: dir.to_string_lossy().into_owned(),
        minecraft_dir: dir.to_string_lossy().into_owned(),
        version_id: base_version,
        loader,
        loader_version,
        icon_base64: None,
    })
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_path)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &dst_path)?;
        }
        // Symlinks are intentionally skipped rather than followed, so a
        // stray link elsewhere on disk can't drag unrelated files in.
    }
    Ok(())
}

/// The folders/files that make up "everything a player would consider part
/// of their instance" - deliberately excludes `versions/`, `libraries/`, and
/// `assets/`, which Mint manages itself and re-downloads per version/account
/// as needed.
const CONTENT_DIRS: &[&str] = &["saves", "resourcepacks", "shaderpacks", "config", "mods", "screenshots"];
const CONTENT_FILES: &[&str] = &["options.txt", "optionsof.txt", "servers.dat"];

/// Creates a brand-new Mint instance from an `ImportCandidate` and copies
/// over its world saves, mods, resource/shader packs, config, options, and
/// server list. Runs under a freshly generated instance id, same as
/// `import_instance` - never touches or moves the original launcher's files.
pub fn import_external(instances_root: &Path, candidate: &ImportCandidate) -> anyhow::Result<Instance> {
    let content_dir = PathBuf::from(&candidate.minecraft_dir);
    if !content_dir.is_dir() {
        anyhow::bail!("The source folder is no longer there - it may have moved or been deleted");
    }

    let mut inst = instance::create_instance(
        instances_root,
        candidate.name.clone(),
        candidate.version_id.clone(),
        candidate.loader,
        candidate.loader_version.clone(),
    )?;
    let game_dir = inst.game_dir(instances_root);

    for dir_name in CONTENT_DIRS {
        let src = content_dir.join(dir_name);
        if src.is_dir() {
            copy_dir_recursive(&src, &game_dir.join(dir_name))?;
        }
    }
    for file_name in CONTENT_FILES {
        let src = content_dir.join(file_name);
        if src.is_file() {
            let _ = fs::copy(&src, game_dir.join(file_name));
        }
    }

    if let Some(icon_b64) = &candidate.icon_base64 {
        if let Ok(data) = STANDARD.decode(icon_b64) {
            if instance::sniff_image_mime(&data).is_some() {
                fs::write(inst.icon_path(instances_root), &data)?;
                inst.has_icon = true;
            }
        }
    }

    inst.save(instances_root)?;
    Ok(inst)
}
