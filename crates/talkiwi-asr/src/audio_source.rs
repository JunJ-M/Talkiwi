//! AudioSource trait — abstracts audio input for testability.
//!
//! Real implementation: `AudioCapture` (cpal + rubato + ringbuf).
//! Tests use `MockAudioSource`.

use talkiwi_core::traits::asr::AudioChunk;
use tokio::sync::mpsc;

/// Audio source trait — implemented by cpal-based AudioCapture
/// and mock sources for testing.
#[async_trait::async_trait]
pub trait AudioSource: Send + Sync {
    /// Start capturing audio, sending chunks to `tx`.
    async fn start(&mut self, tx: mpsc::Sender<AudioChunk>) -> anyhow::Result<()>;

    /// Stop capturing audio.
    async fn stop(&mut self) -> anyhow::Result<()>;
}
