//! Keeps an instance's `servers.dat` in sync with Mint's own server list UI,
//! in both directions: a server added in-game (via the vanilla multiplayer
//! screen) shows up in Mint, and one added in Mint shows up in-game - since
//! both read and write the very same file, there's no separate copy of the
//! list to fall out of sync.

use super::nbt::Nbt;
use crate::instance::ServerEntry;
use std::collections::HashMap;
use std::path::Path;

fn read_root(path: &Path) -> Nbt {
    std::fs::read(path)
        .ok()
        .and_then(|data| super::nbt::parse(&data).ok())
        .unwrap_or_else(|| Nbt::Compound(HashMap::new()))
}

pub fn read_servers(game_dir: &Path) -> Vec<ServerEntry> {
    let root = read_root(&game_dir.join("servers.dat"));
    let Nbt::Compound(map) = &root else {
        return Vec::new();
    };
    let Some(Nbt::List(list)) = map.get("servers") else {
        return Vec::new();
    };
    list.iter()
        .filter_map(|entry| {
            let Nbt::Compound(c) = entry else {
                return None;
            };
            let address = match c.get("ip") {
                Some(Nbt::String(s)) => s.clone(),
                _ => return None,
            };
            let name = match c.get("name") {
                Some(Nbt::String(s)) if !s.trim().is_empty() => s.clone(),
                _ => address.clone(),
            };
            Some(ServerEntry { name, address })
        })
        .collect()
}

/// Writes `servers` back to `servers.dat`. Entries are matched against
/// whatever was already on disk by address, so per-server extras the game
/// itself stores (icon, the "accept resource packs" prompt state, ...) carry
/// over instead of being silently dropped just because Mint doesn't know
/// about them.
pub fn write_servers(game_dir: &Path, servers: &[ServerEntry]) -> std::io::Result<()> {
    std::fs::create_dir_all(game_dir)?;
    let path = game_dir.join("servers.dat");
    let mut root = read_root(&path);
    let Nbt::Compound(root_map) = &mut root else {
        unreachable!("read_root always returns a Compound");
    };

    let old_list = match root_map.get("servers") {
        Some(Nbt::List(list)) => list.clone(),
        _ => Vec::new(),
    };

    let new_list: Vec<Nbt> = servers
        .iter()
        .map(|entry| {
            let existing = old_list.iter().find(|old| {
                matches!(old, Nbt::Compound(c) if matches!(c.get("ip"), Some(Nbt::String(s)) if s == &entry.address))
            });
            let mut fields = match existing {
                Some(Nbt::Compound(c)) => c.clone(),
                _ => HashMap::new(),
            };
            fields.insert("name".to_string(), Nbt::String(entry.name.clone()));
            fields.insert("ip".to_string(), Nbt::String(entry.address.clone()));
            Nbt::Compound(fields)
        })
        .collect();

    root_map.insert("servers".to_string(), Nbt::List(new_list));
    std::fs::write(path, super::nbt::write_root(&root))
}
