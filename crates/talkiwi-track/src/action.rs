//! ActionTrack — manages action capture lifecycle and event collection.
//!
//! Holds a set of `ActionCapture` implementations, starts/stops them,
//! and provides `inject_event()` for externally-triggered events
//! (screenshots, file drops).

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, Mutex};
use tracing::{info, warn};
use uuid::Uuid;

use talkiwi_core::clock::SessionClock;
use talkiwi_core::event::ActionEvent;
use talkiwi_core::preview::PreviewEvent;
use talkiwi_core::telemetry::{CaptureHealthEntry, CaptureStatus};
use talkiwi_core::traits::capture::{ActionCapture, PermissionStatus};

/// ActionTrack manages the lifecycle of multiple ActionCapture instances
/// and collects their events into a unified stream.
pub struct ActionTrack {
    captures: Vec<Box<dyn ActionCapture>>,
    events: Arc<Mutex<Vec<ActionEvent>>>,
    clock: SessionClock,
    event_tx: Option<mpsc::Sender<ActionEvent>>,
    preview_tx: Option<mpsc::Sender<PreviewEvent>>,
    /// Internal aggregation sender — dropped on stop to signal the aggregator.
    agg_tx: Option<mpsc::Sender<ActionEvent>>,
    /// Aggregator task handle — joined on stop.
    agg_handle: Option<tokio::task::JoinHandle<()>>,
    capture_health: Arc<Mutex<HashMap<String, CaptureHealthEntry>>>,
    /// The active session id. Captures are constructed once with a
    /// placeholder id and never updated, so we rewrite event.session_id
    /// in the aggregator loop to match the active session. Without this,
    /// the db FOREIGN KEY on action_events.session_id fails at save time.
    active_session_id: Option<Uuid>,
}

impl ActionTrack {
    /// Create a new empty ActionTrack.
    pub fn new() -> Self {
        Self::with_clock(SessionClock::new())
    }

