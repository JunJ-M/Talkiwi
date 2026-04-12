use std::sync::Arc;

use tauri::State;

use talkiwi_core::session::SessionSummary;
use talkiwi_db::{SessionDetail, SessionRepo};

use crate::AppState;

#[tauri::command]
pub async fn history_list(
    state: State<'_, AppState>,
    limit: Option<usize>,
    offset: Option<usize>,
) -> Result<Vec<SessionSummary>, String> {
    let db = Arc::clone(&state.db);
    let limit = limit.unwrap_or(20);
    let offset = offset.unwrap_or(0);

    tokio::task::spawn_blocking(move || {
        let db = db.lock().map_err(|e| e.to_string())?;
        let repo = SessionRepo::new(&db);
        repo.list_sessions(limit, offset).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn history_detail(
    state: State<'_, AppState>,
    id: String,
) -> Result<SessionDetail, String> {
    let db = Arc::clone(&state.db);

    tokio::task::spawn_blocking(move || {
        let db = db.lock().map_err(|e| e.to_string())?;
        let repo = SessionRepo::new(&db);
        repo.get_session_detail(&id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
