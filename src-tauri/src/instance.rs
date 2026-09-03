use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

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
    #[serde(default)]
    pub servers: Vec<ServerEntry>,
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

    fn meta_path(&self, instances_root: &Path) -> PathBuf {
        self.dir(instances_root).join("instance.json")
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
        servers: Vec::new(),
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
