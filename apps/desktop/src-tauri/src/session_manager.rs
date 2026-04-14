//! SessionManager — top-level orchestrator for Talkiwi sessions.
//!
//! Sits at the dependency graph apex: consumes SpeakTrack, ActionTrack,
//! IntentEngine, and SessionRepo. Lives in the app layer to avoid cycles.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use tokio::sync::mpsc;
use tokio::sync::Mutex;
use tracing::{error, info};
use uuid::Uuid;

use talkiwi_core::clock::SessionClock;
use talkiwi_core::error::TalkiwiError;
use talkiwi_core::event::ActionEvent;
use talkiwi_core::output::IntentOutput;
use talkiwi_core::preview::PreviewEvent;
use talkiwi_core::session::{Session, SessionState, SpeakSegment};
use talkiwi_core::telemetry::{CaptureHealthEntry, TraceTelemetry};
use talkiwi_db::SessionRepo;
use talkiwi_engine::IntentEngine;
use talkiwi_track::{ActionTrack, SpeakTrack};

/// SessionManager orchestrates the full session lifecycle:
/// start recording → collect speech + actions → stop → process → persist.
pub struct SessionManager {
    state: Mutex<SessionState>,
    current_session: Mutex<Option<Session>>,
    speak_track: Mutex<SpeakTrack>,
    action_track: Mutex<ActionTrack>,
    engine: IntentEngine,
    /// Shared DB connection — same Arc as AppState.db.
    /// Uses `std::sync::Mutex` because `Connection` is `!Send`.
    /// All DB operations run in `spawn_blocking` to avoid blocking the executor.
    db: Arc<std::sync::Mutex<Connection>>,
    current_clock: Mutex<Option<SessionClock>>,
    preview_tx: Mutex<Option<mpsc::Sender<PreviewEvent>>>,
}

impl SessionManager {
    pub fn new(
        speak_track: SpeakTrack,
        action_track: ActionTrack,
        engine: IntentEngine,
        db: Arc<std::sync::Mutex<Connection>>,
    ) -> Self {
        Self {
            state: Mutex::new(SessionState::Idle),
            current_session: Mutex::new(None),
            speak_track: Mutex::new(speak_track),
            action_track: Mutex::new(action_track),
            engine,
            db,
            current_clock: Mutex::new(None),
            preview_tx: Mutex::new(None),
        }
    }

    /// Start a new recording session.
    ///
    /// Returns the session ID. Speak segments and action events are forwarded
    /// to the provided senders for real-time streaming to the frontend.
    /// When `output_dir` is provided, audio is recorded to a WAV file in
    /// `<output_dir>/sessions/<session_id>/audio.wav`.
    pub async fn start(
        &self,
        speak_tx: mpsc::Sender<SpeakSegment>,
        action_tx: mpsc::Sender<ActionEvent>,
        preview_tx: Option<mpsc::Sender<PreviewEvent>>,
        clock: SessionClock,
        asr_provider: Box<dyn talkiwi_core::traits::asr::AsrProvider>,
        output_dir: Option<PathBuf>,
        input_gain_db: f32,
    ) -> Result<Uuid, TalkiwiError> {
        // Atomic check-and-set to prevent TOCTOU race
        {
            let mut state = self.state.lock().await;
            if *state == SessionState::Recording {
                return Err(TalkiwiError::AlreadyRecording);
            }
            *state = SessionState::Recording;
        }

        let session_id = Uuid::new_v4();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let session = Session {
            id: session_id,
            state: SessionState::Recording,
            started_at: Some(now),
            ended_at: None,
            duration_ms: None,
        };

        *self.current_session.lock().await = Some(session);
        *self.current_clock.lock().await = Some(clock.clone());
        *self.preview_tx.lock().await = preview_tx.clone();

        // Prepare audio recording directory
        let audio_dir = output_dir.map(|dir| {
            let session_dir = dir.join("sessions").join(session_id.to_string());
            if let Err(e) = std::fs::create_dir_all(&session_dir) {
                tracing::warn!(error = %e, "failed to create session dir");
            }
            session_dir
        });

        // Start SpeakTrack
        {
            let mut speak = self.speak_track.lock().await;
            if let Err(error) = speak
                .start(
                    speak_tx,
                    preview_tx.clone(),
                    asr_provider,
                    audio_dir,
                    input_gain_db,
                )
                .await
            {
                self.reset_after_start_failure().await;
                return Err(TalkiwiError::AsrFailed(error.to_string()));
            }
        }

        // Start ActionTrack (synchronous — does not block executor)
        {
            let mut action = self.action_track.lock().await;
            if let Err(error) = action.start(action_tx, clock, preview_tx).await {
                self.reset_after_start_failure().await;
                return Err(TalkiwiError::CaptureFailed {
                    module: "action_track".into(),
                    reason: error.to_string(),
                });
            }
        }

        self.emit_preview_state(SessionState::Recording).await;

        info!(session_id = %session_id, "session started");
        Ok(session_id)
    }

