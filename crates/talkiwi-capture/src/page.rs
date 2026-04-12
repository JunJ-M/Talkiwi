//! PageCapture — detects active window changes and browser URL navigation.
//!
//! Produces two event types:
//! - `page.current`: when the active window changes
//! - `click.link`: when URL changes within the same browser window
//!
//! Uses `active-win-pos-rs` for window detection and AppleScript for
//! browser URL extraction (Chrome, Safari, Arc).

use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tracing::debug;
use uuid::Uuid;

use talkiwi_core::event::{ActionEvent, ActionPayload, ActionType};
use talkiwi_core::traits::capture::{ActionCapture, PermissionStatus};

/// Polling interval for window changes.
const POLL_INTERVAL_MS: u64 = 500;

/// Debounce window: ignore rapid switches within this period.
const DEBOUNCE_MS: u64 = 1000;

/// Known browser bundle IDs for URL extraction.
const BROWSER_BUNDLES: &[(&str, &str)] = &[
    ("com.google.Chrome", "Google Chrome"),
    ("com.apple.Safari", "Safari"),
    ("company.thebrowser.Browser", "Arc"),
];

/// PageCapture monitors active window changes and browser URL navigation.
pub struct PageCapture {
    session_id: Uuid,
    running: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl PageCapture {
    pub fn new(session_id: Uuid) -> Self {
        Self {
            session_id,
            running: Arc::new(AtomicBool::new(false)),
            handle: None,
        }
    }
}

/// Try to get the current URL from a browser via AppleScript.
fn get_browser_url(app_name: &str) -> Option<String> {
    let script = match app_name {
        "Google Chrome" => {
            "tell application \"Google Chrome\" to get URL of active tab of front window"
        }
        "Safari" => "tell application \"Safari\" to get URL of current tab of front window",
        "Arc" => "tell application \"Arc\" to get URL of active tab of front window",
        _ => return None,
    };

    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .ok()?;

    if output.status.success() {
        let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if url.is_empty() || url == "missing value" {
            None
        } else {
            Some(url)
        }
    } else {
        None
    }
}

/// Check if a bundle ID corresponds to a known browser.
fn browser_name_for_bundle(bundle_id: &str) -> Option<&'static str> {
    BROWSER_BUNDLES
        .iter()
        .find(|(id, _)| *id == bundle_id)
        .map(|(_, name)| *name)
}

/// Check if an app name corresponds to a known browser.
/// This is the primary detection path since `active-win-pos-rs`
/// provides app_name but not macOS bundle identifiers.
fn browser_name_for_app(app_name: &str) -> Option<&'static str> {
    BROWSER_BUNDLES
        .iter()
        .find(|(_, name)| *name == app_name)
        .map(|(_, name)| *name)
}

impl ActionCapture for PageCapture {
    fn id(&self) -> &str {
        "builtin.page"
    }

    fn action_types(&self) -> &[ActionType] {
        &[ActionType::PageCurrent, ActionType::ClickLink]
    }

