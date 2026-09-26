//! Turns a finished game/server process into a message that says what
//! actually happened, instead of a bare "exited with code N": a crash report
//! the game wrote during this run beats anything else (it names the cause),
//! then a JVM native crash log, then what the exit code itself means on
//! this OS. Every non-zero message still contains "exit code N", which the
//! frontend reads back to decide whether to treat it as a crash.

use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::time::SystemTime;

/// The process's exit code, or `128 + signal` when it was killed by a
/// signal on unix (the shell's convention) instead of `-1` for everything.
pub fn exit_code(status: &ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    -1
}

/// Newest file in `dir` whose name matches `pred`, written at or after `since`.
fn newest_since(dir: &Path, since: SystemTime, pred: impl Fn(&str) -> bool) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| pred(&e.file_name().to_string_lossy()))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .filter(|(modified, _)| *modified >= since)
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
}

/// "Description: ..." plus the first exception line from a crash report.
fn crash_report_summary(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    let description = lines.find_map(|l| l.strip_prefix("Description:"))?.trim().to_string();
    let exception = lines.map(str::trim).find(|l| !l.is_empty()).map(|l| {
        let l = l.strip_prefix("java.lang.").unwrap_or(l);
        if l.chars().count() > 160 { format!("{}…", l.chars().take(160).collect::<String>()) } else { l.to_string() }
    });
    Some(match exception {
        Some(e) => format!("{description} - {e}"),
        None => description,
    })
}

fn code_meaning(code: i32) -> Option<&'static str> {
    Some(match code {
        1 => "it hit an error it couldn't recover from - the console shows the cause",
        -1 => "it stopped without reporting an exit code",
        #[cfg(unix)]
        130 => "it was interrupted (Ctrl+C)",
        #[cfg(unix)]
        134 => "Java aborted - usually a native crash (graphics driver or a mod with native code)",
        #[cfg(unix)]
        137 => "it was killed - either force-stopped, or the system ran out of memory and killed it",
        #[cfg(unix)]
        139 => "it crashed in native code (segfault) - often a graphics driver or a mod with native code",
        #[cfg(unix)]
        143 => "it was terminated by the system",
        #[cfg(windows)]
        -1073741819 => "it crashed in native code (access violation) - often a graphics driver or a mod with native code",
        #[cfg(windows)]
        -1073740791 => "it crashed in native code (stack buffer overrun)",
        #[cfg(windows)]
        -1073741571 => "it ran out of stack space (stack overflow)",
        #[cfg(windows)]
        -1073741515 => "a required DLL is missing",
        #[cfg(windows)]
        -1073741510 => "its console window was closed or it was interrupted (Ctrl+C)",
        #[cfg(windows)]
        -805306369 => "it stopped responding and was closed",
        _ => return None,
    })
}

fn code_label(code: i32) -> String {
    // Windows NTSTATUS crash codes are negative as i32 and only recognizable
    // in hex (0xC0000005 etc).
    if cfg!(windows) && code < -1 {
        format!("exit code {code} / 0x{:08X}", code as u32)
    } else {
        format!("exit code {code}")
    }
}

/// `what` is "Minecraft" or "Server"; `since` is when the process was started.
pub fn describe(what: &str, code: i32, game_dir: &Path, since: SystemTime) -> String {
    if code == 0 {
        return if what == "Server" { "Server stopped".to_string() } else { "Minecraft closed".to_string() };
    }
    let label = code_label(code);
    if let Some(report) = newest_since(&game_dir.join("crash-reports"), since, |n| n.ends_with(".txt")) {
        let name = report.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        return match crash_report_summary(&report) {
            Some(summary) => format!("{what} crashed: {summary} (crash report {name}, {label})"),
            None => format!("{what} crashed - see crash-reports/{name} ({label})"),
        };
    }
    if let Some(log) = newest_since(game_dir, since, |n| n.starts_with("hs_err_pid") && n.ends_with(".log")) {
        let name = log.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        return format!(
            "{what} crashed inside Java itself (native crash, details in {name}) - often a graphics driver or a mod with native code ({label})"
        );
    }
    match code_meaning(code) {
        Some(meaning) => format!("{what} stopped unexpectedly: {meaning} ({label})"),
        None => format!("{what} stopped unexpectedly ({label}) - the console shows what happened before it closed"),
    }
}