    /// Stop the current session, process through the engine, and persist.
    pub async fn stop(&self) -> Result<IntentOutput, TalkiwiError> {
        // Atomic check-and-set to prevent TOCTOU race
        {
            let mut state = self.state.lock().await;
            if *state != SessionState::Recording {
                return Err(TalkiwiError::NoActiveSession);
            }
            *state = SessionState::Processing;
        }
        self.emit_preview_state(SessionState::Processing).await;
        info!("session stopping, state → Processing");

        // Stop both tracks
        let speak_result = {
            let mut speak = self.speak_track.lock().await;
            speak
                .stop()
                .await
                .map_err(|e| TalkiwiError::AsrFailed(e.to_string()))?
        };
        let segments = speak_result.segments;
        let audio_path = speak_result.audio_path;

        let (capture_health, events) = {
            let mut action = self.action_track.lock().await;
            let capture_health = action.capture_health().await;
            let events = action
                .stop()
                .await
                .map_err(|e| TalkiwiError::CaptureFailed {
                    module: "action_track".into(),
                    reason: e.to_string(),
                })?;
            (capture_health, events)
        };

        // Get session info
        let session_id = {
            let session_guard = self.current_session.lock().await;
            session_guard
                .as_ref()
                .ok_or(TalkiwiError::NoActiveSession)?
                .id
        };

        // Process through engine
        let (output, intent_telemetry) = self
            .engine
            .process_with_telemetry(&segments, &events, session_id)
            .await
            .map_err(|e| TalkiwiError::IntentFailed(e.to_string()))?;

        // Finalize session
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let session = {
            let mut session_guard = self.current_session.lock().await;
            let mut session = session_guard.take().ok_or(TalkiwiError::NoActiveSession)?;
            session.state = SessionState::Ready;
            session.ended_at = Some(now);
            session.duration_ms = session.started_at.map(|start| now.saturating_sub(start));
            session
        };

        let trace_telemetry = build_trace_telemetry(
            session.id,
            session.duration_ms.unwrap_or_default(),
            &segments,
            &events,
            capture_health.clone(),
        );

        // Persist to DB via spawn_blocking (Connection is !Send).
        // Single-user app: blocking pool exhaustion is not a concern.
        let db = Arc::clone(&self.db);
        let session_c = session.clone();
        let output_c = output.clone();
        let segments_c = segments;
        let events_c = events;
        let audio_path_str = audio_path.map(|p| p.to_string_lossy().to_string());
        let db_result: Result<(), TalkiwiError> =
            tokio::task::spawn_blocking(move || -> Result<(), TalkiwiError> {
                let db = db
                    .lock()
                    .map_err(|e| TalkiwiError::Storage(format!("DB lock poisoned: {}", e)))?;
                let repo = SessionRepo::new(&db);
                repo.save_session_with_audio(
                    &session_c,
                    &output_c,
                    &segments_c,
                    &events_c,
                    audio_path_str.as_deref(),
                )?;
                repo.save_intent_telemetry(&intent_telemetry)?;
                repo.save_trace_telemetry(&trace_telemetry)?;
                Ok(())
            })
            .await
            .map_err(|e| TalkiwiError::Storage(format!("spawn_blocking failed: {}", e)))?;

        if let Err(e) = db_result {
            error!(error = %e, "failed to persist session");
            *self.state.lock().await = SessionState::Error(e.to_string());
            self.emit_preview_state(SessionState::Error(e.to_string()))
                .await;
            return Err(e);
        }

        *self.state.lock().await = SessionState::Ready;
        *self.current_clock.lock().await = None;
        *self.preview_tx.lock().await = None;
        self.emit_preview_state(SessionState::Ready).await;
        info!(session_id = %session_id, "session completed successfully");

        Ok(output)
    }

    /// Get current session state.
    pub async fn state(&self) -> SessionState {
        self.state.lock().await.clone()
    }

    /// Inject an external event into the current session's ActionTrack.
    pub async fn inject_event(&self, event: ActionEvent) -> Result<(), TalkiwiError> {
        let action = self.action_track.lock().await;
        action
            .inject_event(event)
            .await
            .map_err(|e| TalkiwiError::CaptureFailed {
                module: "inject".into(),
                reason: e.to_string(),
            })
    }

    /// Get the current session ID, if any.
    pub async fn current_session_id(&self) -> Option<Uuid> {
        self.current_session.lock().await.as_ref().map(|s| s.id)
    }

