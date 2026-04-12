use tauri::State;

use talkiwi_core::config::AppConfig;

use crate::AppState;

#[derive(Debug, serde::Deserialize)]
pub struct ConfigPatch {
    path: String,
    value: serde_json::Value,
}

#[tauri::command]
pub async fn config_get(state: State<'_, AppState>) -> Result<AppConfig, String> {
    let content = tokio::fs::read_to_string(&state.config_path)
        .await
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&content).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn config_update(
    state: State<'_, AppState>,
    path: String,
    value: serde_json::Value,
) -> Result<(), String> {
    apply_and_persist_config(&state, vec![ConfigPatch { path, value }]).await
}

#[tauri::command]
pub async fn config_update_many(
    state: State<'_, AppState>,
    updates: Vec<ConfigPatch>,
) -> Result<(), String> {
    apply_and_persist_config(&state, updates).await
}

async fn apply_and_persist_config(
    state: &State<'_, AppState>,
    updates: Vec<ConfigPatch>,
) -> Result<(), String> {
    let content = tokio::fs::read_to_string(&state.config_path)
        .await
        .map_err(|e| e.to_string())?;
    let mut config: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| e.to_string())?;

    for patch in updates {
        set_nested_value(&mut config, &patch.path, patch.value).map_err(|e| e.to_string())?;
    }

    let tmp = state.config_path.with_extension("json.tmp");
    let serialized = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    tokio::fs::write(&tmp, &serialized)
        .await
        .map_err(|e| e.to_string())?;
    tokio::fs::rename(&tmp, &state.config_path)
        .await
        .map_err(|e| e.to_string())?;

    let parsed: AppConfig = serde_json::from_value(config).map_err(|e| e.to_string())?;
    let mut app_config = state
        .config
        .lock()
        .map_err(|e| format!("config lock poisoned: {e}"))?;
    *app_config = parsed;

    Ok(())
}

fn set_nested_value(
    root: &mut serde_json::Value,
    path: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = root;

    for (i, part) in parts.iter().enumerate() {
        if i == parts.len() - 1 {
            current
                .as_object_mut()
                .ok_or_else(|| format!("path '{}' is not an object", path))?
                .insert(part.to_string(), value);
            return Ok(());
        }

        current = current
            .as_object_mut()
            .ok_or_else(|| format!("path segment '{}' is not an object", part))?
            .entry(part.to_string())
            .or_insert(serde_json::Value::Object(serde_json::Map::new()));
    }

    Err("empty path".to_string())
}
