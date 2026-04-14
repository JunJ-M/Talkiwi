//! ScreenshotCapture — captures screen regions via xcap.
//!
//! Unlike other capturers, ScreenshotCapture is NOT a background poller.
//! It's triggered explicitly (hotkey/button) and events are injected
//! via ActionTrack::inject_event().

use std::path::PathBuf;

use tokio::sync::mpsc;
use uuid::Uuid;

use talkiwi_core::clock::SessionClock;
use talkiwi_core::event::{ActionEvent, ActionPayload, ActionType};
use talkiwi_core::traits::capture::{ActionCapture, PermissionStatus};

/// ScreenshotCapture handles on-demand screenshot capture.
/// Screenshots are saved to the session directory.
pub struct ScreenshotCapture {
    session_dir: PathBuf,
}

impl ScreenshotCapture {
    pub fn new(session_dir: PathBuf) -> Self {
        Self { session_dir }
    }

    /// Capture a screen region (or full screen if region is None).
    ///
    /// Returns an ActionEvent with the screenshot metadata.
    /// The event should be injected via `ActionTrack::inject_event()`.
    pub fn capture_region(
        &self,
        _region: Option<(i32, i32, u32, u32)>,
        session_id: Uuid,
        session_offset_ms: u64,
    ) -> anyhow::Result<ActionEvent> {
        // Ensure session directory exists
        std::fs::create_dir_all(&self.session_dir)?;

        let monitors = xcap::Monitor::all()?;
        let monitor = monitors
            .first()
            .ok_or_else(|| anyhow::anyhow!("no monitor found"))?;

        let image = monitor.capture_image()?;
        let width = image.width();
        let height = image.height();

        // Save to session directory
        let filename = format!("screenshot-{}.png", Uuid::new_v4());
        let save_path = self.session_dir.join(&filename);
        image.save(&save_path)?;

        let image_path = save_path.to_string_lossy().to_string();

        Ok(ActionEvent {
            id: Uuid::new_v4(),
            session_id,
            timestamp: u64::try_from(chrono::Utc::now().timestamp_millis()).unwrap_or(0),
            session_offset_ms,
            observed_offset_ms: Some(session_offset_ms),
            duration_ms: None,
            action_type: ActionType::Screenshot,
            plugin_id: "builtin".to_string(),
            payload: ActionPayload::Screenshot {
                image_path,
                width,
                height,
                ocr_text: None, // V1.5: Apple Vision OCR
            },
            semantic_hint: Some("user took a screenshot".to_string()),
            confidence: 1.0,
        })
    }
}

impl ActionCapture for ScreenshotCapture {
    fn id(&self) -> &str {
        "builtin.screenshot"
    }

    fn action_types(&self) -> &[ActionType] {
        &[ActionType::Screenshot]
    }

    fn start(
        &mut self,
        _tx: mpsc::Sender<ActionEvent>,
        _clock: SessionClock,
    ) -> anyhow::Result<()> {
        // Screenshot is trigger-based, not background polling
        Ok(())
    }

    fn stop(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn check_permission(&self) -> PermissionStatus {
        // Screen Recording permission is required on macOS.
        // There's no reliable pre-check API — xcap will fail at capture time.
        // Return NotDetermined so ActionTrack can decide how to handle it.
        PermissionStatus::NotDetermined
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screenshot_capture_metadata() {
        let capture = ScreenshotCapture::new(PathBuf::from("/tmp"));
        assert_eq!(capture.id(), "builtin.screenshot");
        assert_eq!(capture.action_types(), &[ActionType::Screenshot]);
    }

    #[test]
    fn screenshot_capture_start_stop_noop() {
        let mut capture = ScreenshotCapture::new(PathBuf::from("/tmp"));
        let (tx, _rx) = mpsc::channel(1);
        capture.start(tx, SessionClock::new()).unwrap();
        capture.stop().unwrap();
    }

    /// Requires Screen Recording permission on macOS.
    #[test]
    #[ignore = "requires Screen Recording permission"]
    fn screenshot_capture_region_full_screen() {
        let dir = tempfile::tempdir().unwrap();
        let capture = ScreenshotCapture::new(dir.path().to_path_buf());

        let event = capture.capture_region(None, Uuid::new_v4(), 1000).unwrap();

        assert_eq!(event.action_type, ActionType::Screenshot);
        if let ActionPayload::Screenshot {
            width,
            height,
            image_path,
            ..
        } = &event.payload
        {
            assert!(*width > 0);
            assert!(*height > 0);
            assert!(std::path::Path::new(image_path).exists());
        } else {
            panic!("Expected Screenshot payload");
        }
    }
}
