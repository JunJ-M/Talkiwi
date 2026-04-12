//! ActionTrack — manages action capture lifecycle and event collection.
//!
//! Holds a set of `ActionCapture` implementations, starts/stops them,
//! and provides `inject_event()` for externally-triggered events
//! (screenshots, file drops).

use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::time::Instant;
use tracing::{info, warn};

use talkiwi_core::event::ActionEvent;
use talkiwi_core::traits::capture::{ActionCapture, PermissionStatus};

/// ActionTrack manages the lifecycle of multiple ActionCapture instances
/// and collects their events into a unified stream.
pub struct ActionTrack {
    captures: Vec<Box<dyn ActionCapture>>,
    events: Arc<Mutex<Vec<ActionEvent>>>,
    session_start: Option<Instant>,
    event_tx: Option<mpsc::Sender<ActionEvent>>,
    /// Internal aggregation sender — dropped on stop to signal the aggregator.
    agg_tx: Option<mpsc::Sender<ActionEvent>>,
    /// Aggregator task handle — joined on stop.
    agg_handle: Option<tokio::task::JoinHandle<()>>,
}

impl ActionTrack {
    /// Create a new empty ActionTrack.
    pub fn new() -> Self {
        Self {
            captures: Vec::new(),
            events: Arc::new(Mutex::new(Vec::new())),
            session_start: None,
            event_tx: None,
            agg_tx: None,
            agg_handle: None,
        }
    }

    /// Register an ActionCapture implementation.
    pub fn register(&mut self, capture: Box<dyn ActionCapture>) {
        info!(capture_id = capture.id(), "registered action capture");
        self.captures.push(capture);
    }

    /// Start all registered captures. Permission-denied captures are skipped.
    ///
    /// Events from all captures are aggregated and forwarded to `event_tx`.
    pub fn start(&mut self, event_tx: mpsc::Sender<ActionEvent>) -> anyhow::Result<()> {
        self.session_start = Some(Instant::now());
        self.event_tx = Some(event_tx.clone());

        let events = Arc::clone(&self.events);

        // Create an internal aggregation channel
        let (agg_tx, mut agg_rx) = mpsc::channel::<ActionEvent>(256);

        // Start each capture that has permission
        for capture in &mut self.captures {
            let perm = capture.check_permission();
            match perm {
                PermissionStatus::Granted | PermissionStatus::NotRequired => {
                    if let Err(e) = capture.start(agg_tx.clone()) {
                        warn!(
                            capture_id = capture.id(),
                            error = %e,
                            "failed to start capture, skipping"
                        );
                    } else {
                        info!(capture_id = capture.id(), "started action capture");
                    }
                }
                PermissionStatus::Denied | PermissionStatus::NotDetermined => {
                    warn!(
                        capture_id = capture.id(),
                        permission = ?perm,
                        "capture permission not granted, skipping"
                    );
                }
            }
        }

        // Store agg_tx so dropping it on stop signals the aggregator
        self.agg_tx = Some(agg_tx);

        // Spawn aggregator task: collect events, store, and forward
        let forward_tx = event_tx;
        let handle = tokio::spawn(async move {
            while let Some(event) = agg_rx.recv().await {
                events.lock().await.push(event.clone());
                // Forward to external listener (frontend), ignore send errors
                let _ = forward_tx.send(event).await;
            }
        });
        self.agg_handle = Some(handle);

        Ok(())
    }

    /// Stop all captures and return collected events.
    ///
    /// Capture `stop()` calls are run via `spawn_blocking` since polling
    /// thread joins may block for up to one poll interval.
    pub async fn stop(&mut self) -> anyhow::Result<Vec<ActionEvent>> {
        // Take captures out to stop them in spawn_blocking (avoids blocking executor)
        let mut captures = std::mem::take(&mut self.captures);
        tokio::task::spawn_blocking(move || {
            for capture in &mut captures {
                if let Err(e) = capture.stop() {
                    warn!(capture_id = capture.id(), error = %e, "error stopping capture");
                }
            }
        })
        .await?;

        self.event_tx = None;

        // Drop the aggregation sender to signal the aggregator task to finish
        self.agg_tx = None;

        // Wait for the aggregator to flush remaining events
        if let Some(handle) = self.agg_handle.take() {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), handle).await;
        }

        let mut guard = self.events.lock().await;
        Ok(std::mem::take(&mut *guard))
    }

    /// Inject an externally-triggered event (screenshot, file drop).
    ///
    /// Updates `session_offset_ms` based on elapsed time, stores the event,
    /// and forwards it to the event channel for real-time display.
    pub async fn inject_event(&self, mut event: ActionEvent) -> anyhow::Result<()> {
        if let Some(start) = self.session_start {
            event.session_offset_ms = start.elapsed().as_millis() as u64;
        }
        self.events.lock().await.push(event.clone());
        if let Some(tx) = &self.event_tx {
            tx.send(event).await.ok();
        }
        Ok(())
    }

    /// Get elapsed time since session start in milliseconds.
    pub fn elapsed_ms(&self) -> u64 {
        self.session_start
            .map(|s| s.elapsed().as_millis() as u64)
            .unwrap_or(0)
    }

    /// Get number of registered captures.
    pub fn capture_count(&self) -> usize {
        self.captures.len()
    }
}

impl Default for ActionTrack {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use talkiwi_core::event::{ActionPayload, ActionType};
    use uuid::Uuid;

    /// Mock ActionCapture for testing.
    struct MockCapture {
        id: String,
        permission: PermissionStatus,
        started: bool,
    }

    impl MockCapture {
        fn new(id: &str, permission: PermissionStatus) -> Self {
            Self {
                id: id.to_string(),
                permission,
                started: false,
            }
        }
    }

