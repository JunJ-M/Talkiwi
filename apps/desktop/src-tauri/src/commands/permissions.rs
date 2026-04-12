use serde::{Deserialize, Serialize};
use tauri::State;

use crate::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct PermissionEntry {
    pub module: String,
    pub granted: bool,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PermissionReport {
    pub entries: Vec<PermissionEntry>,
}

#[tauri::command]
pub async fn permissions_check(_state: State<'_, AppState>) -> Result<PermissionReport, String> {
    // V1: report macOS permission status for accessibility and screen recording.
    // Actual system API calls require objc bindings; stub for now.
    let entries = vec![
        PermissionEntry {
            module: "accessibility".to_string(),
            granted: false,
            description: "Required for text selection capture".to_string(),
        },
        PermissionEntry {
            module: "screen_recording".to_string(),
            granted: false,
            description: "Required for screenshot capture".to_string(),
        },
        PermissionEntry {
            module: "microphone".to_string(),
            granted: false,
            description: "Required for voice recording".to_string(),
        },
    ];

    Ok(PermissionReport { entries })
}

#[tauri::command]
pub async fn permissions_request(
    _state: State<'_, AppState>,
    module: String,
) -> Result<bool, String> {
    // V1: open System Settings to the relevant pane.
    // Actual granting happens in System Settings; we just navigate there.
    match module.as_str() {
        "accessibility" => {
            let _ = std::process::Command::new("open")
                .arg(
                    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
                )
                .spawn();
        }
        "screen_recording" => {
            let _ = std::process::Command::new("open")
                .arg(
                    "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
                )
                .spawn();
        }
        "microphone" => {
            let _ = std::process::Command::new("open")
                .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
                .spawn();
        }
        _ => return Err(format!("Unknown permission module: {}", module)),
    }

    Ok(false) // Can't know if granted until user acts in System Settings
}
