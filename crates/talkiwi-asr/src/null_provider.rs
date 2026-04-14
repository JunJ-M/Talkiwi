//! NullAsrProvider — a no-op ASR provider used as a fallback when the
//! configured provider is unavailable (e.g. whisper model not downloaded).
//!
//! The provider deliberately *consumes* audio chunks instead of dropping the
//! receiver. Without this, `SpeakTrack`'s tee task would see the ASR channel
//! close on the first chunk and shut down the whole audio pipeline, which
//! means the user's widget would never see a waveform when no model is
//! installed.
//!
//! Using this provider lets the capture → preview pipeline keep running
//! (waveform, VAD, levels) even when transcription is disabled. The UI
//! separately surfaces a warning so the user knows transcripts are not being
//! produced.
//!
//! Intended use: fallback only. Real transcription still goes through
//! [`crate::whisper_provider::WhisperLocalProvider`] or the OpenAI provider.

use tokio::sync::mpsc;
use tracing::{debug, info};

use talkiwi_core::session::SpeakSegment;
use talkiwi_core::traits::asr::{AsrProvider, AudioChunk};

/// A no-op ASR provider that drains audio chunks without transcribing.
///
/// Exists solely so the audio capture pipeline can run end-to-end when the
/// real ASR provider is not available. It never emits any `SpeakSegment`.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullAsrProvider;

impl NullAsrProvider {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl AsrProvider for NullAsrProvider {
    fn id(&self) -> &str {
        "null"
    }

    fn name(&self) -> &str {
        "Null ASR (audio-only fallback)"
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
        _segment_tx: mpsc::Sender<SpeakSegment>,
    ) -> anyhow::Result<()> {
        info!("null ASR provider: draining audio without transcription");
        let mut chunks = 0u64;
        while let Some(chunk) = audio_rx.recv().await {
            chunks += 1;
            debug!(
                chunks,
                offset_ms = chunk.offset_ms,
                samples = chunk.samples.len(),
                "null ASR drained chunk"
            );
        }
        info!(chunks, "null ASR provider finished draining");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn null_provider_drains_all_chunks_without_emitting_segments() {
        let provider = NullAsrProvider::new();
        assert!(provider.is_available().await);
        assert_eq!(provider.id(), "null");

        let (audio_tx, audio_rx) = mpsc::channel::<AudioChunk>(8);
        let (segment_tx, mut segment_rx) = mpsc::channel::<SpeakSegment>(8);

        // Push a few chunks then drop the sender so the stream terminates.
        tokio::spawn(async move {
            for i in 0..5 {
                let _ = audio_tx
                    .send(AudioChunk {
                        samples: vec![0.0; 160],
                        offset_ms: i * 10,
                        sample_rate: 16_000,
                    })
                    .await;
            }
        });

        provider
            .transcribe_stream(audio_rx, segment_tx)
            .await
            .expect("null provider should not fail");

        // No segments should ever be produced.
        assert!(segment_rx.try_recv().is_err());
    }
}
