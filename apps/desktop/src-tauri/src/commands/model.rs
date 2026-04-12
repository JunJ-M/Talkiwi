//! Model management commands — check status and download Whisper models.

use serde::Serialize;
use tauri::State;

use crate::AppState;

/// Response for the model_status command.
#[derive(Debug, Clone, Serialize)]
pub struct ModelStatusResponse {
    pub exists: bool,
    pub path: String,
    pub size_name: String,
    pub file_size_bytes: u64,
    pub download_url: String,
    pub expected_size_display: String,
}

/// Check the status of the configured Whisper model file.
#[tauri::command]
pub async fn model_status(state: State<'_, AppState>) -> Result<ModelStatusResponse, String> {
    let config = state
        .config
        .lock()
        .map_err(|e| format!("config lock poisoned: {e}"))?
        .clone();

    let size_name = config.asr.whisper_model_size.as_deref().unwrap_or("small");

    let status = talkiwi_asr::check_model_status(
        config.asr.whisper_model_path.as_deref(),
        size_name,
        &state.data_dir,
    );

    let download_url = talkiwi_asr::model_manager::model_download_url(size_name);

    let expected_size_display = talkiwi_asr::ModelSize::parse(size_name)
        .map(|s| s.approx_size_display())
        .unwrap_or("unknown")
        .to_string();

    Ok(ModelStatusResponse {
        exists: status.exists,
        path: status.path.to_string_lossy().to_string(),
        size_name: status.size_name,
        file_size_bytes: status.file_size_bytes,
        download_url,
        expected_size_display,
    })
}

/// Download the configured Whisper model.
///
/// This is a long-running operation. Progress is logged via tracing.
/// For V1, this blocks until complete. V1.5 can add Tauri Channel progress.
#[cfg(feature = "download")]
#[tauri::command]
pub async fn model_download(state: State<'_, AppState>) -> Result<String, String> {
    let config = state
        .config
        .lock()
        .map_err(|e| format!("config lock poisoned: {e}"))?
        .clone();

    let size_name = config.asr.whisper_model_size.as_deref().unwrap_or("small");

    let dest = talkiwi_asr::resolve_model_path(
        config.asr.whisper_model_path.as_deref(),
        size_name,
        &state.data_dir,
    );

    let url = talkiwi_asr::model_manager::model_download_url(size_name);

    talkiwi_asr::model_manager::download_model(&url, &dest, None)
        .await
        .map_err(|e| format!("download failed: {e}"))?;

    Ok(dest.to_string_lossy().to_string())
}
