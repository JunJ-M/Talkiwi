//! E2E integration test: validates the full mock pipeline.
//!
//! Flow: MockAudioSource → MockAsrProvider → SpeakTrack
//!       MockActionCapture → ActionTrack + inject_event
//!       → align_timeline → timeline_to_summary
//!
//! No hardware dependencies — purely mock-driven.

use tokio::sync::mpsc;
use uuid::Uuid;

use talkiwi_asr::AudioSource;
use talkiwi_core::event::{ActionEvent, ActionPayload, ActionType, ClipboardContentType};
use talkiwi_core::session::SpeakSegment;
use talkiwi_core::timeline::{align_timeline, timeline_to_summary, TimelineEntry};
use talkiwi_core::traits::asr::{AsrProvider, AudioChunk};
use talkiwi_core::traits::capture::{ActionCapture, PermissionStatus};
use talkiwi_track::{ActionTrack, SpeakTrack};

// ── Mock implementations ──────────────────────────────────────────

struct MockAudioSource {
    chunks: Vec<AudioChunk>,
}

impl MockAudioSource {
    fn new(chunks: Vec<AudioChunk>) -> Self {
        Self { chunks }
    }
}

#[async_trait::async_trait]
impl AudioSource for MockAudioSource {
    async fn start(&mut self, tx: mpsc::Sender<AudioChunk>) -> anyhow::Result<()> {
        let chunks = self.chunks.clone();
        tokio::spawn(async move {
            for chunk in chunks {
                if tx.send(chunk).await.is_err() {
                    break;
                }
            }
        });
        Ok(())
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}

struct MockAsrProvider;

#[async_trait::async_trait]
impl AsrProvider for MockAsrProvider {
    fn id(&self) -> &str {
        "mock-asr"
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
        let mut offset = 0u64;
        while let Some(chunk) = audio_rx.recv().await {
            let duration_ms = (chunk.samples.len() as u64 * 1000) / chunk.sample_rate as u64;
            let segment = SpeakSegment {
                text: format!("帮我重写这段代码"),
                start_ms: offset,
                end_ms: offset + duration_ms,
                confidence: 0.92,
                is_final: true,
            };
            offset += duration_ms;
            if segment_tx.send(segment).await.is_err() {
                break;
            }
        }
        Ok(())
    }
}

/// Mock capture that emits clipboard events on a timer.
struct MockClipboardCapture {
    session_id: Uuid,
}

impl ActionCapture for MockClipboardCapture {
    fn id(&self) -> &str {
        "mock-clipboard"
    }
    fn action_types(&self) -> &[ActionType] {
        &[ActionType::ClipboardChange]
    }

