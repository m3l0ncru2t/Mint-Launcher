use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModLoader {
    Vanilla,
    Fabric,
    Forge,
    Quilt,
}

impl ModLoader {
    /// The loader tag Modrinth's API expects, or None for Vanilla (which
    /// has no loader tag - update checks fall back to game-version only).
    pub fn modrinth_loader(&self) -> Option<&'static str> {
        match self {
            ModLoader::Vanilla => None,
            ModLoader::Fabric => Some("fabric"),
            ModLoader::Forge => Some("forge"),
            ModLoader::Quilt => Some("quilt"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerEntry {
    pub name: String,
    pub address: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub version_id: String,
    pub loader: ModLoader,
    /// Loader version string (e.g. Fabric loader version). None for vanilla.
    pub loader_version: Option<String>,
    pub created_at: String,
    pub last_played: Option<String>,
    pub memory_mb: u32,
    /// Extra raw JVM arguments (space-separated), appended after `-Xmx`.
    #[serde(default)]
    pub java_args: Option<String>,
    /// A specific saved Microsoft account's id to always launch this
    /// instance as, regardless of whichever account is currently active -
    /// lets different instances run under different accounts. `None` falls
    /// back to whichever account is currently signed in.
    #[serde(default)]
    pub account_id: Option<String>,
    /// Whether a custom icon has been uploaded (see `icon_path`) - kept as
    /// its own flag rather than inferred from the file existing so the
    /// (much larger) icon bytes never need to be part of this struct or the
    /// `list_instances` payload.
    #[serde(default)]
    pub has_icon: bool,
}

impl Instance {
    pub fn dir(&self, instances_root: &Path) -> PathBuf {
        instances_root.join(&self.id)
    }

    pub fn game_dir(&self, instances_root: &Path) -> PathBuf {
        self.dir(instances_root).join("game")
    }

    pub fn natives_dir(&self, instances_root: &Path) -> PathBuf {
        self.dir(instances_root).join("natives")
    }

    pub fn mods_dir(&self, instances_root: &Path) -> PathBuf {
        self.game_dir(instances_root).join("mods")
    }

    pub fn resourcepacks_dir(&self, instances_root: &Path) -> PathBuf {
        self.game_dir(instances_root).join("resourcepacks")
    }

    fn meta_path(&self, instances_root: &Path) -> PathBuf {
        self.dir(instances_root).join("instance.json")
    }

    /// Uploaded custom icons are stored under a fixed, extension-less name -
    /// the actual format is sniffed from content when serving it back (see
    /// `sniff_image_mime`), so there's no mismatch to worry about between a
    /// file's extension and its real encoding.
    pub fn icon_path(&self, instances_root: &Path) -> PathBuf {
        self.dir(instances_root).join("icon")
    }

    pub fn save(&self, instances_root: &Path) -> std::io::Result<()> {
        fs::create_dir_all(self.dir(instances_root))?;
        fs::create_dir_all(self.game_dir(instances_root))?;
        let json = serde_json::to_string_pretty(self)?;
        fs::write(self.meta_path(instances_root), json)
    }
}

pub fn list_instances(instances_root: &Path) -> std::io::Result<Vec<Instance>> {
    let mut result = Vec::new();
    if !instances_root.exists() {
        return Ok(result);
    }
    for entry in fs::read_dir(instances_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let meta_path = entry.path().join("instance.json");
        if meta_path.exists() {
            let data = fs::read_to_string(meta_path)?;
            if let Ok(instance) = serde_json::from_str::<Instance>(&data) {
                result.push(instance);
            }
        }
    }
    result.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(result)
}

pub fn create_instance(
    instances_root: &Path,
    name: String,
    version_id: String,
    loader: ModLoader,
    loader_version: Option<String>,
) -> std::io::Result<Instance> {
    let instance = Instance {
        id: Uuid::new_v4().to_string(),
        name,
        version_id,
        loader,
        loader_version,
        created_at: chrono::Utc::now().to_rfc3339(),
        last_played: None,
        memory_mb: 2048,
        java_args: None,
        account_id: None,
        has_icon: false,
    };
    instance.save(instances_root)?;
    Ok(instance)
}

pub fn delete_instance(instances_root: &Path, id: &str) -> std::io::Result<()> {
    let dir = instances_root.join(id);
    if dir.exists() {
        fs::remove_dir_all(dir)?;
    }
    Ok(())
}

pub fn get_instance(instances_root: &Path, id: &str) -> std::io::Result<Option<Instance>> {
    let meta_path = instances_root.join(id).join("instance.json");
    if !meta_path.exists() {
        return Ok(None);
    }
    let data = fs::read_to_string(meta_path)?;
    Ok(serde_json::from_str(&data).ok())
}

pub fn touch_last_played(instances_root: &Path, id: &str) -> std::io::Result<()> {
    if let Some(mut instance) = get_instance(instances_root, id)? {
        instance.last_played = Some(chrono::Utc::now().to_rfc3339());
        instance.save(instances_root)?;
    }
    Ok(())
}

/// Packages an instance's world saves, mods, config, and icon into a single
/// portable .zip - a manual backup, and how instances move between machines.
/// `natives/` is deliberately left out: it's re-extracted from library jars
/// on every launch, so backing it up would only bloat the archive with
/// platform-specific junk.
pub fn export_instance(instances_root: &Path, id: &str, dest_zip: &Path) -> anyhow::Result<()> {
    let inst = get_instance(instances_root, id)?.ok_or_else(|| anyhow::anyhow!("Instance not found"))?;
    let dir = inst.dir(instances_root);

    let file = fs::File::create(dest_zip)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("instance.json", options)?;
    zip.write_all(&fs::read(dir.join("instance.json"))?)?;

    let icon_path = dir.join("icon");
    if icon_path.exists() {
        zip.start_file("icon", options)?;
        zip.write_all(&fs::read(&icon_path)?)?;
    }

    add_dir_to_zip(&mut zip, &dir.join("game"), "game", options)?;
    zip.finish()?;
    Ok(())
}

fn add_dir_to_zip(
    zip: &mut ZipWriter<fs::File>,
    dir: &Path,
    prefix: &str,
    options: SimpleFileOptions,
) -> anyhow::Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let zip_path = format!("{prefix}/{}", entry.file_name().to_string_lossy());
        if path.is_dir() {
            add_dir_to_zip(zip, &path, &zip_path, options)?;
        } else {
            zip.start_file(&zip_path, options)?;
            zip.write_all(&fs::read(&path)?)?;
        }
    }
    Ok(())
}

/// Rejects zip entries that would escape the extraction directory (a
/// maliciously crafted "backup" could otherwise write anywhere on disk via
/// `../` components - the classic zip-slip vulnerability).
fn is_safe_zip_entry(name: &str) -> bool {
    let path = Path::new(name);
    !path.is_absolute() && !path.components().any(|c| matches!(c, std::path::Component::ParentDir))
}

/// Recreates an instance from a file produced by `export_instance`, under a
/// freshly generated id so it can never collide with (or be confused for) an
/// existing instance - including the one it was originally exported from.
/// The account binding is dropped: a saved-account id is only meaningful on
/// the machine that saved it.
pub fn import_instance(instances_root: &Path, zip_path: &Path) -> anyhow::Result<Instance> {
    let file = fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    let mut meta_bytes = Vec::new();
    archive
        .by_name("instance.json")
        .map_err(|_| anyhow::anyhow!("Not a valid Mint Launcher backup - instance.json is missing"))?
        .read_to_end(&mut meta_bytes)?;
    let mut inst: Instance =
        serde_json::from_slice(&meta_bytes).map_err(|_| anyhow::anyhow!("Not a valid Mint Launcher backup"))?;

    let new_id = Uuid::new_v4().to_string();
    inst.id = new_id.clone();
    inst.account_id = None;
    inst.last_played = None;

    let dest_dir = instances_root.join(&new_id);
    fs::create_dir_all(dest_dir.join("game"))?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();
        if name.ends_with('/') || name == "instance.json" || !is_safe_zip_entry(&name) {
            continue;
        }
        let out_path = dest_dir.join(&name);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut out_file = fs::File::create(&out_path)?;
        std::io::copy(&mut entry, &mut out_file)?;
    }

    inst.save(instances_root)?;
    Ok(inst)
}

/// Identifies an uploaded icon's format from its content rather than trusting
/// a file extension or client-supplied MIME type, both for the data URL
/// served back to the frontend and to reject non-image uploads up front.
pub fn sniff_image_mime(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(&[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n']) {
        Some("image/png")
    } else if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}
