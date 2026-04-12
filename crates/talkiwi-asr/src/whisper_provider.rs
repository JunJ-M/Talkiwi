//! WhisperLocalProvider — local Whisper ASR via whisper-rs.
//!
//! Uses configurable decoding and chunking:
//! - VAD-driven segmentation for better phrase boundaries
//! - Optional fixed max segment duration as a safety valve
//! - Beam search / language hint / initial prompt for better Chinese accuracy
//!
//! The actual whisper-rs dependency is gated behind the `whisper` feature
//! flag. Without it, this module provides a stub implementation that
//! produces placeholder text, allowing the workspace to build without
//! requiring whisper.cpp system dependencies.

use tokio::sync::mpsc;
#[cfg(feature = "whisper")]
use tracing::warn;
use tracing::{debug, info};

use crate::vad::{VadConfig, VoiceActivityDetector};
use talkiwi_core::config::AsrConfig;
use talkiwi_core::session::SpeakSegment;
use talkiwi_core::traits::asr::{AsrProvider, AudioChunk};

/// Whisper runtime configuration derived from app settings.
#[derive(Debug, Clone)]
pub struct WhisperRuntimeConfig {
    pub language: Option<String>,
    pub beam_size: u32,
    pub condition_on_previous_text: bool,
    pub initial_prompt: Option<String>,
    pub vad_enabled: bool,
    pub vad_threshold: f32,
    pub vad_silence_timeout_ms: u64,
    pub vad_min_speech_duration_ms: u64,
    pub max_segment_ms: u64,
}

impl Default for WhisperRuntimeConfig {
    fn default() -> Self {
        let config = AsrConfig::default();
        Self::from(&config)
    }
}

