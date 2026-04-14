use tauri::{Emitter, State};
use tokio::sync::mpsc;
use uuid::Uuid;

use talkiwi_core::clock::SessionClock;
use talkiwi_core::event::ActionEvent;
use talkiwi_core::output::IntentOutput;
use talkiwi_core::session::{SessionState, SpeakSegment};
use talkiwi_db::SessionRepo;

use crate::AppState;

#[tauri::command]
pub async fn session_start(state: State<'_, AppState>) -> Result<String, String> {
    let (speak_tx, mut speak_rx) = mpsc::channel::<SpeakSegment>(64);
    let (action_tx, mut action_rx) = mpsc::channel::<ActionEvent>(256);

    // Create ASR provider from config
    let asr_provider = crate::init_asr_provider_from_state(&state).map_err(|e| e.to_string())?;
    let input_gain_db = state
        .config
        .lock()
        .map_err(|e| format!("config lock poisoned: {e}"))?
        .asr
        .input_gain_db;

    // Pass output_dir for WAV recording
    let output_dir = Some(state.output_dir.clone());
    let clock = SessionClock::new();
    let selected_mic = state
        .audio_input_manager
        .resolve_selected_input()
        .map_err(|e| e.to_string())?;
    let preview_tx = state
        .widget_preview
        .start_session(clock.clone(), selected_mic)
        .await;

    let session_id = match state
        .session_manager
        .start(
            speak_tx,
            action_tx,
            Some(preview_tx),
            clock,
            asr_provider,
            output_dir,
            input_gain_db,
        )
        .await
    {
        Ok(session_id) => session_id,
        Err(error) => {
            state.widget_preview.reset().await;
            return Err(error.to_string());
        }
    };

    // Spawn forwarders for Tauri event emission
    let app_handle = state.app_handle.clone();
    tokio::spawn(async move {
        while let Some(segment) = speak_rx.recv().await {
            let _ = app_handle.emit("talkiwi://speak-segment", &segment);
        }
    });

    let app_handle2 = state.app_handle.clone();
    tokio::spawn(async move {
        while let Some(event) = action_rx.recv().await {
            let _ = app_handle2.emit("talkiwi://action-event", &event);
        }
    });

    Ok(session_id.to_string())
}

#[tauri::command]
pub async fn session_stop(state: State<'_, AppState>) -> Result<IntentOutput, String> {
    let output = state
        .session_manager
        .stop()
        .await
        .map_err(|e| e.to_string())?;

    // Emit output-ready event
    let _ = state.app_handle.emit("talkiwi://output-ready", &output);

    let session_id = output.session_id.to_string();
    let db = std::sync::Arc::clone(&state.db);
    let detail = tokio::task::spawn_blocking(move || {
        let db = db.lock().map_err(|e| e.to_string())?;
        let repo = SessionRepo::new(&db);
        repo.get_session_detail(&session_id)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;

    let _ = state.app_handle.emit("talkiwi://session-complete", &detail);

    Ok(output)
}

#[tauri::command]
pub async fn session_state(state: State<'_, AppState>) -> Result<SessionState, String> {
    Ok(state.session_manager.state().await)
}

#[tauri::command]
pub async fn session_regenerate(
    state: State<'_, AppState>,
    session_id: String,
    segments: Vec<SpeakSegment>,
    events: Vec<ActionEvent>,
) -> Result<IntentOutput, String> {
    let uuid = Uuid::parse_str(&session_id).map_err(|e| e.to_string())?;

    state
        .session_manager
        .regenerate(&segments, &events, uuid)
        .await
        .map_err(|e| e.to_string())
}
