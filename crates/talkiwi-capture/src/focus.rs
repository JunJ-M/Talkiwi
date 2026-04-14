//! FocusCapture — monitors frontmost app/window focus changes.

use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tracing::debug;
use uuid::Uuid;

use talkiwi_core::clock::SessionClock;
use talkiwi_core::event::{ActionEvent, ActionPayload, ActionType};
use talkiwi_core::traits::capture::{ActionCapture, PermissionStatus};

const POLL_INTERVAL_MS: u64 = 300;
const DEBOUNCE_MS: u64 = 500;

pub struct FocusCapture {
    session_id: Uuid,
    running: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl FocusCapture {
    pub fn new(session_id: Uuid) -> Self {
        Self {
            session_id,
            running: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }
}

fn is_accessibility_trusted() -> bool {
    let output = Command::new("osascript")
        .arg("-e")
        .arg("tell application \"System Events\" to get name of first application process whose frontmost is true")
        .output();

    match output {
        Ok(result) => result.status.success() && !result.stdout.is_empty(),
        Err(_) => false,
    }
}

impl ActionCapture for FocusCapture {
    fn id(&self) -> &str {
        "builtin.focus"
    }

    fn action_types(&self) -> &[ActionType] {
        &[ActionType::WindowFocus]
    }

    fn start(&mut self, tx: mpsc::Sender<ActionEvent>, clock: SessionClock) -> anyhow::Result<()> {
        let running = Arc::clone(&self.running);
        running.store(true, Ordering::SeqCst);

        let session_id = self.session_id;
        let handle = std::thread::spawn(move || {
            let mut last_focus: Option<(String, String)> = None;
            let mut last_emit_ms: Option<u64> = None;

            while running.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
                if !running.load(Ordering::SeqCst) {
                    break;
                }

                let window = match active_win_pos_rs::get_active_window() {
                    Ok(window) => window,
                    Err(_) => continue,
                };

                if window.app_name.to_lowercase().contains("talkiwi") {
                    continue;
                }

                let current = (window.app_name.clone(), window.title.clone());
                if last_focus.as_ref() == Some(&current) {
                    continue;
                }

                let offset_ms = clock.elapsed_ms();
                if let Some(last_emit) = last_emit_ms {
                    if offset_ms.saturating_sub(last_emit) < DEBOUNCE_MS {
                        last_focus = Some(current);
                        continue;
                    }
                }

                let event = ActionEvent {
                    id: Uuid::new_v4(),
                    session_id,
                    timestamp: u64::try_from(chrono::Utc::now().timestamp_millis()).unwrap_or(0),
                    session_offset_ms: offset_ms,
                    observed_offset_ms: Some(offset_ms),
                    duration_ms: None,
                    action_type: ActionType::WindowFocus,
                    plugin_id: "builtin.focus".to_string(),
                    payload: ActionPayload::WindowFocus {
                        app_name: window.app_name,
                        window_title: window.title,
                    },
                    semantic_hint: None,
                    confidence: 1.0,
                };

                debug!("window focus captured");
                if tx.blocking_send(event).is_err() {
                    break;
                }

                last_focus = Some(current);
                last_emit_ms = Some(offset_ms);
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
    fn focus_capture_metadata() {
        let capture = FocusCapture::new(Uuid::new_v4());
        assert_eq!(capture.id(), "builtin.focus");
        assert_eq!(capture.action_types(), &[ActionType::WindowFocus]);
    }

    #[tokio::test]
    async fn focus_capture_start_stop_lifecycle() {
        let mut capture = FocusCapture::new(Uuid::new_v4());
        let (tx, _rx) = mpsc::channel(16);
        capture.start(tx, SessionClock::new()).unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        capture.stop().unwrap();
    }
}
