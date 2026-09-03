use crate::importer::{self, ImportCandidate, SuggestedPath};
use crate::instance::Instance;
use crate::state::AppState;
use serde::Serialize;
use tauri::{Emitter, State};

#[tauri::command]
pub fn suggest_launcher_paths() -> Vec<SuggestedPath> {
    importer::suggest_paths()
}

#[tauri::command]
pub fn scan_external_launcher(path: String) -> Result<Vec<ImportCandidate>, String> {
    importer::scan(std::path::Path::new(&path)).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportProgress {
    name: String,
    current: u64,
    total: u64,
    message: String,
}

/// Copying a real launcher's worlds/mods can take a while (world saves in
/// particular can run into the gigabytes) - `async` plus `spawn_blocking`
/// keeps that file I/O off the main thread, so the window stays responsive
/// instead of the OS reporting Mint as "not responding" mid-import. Progress
/// is throttled to at most one event per ~80ms - a large world save can be
/// thousands of files, far more than is useful (or good for IPC traffic) to
/// report on individually.
#[tauri::command]
pub async fn import_external_instance(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    candidate: ImportCandidate,
) -> Result<Instance, String> {
    let instances_dir = state.instances_dir();
    let name = candidate.name.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut last_emit = std::time::Instant::now().checked_sub(std::time::Duration::from_secs(1));
        importer::import_external(&instances_dir, &candidate, |current, total, message| {
            let now = std::time::Instant::now();
            let due = last_emit.is_none_or(|t| now.duration_since(t).as_millis() >= 80);
            if due || current == total {
                last_emit = Some(now);
                let _ = app.emit(
                    "import-progress",
                    ImportProgress {
                        name: name.clone(),
                        current,
                        total,
                        message: message.to_string(),
                    },
                );
            }
        })
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}
