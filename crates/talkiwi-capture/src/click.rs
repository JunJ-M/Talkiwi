//! ClickCapture — polls global mouse button state and emits click events.

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

const POLL_INTERVAL_MS: u64 = 40;

pub struct ClickCapture {
    session_id: Uuid,
    running: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl ClickCapture {
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

#[cfg(target_os = "macos")]
fn button_state(button: objc2_core_graphics::CGMouseButton) -> bool {
    use objc2_core_graphics::{CGEventSource, CGEventSourceStateID};

    CGEventSource::button_state(CGEventSourceStateID::HIDSystemState, button)
}

#[cfg(target_os = "macos")]
fn mouse_location() -> (f64, f64) {
    use objc2_app_kit::NSEvent;

    let point = NSEvent::mouseLocation();
    (point.x, point.y)
}

#[cfg(not(target_os = "macos"))]
fn mouse_location() -> (f64, f64) {
    (0.0, 0.0)
}

impl ActionCapture for ClickCapture {
    fn id(&self) -> &str {
        "builtin.click"
    }

    fn action_types(&self) -> &[ActionType] {
        &[ActionType::ClickMouse]
    }

    fn start(&mut self, tx: mpsc::Sender<ActionEvent>, clock: SessionClock) -> anyhow::Result<()> {
        let running = Arc::clone(&self.running);
        running.store(true, Ordering::SeqCst);

        let session_id = self.session_id;
        let handle = std::thread::spawn(move || {
            #[cfg(target_os = "macos")]
            let mut last_left = false;
            #[cfg(target_os = "macos")]
            let mut last_right = false;

            while running.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
                if !running.load(Ordering::SeqCst) {
                    break;
                }

                #[cfg(target_os = "macos")]
                {
                    let left_down = button_state(objc2_core_graphics::CGMouseButton::Left);
                    let right_down = button_state(objc2_core_graphics::CGMouseButton::Right);

                    if !last_left
                        && left_down
                        && emit_click(&tx, &clock, session_id, "left").is_err()
                    {
                        break;
                    }
                    if !last_right
                        && right_down
                        && emit_click(&tx, &clock, session_id, "right").is_err()
                    {
                        break;
                    }

                    last_left = left_down;
                    last_right = right_down;
                }

                #[cfg(not(target_os = "macos"))]
                {
                    let _ = (&tx, &clock, session_id);
                    break;
                }
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

fn emit_click(
    tx: &mpsc::Sender<ActionEvent>,
    clock: &SessionClock,
    session_id: Uuid,
    button: &str,
) -> anyhow::Result<()> {
    let offset_ms = clock.elapsed_ms();
    let (x, y) = mouse_location();
    let window = active_win_pos_rs::get_active_window().ok();
    let app_name = window.as_ref().map(|window| window.app_name.clone());
    let window_title = window.as_ref().map(|window| window.title.clone());

    if app_name
        .as_deref()
        .unwrap_or_default()
        .to_lowercase()
        .contains("talkiwi")
    {
        return Ok(());
    }

    let event = ActionEvent {
        id: Uuid::new_v4(),
        session_id,
        timestamp: u64::try_from(chrono::Utc::now().timestamp_millis()).unwrap_or(0),
        session_offset_ms: offset_ms,
        observed_offset_ms: Some(offset_ms),
        duration_ms: None,
        action_type: ActionType::ClickMouse,
        plugin_id: "builtin.click".to_string(),
        payload: ActionPayload::ClickMouse {
            app_name,
            window_title,
            button: button.to_string(),
            x,
            y,
        },
        semantic_hint: None,
        confidence: 1.0,
    };

    debug!(button, x, y, "mouse click captured");
    tx.blocking_send(event)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_capture_metadata() {
        let capture = ClickCapture::new(Uuid::new_v4());
        assert_eq!(capture.id(), "builtin.click");
        assert_eq!(capture.action_types(), &[ActionType::ClickMouse]);
    }

    #[tokio::test]
    async fn click_capture_start_stop_lifecycle() {
        let mut capture = ClickCapture::new(Uuid::new_v4());
        let (tx, _rx) = mpsc::channel(16);
        capture.start(tx, SessionClock::new()).unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        capture.stop().unwrap();
    }
}