impl From<&AsrConfig> for WhisperRuntimeConfig {
    fn from(config: &AsrConfig) -> Self {
        Self {
            language: config.language.clone(),
            beam_size: config.beam_size.max(1),
            condition_on_previous_text: config.condition_on_previous_text,
            initial_prompt: config
                .initial_prompt
                .as_ref()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
            vad_enabled: config.vad_enabled,
            vad_threshold: config.vad_threshold,
            vad_silence_timeout_ms: config.vad_silence_timeout_ms,
            vad_min_speech_duration_ms: config.vad_min_speech_duration_ms,
            max_segment_ms: config.max_segment_ms.max(1_000),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlushReason {
    SpeechEnd,
    MaxSegment,
    StreamEnd,
}

#[derive(Debug)]
struct PreparedBuffer {
    samples: Vec<f32>,
    offset_ms: u64,
    is_final: bool,
}

struct AudioChunkSegmenter {
    config: WhisperRuntimeConfig,
    vad: Option<VoiceActivityDetector>,
    buffer: Vec<f32>,
    current_start_ms: Option<u64>,
}

impl AudioChunkSegmenter {
    fn new(config: WhisperRuntimeConfig) -> Self {
        let vad = config.vad_enabled.then(|| {
            VoiceActivityDetector::new(VadConfig {
                threshold: config.vad_threshold,
                silence_timeout_ms: config.vad_silence_timeout_ms,
                min_speech_duration_ms: config.vad_min_speech_duration_ms,
            })
        });

        Self {
            config,
            vad,
            buffer: Vec::new(),
            current_start_ms: None,
        }
    }

    fn process_chunk(&mut self, chunk: AudioChunk) -> Option<PreparedBuffer> {
        let chunk_end_ms = chunk.offset_ms + samples_to_ms(chunk.samples.len(), chunk.sample_rate);

        match self.vad.as_mut() {
            Some(vad) => {
                let event = vad.process_chunk(&chunk.samples, chunk.offset_ms, chunk.sample_rate);

                if self.current_start_ms.is_none() && vad.is_speaking() {
                    self.current_start_ms = Some(chunk.offset_ms);
                }

                if self.current_start_ms.is_some() {
                    self.buffer.extend_from_slice(&chunk.samples);
                }

                if matches!(event, Some(crate::vad::VadEvent::SpeechEnd { .. })) {
                    return self.flush(FlushReason::SpeechEnd);
                }

                if let Some(start_ms) = self.current_start_ms {
                    if chunk_end_ms.saturating_sub(start_ms) >= self.config.max_segment_ms {
                        return self.flush(FlushReason::MaxSegment);
                    }
                }

                None
            }
            None => {
                if self.current_start_ms.is_none() {
                    self.current_start_ms = Some(chunk.offset_ms);
                }
                self.buffer.extend_from_slice(&chunk.samples);

                let start_ms = self.current_start_ms.unwrap_or(chunk.offset_ms);
                if chunk_end_ms.saturating_sub(start_ms) >= self.config.max_segment_ms {
                    return self.flush(FlushReason::MaxSegment);
                }

                None
            }
        }
    }

    fn finish(&mut self) -> Option<PreparedBuffer> {
        self.flush(FlushReason::StreamEnd)
    }

    fn flush(&mut self, reason: FlushReason) -> Option<PreparedBuffer> {
        if self.buffer.is_empty() {
            self.current_start_ms = None;
            return None;
        }

        let samples = std::mem::take(&mut self.buffer);
        let offset_ms = self.current_start_ms.take().unwrap_or(0);

        Some(PreparedBuffer {
            samples,
            offset_ms,
            is_final: matches!(reason, FlushReason::SpeechEnd | FlushReason::StreamEnd),
        })
    }
}

fn samples_to_ms(sample_count: usize, sample_rate: u32) -> u64 {
    (sample_count as u64 * 1000) / sample_rate.max(1) as u64
}

/// WhisperLocalProvider wraps whisper.cpp via whisper-rs for local
/// speech-to-text transcription.
pub struct WhisperLocalProvider {
    /// Path to the GGML model file.
    model_path: String,
    config: WhisperRuntimeConfig,
}

impl WhisperLocalProvider {
    /// Create a new provider. The model file is loaded lazily on first
    /// transcription call, not at construction time.
    pub fn new(model_path: impl Into<String>, language: Option<String>) -> Self {
        let mut config = WhisperRuntimeConfig::default();
        config.language = language;
        Self {
            model_path: model_path.into(),
            config,
        }
    }

    pub fn with_config(model_path: impl Into<String>, config: WhisperRuntimeConfig) -> Self {
        Self {
            model_path: model_path.into(),
            config,
        }
    }

    /// Returns the configured model path.
    pub fn model_path(&self) -> &str {
        &self.model_path
    }
}

// ─── Real whisper-rs implementation ────────────────────────────────────────────

#[cfg(feature = "whisper")]
mod real_impl {
    use super::*;
    use std::sync::mpsc as std_mpsc;
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

    /// Message sent from the async accumulator to the inference thread.
    enum InferenceRequest {
        /// A buffer of f32 samples to transcribe.
        Buffer {
            samples: Vec<f32>,
            offset_ms: u64,
            is_final: bool,
        },
        /// Signal that no more audio is coming.
        Shutdown,
    }

    /// Result returned from the inference thread.
    struct InferenceResult {
        segments: Vec<SpeakSegment>,
    }

    #[async_trait::async_trait]
    impl AsrProvider for WhisperLocalProvider {
        fn id(&self) -> &str {
            "whisper-local"
        }

        fn name(&self) -> &str {
            "Whisper Local (whisper.cpp)"
        }

        fn requires_network(&self) -> bool {
            false
        }

        async fn is_available(&self) -> bool {
            std::path::Path::new(&self.model_path).exists()
        }

        async fn transcribe_stream(
            &self,
            mut audio_rx: mpsc::Receiver<AudioChunk>,
            segment_tx: mpsc::Sender<SpeakSegment>,
        ) -> anyhow::Result<()> {
            info!(
                model = %self.model_path,
                language = ?self.config.language,
                beam_size = self.config.beam_size,
                vad_enabled = self.config.vad_enabled,
                "whisper provider starting transcription stream (real)"
            );

            let model_path = self.model_path.clone();
            let runtime_config = self.config.clone();

            // Channels to communicate with the inference thread.
            // We use std::sync::mpsc because the inference thread is a
            // plain std::thread (WhisperState is !Send).
            let (req_tx, req_rx) = std_mpsc::channel::<InferenceRequest>();
            let (res_tx, mut res_rx_async) = mpsc::channel::<InferenceResult>(16);

            // Spawn dedicated inference thread — owns WhisperContext + WhisperState.
            // WhisperState is !Send on macOS (CoreAudio), so all whisper-rs
            // operations must stay on this single std::thread.
            let inference_handle = std::thread::spawn(move || {
                inference_worker(model_path, runtime_config, req_rx, res_tx);
            });

            // Segmenter loop — runs in tokio async context
            let mut segmenter = AudioChunkSegmenter::new(self.config.clone());

            // Spawn a task to forward inference results to segment_tx
            let segment_tx_clone = segment_tx.clone();
            let forwarder = tokio::spawn(async move {
                while let Some(result) = res_rx_async.recv().await {
                    for seg in result.segments {
                        if segment_tx_clone.send(seg).await.is_err() {
                            break;
                        }
                    }
                }
            });

            while let Some(chunk) = audio_rx.recv().await {
                if let Some(buffer) = segmenter.process_chunk(chunk) {
                    if req_tx
                        .send(InferenceRequest::Buffer {
                            samples: buffer.samples,
                            offset_ms: buffer.offset_ms,
                            is_final: buffer.is_final,
                        })
                        .is_err()
                    {
                        warn!("inference thread dropped, stopping accumulator");
                        break;
                    }
                }
            }

            // Flush remaining audio (send error is ok — thread may have exited)
            if let Some(buffer) = segmenter.finish() {
                let _ = req_tx.send(InferenceRequest::Buffer {
                    samples: buffer.samples,
                    offset_ms: buffer.offset_ms,
                    is_final: buffer.is_final,
                });
            }

            // Signal shutdown to inference thread
            let _ = req_tx.send(InferenceRequest::Shutdown);

            // Wait for inference thread to finish
            let thread_result = inference_handle.join();

            // Drop segment_tx so forwarder sees channel close, then await it
            drop(segment_tx);
            let _ = forwarder.await;

            // Propagate thread panic as error
            if let Err(e) = thread_result {
                anyhow::bail!("inference thread panicked: {:?}", e);
            }

            info!("whisper provider transcription stream ended");
            Ok(())
        }
    }

    /// Dedicated inference worker — runs on its own std::thread.
    /// Owns WhisperContext and WhisperState (both are !Send).
    fn inference_worker(
        model_path: String,
        runtime_config: WhisperRuntimeConfig,
        req_rx: std_mpsc::Receiver<InferenceRequest>,
        res_tx: mpsc::Sender<InferenceResult>,
    ) {
        info!(model = %model_path, "loading whisper model");

        let ctx =
            match WhisperContext::new_with_params(&model_path, WhisperContextParameters::default())
            {
                Ok(ctx) => ctx,
                Err(e) => {
                    warn!(error = %e, "failed to load whisper model");
                    return;
                }
            };

        let mut state = match ctx.create_state() {
            Ok(s) => s,
            Err(e) => {
                warn!(error = %e, "failed to create whisper state");
                return;
            }
        };

        info!("whisper model loaded, inference worker ready");

        for request in req_rx.iter() {
            match request {
                InferenceRequest::Buffer {
                    samples,
                    offset_ms,
                    is_final,
                } => {
                    let segments =
                        run_inference(&mut state, &samples, offset_ms, is_final, &runtime_config);
                    if res_tx.blocking_send(InferenceResult { segments }).is_err() {
                        debug!("result channel closed, stopping inference worker");
                        break;
                    }
                }
                InferenceRequest::Shutdown => {
                    debug!("inference worker received shutdown signal");
                    break;
                }
            }
        }

        info!("inference worker exiting");
    }

    /// Run whisper inference on a buffer and extract SpeakSegments.
    fn run_inference(
        state: &mut whisper_rs::WhisperState,
        samples: &[f32],
        offset_ms: u64,
        is_final: bool,
        runtime_config: &WhisperRuntimeConfig,
    ) -> Vec<SpeakSegment> {
        let strategy = if runtime_config.beam_size > 1 {
            SamplingStrategy::BeamSearch {
                beam_size: runtime_config.beam_size as i32,
                patience: 1.0,
            }
        } else {
            SamplingStrategy::Greedy { best_of: 1 }
        };
        let mut params = FullParams::new(strategy);

        if let Some(lang) = runtime_config.language.as_deref() {
            params.set_language(Some(lang));
            params.set_detect_language(false);
        } else {
            params.set_detect_language(true);
        }
        params.set_print_special(false);
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);
        params.set_translate(false);
        params.set_no_context(!runtime_config.condition_on_previous_text);
        params.set_single_segment(false);
        params.set_no_timestamps(false);
        params.set_suppress_nst(true);
        params.set_n_threads(
            std::thread::available_parallelism()
                .map(|n| n.get().min(8) as i32)
                .unwrap_or(4),
        );

        if let Some(prompt) = runtime_config.initial_prompt.as_deref() {
            params.set_initial_prompt(prompt);
        }

        debug!(
            samples = samples.len(),
            offset_ms, is_final, "running whisper inference"
        );

        if let Err(e) = state.full(params, samples) {
            warn!(error = %e, "whisper inference failed");
            return Vec::new();
        }

        let n_segments = match state.full_n_segments() {
            Ok(n) => n,
            Err(e) => {
                warn!(error = %e, "failed to get segment count");
                return Vec::new();
            }
        };

        let mut results = Vec::with_capacity(n_segments as usize);

        for i in 0..n_segments {
            let text = match state.full_get_segment_text(i) {
                Ok(t) => t.trim().to_string(),
                Err(e) => {
                    warn!(segment = i, error = %e, "failed to get segment text");
                    continue;
                }
            };

            // Skip empty segments
            if text.is_empty() {
                continue;
            }

            // whisper timestamps are in centiseconds (10ms units)
            let seg_start = state.full_get_segment_t0(i).unwrap_or(0);
            let seg_end = state.full_get_segment_t1(i).unwrap_or(0);

            let start_ms = offset_ms + (seg_start as u64 * 10);
            let end_ms = offset_ms + (seg_end as u64 * 10);

            // Compute confidence as average token probability
            let confidence = compute_segment_confidence(state, i);

            results.push(SpeakSegment {
                text,
                start_ms,
                end_ms,
                confidence,
                is_final,
            });
        }

        debug!(
            segments = results.len(),
            "whisper inference produced segments"
        );
        results
    }

    /// Compute average token probability for a segment as confidence score.
    fn compute_segment_confidence(state: &whisper_rs::WhisperState, segment_idx: i32) -> f32 {
        let n_tokens = match state.full_n_tokens(segment_idx) {
            Ok(n) => n,
            Err(_) => return 0.0,
        };

        if n_tokens == 0 {
            return 0.0;
        }

        let mut sum = 0.0f32;
        let mut count = 0u32;

        for token_idx in 0..n_tokens {
            if let Ok(prob) = state.full_get_token_prob(segment_idx, token_idx) {
                sum += prob;
                count += 1;
            }
        }

        if count > 0 {
            sum / count as f32
        } else {
            0.0
        }
    }
}

// ─── Stub implementation (no whisper feature) ──────────────────────────────────

#[cfg(not(feature = "whisper"))]
mod stub_impl {
    use super::*;

    #[async_trait::async_trait]
    impl AsrProvider for WhisperLocalProvider {
        fn id(&self) -> &str {
            "whisper-local"
        }

        fn name(&self) -> &str {
            "Whisper Local (whisper.cpp)"
        }

        fn requires_network(&self) -> bool {
            false
        }

        async fn is_available(&self) -> bool {
            std::path::Path::new(&self.model_path).exists()
        }

        async fn transcribe_stream(
            &self,
            mut audio_rx: mpsc::Receiver<AudioChunk>,
            segment_tx: mpsc::Sender<SpeakSegment>,
        ) -> anyhow::Result<()> {
            info!(
                model = %self.model_path,
                language = ?self.config.language,
                beam_size = self.config.beam_size,
                vad_enabled = self.config.vad_enabled,
                "whisper provider starting transcription stream (stub)"
            );

            let mut segment_index: u32 = 0;
            let mut segmenter = AudioChunkSegmenter::new(self.config.clone());

            while let Some(chunk) = audio_rx.recv().await {
                if let Some(buffer) = segmenter.process_chunk(chunk) {
                    let duration_ms = samples_to_ms(buffer.samples.len(), 16000);
                    let segment = SpeakSegment {
                        text: format!("[whisper-stub] segment {}", segment_index),
                        start_ms: buffer.offset_ms,
                        end_ms: buffer.offset_ms + duration_ms,
                        confidence: 1.0,
                        is_final: buffer.is_final,
                    };

                    debug!(
                        segment = segment_index,
                        samples = buffer.samples.len(),
                        "processing audio buffer (stub)"
                    );

                    segment_index += 1;

                    if segment_tx.send(segment).await.is_err() {
                        break;
                    }
                }
            }

            // Process remaining buffer
            if let Some(buffer) = segmenter.finish() {
                let duration_ms = samples_to_ms(buffer.samples.len(), 16000);
                let segment = SpeakSegment {
                    text: format!("[whisper-stub] final segment {}", segment_index),
                    start_ms: buffer.offset_ms,
                    end_ms: buffer.offset_ms + duration_ms,
                    confidence: 0.0,
                    is_final: buffer.is_final,
                };

                let _ = segment_tx.send(segment).await;
            }

            info!("whisper provider transcription stream ended (stub)");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_chunk(offset_ms: u64, sample_count: usize, amplitude: f32) -> AudioChunk {
        AudioChunk {
            samples: vec![amplitude; sample_count],
            offset_ms,
            sample_rate: 16_000,
        }
    }

    #[test]
    fn runtime_config_from_asr_config_applies_defaults() {
        let runtime = WhisperRuntimeConfig::default();
        assert_eq!(runtime.language, Some("zh".to_string()));
        assert_eq!(runtime.beam_size, 5);
        assert!(runtime.condition_on_previous_text);
        assert!(runtime.vad_enabled);
        assert_eq!(runtime.max_segment_ms, 15_000);
        assert!(runtime.initial_prompt.is_some());
    }

    #[test]
    fn segmenter_flushes_on_max_segment_without_vad() {
        let mut segmenter = AudioChunkSegmenter::new(WhisperRuntimeConfig {
            vad_enabled: false,
            max_segment_ms: 1_000,
            ..WhisperRuntimeConfig::default()
        });

        assert!(segmenter.process_chunk(make_chunk(0, 1600, 0.1)).is_none());
        assert!(segmenter
            .process_chunk(make_chunk(100, 1600, 0.1))
            .is_none());
        assert!(segmenter
            .process_chunk(make_chunk(200, 1600, 0.1))
            .is_none());
        assert!(segmenter
            .process_chunk(make_chunk(300, 1600, 0.1))
            .is_none());
        assert!(segmenter
            .process_chunk(make_chunk(400, 1600, 0.1))
            .is_none());
        assert!(segmenter
            .process_chunk(make_chunk(500, 1600, 0.1))
            .is_none());
        assert!(segmenter
            .process_chunk(make_chunk(600, 1600, 0.1))
            .is_none());
        assert!(segmenter
            .process_chunk(make_chunk(700, 1600, 0.1))
            .is_none());
        assert!(segmenter
            .process_chunk(make_chunk(800, 1600, 0.1))
            .is_none());

        let flushed = segmenter
            .process_chunk(make_chunk(900, 1600, 0.1))
            .expect("expected segment flush");
        assert_eq!(flushed.offset_ms, 0);
        assert_eq!(flushed.samples.len(), 16_000);
        assert!(!flushed.is_final);
    }

    #[test]
    fn segmenter_flushes_on_speech_end_with_vad() {
        let mut segmenter = AudioChunkSegmenter::new(WhisperRuntimeConfig {
            max_segment_ms: 10_000,
            ..WhisperRuntimeConfig::default()
        });

        for i in 0..3 {
            assert!(segmenter
                .process_chunk(make_chunk(i * 100, 1600, 0.1))
                .is_none());
        }

        assert!(segmenter
            .process_chunk(make_chunk(300, 1600, 0.0))
            .is_none());
        assert!(segmenter
            .process_chunk(make_chunk(400, 1600, 0.0))
            .is_none());
        assert!(segmenter
            .process_chunk(make_chunk(500, 1600, 0.0))
            .is_none());
        assert!(segmenter
            .process_chunk(make_chunk(600, 1600, 0.0))
            .is_none());
        assert!(segmenter
            .process_chunk(make_chunk(700, 1600, 0.0))
            .is_none());
        assert!(segmenter
            .process_chunk(make_chunk(800, 1600, 0.0))
            .is_none());
        assert!(segmenter
            .process_chunk(make_chunk(900, 1600, 0.0))
            .is_none());

        let flushed = segmenter
            .process_chunk(make_chunk(1000, 1600, 0.0))
            .expect("expected VAD speech end flush");
        assert_eq!(flushed.offset_ms, 0);
        assert!(flushed.is_final);
        assert_eq!(flushed.samples.len(), 17_600);
    }

    #[test]
    fn whisper_provider_metadata() {
        let provider = WhisperLocalProvider::new("/tmp/model.bin", Some("zh".to_string()));
        assert_eq!(provider.id(), "whisper-local");
        assert_eq!(provider.name(), "Whisper Local (whisper.cpp)");
        assert!(!provider.requires_network());
        assert_eq!(provider.model_path(), "/tmp/model.bin");
    }

    #[tokio::test]
    async fn whisper_provider_unavailable_when_no_model() {
        let provider = WhisperLocalProvider::new("/nonexistent/model.bin", None);
        assert!(!provider.is_available().await);
    }

    /// Real impl test: transcribe_stream returns error when model is missing.
    #[cfg(feature = "whisper")]
    #[tokio::test]
    async fn whisper_provider_transcribe_stream_no_model() {
        let provider = WhisperLocalProvider::new("/nonexistent/model.bin", None);

        let (audio_tx, audio_rx) = mpsc::channel(16);
        let (segment_tx, mut segment_rx) = mpsc::channel(16);

        let handle = tokio::spawn(async move {
            provider
                .transcribe_stream(audio_rx, segment_tx)
                .await
                .unwrap();
        });

        // Send a small amount of audio
        for i in 0..20 {
            let chunk = AudioChunk {
                samples: vec![0.1; 1600],
                offset_ms: i * 100,
                sample_rate: 16000,
            };
            if audio_tx.send(chunk).await.is_err() {
                break;
            }
        }

        drop(audio_tx);
        handle.await.unwrap();

        // With no model file, the inference thread fails to load and
        // produces no segments
        let mut segments = Vec::new();
        while let Ok(seg) = segment_rx.try_recv() {
            segments.push(seg);
        }
        assert!(
            segments.is_empty(),
            "Should produce no segments when model is missing"
        );
    }

    /// Real impl test: verifies inference worker handles empty audio gracefully.
    #[cfg(feature = "whisper")]
    #[tokio::test]
    async fn whisper_provider_empty_audio_no_model() {
        let provider = WhisperLocalProvider::new("/nonexistent/model.bin", None);

        let (_audio_tx, audio_rx) = mpsc::channel::<AudioChunk>(16);
        let (segment_tx, _segment_rx) = mpsc::channel(16);

        // Immediately drop audio_tx to close channel
        drop(_audio_tx);

        let result = provider.transcribe_stream(audio_rx, segment_tx).await;
        assert!(result.is_ok(), "Should not error on empty stream");
    }

    /// Stub-specific test: verifies placeholder text output.
    #[cfg(not(feature = "whisper"))]
    #[tokio::test]
    async fn whisper_provider_transcribe_stream_stub() {
        let provider = WhisperLocalProvider::new("/tmp/model.bin", None);

        let (audio_tx, audio_rx) = mpsc::channel(16);
        let (segment_tx, mut segment_rx) = mpsc::channel(16);

        let handle = tokio::spawn(async move {
            provider
                .transcribe_stream(audio_rx, segment_tx)
                .await
                .unwrap();
        });

        // Send enough audio to trigger one inference (32000 samples)
        for i in 0..20 {
            let chunk = AudioChunk {
                samples: vec![0.1; 1600], // 100ms
                offset_ms: i * 100,
                sample_rate: 16000,
            };
            audio_tx.send(chunk).await.unwrap();
        }

        drop(audio_tx);
        handle.await.unwrap();

        let mut segments = Vec::new();
        while let Ok(seg) = segment_rx.try_recv() {
            segments.push(seg);
        }
        assert!(
            !segments.is_empty(),
            "Should have produced at least 1 segment"
        );
        assert!(segments[0].text.contains("whisper-stub"));
    }

    /// Stub-specific test: verifies remaining buffer is flushed.
    #[cfg(not(feature = "whisper"))]
    #[tokio::test]
    async fn whisper_provider_flushes_remaining_buffer() {
        let provider = WhisperLocalProvider::new("/tmp/model.bin", None);

        let (audio_tx, audio_rx) = mpsc::channel(16);
        let (segment_tx, mut segment_rx) = mpsc::channel(16);

        let handle = tokio::spawn(async move {
            provider
                .transcribe_stream(audio_rx, segment_tx)
                .await
                .unwrap();
        });

        // Send less than INFERENCE_THRESHOLD samples
        for i in 0..5 {
            let chunk = AudioChunk {
                samples: vec![0.1; 1600],
                offset_ms: i * 100,
                sample_rate: 16000,
            };
            audio_tx.send(chunk).await.unwrap();
        }

        drop(audio_tx);
        handle.await.unwrap();

        let mut segments = Vec::new();
        while let Ok(seg) = segment_rx.try_recv() {
            segments.push(seg);
        }
        assert_eq!(segments.len(), 1);
        assert!(segments[0].text.contains("final"));
    }
}
