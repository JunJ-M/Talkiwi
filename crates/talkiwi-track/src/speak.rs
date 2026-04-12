//! SpeakTrack — manages audio capture → ASR transcription pipeline.
//!
//! Wires an `AudioSource` to an `AsrProvider`, collecting `SpeakSegment`s
//! and forwarding them to a channel for real-time display.
//! Optionally writes raw audio to a WAV file for waveform display.

use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::{info, warn};

use talkiwi_asr::AudioSource;
use talkiwi_core::session::SpeakSegment;
use talkiwi_core::traits::asr::{AsrProvider, AudioChunk};

/// Result of stopping a SpeakTrack session.
pub struct SpeakTrackResult {
    /// All collected speech segments.
    pub segments: Vec<SpeakSegment>,
    /// Path to the recorded WAV file, if audio recording was enabled.
    pub audio_path: Option<PathBuf>,
}

/// SpeakTrack orchestrates audio capture and ASR transcription.
pub struct SpeakTrack {
    audio_source: Box<dyn AudioSource>,
    segments: Arc<Mutex<Vec<SpeakSegment>>>,
    event_tx: Option<mpsc::Sender<SpeakSegment>>,
    /// Handle to the ASR processing task for cleanup.
    asr_handle: Option<tokio::task::JoinHandle<()>>,
    /// Handle to the WAV writer tee task.
    wav_handle: Option<tokio::task::JoinHandle<Option<PathBuf>>>,
}

impl SpeakTrack {
    /// Create a new SpeakTrack with an audio source.
    pub fn new(audio_source: Box<dyn AudioSource>) -> Self {
        Self {
            audio_source,
            segments: Arc::new(Mutex::new(Vec::new())),
            event_tx: None,
            asr_handle: None,
            wav_handle: None,
        }
    }

    /// Start recording with full ASR pipeline wiring.
    ///
    /// When `audio_dir` is provided, audio chunks are also written to
    /// `<audio_dir>/audio.wav` for later waveform rendering.
    ///
    /// Data flow:
    /// ```text
    /// audio_source.start(tee_tx)
    ///   → tee task: write WAV + forward to asr_audio_tx
    ///   → asr_provider.transcribe_stream(asr_audio_rx, segment_tx)
    ///   → collector task: store + forward to event_tx
    /// ```
    pub async fn start(
        &mut self,
        event_tx: mpsc::Sender<SpeakSegment>,
        asr_provider: Box<dyn AsrProvider>,
        audio_dir: Option<PathBuf>,
    ) -> anyhow::Result<()> {
        self.event_tx = Some(event_tx.clone());

        let (tee_tx, mut tee_rx) = mpsc::channel::<AudioChunk>(256);
        let (asr_audio_tx, asr_audio_rx) = mpsc::channel::<AudioChunk>(256);
        let (segment_tx, mut segment_rx) = mpsc::channel::<SpeakSegment>(64);

        // Start audio capture → feeds into tee channel
        self.audio_source.start(tee_tx).await?;
        info!("audio source started");

        // Spawn tee task: reads from audio source, writes WAV + forwards to ASR
        let wav_handle = tokio::spawn(async move {
            let mut wav_writer = audio_dir.and_then(|dir| {
                let wav_path = dir.join("audio.wav");
                match talkiwi_asr::WavWriter::new(&wav_path) {
                    Ok(w) => {
                        info!(path = %wav_path.display(), "WAV writer started");
                        Some(w)
                    }
                    Err(e) => {
                        warn!(error = %e, "failed to create WAV writer, recording without audio file");
                        None
                    }
                }
            });

            while let Some(chunk) = tee_rx.recv().await {
                // Write to WAV file (best-effort — don't fail the pipeline)
                if let Some(ref mut writer) = wav_writer {
                    if let Err(e) = writer.write_chunk(&chunk.samples) {
                        warn!(error = %e, "WAV write error, disabling writer");
                        wav_writer = None;
                    }
                }

                // Forward to ASR
                if asr_audio_tx.send(chunk).await.is_err() {
                    break;
                }
            }

            // Finalize WAV
            wav_writer.and_then(|w| match w.finalize() {
                Ok(path) => {
                    info!(path = %path.display(), "WAV file finalized");
                    Some(path)
                }
                Err(e) => {
                    warn!(error = %e, "WAV finalize error");
                    None
                }
            })
        });

        self.wav_handle = Some(wav_handle);

        // Spawn ASR task
        tokio::spawn(async move {
            if let Err(e) = asr_provider
                .transcribe_stream(asr_audio_rx, segment_tx)
                .await
            {
                warn!(error = %e, "ASR transcription error");
            }
        });

        // Spawn collector
        let segments = Arc::clone(&self.segments);
        let collector_handle = tokio::spawn(async move {
            while let Some(segment) = segment_rx.recv().await {
                segments.lock().await.push(segment.clone());
                if event_tx.send(segment).await.is_err() {
                    break;
                }
            }
        });

        self.asr_handle = Some(collector_handle);
        Ok(())
    }