    /// Re-process edited segments and events through the intent engine.
    pub async fn regenerate(
        &self,
        segments: &[SpeakSegment],
        events: &[ActionEvent],
        session_id: Uuid,
    ) -> Result<IntentOutput, TalkiwiError> {
        self.engine
            .process(segments, events, session_id)
            .await
            .map_err(|e| TalkiwiError::IntentFailed(e.to_string()))
    }

    /// Get elapsed time since session start in milliseconds.
    pub async fn elapsed_ms(&self) -> u64 {
        self.current_clock
            .lock()
            .await
            .as_ref()
            .map(SessionClock::elapsed_ms)
            .unwrap_or(0)
    }

    async fn emit_preview_state(&self, state: SessionState) {
        if let Some(tx) = self.preview_tx.lock().await.as_ref() {
            let _ = tx.send(PreviewEvent::SessionStateChanged(state)).await;
        }
    }

    async fn reset_after_start_failure(&self) {
        *self.state.lock().await = SessionState::Idle;
        *self.current_session.lock().await = None;
        *self.current_clock.lock().await = None;
        *self.preview_tx.lock().await = None;
    }
}

fn build_trace_telemetry(
    session_id: Uuid,
    duration_ms: u64,
    segments: &[SpeakSegment],
    events: &[ActionEvent],
    capture_health: Vec<CaptureHealthEntry>,
) -> TraceTelemetry {
    let duration_secs = (duration_ms as f32 / 1000.0).max(0.001);
    let alignment_anomalies = events
        .windows(2)
        .filter(|window| window[0].session_offset_ms > window[1].session_offset_ms)
        .count();

    TraceTelemetry {
        session_id,
        duration_ms,
        segment_count: segments.len(),
        event_count: events.len(),
        capture_health,
        event_density: events.len() as f32 / duration_secs,
        alignment_anomalies,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use talkiwi_core::clock::SessionClock;
    use talkiwi_core::event::{ActionPayload, ActionType};
    use talkiwi_core::traits::asr::{AsrProvider, AudioChunk};
    use talkiwi_engine::IntentRaw;

    struct MockAsrProvider;

    #[async_trait::async_trait]
    impl AsrProvider for MockAsrProvider {
        fn id(&self) -> &str {
            "mock"
        }
        fn name(&self) -> &str {
            "Mock ASR"
        }
        fn requires_network(&self) -> bool {
            false
        }
        async fn is_available(&self) -> bool {
            true
        }
        async fn transcribe_stream(
            &self,
            mut audio_rx: mpsc::Receiver<AudioChunk>,
            segment_tx: mpsc::Sender<SpeakSegment>,
        ) -> anyhow::Result<()> {
            while let Some(chunk) = audio_rx.recv().await {
                let seg = SpeakSegment {
                    text: format!("chunk at {}ms", chunk.offset_ms),
                    start_ms: chunk.offset_ms,
                    end_ms: chunk.offset_ms + 100,
                    confidence: 0.95,
                    is_final: true,
                };
                let _ = segment_tx.send(seg).await;
            }
            Ok(())
        }
    }

    struct MockIntentProvider;

    #[async_trait::async_trait]
    impl talkiwi_engine::IntentProvider for MockIntentProvider {
        fn id(&self) -> &str {
            "mock"
        }
        fn name(&self) -> &str {
            "Mock Intent"
        }
        fn requires_network(&self) -> bool {
            false
        }
        async fn is_available(&self) -> bool {
            true
        }
        async fn restructure(
            &self,
            _transcript: &str,
            _events_summary: &str,
            _system_prompt: &str,
        ) -> anyhow::Result<IntentRaw> {
            Ok(IntentRaw {
                task: "Test task".to_string(),
                intent: "analyze".to_string(),
                constraints: vec![],
                missing_context: vec![],
                restructured_speech: "Test restructured".to_string(),
                references: vec![],
            })
        }
    }

    /// Mock audio source that immediately closes (no audio).
    struct EmptyAudioSource;

    #[async_trait::async_trait]
    impl talkiwi_asr::AudioSource for EmptyAudioSource {
        async fn start(&mut self, _tx: mpsc::Sender<AudioChunk>) -> anyhow::Result<()> {
            Ok(())
        }
        async fn stop(&mut self) -> anyhow::Result<()> {
            Ok(())
        }
    }

    fn make_session_manager() -> SessionManager {
        let speak_track = SpeakTrack::new(Box::new(EmptyAudioSource));
        let action_track = ActionTrack::new();
        let engine = IntentEngine::new(Box::new(MockIntentProvider), None);
        let db = talkiwi_db::init_database_memory().unwrap();
        SessionManager::new(
            speak_track,
            action_track,
            engine,
            Arc::new(std::sync::Mutex::new(db)),
        )
    }

    #[tokio::test]
    async fn initial_state_is_idle() {
        let sm = make_session_manager();
        assert_eq!(sm.state().await, SessionState::Idle);
        assert!(sm.current_session_id().await.is_none());
    }

    #[tokio::test]
    async fn start_transitions_to_recording() {
        let sm = make_session_manager();
        let (speak_tx, _speak_rx) = mpsc::channel(16);
        let (action_tx, _action_rx) = mpsc::channel(16);

        let session_id = sm
            .start(
                speak_tx,
                action_tx,
                None,
                SessionClock::new(),
                Box::new(MockAsrProvider),
                None,
                0.0,
            )
            .await
            .unwrap();

        assert_eq!(sm.state().await, SessionState::Recording);
        assert_eq!(sm.current_session_id().await, Some(session_id));
    }

    #[tokio::test]
    async fn double_start_returns_error() {
        let sm = make_session_manager();
        let (tx1, _) = mpsc::channel(16);
        let (tx2, _) = mpsc::channel(16);
        sm.start(
            tx1,
            tx2,
            None,
            SessionClock::new(),
            Box::new(MockAsrProvider),
            None,
            0.0,
        )
        .await
        .unwrap();

        let (tx3, _) = mpsc::channel(16);
        let (tx4, _) = mpsc::channel(16);
        let result = sm
            .start(
                tx3,
                tx4,
                None,
                SessionClock::new(),
                Box::new(MockAsrProvider),
                None,
                0.0,
            )
            .await;
        assert!(matches!(result, Err(TalkiwiError::AlreadyRecording)));
    }

    #[tokio::test]
    async fn stop_without_start_returns_error() {
        let sm = make_session_manager();
        let result = sm.stop().await;
        assert!(matches!(result, Err(TalkiwiError::NoActiveSession)));
    }

    #[tokio::test]
    async fn full_lifecycle_start_stop() {
        let sm = make_session_manager();
        let (speak_tx, _speak_rx) = mpsc::channel(16);
        let (action_tx, _action_rx) = mpsc::channel(16);

        let session_id = sm
            .start(
                speak_tx,
                action_tx,
                None,
                SessionClock::new(),
                Box::new(MockAsrProvider),
                None,
                0.0,
            )
            .await
            .unwrap();

        let output = sm.stop().await.unwrap();

        assert_eq!(sm.state().await, SessionState::Ready);
        assert_eq!(output.session_id, session_id);
        assert!(sm.current_session_id().await.is_none());
    }

    #[tokio::test]
    async fn inject_event_during_recording() {
        let sm = make_session_manager();
        let (speak_tx, _speak_rx) = mpsc::channel(16);
        let (action_tx, mut action_rx) = mpsc::channel(16);

        let session_id = sm
            .start(
                speak_tx,
                action_tx,
                None,
                SessionClock::new(),
                Box::new(MockAsrProvider),
                None,
                0.0,
            )
            .await
            .unwrap();

        let event = ActionEvent {
            id: Uuid::new_v4(),
            session_id,
            timestamp: 0,
            session_offset_ms: 0,
            observed_offset_ms: Some(0),
            duration_ms: None,
            action_type: ActionType::Screenshot,
            plugin_id: "builtin".into(),
            payload: ActionPayload::Screenshot {
                image_path: "/tmp/test.png".into(),
                width: 1920,
                height: 1080,
                ocr_text: None,
            },
            semantic_hint: None,
            confidence: 1.0,
        };

        sm.inject_event(event.clone()).await.unwrap();

        // Event should be forwarded
        let received =
            tokio::time::timeout(std::time::Duration::from_millis(500), action_rx.recv())
                .await
                .ok()
                .flatten();
        assert!(received.is_some());

        let output = sm.stop().await.unwrap();
        assert_eq!(output.session_id, session_id);
    }

    #[tokio::test]
    async fn stop_persists_to_db() {
        let sm = make_session_manager();
        let (speak_tx, _) = mpsc::channel(16);
        let (action_tx, _) = mpsc::channel(16);

        let session_id = sm
            .start(
                speak_tx,
                action_tx,
                None,
                SessionClock::new(),
                Box::new(MockAsrProvider),
                None,
                0.0,
            )
            .await
            .unwrap();

        sm.stop().await.unwrap();

        // Verify persisted in DB
        let db = sm.db.lock().unwrap();
        let repo = SessionRepo::new(&db);
        let detail = repo.get_session_detail(&session_id.to_string()).unwrap();
        assert_eq!(detail.session.id, session_id);
        assert_eq!(detail.output.task, "Test task");
    }
}
