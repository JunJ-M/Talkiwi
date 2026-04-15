use std::collections::VecDeque;
use std::sync::Arc;

use tauri::{AppHandle, Emitter};
use tokio::sync::{mpsc, Mutex};

use talkiwi_core::clock::SessionClock;
use talkiwi_core::preview::{
    AudioInputInfo, PreviewEvent, WidgetActionPin, WidgetHealthState, WidgetSnapshot,
    WidgetTranscriptState,
};
use talkiwi_core::session::{SessionState, SpeakSegment};
use talkiwi_core::telemetry::{CaptureHealthEntry, CaptureStatus};

const SNAPSHOT_INTERVAL_MS: u64 = 100;
const WINDOW_MS: u64 = 30_000;
const BIN_MS: u64 = 250;
const BIN_COUNT: usize = (WINDOW_MS / BIN_MS) as usize;
const ACTION_MERGE_MS: u64 = 500;

#[derive(Clone)]
pub struct WidgetPreviewHub {
    app_handle: AppHandle,
    session: Arc<Mutex<Option<PreviewSessionHandle>>>,
}

struct PreviewSessionHandle {
    tx: mpsc::Sender<PreviewEvent>,
    task: tokio::task::JoinHandle<()>,
}

impl WidgetPreviewHub {
    pub fn new(app_handle: AppHandle) -> Self {
        Self {
            app_handle,
            session: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn start_session(
        &self,
        clock: SessionClock,
        mic: Option<AudioInputInfo>,
    ) -> mpsc::Sender<PreviewEvent> {
        self.reset().await;

        let (tx, rx) = mpsc::channel(512);
        let app_handle = self.app_handle.clone();
        let task = tokio::spawn(async move {
            run_preview_loop(app_handle, clock, mic, rx).await;
        });

        *self.session.lock().await = Some(PreviewSessionHandle {
            tx: tx.clone(),
            task,
        });

        let _ = tx
            .send(PreviewEvent::SessionStateChanged(SessionState::Recording))
            .await;
        tx
    }

    pub async fn reset(&self) {
        if let Some(handle) = self.session.lock().await.take() {
            drop(handle.tx);
            handle.task.abort();
        }
    }
}

struct PreviewState {
    session_state: SessionState,
    mic: Option<AudioInputInfo>,
    audio_levels: VecDeque<(u64, f32, f32)>,
    action_pins: VecDeque<WidgetActionPin>,
    final_segments: VecDeque<SpeakSegment>,
    partial_text: Option<String>,
    capture_status: Vec<CaptureHealthEntry>,
    frozen_elapsed_ms: Option<u64>,
}

impl PreviewState {
    fn new(mic: Option<AudioInputInfo>) -> Self {
        Self {
            session_state: SessionState::Idle,
            mic,
            audio_levels: VecDeque::new(),
            action_pins: VecDeque::new(),
            final_segments: VecDeque::new(),
            partial_text: None,
            capture_status: Vec::new(),
            frozen_elapsed_ms: None,
        }
    }

    fn apply(&mut self, event: PreviewEvent, elapsed_ms: u64) {
        match event {
            PreviewEvent::SessionStateChanged(state) => {
                if matches!(state, SessionState::Recording) {
                    self.frozen_elapsed_ms = None;
                } else if self.frozen_elapsed_ms.is_none()
                    && matches!(self.session_state, SessionState::Recording)
                {
                    self.frozen_elapsed_ms = Some(elapsed_ms);
                }
                self.session_state = state;
            }
            PreviewEvent::MicSelected(mic) => {
                self.mic = mic;
            }
            PreviewEvent::AudioLevel {
                offset_ms,
                rms,
                peak,
                vad_active,
            } => {
                self.audio_levels.push_back((
                    offset_ms,
                    peak.max(rms),
                    if vad_active { 1.0 } else { 0.0 },
                ));
            }
            PreviewEvent::TranscriptPartial { text, .. } => {
                self.partial_text = Some(text);
            }
            PreviewEvent::TranscriptFinal(segment) => {
                self.partial_text = None;
                self.final_segments.push_back(segment);
                while self.final_segments.len() > 4 {
                    self.final_segments.pop_front();
                }
            }
            PreviewEvent::ActionOccurred {
                id,
                offset_ms,
                action_type,
                source,
            } => {
                if let Some(last) = self.action_pins.back_mut() {
                    if last.action_type == action_type
                        && last.source == source
                        && offset_ms.saturating_sub(last.t) <= ACTION_MERGE_MS
                    {
                        last.count = Some(last.count.unwrap_or(1) + 1);
                        return;
                    }
                }

                self.action_pins.push_back(WidgetActionPin {
                    id,
                    t: offset_ms,
                    action_type,
                    count: None,
                    source,
                });
            }
            PreviewEvent::ActionRemoved { id } => {
                self.action_pins.retain(|pin| pin.id != id);
            }
            PreviewEvent::CaptureHealthUpdated(entries) => {
                self.capture_status = entries;
            }
            PreviewEvent::Reset => {
                self.audio_levels.clear();
                self.action_pins.clear();
                self.final_segments.clear();
                self.partial_text = None;
                self.capture_status.clear();
                self.session_state = SessionState::Idle;
                self.frozen_elapsed_ms = None;
            }
        }
    }

    fn build_snapshot(&mut self, elapsed_ms: u64) -> WidgetSnapshot {
        let effective_elapsed_ms = self.frozen_elapsed_ms.unwrap_or(elapsed_ms);
        let window_start = effective_elapsed_ms.saturating_sub(WINDOW_MS);

        while let Some((offset_ms, _, _)) = self.audio_levels.front() {
            if *offset_ms >= window_start {
                break;
            }
            self.audio_levels.pop_front();
        }
        while let Some(pin) = self.action_pins.front() {
            if pin.t >= window_start {
                break;
            }
            self.action_pins.pop_front();
        }
        while let Some(segment) = self.final_segments.front() {
            if segment.end_ms >= window_start {
                break;
            }
            self.final_segments.pop_front();
        }

        // Bins are indexed by absolute time within the rolling window:
        //
        //   bin[0]           = samples in [window_start, window_start + BIN_MS)
        //   bin[1]           = samples in [window_start + BIN_MS, window_start + 2*BIN_MS)
        //   ...
        //   bin[BIN_COUNT-1] = samples in [window_end - BIN_MS, window_end)
        //
        // For elapsed < WINDOW_MS:
        //   window_start = 0, so bin[i] = time [i*BIN_MS, (i+1)*BIN_MS).
        //   Only bins 0..(elapsed/BIN_MS) are populated; higher bins stay 0.
        //   The frontend positions bar i at `i / BIN_COUNT * 100%` of the
        //   timeline, so the populated bars appear on the LEFT and grow
        //   toward the right as time progresses, with the playhead at
        //   `elapsed / WINDOW_MS * 100%` tracking the newest bar.
        //
        // For elapsed >= WINDOW_MS:
        //   window_start slides forward, older samples drop out via the
        //   pruning loop above, and all BIN_COUNT slots are eventually
        //   populated (steady-state sliding window).
        let mut audio_bins = vec![0.0_f32; BIN_COUNT];
        let mut speech_bins = vec![0.0_f32; BIN_COUNT];
        for (offset_ms, level, speech) in &self.audio_levels {
            if *offset_ms < window_start {
                continue;
            }
            let idx = ((*offset_ms - window_start) / BIN_MS) as usize;
            if idx >= BIN_COUNT {
                continue;
            }
            audio_bins[idx] = audio_bins[idx].max(*level);
            speech_bins[idx] = speech_bins[idx].max(*speech);
        }

        let degraded = self.capture_status.iter().any(|entry| {
            matches!(
                entry.status,
                CaptureStatus::PermissionDenied | CaptureStatus::Stale | CaptureStatus::Error
            )
        });

        WidgetSnapshot {
            session_state: self.session_state.clone(),
            elapsed_ms: effective_elapsed_ms,
            mic: self.mic.clone(),
            audio_bins,
            speech_bins,
            action_pins: self.action_pins.iter().cloned().collect(),
            transcript: WidgetTranscriptState {
                partial_text: self.partial_text.clone(),
                final_segments: self.final_segments.iter().cloned().collect(),
            },
            health: WidgetHealthState {
                capture_status: self.capture_status.clone(),
                degraded,
            },
        }
    }
}

async fn run_preview_loop(
    app_handle: AppHandle,
    clock: SessionClock,
    mic: Option<AudioInputInfo>,
    mut rx: mpsc::Receiver<PreviewEvent>,
) {
    let mut interval =
        tokio::time::interval(std::time::Duration::from_millis(SNAPSHOT_INTERVAL_MS));
    let mut state = PreviewState::new(mic);

    loop {
        tokio::select! {
            maybe_event = rx.recv() => {
                match maybe_event {
                    Some(event) => {
                        if let PreviewEvent::SessionStateChanged(session_state) = &event {
                            let _ = app_handle.emit("talkiwi://session-state", session_state);
                        }
                        state.apply(event, clock.elapsed_ms());
                    }
                    None => break,
                }
            }
            _ = interval.tick() => {
                let snapshot = state.build_snapshot(clock.elapsed_ms());
                let _ = app_handle.emit("talkiwi://widget-snapshot", &snapshot);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_state_merges_actions_and_bounds_segments() {
        use talkiwi_core::event::TraceSource;

        let mut state = PreviewState::new(None);
        state.apply(PreviewEvent::ActionOccurred {
            id: "1".to_string(),
            offset_ms: 100,
            action_type: "click.mouse".to_string(),
            source: TraceSource::Passive,
        }, 100);
        state.apply(PreviewEvent::ActionOccurred {
            id: "2".to_string(),
            offset_ms: 400,
            action_type: "click.mouse".to_string(),
            source: TraceSource::Passive,
        }, 400);
        state.apply(PreviewEvent::TranscriptFinal(SpeakSegment {
            text: "hello".to_string(),
            start_ms: 0,
            end_ms: 100,
            confidence: 1.0,
            is_final: true,
        }), 100);

        let snapshot = state.build_snapshot(1_000);
        assert_eq!(snapshot.action_pins.len(), 1);
        assert_eq!(snapshot.action_pins[0].count, Some(2));
        assert_eq!(snapshot.transcript.final_segments.len(), 1);
    }

    #[test]
    fn preview_state_keeps_user_sourced_pins_separate_from_passive() {
        use talkiwi_core::event::TraceSource;

        let mut state = PreviewState::new(None);
        state.apply(
            PreviewEvent::ActionOccurred {
                id: "passive-1".to_string(),
                offset_ms: 100,
                action_type: "screenshot".to_string(),
                source: TraceSource::Passive,
            },
            100,
        );
        state.apply(
            PreviewEvent::ActionOccurred {
                id: "toolbar-1".to_string(),
                offset_ms: 300,
                action_type: "screenshot".to_string(),
                source: TraceSource::Toolbar,
            },
            300,
        );

        let snapshot = state.build_snapshot(1_000);
        assert_eq!(snapshot.action_pins.len(), 2);
        assert_eq!(snapshot.action_pins[0].source, TraceSource::Passive);
        assert_eq!(snapshot.action_pins[1].source, TraceSource::Toolbar);
    }

    #[test]
    fn preview_state_drops_pin_on_action_removed() {
        use talkiwi_core::event::TraceSource;

        let mut state = PreviewState::new(None);
        state.apply(
            PreviewEvent::ActionOccurred {
                id: "abc".to_string(),
                offset_ms: 100,
                action_type: "manual.note".to_string(),
                source: TraceSource::Manual,
            },
            100,
        );
        assert_eq!(state.build_snapshot(500).action_pins.len(), 1);

        state.apply(
            PreviewEvent::ActionRemoved {
                id: "abc".to_string(),
            },
            500,
        );
        assert_eq!(state.build_snapshot(500).action_pins.len(), 0);
    }

    #[test]
    fn preview_state_marks_degraded_on_permission_issue() {
        let mut state = PreviewState::new(None);
        state.apply(PreviewEvent::CaptureHealthUpdated(vec![
            CaptureHealthEntry {
                capture_id: "builtin.focus".to_string(),
                status: CaptureStatus::PermissionDenied,
                event_count: 0,
                last_event_offset_ms: None,
            },
        ]), 0);

        let snapshot = state.build_snapshot(0);
        assert!(snapshot.health.degraded);
    }

    #[test]
    fn preview_state_bins_by_absolute_time_within_first_window() {
        // While elapsed_ms < WINDOW_MS the timeline is anchored at t=0 and
        // bins are indexed by absolute time. The frontend renders bars at
        // position `i / BIN_COUNT * 100%` so a "progress bar" fills from
        // the left edge and the playhead (computed from elapsed_ms)
        // advances rightward in lockstep with time.
        let mut state = PreviewState::new(None);

        // 20 levels, one per BIN_MS (250 ms), covering [0, 5000) ms.
        for i in 0..20u64 {
            state.apply(
                PreviewEvent::AudioLevel {
                    offset_ms: i * BIN_MS,
                    rms: 0.5,
                    peak: 0.8,
                    vad_active: true,
                },
                i * BIN_MS,
            );
        }

        let snapshot = state.build_snapshot(5_000);

        // First 20 bins (absolute time [0, 5000) ms) should be populated,
        // everything past them should still be zero. That yields a "growing
        // from the left" visualization on the frontend.
        for i in 0..20 {
            assert!(
                snapshot.audio_bins[i] > 0.0,
                "bin {} should be populated within [0, 5000) ms",
                i
            );
        }
        for i in 20..BIN_COUNT {
            assert_eq!(
                snapshot.audio_bins[i], 0.0,
                "bin {} is past the playhead and must be empty",
                i
            );
        }
    }

    #[test]
    fn preview_state_bins_slide_after_window_fills() {
        // Once elapsed_ms exceeds WINDOW_MS the window slides forward and
        // every bin ends up populated. This guarantees there's no "stuck at
        // the start" regression for long sessions — the backend must keep
        // emitting a fresh rolling view, not a fixed left-anchored one.
        let mut state = PreviewState::new(None);

        // Emit one level per bin for 40 seconds of simulated time.
        for i in 0..160u64 {
            state.apply(
                PreviewEvent::AudioLevel {
                    offset_ms: i * BIN_MS,
                    rms: 0.4,
                    peak: 0.6,
                    vad_active: true,
                },
                i * BIN_MS,
            );
        }

        let snapshot = state.build_snapshot(40_000);
        // After 40 s the window should show [10_000, 40_000) ms — all
        // 120 bins populated, the oldest at bin[0] and newest at bin[119].
        assert!(snapshot.audio_bins[0] > 0.0);
        assert!(snapshot.audio_bins[BIN_COUNT - 1] > 0.0);
    }

    #[test]
    fn preview_state_freezes_elapsed_once_recording_stops() {
        let mut state = PreviewState::new(None);
        state.apply(
            PreviewEvent::SessionStateChanged(SessionState::Recording),
            1_200,
        );
        state.apply(
            PreviewEvent::SessionStateChanged(SessionState::Processing),
            9_000,
        );

        let snapshot = state.build_snapshot(15_000);
        assert_eq!(snapshot.elapsed_ms, 9_000);
    }
}
