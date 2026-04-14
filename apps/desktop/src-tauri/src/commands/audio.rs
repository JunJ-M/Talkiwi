use tauri::State;

use talkiwi_core::preview::AudioInputInfo;
use talkiwi_core::session::SessionState;

use crate::AppState;

fn can_change_microphone(state: &SessionState) -> bool {
    matches!(state, SessionState::Idle | SessionState::Ready)
}

#[tauri::command]
pub async fn audio_list_inputs(state: State<'_, AppState>) -> Result<Vec<AudioInputInfo>, String> {
    state
        .audio_input_manager
        .list_inputs()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn audio_get_selected_input(
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    Ok(state
        .audio_input_manager
        .resolve_selected_input()
        .map_err(|e| e.to_string())?
        .map(|input| input.id))
}

#[tauri::command]
pub async fn audio_set_selected_input(
    state: State<'_, AppState>,
    id_or_name: String,
) -> Result<(), String> {
    if !can_change_microphone(&state.session_manager.state().await) {
        return Err("microphone can only be changed while session is idle or ready".to_string());
    }

    let selected = state
        .audio_input_manager
        .set_selected_input(&id_or_name)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "selected microphone not found".to_string())?;

    let updated_config = {
        let mut guard = state
            .config
            .lock()
            .map_err(|e| format!("config lock poisoned: {e}"))?;
        guard.audio.input_device_id = Some(selected.id.clone());
        guard.audio.input_device_name = Some(selected.name.clone());
        guard.clone()
    };

    let serialized = serde_json::to_string_pretty(&updated_config).map_err(|e| e.to_string())?;
    let tmp_path = state.config_path.with_extension("json.tmp");
    tokio::fs::write(&tmp_path, serialized)
        .await
        .map_err(|e| e.to_string())?;
    tokio::fs::rename(&tmp_path, &state.config_path)
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_change_microphone_only_when_idle_or_ready() {
        assert!(can_change_microphone(&SessionState::Idle));
        assert!(can_change_microphone(&SessionState::Ready));
        assert!(!can_change_microphone(&SessionState::Recording));
        assert!(!can_change_microphone(&SessionState::Processing));
        assert!(!can_change_microphone(&SessionState::Error(
            "x".to_string()
        )));
    }
}
