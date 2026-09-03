use crate::importer::{self, ImportCandidate, SuggestedPath};
use crate::instance::Instance;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub fn suggest_launcher_paths() -> Vec<SuggestedPath> {
    importer::suggest_paths()
}

#[tauri::command]
pub fn scan_external_launcher(path: String) -> Result<Vec<ImportCandidate>, String> {
    importer::scan(std::path::Path::new(&path)).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn import_external_instance(state: State<AppState>, candidate: ImportCandidate) -> Result<Instance, String> {
    importer::import_external(&state.instances_dir(), &candidate).map_err(|e| e.to_string())
}
