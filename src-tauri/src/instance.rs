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
    /// The instance folder's name on disk - a sanitized, deduplicated form
    /// of `name` chosen once at creation (see `unique_dir_name`), so the
    /// instances folder shows real names instead of raw ids. Deliberately
    /// not kept in sync with later renames (see `Instance::dir`): an
    /// in-progress game has this as its working directory, and racing a
    /// rename against a running JVM is worse than a stale folder name.
    /// `#[serde(default)]` makes this empty for instances saved before this
    /// field existed - `dir()` treats that as "folder is named after `id`",
    /// which is exactly how those old instances were actually laid out.
    #[serde(default)]
    pub dir_name: String,
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
    /// Manual sort position for the sidebar list. Instances that have never
    /// been dragged all default to 0 (via `serde(default)`) and fall back to
    /// sorting by name among themselves; `reorder_instances` assigns 0..n
    /// sequential values on an explicit drag-and-drop reorder. New instances
    /// get a millisecond timestamp (see `create_instance`) so they land after
    /// any explicitly-ordered ones instead of jumping to the top.
    #[serde(default)]
    pub sort_order: i64,
}

impl Instance {
    pub fn dir(&self, instances_root: &Path) -> PathBuf {
        if self.dir_name.is_empty() {
            instances_root.join(&self.id)
        } else {
            instances_root.join(&self.dir_name)
        }
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
    result.sort_by(|a, b| {
        a.sort_order
            .cmp(&b.sort_order)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(result)
}

/// Applies a manual sidebar order: `ordered_ids` should list every instance's
/// id in the desired order (extra/missing ids are simply ignored/left alone).
/// Assigns sequential `sort_order` values so a freshly created instance
/// (which gets a large millisecond-timestamp `sort_order`, see
/// `create_instance`) naturally lands after all of these instead of jumping
/// to the top.
pub fn reorder_instances(instances_root: &Path, ordered_ids: &[String]) -> std::io::Result<Vec<Instance>> {
    for (i, id) in ordered_ids.iter().enumerate() {
        if let Some(mut instance) = get_instance(instances_root, id)? {
            instance.sort_order = i as i64;
            instance.save(instances_root)?;
        }
    }
    list_instances(instances_root)
}

/// Strips characters that aren't safe in a folder name on Windows/macOS/
/// Linux, trims trailing dots/spaces (Windows rejects those), and avoids
/// Windows' reserved device names - so any instance name, how ever exotic,
/// produces a folder Explorer/Finder/the shell can actually create.
fn sanitize_dir_name(name: &str) -> String {
    let mut cleaned: String = name
        .trim()
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    while matches!(cleaned.chars().last(), Some('.') | Some(' ')) {
        cleaned.pop();
    }
    cleaned.truncate(64);
    let cleaned = cleaned.trim().to_string();

    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if cleaned.is_empty() || RESERVED.contains(&cleaned.to_uppercase().as_str()) {
        return "instance".to_string();
    }
    cleaned
}

/// Appends " (2)", " (3)", etc. until the folder name doesn't collide with
/// an existing instance directory - the same convention Windows/macOS use
/// for duplicate file names.
fn unique_dir_name(instances_root: &Path, name: &str) -> String {
    let base = sanitize_dir_name(name);
    let mut candidate = base.clone();
    let mut n = 2;
    while instances_root.join(&candidate).exists() {
        candidate = format!("{base} ({n})");
        n += 1;
    }
    candidate
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
        dir_name: unique_dir_name(instances_root, &name),
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
        sort_order: chrono::Utc::now().timestamp_millis(),
    };
    instance.save(instances_root)?;

    // Best-effort - a brand-new instance getting a seeded server list matters
    // far less than instance creation itself succeeding.
    let _ = crate::minecraft::servers_dat::write_servers(
        &instance.game_dir(instances_root),
        &[ServerEntry {
            name: "MintyMC".to_string(),
            address: "mintymc.xyz".to_string(),
        }],
    );

    Ok(instance)
}

pub fn delete_instance(instances_root: &Path, id: &str) -> std::io::Result<()> {
    if let Some(inst) = get_instance(instances_root, id)? {
        let dir = inst.dir(instances_root);
        if dir.exists() {
            fs::remove_dir_all(dir)?;
        }
    }
    Ok(())
}

/// Instance folders are named after `dir_name` (see `unique_dir_name`), not
/// `id`, so finding one by id means checking every folder's `instance.json`
/// rather than joining the id straight onto a path - same approach as
/// `list_instances`, which this delegates to.
pub fn get_instance(instances_root: &Path, id: &str) -> std::io::Result<Option<Instance>> {
    Ok(list_instances(instances_root)?.into_iter().find(|i| i.id == id))
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

    inst.id = Uuid::new_v4().to_string();
    inst.dir_name = unique_dir_name(instances_root, &inst.name);
    inst.account_id = None;
    inst.last_played = None;
    inst.sort_order = chrono::Utc::now().timestamp_millis();

    let dest_dir = inst.dir(instances_root);
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
