use super::manifest::{ArgumentEntry, ArgumentValue, VersionDetail};
use super::rules::rules_allow;
use crate::auth::GameProfile;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

pub struct LaunchContext<'a> {
    pub detail: &'a VersionDetail,
    pub classpath: &'a [PathBuf],
    pub natives_dir: &'a Path,
    pub game_dir: &'a Path,
    pub assets_dir: &'a Path,
    pub profile: &'a GameProfile,
    /// `host:port` to auto-connect to via Quick Play on versions that
    /// support it (1.20+); a harmless no-op on older versions, since their
    /// argument templates simply don't declare the quick-play flag at all.
    pub quick_play_server: Option<&'a str>,
}

fn classpath_separator() -> &'static str {
    if cfg!(target_os = "windows") {
        ";"
    } else {
        ":"
    }
}

fn build_substitutions(ctx: &LaunchContext, classpath_str: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    map.insert("auth_player_name".into(), ctx.profile.username.clone());
    map.insert("version_name".into(), ctx.detail.id.clone());
    map.insert("game_directory".into(), ctx.game_dir.to_string_lossy().into_owned());
    map.insert("assets_root".into(), ctx.assets_dir.to_string_lossy().into_owned());
    map.insert("assets_index_name".into(), ctx.detail.assets.clone());
    map.insert("auth_uuid".into(), ctx.profile.uuid.clone());
    map.insert("auth_access_token".into(), ctx.profile.access_token.clone());
    map.insert("clientid".into(), "mint-launcher".into());
    map.insert("auth_xuid".into(), "0".into());
    map.insert("user_type".into(), ctx.profile.user_type.clone());
    map.insert("version_type".into(), ctx.detail.version_type.clone());
    map.insert(
        "natives_directory".into(),
        ctx.natives_dir.to_string_lossy().into_owned(),
    );
    map.insert("launcher_name".into(), "Mint Launcher".into());
    map.insert("launcher_version".into(), env!("CARGO_PKG_VERSION").into());
    map.insert("classpath".into(), classpath_str.to_string());
    map.insert(
        "quickPlayMultiplayer".into(),
        ctx.quick_play_server.unwrap_or("").to_string(),
    );
    map
}

fn substitute(token: &str, subs: &HashMap<String, String>) -> String {
    let mut result = token.to_string();
    for (key, value) in subs {
        result = result.replace(&format!("${{{key}}}"), value);
    }
    result
}

fn active_features(quick_play_multiplayer: bool) -> HashMap<String, bool> {
    let mut features = HashMap::new();
    for key in [
        "is_demo_user",
        "has_custom_resolution",
        "has_quick_plays_support",
        "is_quick_play_singleplayer",
        "is_quick_play_multiplayer",
        "is_quick_play_realms",
    ] {
        features.insert(key.to_string(), false);
    }
    features.insert("is_quick_play_multiplayer".to_string(), quick_play_multiplayer);
    features
}

fn expand_argument_entries(
    entries: &[ArgumentEntry],
    subs: &HashMap<String, String>,
    features: &HashMap<String, bool>,
    out: &mut Vec<String>,
) {
    for entry in entries {
        match entry {
            ArgumentEntry::Plain(s) => out.push(substitute(s, subs)),
            ArgumentEntry::Conditional { rules, value } => {
                if rules_allow(Some(rules.as_slice()), features) {
                    match value {
                        ArgumentValue::Single(s) => out.push(substitute(s, subs)),
                        ArgumentValue::Multiple(list) => {
                            for s in list {
                                out.push(substitute(s, subs));
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Builds the full `java` argument list (JVM args, main class, game args) -
/// everything after the executable and the `-Xmx` flag.
pub fn build_command_args(ctx: &LaunchContext) -> Vec<String> {
    let classpath_str = {
        let parts: Vec<String> = ctx
            .classpath
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        parts.join(classpath_separator())
    };
    let subs = build_substitutions(ctx, &classpath_str);
    let features = active_features(ctx.quick_play_server.is_some());

    let mut args = Vec::new();

    match &ctx.detail.arguments {
        Some(arguments) => {
            expand_argument_entries(&arguments.jvm, &subs, &features, &mut args);
            args.push(ctx.detail.main_class.clone());
            expand_argument_entries(&arguments.game, &subs, &features, &mut args);
        }
        None => {
            // Legacy (<1.13) versions only ship a flat `minecraftArguments`
            // string and expect the launcher to supply sensible JVM flags.
            args.push(format!(
                "-Djava.library.path={}",
                ctx.natives_dir.to_string_lossy()
            ));
            args.push("-cp".to_string());
            args.push(classpath_str);
            args.push(ctx.detail.main_class.clone());
            if let Some(legacy) = &ctx.detail.minecraft_arguments {
                // Split before substituting so a value with spaces (e.g. a
                // game directory path) stays one argument.
                for token in legacy.split_whitespace() {
                    args.push(substitute(token, &subs));
                }
            }
        }
    }

    args
}

fn java_binary() -> &'static str {
    if cfg!(target_os = "windows") {
        "javaw"
    } else {
        "java"
    }
}

/// Spawns the Java process and streams stdout/stderr as `instance-log`
/// events until it exits, returning the process exit code.
pub async fn spawn_and_stream(
    app: &tauri::AppHandle,
    instance_id: &str,
    memory_mb: u32,
    extra_jvm_args: &[String],
    args: Vec<String>,
    cwd: &Path,
) -> anyhow::Result<i32> {
    let mut cmd = Command::new(java_binary());
    cmd.arg(format!("-Xmx{memory_mb}M"));
    cmd.args(extra_jvm_args);
    cmd.args(&args);
    cmd.current_dir(cwd);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| {
        anyhow::anyhow!(
            "failed to start Java ({}): {e}. Is a JDK installed and on PATH?",
            java_binary()
        )
    })?;

    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");

    let app_out = app.clone();
    let instance_out = instance_id.to_string();
    let out_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = app_out.emit(
                "instance-log",
                serde_json::json!({ "instanceId": instance_out, "line": line }),
            );
        }
    });

    let app_err = app.clone();
    let instance_err = instance_id.to_string();
    let err_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = app_err.emit(
                "instance-log",
                serde_json::json!({ "instanceId": instance_err, "line": line }),
            );
        }
    });

    let status = child.wait().await?;
    let _ = out_task.await;
    let _ = err_task.await;

    Ok(status.code().unwrap_or(-1))
}
