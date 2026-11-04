//! Shader packs, as Iris runs them: packs live in `shaderpacks/` (zips or
//! unzipped folders), and only one is active at a time - `shaderPack=` plus
//! `enableShaders=` in `config/iris.properties`, a Java properties file.
//! Iris reads that at startup and writes it back when the game closes, so a
//! change made while the game is running is overwritten.

use std::path::{Path, PathBuf};

fn properties_path(game_dir: &Path) -> PathBuf {
    game_dir.join("config").join("iris.properties")
}

/// Undoes Java properties escaping (`\:`, `\=`, `\uXXXX`, ...).
fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('t') => out.push('\t'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('f') => out.push('\x0c'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    out.push(ch);
                }
            }
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

/// Escapes a value the way `java.util.Properties.store` does.
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for (i, c) in value.chars().enumerate() {
        match c {
            '\\' | '=' | ':' | '#' | '!' => {
                out.push('\\');
                out.push(c);
            }
            ' ' if i == 0 => out.push_str("\\ "),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if !c.is_ascii() || c.is_ascii_control() => {
                let mut buf = [0u16; 2];
                for unit in c.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{unit:04X}"));
                }
            }
            c => out.push(c),
        }
    }
    out
}

fn read_lines(game_dir: &Path) -> Vec<String> {
    std::fs::read_to_string(properties_path(game_dir))
        .map(|s| s.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

fn value_of(lines: &[String], key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    lines.iter().find_map(|l| l.trim_start().strip_prefix(&prefix).map(unescape))
}

/// The pack Iris will load, or `None` when shaders are off.
pub fn active_shaderpack(game_dir: &Path) -> Option<String> {
    let lines = read_lines(game_dir);
    if value_of(&lines, "enableShaders").is_some_and(|v| v.trim() == "false") {
        return None;
    }
    value_of(&lines, "shaderPack").filter(|p| !p.trim().is_empty())
}

/// Makes `pack` the active shader pack (`None` turns shaders off), keeping
/// every other Iris setting as it was.
pub fn set_active_shaderpack(game_dir: &Path, pack: Option<&str>) -> std::io::Result<()> {
    let path = properties_path(game_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut lines = read_lines(game_dir);
    let mut set = |key: &str, value: String| {
        let line = format!("{key}={value}");
        match lines.iter().position(|l| l.trim_start().starts_with(&format!("{key}="))) {
            Some(i) => lines[i] = line,
            None => lines.push(line),
        }
    };
    match pack {
        Some(pack) => {
            set("shaderPack", escape(pack));
            set("enableShaders", "true".to_string());
        }
        None => set("enableShaders", "false".to_string()),
    }
    std::fs::write(path, lines.join("\n") + "\n")
}

/// Whether Iris is in the mods folder (and not disabled).
pub fn iris_installed(mods_dir: &Path) -> bool {
    std::fs::read_dir(mods_dir)
        .map(|entries| {
            entries.flatten().any(|e| {
                let name = e.file_name().to_string_lossy().to_lowercase();
                name.starts_with("iris") && name.ends_with(".jar")
            })
        })
        .unwrap_or(false)
}
