use sha1::{Digest, Sha1};

pub fn sha1_hex(data: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(data);
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha1_hex_file(path: &std::path::Path) -> std::io::Result<String> {
    Ok(sha1_hex(&std::fs::read(path)?))
}