    fn start(&mut self, tx: mpsc::Sender<ActionEvent>) -> anyhow::Result<()> {
        let session_id = self.session_id;
        tokio::spawn(async move {
            // Simulate clipboard change at ~150ms
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let event = ActionEvent {
                id: Uuid::new_v4(),
                session_id,
                timestamp: 1712900000150,
                session_offset_ms: 150,
                duration_ms: None,
                action_type: ActionType::ClipboardChange,
                plugin_id: "builtin".to_string(),
                payload: ActionPayload::ClipboardChange {
                    content_type: ClipboardContentType::Text,
                    text: Some("fn main() { println!(\"hello\"); }".to_string()),
                    file_path: None,
                    source_app: Some("VSCode".to_string()),
                },
                semantic_hint: Some("code snippet".to_string()),
                confidence: 1.0,
            };
            let _ = tx.send(event).await;
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

// ── Tests ─────────────────────────────────────────────────────────

#[tokio::test]
async fn e2e_full_pipeline_mock() {
    let session_id = Uuid::new_v4();

    // ── 1. Setup SpeakTrack with mock audio ──
    let audio_chunks: Vec<AudioChunk> = (0..3)
        .map(|i| AudioChunk {
            samples: vec![0.1; 1600], // 100ms at 16kHz
            offset_ms: i * 100,
            sample_rate: 16000,
        })
        .collect();

    let audio_source = MockAudioSource::new(audio_chunks);
    let mut speak_track = SpeakTrack::new(Box::new(audio_source));

    // ── 2. Setup ActionTrack with mock clipboard ──
    let mut action_track = ActionTrack::new();
    action_track.register(Box::new(MockClipboardCapture { session_id }));

    // ── 3. Start both tracks ──
    let (speak_tx, mut speak_rx) = mpsc::channel::<SpeakSegment>(16);
    let (action_tx, mut action_rx) = mpsc::channel::<ActionEvent>(16);

    speak_track
        .start(speak_tx, Box::new(MockAsrProvider), None, 0.0)
        .await
        .unwrap();
    action_track.start(action_tx).unwrap();

    // ── 4. Inject a file-drop event (simulating user drag-drop) ──
    let file_event = ActionEvent {
        id: Uuid::new_v4(),
        session_id,
        timestamp: 1712900000300,
        session_offset_ms: 0, // will be updated by inject
        duration_ms: None,
        action_type: ActionType::FileAttach,
        plugin_id: "builtin".to_string(),
        payload: ActionPayload::FileAttach {
            file_path: "/tmp/test.rs".to_string(),
            file_name: "test.rs".to_string(),
            file_size: 256,
            mime_type: "text/x-rust".to_string(),
            preview: Some("fn main() {}".to_string()),
        },
        semantic_hint: None,
        confidence: 1.0,
    };
    action_track.inject_event(file_event).await.unwrap();

    // ── 5. Collect results with timeout ──
    let mut speak_segments = Vec::new();
    let mut action_events = Vec::new();

    // Collect speak segments
    for _ in 0..3 {
        if let Ok(Some(seg)) =
            tokio::time::timeout(std::time::Duration::from_millis(500), speak_rx.recv()).await
        {
            speak_segments.push(seg);
        }
    }

    // Collect action events
    for _ in 0..2 {
        if let Ok(Some(evt)) =
            tokio::time::timeout(std::time::Duration::from_millis(500), action_rx.recv()).await
        {
            action_events.push(evt);
        }
    }

    // ── 6. Stop tracks ──
    let speak_result = speak_track.stop().await.unwrap();
    let stored_segments = speak_result.segments;
    let stored_events = action_track.stop().await.unwrap();

    // ── 7. Validate speak segments ──
    assert!(
        speak_segments.len() >= 2,
        "Expected at least 2 speak segments, got {}",
        speak_segments.len()
    );
    assert!(speak_segments[0].text.contains("重写"));
    assert!(speak_segments[0].is_final);

    // ── 8. Validate action events ──
    // We should have at least the injected file event
    assert!(
        !stored_events.is_empty(),
        "Expected at least 1 stored action event"
    );

    // Find the file attach event
    let file_events: Vec<_> = stored_events
        .iter()
        .filter(|e| e.action_type == ActionType::FileAttach)
        .collect();
    assert_eq!(file_events.len(), 1, "Expected exactly 1 FileAttach event");

    // ── 9. Timeline alignment ──
    let timeline = align_timeline(&stored_segments, &stored_events);
    assert!(!timeline.is_empty(), "Timeline should not be empty");

    // Verify timeline is sorted by time
    let times: Vec<u64> = timeline.iter().map(|e| e.start_ms()).collect();
    for window in times.windows(2) {
        assert!(
            window[0] <= window[1],
            "Timeline not sorted: {} > {}",
            window[0],
            window[1]
        );
    }

    // ── 10. Timeline summary ──
    let summary = timeline_to_summary(&timeline);
    assert!(!summary.is_empty());

    // Should contain both SPEAK and ACTION entries
    let has_speak = timeline
        .iter()
        .any(|e| matches!(e, TimelineEntry::Speak(_)));
    let has_action = timeline
        .iter()
        .any(|e| matches!(e, TimelineEntry::Action(_)));
    assert!(has_speak, "Timeline should contain speak entries");
    assert!(has_action, "Timeline should contain action entries");

    println!("=== E2E Pipeline Results ===");
    println!("Speak segments: {}", stored_segments.len());
    println!("Action events: {}", stored_events.len());
    println!("Timeline entries: {}", timeline.len());
    println!("---");
    println!("{}", summary);
}

#[tokio::test]
async fn e2e_empty_session() {
    // Test edge case: start and immediately stop with no data
    let audio_source = MockAudioSource::new(vec![]);
    let mut speak_track = SpeakTrack::new(Box::new(audio_source));
    let mut action_track = ActionTrack::new();

    let (speak_tx, _speak_rx) = mpsc::channel(16);
    let (action_tx, _action_rx) = mpsc::channel(16);

    speak_track
        .start(speak_tx, Box::new(MockAsrProvider), None, 0.0)
        .await
        .unwrap();
    action_track.start(action_tx).unwrap();

    let speak_result = speak_track.stop().await.unwrap();
    let events = action_track.stop().await.unwrap();

    assert!(speak_result.segments.is_empty());
    assert!(events.is_empty());

    let timeline = align_timeline(&speak_result.segments, &events);
    assert!(timeline.is_empty());
}

#[tokio::test]
async fn e2e_only_speech_no_actions() {
    let chunks: Vec<AudioChunk> = (0..5)
        .map(|i| AudioChunk {
            samples: vec![0.1; 1600],
            offset_ms: i * 100,
            sample_rate: 16000,
        })
        .collect();

    let audio_source = MockAudioSource::new(chunks);
    let mut speak_track = SpeakTrack::new(Box::new(audio_source));

    let (speak_tx, _rx) = mpsc::channel(16);
    speak_track
        .start(speak_tx, Box::new(MockAsrProvider), None, 0.0)
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let speak_result = speak_track.stop().await.unwrap();
    let segments = speak_result.segments;
    assert!(!segments.is_empty());

    let timeline = align_timeline(&segments, &[]);
    assert_eq!(timeline.len(), segments.len());
    assert!(timeline
        .iter()
        .all(|e| matches!(e, TimelineEntry::Speak(_))));
}
