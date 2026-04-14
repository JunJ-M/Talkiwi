use tauri::State;

use talkiwi_db::{QualityOverview, SessionRepo};

use crate::AppState;

#[tauri::command]
pub async fn telemetry_quality_overview(
    state: State<'_, AppState>,
) -> Result<QualityOverview, String> {
    let db = std::sync::Arc::clone(&state.db);
    tokio::task::spawn_blocking(move || {
        let db = db.lock().map_err(|e| e.to_string())?;
        let repo = SessionRepo::new(&db);
        repo.quality_overview(20).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
