use crate::state::AppState;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tauri::Emitter;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;

fn classpath_separator() -> &'static str {
    if cfg!(target_os = "windows") {
        ";"
    } else {
        ":"
    }
}

/// How to actually invoke the JVM for a dedicated server - the two loaders
/// Mint supports need genuinely different invocations, not just a different
/// classpath. A Vanilla server jar has always been a fully self-contained
/// executable (both the old fat-jar format and the newer "bundler" format
/// resolve their own entry point from their own manifest), so it's run via
/// plain `-jar`; passing it a `-cp` and an explicit main class - in
/// particular the *client's* main class, `net.minecraft.client.main.Main`,
/// which is what a naive reuse of the client launch path produces - fails
/// with `ClassNotFoundException` (the server jar has no client classes at
/// all). Fabric's `KnotServer`, on the other hand, isn't a fat jar and needs
/// an explicit classpath (the vanilla server jar plus Fabric's own
/// loader/intermediary libraries, see `fabric::apply_server_loader`) and
/// main class.
pub enum ServerJvmTarget {
    Jar(PathBuf),
    Classpath { classpath: Vec<PathBuf>, main_class: String },
}

/// Spawns a dedicated server process and streams its stdout/stderr as the
/// same `instance-log`/`instance-running-changed`/`launch-progress` events
/// `minecraft::launch::spawn_and_stream` emits for a client (both are
/// already generic over instance id, so the frontend console needs no
/// server-specific handling). The one real difference: stdin is piped and
/// its handle kept open in `AppState.instance_stdins` rather than left at
/// its default, so console commands - including a graceful `stop` (see
/// `commands::launch::stop_instance`) - can be written to the running
/// process later.
pub async fn spawn_and_stream_server(
    app: &tauri::AppHandle,
    state: &AppState,
    instance_id: &str,
    java_path: &Path,
    memory_mb: u32,
    extra_jvm_args: &[String],
    target: ServerJvmTarget,
    cwd: &Path,
) -> anyhow::Result<i32> {
    let mut cmd = Command::new(java_path);
    cmd.arg(format!("-Xmx{memory_mb}M"));
    cmd.args(extra_jvm_args);
    match &target {
        ServerJvmTarget::Jar(jar) => {
            cmd.arg("-jar").arg(jar);
        }
        ServerJvmTarget::Classpath { classpath, main_class } => {
            let classpath_str = classpath
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join(classpath_separator());
            cmd.arg("-cp").arg(classpath_str);
            cmd.arg(main_class);
        }
    }
    cmd.arg("nogui");
    cmd.current_dir(cwd);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to start Java ({}): {e}", java_path.display()))?;

    if let Some(stdin) = child.stdin.take() {
        state.instance_stdins.lock().await.insert(instance_id.to_string(), Arc::new(Mutex::new(stdin)));
    }

    if let Some(pid) = child.id() {
        state.running_instances.lock().await.insert(
            instance_id.to_string(),
            crate::state::RunningInstance { pid, account_uuid: None, account_username: None },
        );
        state.persist_running_instances().await;
        let _ = app.emit(
            "instance-running-changed",
            serde_json::json!({ "instanceId": instance_id, "running": true, "pid": pid }),
        );
        // The "launching" stage (emitted just before this in `do_launch_server`)
        // only covers the JVM's own startup time - without this, the
        // instance detail panel's activity card would otherwise be stuck
        // showing "Starting server" for as long as the server stays open.
        let _ = app.emit(
            "launch-progress",
            super::download::DownloadProgress {
                instance_id: instance_id.to_string(),
                stage: "running".to_string(),
                message: "Server is running".to_string(),
                current: 1,
                total: 1,
            },
        );
    }

    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");

    let app_out = app.clone();
    let instance_out = instance_id.to_string();
    let out_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = app_out.emit("instance-log", serde_json::json!({ "instanceId": instance_out, "line": line }));
        }
    });

    let app_err = app.clone();
    let instance_err = instance_id.to_string();
    let err_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let _ = app_err.emit("instance-log", serde_json::json!({ "instanceId": instance_err, "line": line }));
        }
    });

    let status = child.wait().await?;
    let _ = out_task.await;
    let _ = err_task.await;
    state.running_instances.lock().await.remove(instance_id);
    state.instance_stdins.lock().await.remove(instance_id);
    state.persist_running_instances().await;
    let _ = app.emit(
        "instance-running-changed",
        serde_json::json!({ "instanceId": instance_id, "running": false }),
    );

    Ok(status.code().unwrap_or(-1))
}
