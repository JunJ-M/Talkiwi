//! OpenAI Whisper API provider — cloud-based ASR via /v1/audio/transcriptions.
//!
//! Accumulates audio chunks into segments (VAD-driven or time-based),
//! encodes each segment as a WAV blob in memory, and sends it to the
//! OpenAI API for transcription. Returns `SpeakSegment` results.
//!
//! Gated behind the `openai` feature flag.

use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::vad::{VadConfig, VoiceActivityDetector};
use talkiwi_core::session::SpeakSegment;
use talkiwi_core::traits::asr::{AsrProvider, AudioChunk};

/// Configuration for the OpenAI Whisper provider.
#[derive(Debug, Clone)]
pub struct OpenAiWhisperConfig {
    pub api_key: String,
    /// API base URL (default: `https://api.openai.com/v1`).
    pub base_url: String,
    /// Whisper model name (default: `whisper-1`).
    pub model: String,
    /// Language hint (ISO 639-1, e.g. "zh", "en").
    pub language: Option<String>,
    /// Initial prompt to guide transcription style.
    pub prompt: Option<String>,
    /// Max audio segment duration in ms before forced flush.
    pub max_segment_ms: u64,
    /// VAD configuration.
    pub vad_enabled: bool,
    pub vad_threshold: f32,
    pub vad_silence_timeout_ms: u64,
    pub vad_min_speech_duration_ms: u64,
}

impl OpenAiWhisperConfig {
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: "https://api.openai.com/v1".to_string(),
            model: "whisper-1".to_string(),
            language: Some("zh".to_string()),
            prompt: Some("以下是普通话中文口述，可能包含英文术语。".to_string()),
            max_segment_ms: 15_000,
            vad_enabled: true,
            vad_threshold: 0.02,
            vad_silence_timeout_ms: 800,
            vad_min_speech_duration_ms: 300,
        }
    }
}

pub struct OpenAiWhisperProvider {
    config: OpenAiWhisperConfig,
}

impl OpenAiWhisperProvider {
    pub fn new(config: OpenAiWhisperConfig) -> Self {
        Self { config }
    }
}

#[async_trait::async_trait]
impl AsrProvider for OpenAiWhisperProvider {
    fn id(&self) -> &str {
        "openai-whisper"
    }

    fn name(&self) -> &str {
        "OpenAI Whisper API"
    }

    fn requires_network(&self) -> bool {
        true
    }

    async fn is_available(&self) -> bool {
        !self.config.api_key.is_empty()
    }