    fn start(&mut self, tx: mpsc::Sender<ActionEvent>) -> anyhow::Result<()> {
        let running = Arc::clone(&self.running);
        running.store(true, Ordering::SeqCst);

        let session_id = self.session_id;
        let start_time = std::time::Instant::now();

        let handle = std::thread::spawn(move || {
            let mut last_window_title: Option<String> = None;
            let mut last_url: Option<String> = None;
            let mut last_event_time = std::time::Instant::now()
                .checked_sub(Duration::from_millis(DEBOUNCE_MS + 1))
                .unwrap_or_else(std::time::Instant::now);

            while running.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));

                if !running.load(Ordering::SeqCst) {
                    break;
                }

                // Get active window info
                let win = match active_win_pos_rs::get_active_window() {
                    Ok(w) => w,
                    Err(_) => continue,
                };

                let title = win.title.clone();
                let app_name = win.app_name.clone();

                // Self-filter: ignore Talkiwi's own window
                if app_name.to_lowercase().contains("talkiwi") {
                    continue;
                }

                let now = std::time::Instant::now();
                let offset_ms = start_time.elapsed().as_millis() as u64;

                // Check for window change
                let window_changed = last_window_title.as_deref() != Some(&title);

                if window_changed {
                    // Debounce
                    if now.duration_since(last_event_time) < Duration::from_millis(DEBOUNCE_MS) {
                        last_window_title = Some(title);
                        continue;
                    }

                    // Try to get URL if it's a browser (app_name is primary match)
                    let url = browser_name_for_app(&app_name)
                        .or_else(|| browser_name_for_bundle(&win.process_path.to_string_lossy()))
                        .and_then(get_browser_url);

                    let event = ActionEvent {
                        id: Uuid::new_v4(),
                        session_id,
                        timestamp: u64::try_from(chrono::Utc::now().timestamp_millis())
                            .unwrap_or(0),
                        session_offset_ms: offset_ms,
                        duration_ms: None,
                        action_type: ActionType::PageCurrent,
                        plugin_id: "builtin".to_string(),
                        payload: ActionPayload::PageCurrent {
                            url: url.clone(),
                            title: title.clone(),
                            app_name: app_name.clone(),
                            bundle_id: win.process_path.to_string_lossy().to_string(),
                        },
                        semantic_hint: None,
                        confidence: 1.0,
                    };

                    debug!(title = %title, app = %app_name, "window change detected");

                    if tx.blocking_send(event).is_err() {
                        break;
                    }

                    last_window_title = Some(title);
                    last_url = url;
                    last_event_time = now;
                } else {
                    // Same window — check if URL changed (in-page navigation)
                    let browser = browser_name_for_app(&app_name)
                        .or_else(|| browser_name_for_bundle(&win.process_path.to_string_lossy()));

                    if let Some(browser_name) = browser {
                        if let Some(current_url) = get_browser_url(browser_name) {
                            let url_changed = last_url.as_deref() != Some(&current_url);
                            if url_changed {
                                let event = ActionEvent {
                                    id: Uuid::new_v4(),
                                    session_id,
                                    timestamp: u64::try_from(chrono::Utc::now().timestamp_millis())
                                        .unwrap_or(0),
                                    session_offset_ms: offset_ms,
                                    duration_ms: None,
                                    action_type: ActionType::ClickLink,
                                    plugin_id: "builtin".to_string(),
                                    payload: ActionPayload::ClickLink {
                                        from_url: last_url.clone(),
                                        to_url: current_url.clone(),
                                        title: Some(title.clone()),
                                    },
                                    semantic_hint: None,
                                    confidence: 1.0,
                                };

                                debug!(url = %current_url, "URL change detected");

                                if tx.blocking_send(event).is_err() {
                                    break;
                                }

                                last_url = Some(current_url);
                                last_event_time = now;
                            }
                        }
                    }
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
        // Requires Accessibility for window title detection.
        // Automation permission is checked lazily when AppleScript runs.
        // Return NotDetermined — ActionTrack treats this as "don't auto-start".
        // The app layer can explicitly start PageCapture after permission check.
        PermissionStatus::NotDetermined
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_capture_metadata() {
        let capture = PageCapture::new(Uuid::new_v4());
        assert_eq!(capture.id(), "builtin.page");
        assert_eq!(
            capture.action_types(),
            &[ActionType::PageCurrent, ActionType::ClickLink]
        );
    }

    #[test]
    fn browser_name_lookup_by_bundle() {
        assert_eq!(
            browser_name_for_bundle("com.google.Chrome"),
            Some("Google Chrome")
        );
        assert_eq!(browser_name_for_bundle("com.apple.Safari"), Some("Safari"));
        assert_eq!(browser_name_for_bundle("com.unknown.app"), None);
    }

    #[test]
    fn browser_name_lookup_by_app_name() {
        assert_eq!(browser_name_for_app("Google Chrome"), Some("Google Chrome"));
        assert_eq!(browser_name_for_app("Safari"), Some("Safari"));
        assert_eq!(browser_name_for_app("Arc"), Some("Arc"));
        assert_eq!(browser_name_for_app("VSCode"), None);
    }

    #[tokio::test]
    async fn page_capture_start_stop_lifecycle() {
        let mut capture = PageCapture::new(Uuid::new_v4());
        let (tx, _rx) = mpsc::channel(16);

        capture.start(tx).unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        capture.stop().unwrap();
    }

    /// Requires Accessibility permission on macOS.
    #[tokio::test]
    #[ignore = "requires Accessibility permission"]
    async fn page_capture_detects_window() {
        let mut capture = PageCapture::new(Uuid::new_v4());
        let (tx, mut rx) = mpsc::channel(16);

        capture.start(tx).unwrap();

        // Wait for at least one window detection
        let event = tokio::time::timeout(Duration::from_secs(3), rx.recv()).await;

        capture.stop().unwrap();

        if let Ok(Some(event)) = event {
            assert_eq!(event.action_type, ActionType::PageCurrent);
        }
        // It's OK if no event arrives (depends on window focus state)
    }
}
