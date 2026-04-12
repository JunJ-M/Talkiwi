use crate::session::SpeakSegment;
use tokio::sync::mpsc;

/// A chunk of audio data for ASR processing.
#[derive(Debug, Clone)]
pub struct AudioChunk {
    pub samples: Vec<f32>,
    pub offset_ms: u64,
    pub sample_rate: u32,
}

/// ASR provider trait — implemented by whisper-local, cloud providers, etc.
#[async_trait::async_trait]
pub trait AsrProvider: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn requires_network(&self) -> bool;
    async fn is_available(&self) -> bool;
    async fn transcribe_stream(
        &self,
        audio_rx: mpsc::Receiver<AudioChunk>,
        segment_tx: mpsc::Sender<SpeakSegment>,
    ) -> anyhow::Result<()>;
}