    async fn transcribe_stream(
        &self,
        mut audio_rx: mpsc::Receiver<AudioChunk>,
        segment_tx: mpsc::Sender<SpeakSegment>,
    ) -> anyhow::Result<()> {
        info!(
            model = %self.config.model,
            language = ?self.config.language,
            vad_enabled = self.config.vad_enabled,
            "openai whisper provider starting transcription stream"
        );

        let client = reqwest::Client::new();
        let config = self.config.clone();

        let mut vad = config.vad_enabled.then(|| {
            VoiceActivityDetector::new(VadConfig {
                threshold: config.vad_threshold,
                silence_timeout_ms: config.vad_silence_timeout_ms,
                min_speech_duration_ms: config.vad_min_speech_duration_ms,
            })
        });

        let mut buffer: Vec<f32> = Vec::new();
        let mut segment_start_ms: Option<u64> = None;

        while let Some(chunk) = audio_rx.recv().await {
            let chunk_end_ms = chunk.offset_ms
                + (chunk.samples.len() as u64 * 1000) / chunk.sample_rate.max(1) as u64;

            let should_flush = match vad.as_mut() {
                Some(v) => {
                    let event = v.process_chunk(&chunk.samples, chunk.offset_ms);
                    if segment_start_ms.is_none() && v.is_speaking() {
                        segment_start_ms = Some(chunk.offset_ms);
                    }
                    if segment_start_ms.is_some() {
                        buffer.extend_from_slice(&chunk.samples);
                    }
                    let speech_end = matches!(event, Some(crate::vad::VadEvent::SpeechEnd { .. }));
                    let over_max = segment_start_ms
                        .map(|s| chunk_end_ms.saturating_sub(s) >= config.max_segment_ms)
                        .unwrap_or(false);
                    speech_end || over_max
                }
                None => {
                    if segment_start_ms.is_none() {
                        segment_start_ms = Some(chunk.offset_ms);
                    }
                    buffer.extend_from_slice(&chunk.samples);
                    let over_max = segment_start_ms
                        .map(|s| chunk_end_ms.saturating_sub(s) >= config.max_segment_ms)
                        .unwrap_or(false);
                    over_max
                }
            };

            if should_flush && !buffer.is_empty() {
                let offset = segment_start_ms.take().unwrap_or(0);
                let samples = std::mem::take(&mut buffer);
                let duration_ms = (samples.len() as u64 * 1000) / 16000;

                match transcribe_buffer(&client, &config, &samples, offset, true).await {
                    Ok(segments) => {
                        for seg in segments {
                            if segment_tx.send(seg).await.is_err() {
                                return Ok(());
                            }
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "openai transcription failed for segment at {}ms", offset);
                        // Emit a low-confidence placeholder so the user knows audio was captured
                        let _ = segment_tx
                            .send(SpeakSegment {
                                text: format!("[transcription error: {}]", e),
                                start_ms: offset,
                                end_ms: offset + duration_ms,
                                confidence: 0.0,
                                is_final: true,
                            })
                            .await;
                    }
                }
            }
        }

        // Flush remaining audio
        if !buffer.is_empty() {
            let offset = segment_start_ms.unwrap_or(0);
            let duration_ms = (buffer.len() as u64 * 1000) / 16000;

            match transcribe_buffer(&client, &config, &buffer, offset, true).await {
                Ok(segments) => {
                    for seg in segments {
                        let _ = segment_tx.send(seg).await;
                    }
                }
                Err(e) => {
                    warn!(error = %e, "openai transcription failed for final segment");
                    let _ = segment_tx
                        .send(SpeakSegment {
                            text: format!("[transcription error: {}]", e),
                            start_ms: offset,
                            end_ms: offset + duration_ms,
                            confidence: 0.0,
                            is_final: true,
                        })
                        .await;
                }
            }
        }

        info!("openai whisper provider transcription stream ended");
        Ok(())
    }
}

/// Encode f32 samples as a WAV blob in memory.
fn encode_wav_in_memory(samples: &[f32]) -> Vec<u8> {
    const SAMPLE_RATE: u32 = 16_000;
    const BITS_PER_SAMPLE: u16 = 16;
    const NUM_CHANNELS: u16 = 1;

    let data_size = (samples.len() * 2) as u32; // 16-bit = 2 bytes per sample
    let byte_rate = SAMPLE_RATE * NUM_CHANNELS as u32 * (BITS_PER_SAMPLE / 8) as u32;
    let block_align = NUM_CHANNELS * (BITS_PER_SAMPLE / 8);

    let mut buf = Vec::with_capacity(44 + data_size as usize);

    // RIFF header
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_size).to_le_bytes());
    buf.extend_from_slice(b"WAVE");

    // fmt sub-chunk
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&NUM_CHANNELS.to_le_bytes());
    buf.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());

    // data sub-chunk
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_size.to_le_bytes());

    for &sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
        let pcm = (clamped * i16::MAX as f32) as i16;
        buf.extend_from_slice(&pcm.to_le_bytes());
    }

    buf
}

/// OpenAI verbose_json response for /v1/audio/transcriptions.
#[derive(Debug, serde::Deserialize)]
struct TranscriptionResponse {
    text: String,
    #[serde(default)]
    segments: Option<Vec<ApiSegment>>,
}

#[derive(Debug, serde::Deserialize)]
struct ApiSegment {
    start: f64,
    end: f64,
    text: String,
    #[serde(default)]
    avg_logprob: f64,
}