    pub fn with_clock(clock: SessionClock) -> Self {
        Self {
            captures: Vec::new(),
            events: Arc::new(Mutex::new(Vec::new())),
            clock,
            event_tx: None,
            preview_tx: None,
            agg_tx: None,
            agg_handle: None,
            capture_health: Arc::new(Mutex::new(HashMap::new())),
            active_session_id: None,
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
    ///
    /// `session_id` is the active session identifier. Captures are registered
    /// once at app init with a placeholder id, so every event that flows
    /// through the aggregator has its `session_id` rewritten to match.
    pub async fn start(
        &mut self,
        session_id: Uuid,
        event_tx: mpsc::Sender<ActionEvent>,
        clock: SessionClock,
        preview_tx: Option<mpsc::Sender<PreviewEvent>>,
    ) -> anyhow::Result<()> {
        self.clock = clock.clone();
        self.event_tx = Some(event_tx.clone());
        self.preview_tx = preview_tx.clone();
        self.active_session_id = Some(session_id);
        self.events.lock().await.clear();
        self.capture_health.lock().await.clear();

        let events = Arc::clone(&self.events);
        let health_state = Arc::clone(&self.capture_health);
        let mut initial_health = Vec::new();

        let (agg_tx, mut agg_rx) = mpsc::channel::<ActionEvent>(256);

        for capture in &mut self.captures {
            let capture_id = capture.id().to_string();
            let permission = capture.check_permission();
            match permission {
                PermissionStatus::Granted | PermissionStatus::NotRequired => {
                    if let Err(error) = capture.start(agg_tx.clone(), clock.clone()) {
                        warn!(
                            capture_id = capture.id(),
                            error = %error,
                            "failed to start capture, skipping"
                        );
                        initial_health.push(CaptureHealthEntry {
                            capture_id,
                            status: CaptureStatus::Error,
                            event_count: 0,
                            last_event_offset_ms: None,
                        });
                    } else {
                        initial_health.push(CaptureHealthEntry {
                            capture_id,
                            status: CaptureStatus::Active,
                            event_count: 0,
                            last_event_offset_ms: None,
                        });
                        info!(capture_id = capture.id(), "started action capture");
                    }
                }
                PermissionStatus::Denied => {
                    initial_health.push(CaptureHealthEntry {
                        capture_id,
                        status: CaptureStatus::PermissionDenied,
                        event_count: 0,
                        last_event_offset_ms: None,
                    });
                }
                PermissionStatus::NotDetermined => {
                    initial_health.push(CaptureHealthEntry {
                        capture_id,
                        status: CaptureStatus::NotStarted,
                        event_count: 0,
                        last_event_offset_ms: None,
                    });
                }
            }
        }

        {
            let mut guard = self.capture_health.lock().await;
            for entry in initial_health {
                guard.insert(entry.capture_id.clone(), entry);
            }
        }

        self.agg_tx = Some(agg_tx);

        let forward_tx = event_tx;
        let preview_tx_clone = preview_tx;
        let aggregator_session_id = session_id;
        let handle = tokio::spawn(async move {
            while let Some(mut event) = agg_rx.recv().await {
                // Rewrite session_id — captures carry a nil placeholder
                // set at registration time; the real active session is
                // only known here, at the aggregator boundary.
                event.session_id = aggregator_session_id;

                events.lock().await.push(event.clone());
                {
                    let mut guard = health_state.lock().await;
                    let entry =
                        guard
                            .entry(event.plugin_id.clone())
                            .or_insert(CaptureHealthEntry {
                                capture_id: event.plugin_id.clone(),
                                status: CaptureStatus::Active,
                                event_count: 0,
                                last_event_offset_ms: None,
                            });
                    entry.status = CaptureStatus::Active;
                    entry.event_count += 1;
                    entry.last_event_offset_ms = Some(event.session_offset_ms);
                }

                let _ = forward_tx.send(event.clone()).await;

                if let Some(preview_tx) = &preview_tx_clone {
                    let _ = preview_tx
                        .send(PreviewEvent::ActionOccurred {
                            id: event.id.to_string(),
                            offset_ms: event.session_offset_ms,
                            action_type: event.action_type.as_str().to_string(),
                            source: event.curation.source,
                        })
                        .await;

                    let health = {
                        let guard = health_state.lock().await;
                        guard.values().cloned().collect::<Vec<_>>()
                    };
                    let _ = preview_tx
                        .send(PreviewEvent::CaptureHealthUpdated(health))
                        .await;
                }
            }
        });
        self.agg_handle = Some(handle);

        if let Some(preview_tx) = &self.preview_tx {
            let snapshot = self.capture_health.lock().await.values().cloned().collect();
            let _ = preview_tx
                .send(PreviewEvent::CaptureHealthUpdated(snapshot))
                .await;
        }

        Ok(())
    }

    /// Stop all captures and return collected events.
    pub async fn stop(&mut self) -> anyhow::Result<Vec<ActionEvent>> {
        let mut captures = std::mem::take(&mut self.captures);
        captures = tokio::task::spawn_blocking(move || {
            for capture in &mut captures {
                if let Err(error) = capture.stop() {
                    warn!(capture_id = capture.id(), error = %error, "error stopping capture");
                }
            }
            captures
        })
        .await?;
        self.captures = captures;

        self.event_tx = None;
        self.preview_tx = None;
        self.agg_tx = None;
        self.active_session_id = None;

        if let Some(handle) = self.agg_handle.take() {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(2), handle).await;
        }

        let mut guard = self.events.lock().await;
        Ok(std::mem::take(&mut *guard))
    }

    /// Inject an externally-triggered event (screenshot, file drop).
    pub async fn inject_event(&self, mut event: ActionEvent) -> anyhow::Result<()> {
        let offset_ms = self.clock.elapsed_ms();
        event.session_offset_ms = offset_ms;
        event.observed_offset_ms = Some(offset_ms);
        // Same session_id normalization as the aggregator — injected
        // events from the commands layer may carry a stale id.
        if let Some(active) = self.active_session_id {
            event.session_id = active;
        }
        self.events.lock().await.push(event.clone());
        self.bump_health(&event.plugin_id, offset_ms).await;

        if let Some(tx) = &self.event_tx {
            tx.send(event.clone()).await.ok();
        }
        if let Some(preview_tx) = &self.preview_tx {
            preview_tx
                .send(PreviewEvent::ActionOccurred {
                    id: event.id.to_string(),
                    offset_ms,
                    action_type: event.action_type.as_str().to_string(),
                    source: event.curation.source,
                })
                .await
                .ok();
        }
        Ok(())
    }

    /// Soft-delete an event by id. Marks `curation.deleted = true` on the
    /// stored copy so downstream (timeline summary, assembler) can skip
    /// it, and emits a `PreviewEvent::ActionRemoved` so the widget pin
    /// disappears from the active session.
    ///
    /// Returns `true` if a matching event was found, `false` otherwise.
    pub async fn soft_delete_event(&self, event_id: Uuid) -> anyhow::Result<bool> {
        let mut guard = self.events.lock().await;
        let mut found = false;
        for event in guard.iter_mut() {
            if event.id == event_id {
                event.curation.deleted = true;
                found = true;
                break;
            }
        }
        drop(guard);

        if found {
            if let Some(preview_tx) = &self.preview_tx {
                preview_tx
                    .send(PreviewEvent::ActionRemoved {
                        id: event_id.to_string(),
                    })
                    .await
                    .ok();
            }
        }
        Ok(found)
    }

    /// Get elapsed time since session start in milliseconds.
    pub fn elapsed_ms(&self) -> u64 {
        self.clock.elapsed_ms()
    }

    pub async fn capture_health(&self) -> Vec<CaptureHealthEntry> {
        let elapsed_ms = self.clock.elapsed_ms();
        let mut entries = self
            .capture_health
            .lock()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for entry in &mut entries {
            if matches!(entry.status, CaptureStatus::Active) {
                let stale = match entry.last_event_offset_ms {
                    Some(last_event) => elapsed_ms.saturating_sub(last_event) > 30_000,
                    None => elapsed_ms > 30_000,
                };
                if stale {
                    entry.status = CaptureStatus::Stale;
                }
            }
        }
        entries.sort_by(|left, right| left.capture_id.cmp(&right.capture_id));
        entries
    }

    /// Get number of registered captures.
    pub fn capture_count(&self) -> usize {
        self.captures.len()
    }

    async fn bump_health(&self, capture_id: &str, offset_ms: u64) {
        let mut guard = self.capture_health.lock().await;
        let entry = guard
            .entry(capture_id.to_string())
            .or_insert(CaptureHealthEntry {
                capture_id: capture_id.to_string(),
                status: CaptureStatus::Active,
                event_count: 0,
                last_event_offset_ms: None,
            });
        entry.status = CaptureStatus::Active;
        entry.event_count += 1;
        entry.last_event_offset_ms = Some(offset_ms);
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

        fn start(
            &mut self,
            _tx: mpsc::Sender<ActionEvent>,
            _clock: SessionClock,
        ) -> anyhow::Result<()> {
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

        fn start(
            &mut self,
            tx: mpsc::Sender<ActionEvent>,
            _clock: SessionClock,
        ) -> anyhow::Result<()> {
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
            observed_offset_ms: Some(0),
            duration_ms: None,
            action_type: ActionType::ClipboardChange,
            plugin_id: "builtin.clipboard".to_string(),
            payload: ActionPayload::ClipboardChange {
                content_type: talkiwi_core::event::ClipboardContentType::Text,
                text: Some("test clipboard".to_string()),
                file_path: None,
                source_app: None,
            },
            semantic_hint: None,
            confidence: 1.0,
            curation: Default::default(),
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
        track
            .start(Uuid::new_v4(), tx, SessionClock::new(), None)
            .await
            .unwrap();

        assert!(track.elapsed_ms() < 100);

        let events = track.stop().await.unwrap();
        assert!(events.is_empty());
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

        let (tx, _rx) = mpsc::channel(16);
        track
            .start(Uuid::new_v4(), tx, SessionClock::new(), None)
            .await
            .unwrap();
        let health = track.capture_health().await;
        assert!(health.iter().any(|entry| entry.capture_id == "granted"));
        assert!(health
            .iter()
            .any(|entry| entry.status == CaptureStatus::PermissionDenied));
        track.stop().await.unwrap();
    }

    #[tokio::test]
    async fn action_track_inject_event_stores_and_forwards() {
        let session_id = Uuid::new_v4();
        let mut track = ActionTrack::new();
        let (tx, mut rx) = mpsc::channel(16);
        track
            .start(session_id, tx, SessionClock::new(), None)
            .await
            .unwrap();

        // Inject an event whose session_id differs from the active one —
        // ActionTrack should rewrite it so downstream sees the active id.
        let stale_session_id = Uuid::new_v4();
        let event = make_test_event(stale_session_id);
        track.inject_event(event.clone()).await.unwrap();

        let received = rx.recv().await.unwrap();
        assert_eq!(received.id, event.id);
        assert_eq!(
            received.session_id, session_id,
            "session_id should be normalized to the active session"
        );

        let events = track.stop().await.unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, event.id);
        assert_eq!(events[0].session_id, session_id);
    }

    #[tokio::test]
    async fn action_track_rewrites_session_id_from_captures() {
        // Captures are registered with a nil placeholder and emit events
        // carrying that stale id. The aggregator must rewrite each event's
        // session_id to match the active session before forwarding.
        let active = Uuid::new_v4();
        let stale = Uuid::nil();
        let test_events = vec![make_test_event(stale), make_test_event(stale)];

        let mut track = ActionTrack::new();
        track.register(Box::new(EventEmittingCapture::new(
            "builtin.clipboard",
            test_events,
        )));

        let (tx, mut rx) = mpsc::channel(16);
        track
            .start(active, tx, SessionClock::new(), None)
            .await
            .unwrap();

        let mut received = Vec::new();
        for _ in 0..2 {
            if let Some(event) =
                tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv()).await
                    .ok()
                    .flatten()
            {
                received.push(event);
            }
        }

        assert_eq!(received.len(), 2);
        for event in &received {
            assert_eq!(event.session_id, active);
        }

        let stored = track.stop().await.unwrap();
        for event in &stored {
            assert_eq!(event.session_id, active);
        }
    }

    #[tokio::test]
    async fn action_track_collects_capture_events() {
        let session_id = Uuid::new_v4();
        let test_events = vec![make_test_event(session_id), make_test_event(session_id)];

        let mut track = ActionTrack::new();
        track.register(Box::new(EventEmittingCapture::new(
            "builtin.clipboard",
            test_events.clone(),
        )));

        let (tx, mut rx) = mpsc::channel(16);
        track
            .start(Uuid::new_v4(), tx, SessionClock::new(), None)
            .await
            .unwrap();

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
    async fn action_track_stop_preserves_captures_for_followup_sessions() {
        let session_id = Uuid::new_v4();
        let test_events = vec![make_test_event(session_id)];

        let mut track = ActionTrack::new();
        track.register(Box::new(EventEmittingCapture::new(
            "builtin.clipboard",
            test_events.clone(),
        )));

        let (tx1, mut rx1) = mpsc::channel(16);
        track
            .start(session_id, tx1, SessionClock::new(), None)
            .await
            .unwrap();
        let first = tokio::time::timeout(std::time::Duration::from_millis(500), rx1.recv())
            .await
            .ok()
            .flatten();
        assert!(first.is_some());
        let _ = track.stop().await.unwrap();

        let (tx2, mut rx2) = mpsc::channel(16);
        track
            .start(session_id, tx2, SessionClock::new(), None)
            .await
            .unwrap();
        let second = tokio::time::timeout(std::time::Duration::from_millis(500), rx2.recv())
            .await
            .ok()
            .flatten();
        assert!(second.is_some());
        let _ = track.stop().await.unwrap();
        assert_eq!(track.capture_count(), 1);
    }

    #[tokio::test]
    async fn action_track_inject_without_start_still_works() {
        let track = ActionTrack::new();
        let event = make_test_event(Uuid::new_v4());
        track.inject_event(event).await.unwrap();
    }

    #[test]
    fn action_track_elapsed_zero_before_start() {
        let track = ActionTrack::new();
        assert!(track.elapsed_ms() < 20);
    }
}
