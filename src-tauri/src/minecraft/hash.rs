use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

pub fn sha1_hex(data: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(data);
    to_hex(&hasher.finalize())
}

/// (path, size, mtime) -> hash. Every Modrinth update check and every Browse
/// search page hashes the *whole* installed mod list to ask Modrinth what's
/// installed - and used to re-read every jar in full each time, so a big pack
/// meant hundreds of MB of disk reads per keystroke-driven search. A file's
/// hash can't change without its size or modified time changing too, so a
/// repeat lookup for an untouched jar is just a stat.
type HashCache = Mutex<HashMap<(PathBuf, u64, SystemTime), String>>;

fn cache() -> &'static HashCache {
    static CACHE: OnceLock<HashCache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Streams the file through the hasher in chunks (rather than reading it
/// whole into memory first - a fat modpack jar is easily 100MB+) and
/// remembers the result per (path, size, mtime).
pub fn sha1_hex_file(path: &Path) -> std::io::Result<String> {
    let meta = std::fs::metadata(path)?;
    let key = meta.modified().ok().map(|mtime| (path.to_path_buf(), meta.len(), mtime));
    if let Some(key) = &key {
        if let Some(hit) = cache().lock().ok().and_then(|c| c.get(key).cloned()) {
            return Ok(hit);
        }
    }

    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha1::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let hash = to_hex(&hasher.finalize());

    if let Some(key) = key {
        if let Ok(mut c) = cache().lock() {
            // A file replaced under the same path leaves its old entry
            // behind - drop those so the map tracks files, not history.
            c.retain(|(p, _, _), _| p != &key.0);
            c.insert(key, hash.clone());
        }
    }
    Ok(hash)
}
