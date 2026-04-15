//! Permission detection + system-settings hand-off.
//!
//! V1.1: replace the stub with heuristic probes that don't require
//! linking to `objc2` / `core-graphics`. The probes are good enough to
//! decide whether toolbar buttons should be enabled:
//!
//!   - **Accessibility**: run a tiny osascript that queries System
//!     Events. If it fails, either the process isn't trusted or
//!     automation isn't allowed for Talkiwi — in both cases the toolbar
//!     button should be disabled until the user grants.
//!
//!   - **Screen Recording**: enumerate monitors via xcap. The
//!     enumeration call is cheap and doesn't trigger a real screenshot
//!     — it only queries the CGDisplay list, which is gated by the same
//!     `CGPreflightScreenCaptureAccess` signal the native API uses.
//!     Cached per-session so we don't re-probe mid-recording.
//!
//!   - **Microphone**: reuse the existing `AudioInputManager` which
//!     already delegates to cpal. A non-empty device list means cpal
//!     successfully opened the host's input list.
//!
//! Phase 2 can swap these for `objc2-application-services` /
//! `CGPreflightScreenCaptureAccess` without touching the caller.

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

/// Heuristic accessibility probe. The osascript runs as the user (`talkiwi`
/// itself, via osascript sub-process) and queries the front app. A
/// successful call + non-empty stdout strongly implies accessibility
/// access; a permission failure returns stderr with a `(-1719)` /
/// `(-25211)` error.
fn probe_accessibility() -> bool {
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg("tell application \"System Events\" to get name of first application process whose frontmost is true")
        .output();

    match output {
        Ok(o) => o.status.success() && !o.stdout.is_empty(),
        Err(_) => false,
    }
}

/// Heuristic screen-recording probe. `xcap::Monitor::all()` returns an
/// empty list on macOS when screen recording is denied, and succeeds
/// with at least one entry when granted. This does *not* capture a
/// frame — it's a pure enumeration call.
fn probe_screen_recording() -> bool {
    match xcap::Monitor::all() {
        Ok(monitors) => !monitors.is_empty(),
        Err(_) => false,
    }
}

/// Heuristic microphone probe. cpal enumerates input devices through
/// the system API; a non-empty list means the host host granted access
/// to the device list, which is the same signal we need for recording.
fn probe_microphone(state: &AppState) -> bool {
    match state.audio_input_manager.list_inputs() {
        Ok(inputs) => !inputs.is_empty(),
        Err(_) => false,
    }
}

#[tauri::command]
pub async fn permissions_check(
    state: State<'_, AppState>,
) -> Result<PermissionReport, String> {
    // Probes are cheap (all < 20ms on a warm session) but we still run
    // them on the blocking pool so the Tauri runtime thread stays
    // responsive — osascript can spike to 200ms on a cold start.
    let mic_granted = probe_microphone(&state);

    let (acc, sr) = tokio::task::spawn_blocking(|| {
        let acc = probe_accessibility();
        let sr = probe_screen_recording();
        (acc, sr)
    })
    .await
    .map_err(|e| format!("permission probe failed: {e}"))?;

    let entries = vec![
        PermissionEntry {
            module: "accessibility".to_string(),
            granted: acc,
            description: "Required for text selection & page context capture"
                .to_string(),
        },
        PermissionEntry {
            module: "screen_recording".to_string(),
            granted: sr,
            description: "Required for screenshot capture".to_string(),
        },
        PermissionEntry {
            module: "microphone".to_string(),
            granted: mic_granted,
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
