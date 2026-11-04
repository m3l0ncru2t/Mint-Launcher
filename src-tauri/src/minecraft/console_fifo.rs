//! A server console input that outlives the Mint process that started it
//! (Linux/macOS). A plain stdin pipe closes when Mint exits - after an
//! update, say - and the server's console reader stops at that EOF, leaving
//! only RCON, which vanilla credits as "Rcon" ("[Rcon] hi" instead of
//! "[Server] hi"). Instead the server's stdin is a named pipe (FIFO) that
//! the server itself also holds open for writing, so it never sees EOF, and
//! any later Mint process can reopen it by path and keep typing into the
//! real console.

use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

pub fn path(data_dir: &Path, instance_id: &str) -> PathBuf {
    data_dir.join("consoles").join(format!("{instance_id}.fifo"))
}

/// Makes a fresh FIFO at `path` and opens it read+write, to hand to the
/// server as its stdin.
pub fn create(path: &Path) -> std::io::Result<std::fs::File> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _ = std::fs::remove_file(path);
    let c_path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    if unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    std::fs::OpenOptions::new().read(true).write(true).open(path)
}

/// Writes one console line. Non-blocking: fails (rather than hanging) when
/// no server has the FIFO open any more, or its console isn't reading.
pub fn write_line(path: &Path, line: &str) -> std::io::Result<()> {
    let mut fifo = std::fs::OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)?;
    // One write, so lines from concurrent callers can't interleave.
    fifo.write_all(format!("{line}\n").as_bytes())
}

/// Removes the FIFO at `path` if it's still the one with inode `ino`: by the
/// time a server's exit is cleaned up, its relaunch may already have made a
/// new one there.
pub fn remove_if_same(path: &Path, ino: u64) {
    use std::os::unix::fs::MetadataExt;
    if std::fs::metadata(path).is_ok_and(|m| m.ino() == ino) {
        let _ = std::fs::remove_file(path);
    }
}
