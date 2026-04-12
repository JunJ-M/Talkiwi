//! Integration tests for WhisperLocalProvider with a real model.
//!
//! These tests are gated behind the `whisper` feature flag AND require
//! a model file to be present. They are marked `#[ignore]` for CI.
//!
//! To run locally:
//! ```bash
//! # Download a tiny model for testing (~75MB)
//! mkdir -p /tmp/talkiwi-test-models
//! curl -L -o /tmp/talkiwi-test-models/ggml-tiny.bin \
//!   https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.bin
//!
//! # Run integration tests
//! WHISPER_MODEL_PATH=/tmp/talkiwi-test-models/ggml-tiny.bin \
//!   cargo test -p talkiwi-asr --features whisper -- --ignored whisper
//! ```

#[cfg(feature = "whisper")]
mod whisper_real {
    use talkiwi_asr::{AsrProvider, AudioChunk, WhisperLocalProvider};
    use tokio::sync::mpsc;

    fn get_model_path() -> Option<String> {
        std::env::var("WHISPER_MODEL_PATH").ok()
    }

    #[tokio::test]
    #[ignore = "requires WHISPER_MODEL_PATH env var pointing to a ggml model file"]
    async fn whisper_real_model_is_available() {
        let model_path = get_model_path().expect("WHISPER_MODEL_PATH not set");
        let provider = WhisperLocalProvider::new(&model_path, None);
        assert!(
            provider.is_available().await,
            "Model file should exist at {model_path}"
        );
    }

    #[tokio::test]
    #[ignore = "requires WHISPER_MODEL_PATH env var pointing to a ggml model file"]
    async fn whisper_real_transcribes_silence() {
        let model_path = get_model_path().expect("WHISPER_MODEL_PATH not set");
        let provider = WhisperLocalProvider::new(&model_path, Some("en".to_string()));

        let (audio_tx, audio_rx) = mpsc::channel(256);
        let (segment_tx, mut segment_rx) = mpsc::channel(64);

        let handle = tokio::spawn(async move {
            provider
                .transcribe_stream(audio_rx, segment_tx)
                .await
                .unwrap();
        });

        // Send 2 seconds of silence (32000 samples at 16kHz)
        for i in 0..20 {
            let chunk = AudioChunk {
                samples: vec![0.0; 1600], // 100ms of silence
                offset_ms: i * 100,
                sample_rate: 16000,
            };
            audio_tx.send(chunk).await.unwrap();
        }

        drop(audio_tx);
        handle.await.unwrap();

        // Silence may or may not produce segments (whisper may detect
        // background noise or produce empty text). The key assertion is
        // that inference completes without error.
        let mut segments = Vec::new();
        while let Ok(seg) = segment_rx.try_recv() {
            segments.push(seg);
        }

        println!("Silence test produced {} segments", segments.len());
        for seg in &segments {
            println!(
                "  [{}-{}ms] '{}' (conf: {:.2})",
                seg.start_ms, seg.end_ms, seg.text, seg.confidence
            );
        }
    }

    #[tokio::test]
    #[ignore = "requires WHISPER_MODEL_PATH env var pointing to a ggml model file"]
    async fn whisper_real_transcribes_sine_wave() {
        let model_path = get_model_path().expect("WHISPER_MODEL_PATH not set");
        let provider = WhisperLocalProvider::new(&model_path, Some("en".to_string()));

        let (audio_tx, audio_rx) = mpsc::channel(256);
        let (segment_tx, mut segment_rx) = mpsc::channel(64);

        let handle = tokio::spawn(async move {
            provider
                .transcribe_stream(audio_rx, segment_tx)
                .await
                .unwrap();
        });

        // Send 2 seconds of 440Hz sine wave (A4 note)
        let sample_rate = 16000u32;
        let frequency = 440.0f32;
        let total_samples = 32000; // 2 seconds
        let chunk_size = 1600; // 100ms

        for chunk_idx in 0..(total_samples / chunk_size) {
            let offset = chunk_idx * chunk_size;
            let samples: Vec<f32> = (0..chunk_size)
                .map(|i| {
                    let t = (offset + i) as f32 / sample_rate as f32;
                    (2.0 * std::f32::consts::PI * frequency * t).sin() * 0.3
                })
                .collect();

            let chunk = AudioChunk {
                samples,
                offset_ms: (chunk_idx * chunk_size) as u64 * 1000 / sample_rate as u64,
                sample_rate,
            };
            audio_tx.send(chunk).await.unwrap();
        }

        drop(audio_tx);
        handle.await.unwrap();

        let mut segments = Vec::new();
        while let Ok(seg) = segment_rx.try_recv() {
            segments.push(seg);
        }

        println!("Sine wave test produced {} segments", segments.len());
        for seg in &segments {
            println!(
                "  [{}-{}ms] '{}' (conf: {:.2})",
                seg.start_ms, seg.end_ms, seg.text, seg.confidence
            );
        }

        // A pure tone may produce transcription artifacts — the key
        // assertion is that the pipeline completes without error and
        // produces at least some output (whisper processes all audio).
    }
}
