//! ClipboardCapture — monitors clipboard changes and emits ActionEvents.
//!
//! Uses `clipboard-rs` to poll for `changeCount` changes.
//! Runs a background std::thread (not tokio) since clipboard-rs
//! is synchronous, bridging to tokio via `blocking_send()`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use clipboard_rs::{Clipboard, ClipboardContext};
use tokio::sync::mpsc;
use tracing::{debug, warn};
use uuid::Uuid;

use talkiwi_core::clock::SessionClock;
use talkiwi_core::event::{ActionEvent, ActionPayload, ActionType, ClipboardContentType};
use talkiwi_core::traits::capture::{ActionCapture, PermissionStatus};

/// Polling interval for clipboard changes.
const POLL_INTERVAL_MS: u64 = 500;

/// ClipboardCapture monitors clipboard changes via polling.
pub struct ClipboardCapture {
    session_id: Uuid,
    running: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl ClipboardCapture {
    pub fn new(session_id: Uuid) -> Self {
        Self {
            session_id,
            running: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }
}

impl ActionCapture for ClipboardCapture {
    fn id(&self) -> &str {
        "builtin.clipboard"
    }

    fn action_types(&self) -> &[ActionType] {
        &[ActionType::ClipboardChange]
    }

    fn start(&mut self, tx: mpsc::Sender<ActionEvent>, clock: SessionClock) -> anyhow::Result<()> {
        let running = Arc::clone(&self.running);
        running.store(true, Ordering::SeqCst);

        let session_id = self.session_id;

        let handle = std::thread::spawn(move || {
            let ctx = match ClipboardContext::new() {
                Ok(ctx) => ctx,
                Err(e) => {
                    warn!(error = %e, "failed to create clipboard context");
                    return;
                }
            };

            let mut last_text: Option<String> = None;

            while running.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(POLL_INTERVAL_MS));

                if !running.load(Ordering::SeqCst) {
                    break;
                }

                // Try to detect clipboard content change
                let current_text = ctx.get_text().ok();

                // Check if content has changed
                let changed = match (&last_text, &current_text) {
                    (None, Some(_)) => true,
                    (Some(prev), Some(curr)) => prev != curr,
                    _ => false,
                };

                if !changed {
                    continue;
                }

                // Determine content type and build payload
                let (content_type, text, file_path) = if let Some(ref text) = current_text {
                    (ClipboardContentType::Text, Some(text.clone()), None)
                } else {
                    // For V1, we only handle text clipboard changes
                    continue;
                };

                let offset_ms = clock.elapsed_ms();

                let event = ActionEvent {
                    id: Uuid::new_v4(),
                    session_id,
                    timestamp: u64::try_from(chrono::Utc::now().timestamp_millis()).unwrap_or(0),
                    session_offset_ms: offset_ms,
                    observed_offset_ms: Some(offset_ms),
                    duration_ms: None,
                    action_type: ActionType::ClipboardChange,
                    plugin_id: "builtin.clipboard".to_string(),
                    payload: ActionPayload::ClipboardChange {
                        content_type,
                        text,
                        file_path,
                        source_app: None, // V1: no source app detection
                    },
                    semantic_hint: None,
                    confidence: 1.0,
                };

                debug!(offset_ms, "clipboard change detected");

                if tx.blocking_send(event).is_err() {
                    break; // Channel closed
                }

                last_text = current_text;
            }
        });

        self.handle = Some(handle);
        Ok(())
    }

    fn stop(&mut self) -> anyhow::Result<()> {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            // Wait for the polling thread to exit (with timeout via join)
            let _ = handle.join();
        }
        Ok(())
    }

    fn check_permission(&self) -> PermissionStatus {
        PermissionStatus::NotRequired
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_capture_permission_not_required() {
        let capture = ClipboardCapture::new(Uuid::new_v4());
        assert_eq!(capture.check_permission(), PermissionStatus::NotRequired);
    }

    #[test]
    fn clipboard_capture_id_and_action_types() {
        let capture = ClipboardCapture::new(Uuid::new_v4());
        assert_eq!(capture.id(), "builtin.clipboard");
        assert_eq!(capture.action_types(), &[ActionType::ClipboardChange]);
    }

    #[tokio::test]
    async fn clipboard_capture_start_stop_lifecycle() {
        let mut capture = ClipboardCapture::new(Uuid::new_v4());
        let (tx, _rx) = mpsc::channel(16);

        // Should start and stop without errors
        capture.start(tx, SessionClock::new()).unwrap();
        assert!(capture.running.load(Ordering::SeqCst));

        // Brief pause to let thread start
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        capture.stop().unwrap();
        assert!(!capture.running.load(Ordering::SeqCst));
    }
}