/// Send a WAV buffer to the OpenAI Whisper API and return SpeakSegments.
async fn transcribe_buffer(
    client: &reqwest::Client,
    config: &OpenAiWhisperConfig,
    samples: &[f32],
    offset_ms: u64,
    is_final: bool,
) -> anyhow::Result<Vec<SpeakSegment>> {
    let wav_data = encode_wav_in_memory(samples);

    debug!(
        samples = samples.len(),
        wav_bytes = wav_data.len(),
        offset_ms,
        "sending audio to openai whisper api"
    );

    let file_part = reqwest::multipart::Part::bytes(wav_data)
        .file_name("audio.wav")
        .mime_str("audio/wav")?;

    let mut form = reqwest::multipart::Form::new()
        .part("file", file_part)
        .text("model", config.model.clone())
        .text("response_format", "verbose_json");

    if let Some(lang) = &config.language {
        form = form.text("language", lang.clone());
    }

    if let Some(prompt) = &config.prompt {
        form = form.text("prompt", prompt.clone());
    }

    let url = format!("{}/audio/transcriptions", config.base_url);

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", config.api_key))
        .multipart(form)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("OpenAI API error {}: {}", status, body);
    }

    let result: TranscriptionResponse = response.json().await?;

    // If the API returned detailed segments, use them for precise timing
    if let Some(api_segments) = result.segments {
        let segments: Vec<SpeakSegment> = api_segments
            .into_iter()
            .filter(|s| !s.text.trim().is_empty())
            .map(|s| {
                let confidence = logprob_to_confidence(s.avg_logprob);
                SpeakSegment {
                    text: s.text.trim().to_string(),
                    start_ms: offset_ms + (s.start * 1000.0) as u64,
                    end_ms: offset_ms + (s.end * 1000.0) as u64,
                    confidence,
                    is_final,
                }
            })
            .collect();

        debug!(segments = segments.len(), "openai api returned segments");
        return Ok(segments);
    }

    // Fallback: single segment from the full text
    let text = result.text.trim().to_string();
    if text.is_empty() {
        return Ok(Vec::new());
    }

    let duration_ms = (samples.len() as u64 * 1000) / 16000;
    Ok(vec![SpeakSegment {
        text,
        start_ms: offset_ms,
        end_ms: offset_ms + duration_ms,
        confidence: 0.8, // no per-segment confidence in simple mode
        is_final,
    }])
}

/// Convert log probability to a 0.0–1.0 confidence score.
fn logprob_to_confidence(avg_logprob: f64) -> f32 {
    // avg_logprob is typically negative (e.g., -0.2 to -1.5)
    // Convert to probability: exp(logprob), then clamp
    (avg_logprob.exp() as f32).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_wav_produces_valid_header() {
        let samples = vec![0.0f32; 16000]; // 1 second
        let wav = encode_wav_in_memory(&samples);

        // RIFF header
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");

        // fmt chunk
        assert_eq!(&wav[12..16], b"fmt ");

        // data chunk
        assert_eq!(&wav[36..40], b"data");

        // Total size: 44 header + 16000 * 2 bytes = 32044
        assert_eq!(wav.len(), 44 + 32000);
    }

    #[test]
    fn encode_wav_clamps_samples() {
        let samples = vec![2.0, -2.0, 0.5];
        let wav = encode_wav_in_memory(&samples);

        // Read PCM data starting at byte 44
        let s0 = i16::from_le_bytes([wav[44], wav[45]]);
        let s1 = i16::from_le_bytes([wav[46], wav[47]]);
        let s2 = i16::from_le_bytes([wav[48], wav[49]]);

        assert_eq!(s0, i16::MAX); // clamped from 2.0
        assert_eq!(s1, -i16::MAX); // clamped from -2.0
        assert!(s2 > 0 && s2 < i16::MAX); // 0.5 → somewhere in between
    }

    #[test]
    fn logprob_to_confidence_range() {
        assert!((logprob_to_confidence(0.0) - 1.0).abs() < 0.01);
        assert!(logprob_to_confidence(-1.0) > 0.0);
        assert!(logprob_to_confidence(-1.0) < 1.0);
        assert!(logprob_to_confidence(-10.0) < 0.01);
    }

    #[test]
    fn openai_provider_metadata() {
        let config = OpenAiWhisperConfig::new("test-key");
        let provider = OpenAiWhisperProvider::new(config);
        assert_eq!(provider.id(), "openai-whisper");
        assert_eq!(provider.name(), "OpenAI Whisper API");
        assert!(provider.requires_network());
    }

    #[tokio::test]
    async fn openai_provider_unavailable_without_key() {
        let config = OpenAiWhisperConfig {
            api_key: String::new(),
            ..OpenAiWhisperConfig::new("")
        };
        let provider = OpenAiWhisperProvider::new(config);
        assert!(!provider.is_available().await);
    }

    #[tokio::test]
    async fn openai_provider_available_with_key() {
        let config = OpenAiWhisperConfig::new("sk-test-key");
        let provider = OpenAiWhisperProvider::new(config);
        assert!(provider.is_available().await);
    }
}
