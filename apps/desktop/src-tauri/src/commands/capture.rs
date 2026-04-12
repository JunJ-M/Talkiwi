use tauri::State;
use uuid::Uuid;

use talkiwi_core::event::{ActionEvent, ActionPayload, ActionType};

use crate::AppState;

#[tauri::command]
pub async fn capture_screenshot(state: State<'_, AppState>) -> Result<ActionEvent, String> {
    let sm = &state.session_manager;
    let session_id = sm.current_session_id().await.ok_or("No active session")?;
    let offset_ms = sm.elapsed_ms().await;

    let session_dir = state.output_dir.join(session_id.to_string());
    std::fs::create_dir_all(&session_dir).map_err(|e| e.to_string())?;

    let screenshot_path = session_dir.join(format!("screenshot-{}.png", offset_ms));

    // Run capture in spawn_blocking — xcap and image::save are CPU+IO bound
    let path = screenshot_path.clone();
    let (width, height) = tokio::task::spawn_blocking(move || -> Result<(u32, u32), String> {
        let screens = xcap::Monitor::all().map_err(|e| e.to_string())?;
        let screen = screens.first().ok_or("No monitors found")?;
        let image = screen.capture_image().map_err(|e| e.to_string())?;
        let dims = (image.width(), image.height());
        image.save(&path).map_err(|e| e.to_string())?;
        Ok(dims)
    })
    .await
    .map_err(|e| e.to_string())??;

    let event = ActionEvent {
        id: Uuid::new_v4(),
        session_id,
        timestamp: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        session_offset_ms: offset_ms,
        duration_ms: None,
        action_type: ActionType::Screenshot,
        plugin_id: "builtin".to_string(),
        payload: ActionPayload::Screenshot {
            image_path: screenshot_path.to_string_lossy().to_string(),
            width,
            height,
            ocr_text: None,
        },
        semantic_hint: Some("user took a screenshot".to_string()),
        confidence: 1.0,
    };

    // Inject into ActionTrack
    sm.inject_event(event.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(event)
}
