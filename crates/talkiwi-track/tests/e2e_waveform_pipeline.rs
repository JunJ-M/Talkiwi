//! Minimal E2E test: audio capture → `PreviewEvent::AudioLevel` waveform.
//!
//! This is the smallest test that reproduces the "click Record but the
//! waveform never moves" bug from a root-cause perspective:
//!
//! 1. A mock audio source emits non-silent PCM chunks.
//! 2. `SpeakTrack` is wired to a `NullAsrProvider`, simulating the case
//!    where the real ASR (whisper-local) is unavailable because the model
//!    file is missing. Before the NullAsrProvider fallback existed, the
//!    ASR receiver would be dropped on the first chunk, the tee task
//!    would shut down the audio pipeline, and `PreviewEvent::AudioLevel`
//!    events would never reach the widget hub.
//! 3. We assert that `AudioLevel` events flow through `preview_tx` with
//!    non-zero levels — which is exactly what `LiveAudioSpectrum` needs
//!    to draw a moving waveform in the widget.
//!
//! If this test passes, then:
//!   - The capture → tee → preview hop works.
//!   - The waveform can render even without a downloaded ASR model.
//!   - Regressions in the pipeline (dropped sender, wrong channel wiring,
//!     missing NullAsrProvider fallback, etc.) will be caught here.
//!
//! No microphone / no model files required.

use std::time::Duration;

use tokio::sync::mpsc;

use talkiwi_asr::{AudioSource, NullAsrProvider};
use talkiwi_core::preview::PreviewEvent;
use talkiwi_core::session::SpeakSegment;
use talkiwi_core::traits::asr::AudioChunk;
use talkiwi_track::SpeakTrack;

/// Deterministic "mic" that pushes loud-ish sine-like frames, then holds
/// the sender open for a short while before closing. Holding the sender
/// open lets the tee task actually process chunks before EOF.
struct LoudMockSource {
    chunks: Vec<AudioChunk>,
}

impl LoudMockSource {
    fn speaking_frames() -> Vec<AudioChunk> {
        // 5 × 100 ms frames of a ~0.5 amplitude signal. We alternate signs
        // so compute_levels() reports a real RMS and peak.
        (0..5)
            .map(|i| {
                let samples: Vec<f32> = (0..1600)
                    .map(|n| if (n / 40) % 2 == 0 { 0.5 } else { -0.5 })
                    .collect();
                AudioChunk {
                    samples,
                    offset_ms: (i as u64) * 100,
                    sample_rate: 16_000,
                }
            })
            .collect()
    }
}

#[async_trait::async_trait]
impl AudioSource for LoudMockSource {
    async fn start(&mut self, tx: mpsc::Sender<AudioChunk>) -> anyhow::Result<()> {
        let chunks = std::mem::take(&mut self.chunks);
        tokio::spawn(async move {
            for chunk in chunks {
                if tx.send(chunk).await.is_err() {
                    return;
                }
                // Mimic a real mic's pacing so the tee task has time to
                // forward each chunk before the next arrives.
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            // Hold the sender open briefly so the pipeline doesn't tear
            // down while the last chunks are still in flight.
            tokio::time::sleep(Duration::from_millis(100)).await;
        });
        Ok(())
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn recording_pipeline_emits_waveform_preview_events_without_real_asr() {
    // ── 1. Wire SpeakTrack with our mock audio + NullAsrProvider.
    //       This is the exact fallback that `session_start` now uses
    //       when the whisper model file is missing.
    let audio_source = LoudMockSource {
        chunks: LoudMockSource::speaking_frames(),
    };
    let mut speak_track = SpeakTrack::new(Box::new(audio_source));

    let (speak_tx, _speak_rx) = mpsc::channel::<SpeakSegment>(16);
    let (preview_tx, mut preview_rx) = mpsc::channel::<PreviewEvent>(64);

    speak_track
        .start(
            speak_tx,
            Some(preview_tx),
            Box::new(NullAsrProvider::new()),
            None,
            0.0,
        )
        .await
        .expect("SpeakTrack should start cleanly even without a real ASR model");

    // ── 2. Collect preview events for up to 1s. We expect several
    //       AudioLevel events with non-zero RMS/peak — that is the exact
    //       signal the widget's LiveAudioSpectrum renders.
    let mut audio_levels = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1);
    while tokio::time::Instant::now() < deadline && audio_levels.len() < 3 {
        match tokio::time::timeout(Duration::from_millis(200), preview_rx.recv()).await {
            Ok(Some(PreviewEvent::AudioLevel {
                offset_ms,
                rms,
                peak,
                vad_active,
            })) => {
                audio_levels.push((offset_ms, rms, peak, vad_active));
            }
            Ok(Some(_)) => continue,
            Ok(None) => break,
            Err(_) => continue,
        }
    }

    // ── 3. Stop the track so background tasks wind down cleanly.
    let result = speak_track
        .stop()
        .await
        .expect("SpeakTrack.stop should not error");
    assert!(
        result.segments.is_empty(),
        "NullAsrProvider should not produce any transcript segments"
    );

    // ── 4. Assertions. These directly verify the "widget waveform shows
    //       up when user clicks Record" requirement.
    assert!(
        audio_levels.len() >= 3,
        "expected at least 3 AudioLevel preview events, got {}. \
         This means the capture → tee → preview pipeline is broken \
         and the widget waveform will never animate.",
        audio_levels.len()
    );

    let max_peak = audio_levels
        .iter()
        .map(|(_, _, peak, _)| *peak)
        .fold(0.0_f32, f32::max);
    assert!(
        max_peak > 0.1,
        "expected at least one AudioLevel with peak > 0.1 (loud signal), \
         got max peak {max_peak}. The widget waveform would be flat."
    );

    let any_vad_active = audio_levels.iter().any(|(_, _, _, vad)| *vad);
    assert!(
        any_vad_active,
        "expected VAD to mark at least one frame as active for the loud \
         test signal — the widget speech overlay would never highlight."
    );
}

#[tokio::test]
async fn recording_pipeline_shuts_down_cleanly_when_audio_ends() {
    // Regression guard: when the audio source closes, the pipeline must
    // not panic and must release all background tasks.
    let audio_source = LoudMockSource { chunks: vec![] };
    let mut speak_track = SpeakTrack::new(Box::new(audio_source));

    let (speak_tx, _speak_rx) = mpsc::channel::<SpeakSegment>(4);
    let (preview_tx, mut preview_rx) = mpsc::channel::<PreviewEvent>(8);

    speak_track
        .start(
            speak_tx,
            Some(preview_tx),
            Box::new(NullAsrProvider::new()),
            None,
            0.0,
        )
        .await
        .unwrap();

    // Give the pipeline a brief moment to start and then stop.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let result = speak_track.stop().await.unwrap();

    assert!(result.segments.is_empty());
    // No AudioLevel events expected for an empty source, but also no panic.
    let _ = preview_rx.try_recv();
}