    /// Stop recording and return all collected segments + audio path.
    pub async fn stop(&mut self) -> anyhow::Result<SpeakTrackResult> {
        // Stop audio source — this closes tee_tx, causing tee and ASR to finish
        self.audio_source.stop().await?;
        info!("audio source stopped");

        // Wait for WAV writer to finalize
        let audio_path = if let Some(handle) = self.wav_handle.take() {
            match tokio::time::timeout(std::time::Duration::from_secs(5), handle).await {
                Ok(Ok(path)) => path,
                Ok(Err(e)) => {
                    warn!(error = %e, "WAV task join error");
                    None
                }
                Err(_) => {
                    warn!("WAV task timed out");
                    None
                }
            }
        } else {
            None
        };

        // Wait for ASR to flush
        if let Some(handle) = self.asr_handle.take() {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
        }

        self.event_tx = None;

        let mut segments = self.segments.lock().await;
        Ok(SpeakTrackResult {
            segments: std::mem::take(&mut *segments),
            audio_path,
        })
    }

    /// Get current segment count.
    pub async fn segment_count(&self) -> usize {
        self.segments.lock().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock audio source that sends predefined chunks then closes.
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

    /// Mock ASR provider that produces segments from audio chunks.
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
            let mut offset = 0u64;
            while let Some(chunk) = audio_rx.recv().await {
                let duration = (chunk.samples.len() as u64 * 1000) / chunk.sample_rate as u64;
                let segment = SpeakSegment {
                    text: format!("transcribed chunk at {}ms", chunk.offset_ms),
                    start_ms: offset,
                    end_ms: offset + duration,
                    confidence: 0.95,
                    is_final: true,
                };
                offset += duration;
                if segment_tx.send(segment).await.is_err() {
                    break;
                }
            }
            Ok(())
        }
    }

    fn make_test_chunks(count: usize) -> Vec<AudioChunk> {
        (0..count)
            .map(|i| AudioChunk {
                samples: vec![0.1; 1600], // 100ms at 16kHz
                offset_ms: i as u64 * 100,
                sample_rate: 16000,
            })
            .collect()
    }

    #[tokio::test]
    async fn speak_track_start_produces_segments() {
        let chunks = make_test_chunks(3);
        let audio_source = MockAudioSource::new(chunks);

        let mut track = SpeakTrack::new(Box::new(audio_source));

        let (event_tx, mut event_rx) = mpsc::channel(16);
        track
            .start(event_tx, Box::new(MockAsrProvider), None)
            .await
            .unwrap();

        // Collect forwarded segments
        let mut received = Vec::new();
        for _ in 0..3 {
            if let Some(segment) =
                tokio::time::timeout(std::time::Duration::from_millis(500), event_rx.recv())
                    .await
                    .ok()
                    .flatten()
            {
                received.push(segment);
            }
        }

        assert_eq!(received.len(), 3);
        assert!(received[0].text.contains("transcribed"));
        assert!(received[0].is_final);

        let result = track.stop().await.unwrap();
        assert_eq!(result.segments.len(), 3);
        assert!(result.audio_path.is_none());
    }

    #[tokio::test]
    async fn speak_track_stop_returns_collected_segments() {
        let audio_source = MockAudioSource::new(vec![]);
        let mut track = SpeakTrack::new(Box::new(audio_source));

        let (event_tx, _rx) = mpsc::channel(16);
        track
            .start(event_tx, Box::new(MockAsrProvider), None)
            .await
            .unwrap();

        let result = track.stop().await.unwrap();
        assert!(result.segments.is_empty());
        assert!(result.audio_path.is_none());
    }

    #[tokio::test]
    async fn speak_track_segment_count() {
        let chunks = make_test_chunks(2);
        let audio_source = MockAudioSource::new(chunks);

        let mut track = SpeakTrack::new(Box::new(audio_source));

        let (event_tx, mut _rx) = mpsc::channel(16);
        track
            .start(event_tx, Box::new(MockAsrProvider), None)
            .await
            .unwrap();

        // Wait for segments to be processed
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        let count = track.segment_count().await;
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn speak_track_writes_wav_file() {
        let chunks = make_test_chunks(5);
        let audio_source = MockAudioSource::new(chunks);

        let dir = tempfile::tempdir().unwrap();
        let mut track = SpeakTrack::new(Box::new(audio_source));

        let (event_tx, _rx) = mpsc::channel(16);
        track
            .start(
                event_tx,
                Box::new(MockAsrProvider),
                Some(dir.path().to_path_buf()),
            )
            .await
            .unwrap();

        // Wait for chunks to flow through
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let result = track.stop().await.unwrap();
        assert_eq!(result.segments.len(), 5);

        // WAV file should exist
        let audio_path = result.audio_path.expect("should have audio_path");
        assert!(audio_path.exists());
        assert_eq!(audio_path.file_name().unwrap(), "audio.wav");

        // Verify it's a valid WAV
        let metadata = std::fs::metadata(&audio_path).unwrap();
        assert!(metadata.len() > 44); // header + some data
    }
}