    impl ActionCapture for MockCapture {
        fn id(&self) -> &str {
            &self.id
        }

        fn action_types(&self) -> &[ActionType] {
            &[ActionType::ClipboardChange]
        }

        fn start(&mut self, _tx: mpsc::Sender<ActionEvent>) -> anyhow::Result<()> {
            self.started = true;
            Ok(())
        }

        fn stop(&mut self) -> anyhow::Result<()> {
            self.started = false;
            Ok(())
        }

        fn check_permission(&self) -> PermissionStatus {
            self.permission.clone()
        }
    }

    /// Mock capture that sends events on start.
    struct EventEmittingCapture {
        id: String,
        events_to_emit: Vec<ActionEvent>,
    }

    impl EventEmittingCapture {
        fn new(id: &str, events: Vec<ActionEvent>) -> Self {
            Self {
                id: id.to_string(),
                events_to_emit: events,
            }
        }
    }

    impl ActionCapture for EventEmittingCapture {
        fn id(&self) -> &str {
            &self.id
        }

        fn action_types(&self) -> &[ActionType] {
            &[ActionType::ClipboardChange]
        }

        fn start(&mut self, tx: mpsc::Sender<ActionEvent>) -> anyhow::Result<()> {
            let events = self.events_to_emit.clone();
            tokio::spawn(async move {
                for event in events {
                    let _ = tx.send(event).await;
                }
            });
            Ok(())
        }

        fn stop(&mut self) -> anyhow::Result<()> {
            Ok(())
        }

        fn check_permission(&self) -> PermissionStatus {
            PermissionStatus::NotRequired
        }
    }

    fn make_test_event(session_id: Uuid) -> ActionEvent {
        ActionEvent {
            id: Uuid::new_v4(),
            session_id,
            timestamp: 1712900000000,
            session_offset_ms: 0,
            duration_ms: None,
            action_type: ActionType::ClipboardChange,
            plugin_id: "builtin".to_string(),
            payload: ActionPayload::ClipboardChange {
                content_type: talkiwi_core::event::ClipboardContentType::Text,
                text: Some("test clipboard".to_string()),
                file_path: None,
                source_app: None,
            },
            semantic_hint: None,
            confidence: 1.0,
        }
    }

    #[test]
    fn action_track_register_captures() {
        let mut track = ActionTrack::new();
        assert_eq!(track.capture_count(), 0);

        track.register(Box::new(MockCapture::new(
            "mock1",
            PermissionStatus::Granted,
        )));
        track.register(Box::new(MockCapture::new(
            "mock2",
            PermissionStatus::Granted,
        )));
        assert_eq!(track.capture_count(), 2);
    }

    #[tokio::test]
    async fn action_track_start_stop_lifecycle() {
        let mut track = ActionTrack::new();
        track.register(Box::new(MockCapture::new(
            "mock1",
            PermissionStatus::Granted,
        )));

        let (tx, _rx) = mpsc::channel(16);
        track.start(tx).unwrap();

        assert!(track.elapsed_ms() < 100); // Just started

        let events = track.stop().await.unwrap();
        assert!(events.is_empty()); // No events emitted by MockCapture
    }

    #[tokio::test]
    async fn action_track_skips_denied_captures() {
        let mut track = ActionTrack::new();
        track.register(Box::new(MockCapture::new(
            "granted",
            PermissionStatus::Granted,
        )));
        track.register(Box::new(MockCapture::new(
            "denied",
            PermissionStatus::Denied,
        )));
        track.register(Box::new(MockCapture::new(
            "not_determined",
            PermissionStatus::NotDetermined,
        )));
        track.register(Box::new(MockCapture::new(
            "not_required",
            PermissionStatus::NotRequired,
        )));

        let (tx, _rx) = mpsc::channel(16);
        // Should not error — denied captures are skipped
        track.start(tx).unwrap();
        track.stop().await.unwrap();
    }

    #[tokio::test]
    async fn action_track_inject_event_stores_and_forwards() {
        let mut track = ActionTrack::new();
        let (tx, mut rx) = mpsc::channel(16);
        track.start(tx).unwrap();

        let session_id = Uuid::new_v4();
        let event = make_test_event(session_id);
        track.inject_event(event.clone()).await.unwrap();

        // Event should be forwarded to the channel
        let received = rx.recv().await.unwrap();
        assert_eq!(received.id, event.id);
        assert_eq!(received.session_id, session_id);
        // session_offset_ms should be updated (>= 0)
        // (it's near 0 since we just started)

        let events = track.stop().await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, event.id);
    }

    #[tokio::test]
    async fn action_track_collects_capture_events() {
        let session_id = Uuid::new_v4();
        let test_events = vec![make_test_event(session_id), make_test_event(session_id)];

        let mut track = ActionTrack::new();
        track.register(Box::new(EventEmittingCapture::new(
            "emitter",
            test_events.clone(),
        )));

        let (tx, mut rx) = mpsc::channel(16);
        track.start(tx).unwrap();

        // Wait for events to arrive
        let mut received = Vec::new();
        for _ in 0..2 {
            if let Some(event) =
                tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv())
                    .await
                    .ok()
                    .flatten()
            {
                received.push(event);
            }
        }

        assert_eq!(received.len(), 2);

        let stored = track.stop().await.unwrap();
        assert_eq!(stored.len(), 2);
    }

    #[tokio::test]
    async fn action_track_inject_without_start_still_works() {
        let track = ActionTrack::new();
        let session_id = Uuid::new_v4();
        let event = make_test_event(session_id);

        // inject_event should work even without start (no forwarding, just stores)
        track.inject_event(event).await.unwrap();
    }

    #[test]
    fn action_track_elapsed_zero_before_start() {
        let track = ActionTrack::new();
        assert_eq!(track.elapsed_ms(), 0);
    }
}
