use std::env;
use std::path::Path;

use hound::WavReader;
use talkiwi_asr::{AsrProvider, AudioChunk, WhisperLocalProvider, WhisperRuntimeConfig};
use tokio::sync::mpsc;

const SAMPLE_RATE: u32 = 16_000;
const CHUNK_SAMPLES: usize = 1_600;

fn usage() -> String {
    "usage: cargo run -p talkiwi-asr --example inspect_whisper --features whisper -- <audio.wav> <model.bin> [language]".to_string()
}

fn read_wav(path: &Path) -> anyhow::Result<Vec<f32>> {
    let mut reader = WavReader::open(path)?;
    let spec = reader.spec();

    if spec.sample_rate != SAMPLE_RATE {
        anyhow::bail!("expected {}Hz wav, got {}Hz", SAMPLE_RATE, spec.sample_rate);
    }

    let channels = spec.channels.max(1) as usize;

    let raw: Vec<f32> = reader
        .samples::<i16>()
        .map(|sample| sample.map(|value| value as f32 / i16::MAX as f32))
        .collect::<Result<Vec<_>, _>>()?;

    if channels == 1 {
        return Ok(raw);
    }

    Ok(raw
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect())
}

fn env_bool(key: &str, default: bool) -> bool {
    env::var(key)
        .ok()
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "on"))
        .unwrap_or(default)
}

fn env_u32(key: &str, default: u32) -> u32 {
    env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_u64(key: &str, default: u64) -> u64 {
    env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_f32(key: &str, default: f32) -> f32 {
    env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        anyhow::bail!(usage());
    }

    let wav_path = Path::new(&args[1]);
    let model_path = &args[2];
    let language = args
        .get(3)
        .cloned()
        .or_else(|| env::var("TALKIWI_LANGUAGE").ok())
        .filter(|value| !value.trim().is_empty());

    let samples = read_wav(wav_path)?;
    let mut runtime = WhisperRuntimeConfig::default();
    runtime.language = language;
    runtime.beam_size = env_u32("TALKIWI_BEAM_SIZE", runtime.beam_size);
    runtime.condition_on_previous_text = env_bool(
        "TALKIWI_CONDITION_ON_PREVIOUS_TEXT",
        runtime.condition_on_previous_text,
    );
    runtime.vad_enabled = env_bool("TALKIWI_VAD_ENABLED", runtime.vad_enabled);
    runtime.vad_threshold = env_f32("TALKIWI_VAD_THRESHOLD", runtime.vad_threshold);
    runtime.vad_silence_timeout_ms = env_u64(
        "TALKIWI_VAD_SILENCE_TIMEOUT_MS",
        runtime.vad_silence_timeout_ms,
    );
    runtime.vad_min_speech_duration_ms = env_u64(
        "TALKIWI_VAD_MIN_SPEECH_DURATION_MS",
        runtime.vad_min_speech_duration_ms,
    );
    runtime.max_segment_ms = env_u64("TALKIWI_MAX_SEGMENT_MS", runtime.max_segment_ms);
    runtime.initial_prompt = env::var("TALKIWI_INITIAL_PROMPT")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or(runtime.initial_prompt);

    println!(
        "inspect_whisper: wav={} model={} language={:?} beam_size={} vad_enabled={} max_segment_ms={}",
        wav_path.display(),
        model_path,
        runtime.language,
        runtime.beam_size,
        runtime.vad_enabled,
        runtime.max_segment_ms
    );

    let provider = WhisperLocalProvider::with_config(model_path.to_string(), runtime);
    let (audio_tx, audio_rx) = mpsc::channel(256);
    let (segment_tx, mut segment_rx) = mpsc::channel(128);

    let handle =
        tokio::spawn(async move { provider.transcribe_stream(audio_rx, segment_tx).await });

    for (index, chunk) in samples.chunks(CHUNK_SAMPLES).enumerate() {
        let audio_chunk = AudioChunk {
            samples: chunk.to_vec(),
            offset_ms: (index as u64 * CHUNK_SAMPLES as u64 * 1000) / SAMPLE_RATE as u64,
            sample_rate: SAMPLE_RATE,
        };
        audio_tx.send(audio_chunk).await?;
    }

    drop(audio_tx);
    handle.await??;

    let mut segment_count = 0usize;
    while let Some(segment) = segment_rx.recv().await {
        segment_count += 1;
        println!(
            "[{}] {}-{}ms final={} conf={:.3} text={}",
            segment_count,
            segment.start_ms,
            segment.end_ms,
            segment.is_final,
            segment.confidence,
            segment.text
        );
    }

    println!("segments={segment_count}");
    Ok(())
}
