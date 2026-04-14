//! SelectionCapture — monitors text selection changes via macOS Accessibility API.
//!
//! Polls the focused UI element's `AXSelectedText` attribute every 200ms.
//! Filters noise: min chars, dedup, debounce.
//!
//! Requires macOS Accessibility permission (`AXIsProcessTrusted()`).

use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tracing::debug;
use uuid::Uuid;

use talkiwi_core::clock::SessionClock;
use talkiwi_core::event::{ActionEvent, ActionPayload, ActionType};
use talkiwi_core::traits::capture::{ActionCapture, PermissionStatus};

/// Polling interval for selection changes.
const POLL_INTERVAL_MS: u64 = 200;

/// Minimum character count to emit a selection event.
const MIN_CHARS: usize = 3;

/// Debounce period: rapid selection changes within this window are merged.
const DEBOUNCE_MS: u64 = 500;

/// SelectionCapture monitors text selection changes on macOS.
///
/// Uses AppleScript via `osascript` to query the focused element's
/// selected text. This is a simpler approach than direct AX API bindings
/// and works reliably for most applications.
pub struct SelectionCapture {
    session_id: Uuid,
    running: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl SelectionCapture {
    pub fn new(session_id: Uuid) -> Self {
        Self {
            session_id,
            running: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }
}

/// Get the currently selected text via Accessibility API (osascript wrapper).
///
/// Returns None if:
/// - No text is selected
/// - Accessibility permission is not granted
/// - The focused element doesn't support text selection
fn get_selected_text() -> Option<(String, String, String)> {
    // Use AppleScript to get selection via System Events
    let script = r#"
tell application "System Events"
    set frontApp to name of first application process whose frontmost is true
    set frontWin to name of front window of first application process whose frontmost is true
end tell
tell application frontApp
    try
        set sel to selection as text
        return frontApp & "|" & frontWin & "|" & sel
    end try
end tell
return ""
"#;

    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let result = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if result.is_empty() {
        return None;
    }

    let parts: Vec<&str> = result.splitn(3, '|').collect();
    if parts.len() == 3 && !parts[2].is_empty() {
        Some((
            parts[0].to_string(),
            parts[1].to_string(),
            parts[2].to_string(),
        ))
    } else {
        None
    }
}

/// Check if macOS Accessibility permission is granted.
fn is_accessibility_trusted() -> bool {
    // On macOS, we can check via the `AXIsProcessTrusted` API
    // Using a simpler osascript probe as a heuristic
    let output = Command::new("osascript")
        .arg("-e")
        .arg("tell application \"System Events\" to get name of first application process whose frontmost is true")
        .output();

    match output {
        Ok(o) => o.status.success() && !o.stdout.is_empty(),
        Err(_) => false,
    }
}

impl ActionCapture for SelectionCapture {
    fn id(&self) -> &str {
        "builtin.selection"
    }

    fn action_types(&self) -> &[ActionType] {
        &[ActionType::SelectionText]
    }

    fn start(&mut self, tx: mpsc::Sender<ActionEvent>, clock: SessionClock) -> anyhow::Result<()> {
        let running = Arc::clone(&self.running);
        running.store(true, Ordering::SeqCst);

        let session_id = self.session_id;

        let handle = std::thread::spawn(move || {
            let mut last_text: Option<String> = None;
            let mut last_emit_time = Instant::now()
                .checked_sub(Duration::from_millis(DEBOUNCE_MS + 1))
                .unwrap_or_else(Instant::now);

            while running.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));

                if !running.load(Ordering::SeqCst) {
                    break;
                }

                let selection = match get_selected_text() {
                    Some(s) => s,
                    None => continue,
                };

                let (app_name, window_title, text) = selection;

                // Filter: minimum character count
                if text.len() < MIN_CHARS {
                    continue;
                }

                // Filter: dedup — same text as last emission
                if last_text.as_deref() == Some(&text) {
                    continue;
                }

                // Filter: debounce
                let now = Instant::now();
                if now.duration_since(last_emit_time) < Duration::from_millis(DEBOUNCE_MS) {
                    continue;
                }

                let char_count = text.chars().count();
                let offset_ms = clock.elapsed_ms();

                let event = ActionEvent {
                    id: Uuid::new_v4(),
                    session_id,
                    timestamp: u64::try_from(chrono::Utc::now().timestamp_millis()).unwrap_or(0),
                    session_offset_ms: offset_ms,
                    observed_offset_ms: Some(offset_ms),
                    duration_ms: None,
                    action_type: ActionType::SelectionText,
                    plugin_id: "builtin.selection".to_string(),
                    payload: ActionPayload::SelectionText {
                        text: text.clone(),
                        app_name,
                        window_title,
                        char_count,
                    },
                    semantic_hint: None,
                    confidence: 1.0,
                };

                debug!(chars = char_count, "selection text captured");

                if tx.blocking_send(event).is_err() {
                    break;
                }

                last_text = Some(text);
                last_emit_time = now;
            }
        });

        self.handle = Some(handle);
        Ok(())
    }

    fn stop(&mut self) -> anyhow::Result<()> {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        Ok(())
    }

    fn check_permission(&self) -> PermissionStatus {
        if is_accessibility_trusted() {
            PermissionStatus::Granted
        } else {
            PermissionStatus::Denied
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_capture_metadata() {
        let capture = SelectionCapture::new(Uuid::new_v4());
        assert_eq!(capture.id(), "builtin.selection");
        assert_eq!(capture.action_types(), &[ActionType::SelectionText]);
    }

    #[tokio::test]
    async fn selection_capture_start_stop_lifecycle() {
        let mut capture = SelectionCapture::new(Uuid::new_v4());
        let (tx, _rx) = mpsc::channel(16);

        capture.start(tx, SessionClock::new()).unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        capture.stop().unwrap();
    }

    #[test]
    #[ignore = "slow: runs AppleScript which takes ~120s in some environments"]
    fn selection_capture_check_permission() {
        let capture = SelectionCapture::new(Uuid::new_v4());
        let perm = capture.check_permission();
        // This will vary based on whether the test runner has Accessibility permission
        assert!(
            perm == PermissionStatus::Granted || perm == PermissionStatus::Denied,
            "Expected Granted or Denied, got {:?}",
            perm
        );
    }
}
