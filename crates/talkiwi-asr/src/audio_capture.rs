//! AudioCapture — cpal-based microphone input with rubato resampling.
//!
//! Architecture:
//! ```text
//! cpal callback (system audio thread)
//!   │ f32 samples, native sample rate
//!   ▼
//! HeapRb (lock-free ring buffer, 2s capacity)
//!   │ dedicated std::thread polls every 100ms
//!   ▼
//! rubato SincFixedIn resampler (if needed)
//!   │ 16kHz mono f32
//!   ▼
//! mpsc::channel<AudioChunk>
//!   → AsrProvider::transcribe_stream()
//! ```
//!
//! The cpal::Stream is not Send+Sync, so all cpal operations happen
//! on a dedicated std::thread. The AudioSource trait is still async
//! for the interface, using oneshot channels for signaling.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::traits::{Consumer, Split};
use ringbuf::HeapRb;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use talkiwi_core::traits::asr::AudioChunk;

use crate::audio_input::SelectedAudioInput;
use crate::audio_source::AudioSource;

/// Target sample rate for ASR (Whisper expects 16kHz).
const TARGET_SAMPLE_RATE: u32 = 16000;

/// Ring buffer capacity in samples.
///
/// Sized to ~4 seconds at 48 kHz mono so that a brief reader stall
/// (e.g. tokio runtime hiccup, disk flush, slow whisper startup) can be
/// absorbed without dropping samples. The previous 2 s value was too tight:
/// tauri+whisper startup transients would saturate the buffer and spam
/// "ring buffer overflow" warnings that drowned out the rest of the log.
const RING_BUFFER_CAPACITY: usize = 192_000; // 48kHz * 4s

/// How often the reader thread reads from the ring buffer.
const READ_INTERVAL_MS: u64 = 100;

/// Minimum time between overflow warnings — aggregates drops over this
/// window so we log once per `LOG_INTERVAL_MS` with the total dropped,
/// instead of spamming the log on every cpal callback.
const OVERFLOW_LOG_INTERVAL_MS: u64 = 1000;

/// Canary: if no non-silent sample is observed in the first N seconds of
/// capture, emit a loud warning. CoreAudio returns silence (not an error)
/// when TCC denies microphone access, so this is the only way to surface
/// that failure mode.
const SILENCE_CANARY_SECONDS: u64 = 2;

fn microphone_startup_error(detail: impl std::fmt::Display) -> String {
    format!(
        "unable to start microphone recording. Grant Talkiwi microphone access in \
         System Settings > Privacy & Security > Microphone, or connect an available \
         input device. Details: {detail}"
    )
}

/// AudioCapture reads from the default microphone input using cpal,
/// resamples to 16kHz mono via rubato, and sends AudioChunks.
///
/// All cpal operations are managed on a dedicated std::thread since
/// cpal::Stream is not Send+Sync on macOS.
pub struct AudioCapture {
    running: Arc<AtomicBool>,
    audio_thread: Option<thread::JoinHandle<()>>,
    selected_input: SelectedAudioInput,
}

impl AudioCapture {
    pub fn new() -> Self {
        Self::with_selected_input(SelectedAudioInput::default())
    }

    pub fn with_selected_input(selected_input: SelectedAudioInput) -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            audio_thread: None,
            selected_input,
        }
    }
}

impl Default for AudioCapture {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl AudioSource for AudioCapture {
    async fn start(&mut self, tx: mpsc::Sender<AudioChunk>) -> anyhow::Result<()> {
        self.running.store(true, Ordering::SeqCst);
        let running = Arc::clone(&self.running);
        let selected_input = self.selected_input.clone();

        // Use a oneshot to report startup errors from the audio thread
        let (err_tx, err_rx) = tokio::sync::oneshot::channel::<Option<String>>();

        let handle = thread::spawn(move || {
            // All cpal operations happen here, on this dedicated thread
            let host = cpal::default_host();
            let device = match select_input_device(&host, selected_input.get()) {
                Some(d) => d,
                None => {
                    let _ = err_tx.send(Some(microphone_startup_error(
                        "no audio input device found",
                    )));
                    return;
                }
            };

            let config = match pick_input_config(&device) {
                Ok(c) => c,
                Err(e) => {
                    let _ = err_tx.send(Some(microphone_startup_error(format!(
                        "failed to get input config: {e}"
                    ))));
                    return;
                }
            };

            let device_sample_rate = config.sample_rate().0;
            let device_channels = config.channels() as usize;
            let native_16k = device_sample_rate == TARGET_SAMPLE_RATE;

            info!(
                device = device.name().unwrap_or_default(),
                sample_rate = device_sample_rate,
                channels = device_channels,
                native_16k,
                "audio capture starting"
            );

            // Create lock-free ring buffer
            let rb = HeapRb::<f32>::new(RING_BUFFER_CAPACITY);
            let (producer, mut consumer) = rb.split();

            // Build cpal stream
            let channels = device_channels;
            let stream =
                match build_input_stream(&device, &config, channels, producer, move |err| {
                    error!(error = %err, "cpal input stream error");
                }) {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = err_tx.send(Some(microphone_startup_error(format!(
                            "failed to build input stream: {e}"
                        ))));
                        return;
                    }
                };

            if let Err(e) = stream.play() {
                let _ = err_tx.send(Some(microphone_startup_error(format!(
                    "failed to play stream: {e}"
                ))));
                return;
            }

            // Signal success
            let _ = err_tx.send(None);

            // Setup resampler if needed.
            //
            // Quality notes:
            //
            // * `Cubic` (not `Linear`) interpolation is required for acceptable
            //   audio quality on 44.1→16k and 48→16k. Linear interpolation in
            //   the sinc filter's lookup destroys high-frequency content and
            //   produces a badly low-passed "隔壁墙说话" signal that whisper
            //   cannot decode (it enters repetition-loop / entropy failure and
            //   returns garbage like "为什 为什 ...").
            //
            // * `SincFixedIn` requires EXACTLY `input_frames_next()` samples
            //   per `process()` call — whatever remainder we can't feed in the
            //   current iteration must persist to the next loop iteration via
            //   `residual` below. The previous implementation silently dropped
            //   that remainder, causing periodic ~7% sample loss and audible
            //   amplitude modulation artifacts.
            let (mut resampler, resampler_chunk_size) = if native_16k {
                info!(
                    sample_rate = TARGET_SAMPLE_RATE,
                    "audio capture using native 16 kHz — no resampling"
                );
                (None, 0usize)
            } else {
                match rubato::SincFixedIn::<f32>::new(
                    TARGET_SAMPLE_RATE as f64 / device_sample_rate as f64,
                    2.0,
                    rubato::SincInterpolationParameters {
                        sinc_len: 256,
                        f_cutoff: 0.95,
                        oversampling_factor: 256,
                        interpolation: rubato::SincInterpolationType::Cubic,
                        window: rubato::WindowFunction::BlackmanHarris2,
                    },
                    1024,
                    1,
                ) {
                    Ok(r) => {
                        let chunk_size = rubato::Resampler::input_frames_next(&r);
                        info!(
                            input_rate = device_sample_rate,
                            output_rate = TARGET_SAMPLE_RATE,
                            chunk_size,
                            "built rubato resampler (Cubic, sinc_len=256)"
                        );
                        (Some(r), chunk_size)
                    }
                    Err(e) => {
                        warn!(error = %e, "failed to create resampler");
                        (None, 0usize)
                    }
                }
            };

            // Read loop: pull from ring buffer, resample, send as AudioChunk.
            //
            // `residual` carries the tail samples that couldn't form a full
            // resampler chunk in the current iteration. Keeping them across
            // iterations preserves signal continuity for the sinc filter.
            let mut offset_ms: u64 = 0;
            let mut read_buf = vec![0.0f32; RING_BUFFER_CAPACITY];
            let mut residual: Vec<f32> = Vec::with_capacity(RING_BUFFER_CAPACITY);

            // Silent-audio canary: if we don't observe a single non-zero
            // sample in the first `SILENCE_CANARY_SECONDS`, the mic is
            // almost certainly denied by TCC (CoreAudio returns silence
            // instead of an error on permission denial). Surface it loudly.
            let capture_start = Instant::now();
            let mut nonzero_seen = false;
            let mut canary_fired = false;

            while running.load(Ordering::SeqCst) {
                thread::sleep(std::time::Duration::from_millis(READ_INTERVAL_MS));

                let count = consumer.pop_slice(&mut read_buf);
                if count > 0 {
                    residual.extend_from_slice(&read_buf[..count]);
                }
                if residual.is_empty() {
                    continue;
                }

                // Silent canary: peek at raw samples to detect "all zero".
                if !nonzero_seen {
                    nonzero_seen = residual.iter().any(|s| s.abs() > 1e-5);
                }
                if !nonzero_seen
                    && !canary_fired
                    && capture_start.elapsed().as_secs() >= SILENCE_CANARY_SECONDS
                {
                    warn!(
                        elapsed_s = capture_start.elapsed().as_secs(),
                        "microphone canary: captured only silence for the first \
                         {SILENCE_CANARY_SECONDS}s — this usually means macOS TCC \
                         denied mic access silently. Check System Settings → \
                         Privacy & Security → Microphone, or run \
                         `tccutil reset Microphone com.talkiwi.app` and re-launch."
                    );
                    canary_fired = true;
                }

                let output_samples: Vec<f32> = if let Some(ref mut resampler) = resampler {
                    use rubato::Resampler;

                    // Feed the resampler exactly `resampler_chunk_size` samples
                    // per call. Anything that doesn't fit stays in `residual`
                    // for the next loop iter (preserves signal continuity).
                    let mut out: Vec<f32> = Vec::new();
                    let mut consumed = 0usize;
                    while residual.len() - consumed >= resampler_chunk_size {
                        let chunk = &residual[consumed..consumed + resampler_chunk_size];
                        let input_vec = vec![chunk.to_vec()];
                        match resampler.process(&input_vec, None) {
                            Ok(result) => {
                                if let Some(channel) = result.first() {
                                    out.extend_from_slice(channel);
                                }
                            }
                            Err(e) => {
                                warn!(error = %e, "rubato resample error, dropping chunk");
                            }
                        }
                        consumed += resampler_chunk_size;
                    }
                    if consumed > 0 {
                        residual.drain(..consumed);
                    }
                    out
                } else {
                    // Native 16 kHz path — pass through directly.
                    std::mem::take(&mut residual)
                };

                if output_samples.is_empty() {
                    continue;
                }

                let sample_count = output_samples.len() as u64;
                let duration_ms = (sample_count * 1000) / TARGET_SAMPLE_RATE as u64;
                let chunk = AudioChunk {
                    samples: output_samples,
                    offset_ms,
                    sample_rate: TARGET_SAMPLE_RATE,
                };
                offset_ms += duration_ms;

                if tx.blocking_send(chunk).is_err() {
                    break;
                }
            }

            // Stream is dropped here, stopping cpal
            drop(stream);
            info!("audio capture thread exiting");
        });

        // Wait for startup result
        match err_rx.await {
            Ok(Some(err_msg)) => {
                self.running.store(false, Ordering::SeqCst);
                anyhow::bail!(err_msg);
            }
            Ok(None) => {
                // Success
                self.audio_thread = Some(handle);
                Ok(())
            }
            Err(_) => {
                self.running.store(false, Ordering::SeqCst);
                anyhow::bail!("audio thread terminated unexpectedly during startup");
            }
        }
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        self.running.store(false, Ordering::SeqCst);

        if let Some(handle) = self.audio_thread.take() {
            // Give it a moment then join
            let _ = handle.join();
        }

        info!("audio capture stopped");
        Ok(())
    }
}

/// Pick the best available input config for the device.
///
/// Priority:
///   1. Mono 16 kHz (native — no resample needed, best for Whisper).
///   2. Stereo 16 kHz (native; we'll downmix in `push_mono`).
///   3. Device's default config (whatever CoreAudio/WASAPI/ALSA advertises).
///
/// Most MacBook Pro built-in microphones only advertise 44.1 or 48 kHz,
/// so in practice we fall through to #3 and rely on the rubato Cubic
/// resampler in the read loop. But when a USB / bluetooth device does
/// expose 16 kHz natively, we take it and skip resampling entirely —
/// that path has zero resample artifacts and is the cleanest ASR input.
fn pick_input_config(device: &cpal::Device) -> anyhow::Result<cpal::SupportedStreamConfig> {
    use cpal::SampleRate;

    if let Ok(ranges) = device.supported_input_configs() {
        let ranges: Vec<cpal::SupportedStreamConfigRange> = ranges.collect();

        // 1. Prefer mono @ 16 kHz.
        for range in ranges.iter().filter(|r| r.channels() == 1) {
            if range.min_sample_rate().0 <= TARGET_SAMPLE_RATE
                && TARGET_SAMPLE_RATE <= range.max_sample_rate().0
            {
                return Ok(range
                    .clone()
                    .with_sample_rate(SampleRate(TARGET_SAMPLE_RATE)));
            }
        }

        // 2. Fall back to any channel count @ 16 kHz (stereo will be downmixed).
        for range in &ranges {
            if range.min_sample_rate().0 <= TARGET_SAMPLE_RATE
                && TARGET_SAMPLE_RATE <= range.max_sample_rate().0
            {
                return Ok(range
                    .clone()
                    .with_sample_rate(SampleRate(TARGET_SAMPLE_RATE)));
            }
        }
    }

    // 3. Default config — we'll resample in the read loop.
    device
        .default_input_config()
        .map_err(|e| anyhow::anyhow!("failed to get default input config: {}", e))
}

fn select_input_device(host: &cpal::Host, selected: Option<String>) -> Option<cpal::Device> {
    if let Some(selected) = selected {
        if let Ok(devices) = host.input_devices() {
            for device in devices {
                if device.name().map(|name| name == selected).unwrap_or(false) {
                    return Some(device);
                }
            }
        }
        warn!(
            selected,
            "selected input device not found, falling back to default"
        );
    }

    host.default_input_device()
}

/// Shared overflow accounting used by the cpal callback to aggregate
/// "dropped sample" events. Using atomics only (no locks) so the audio
/// callback stays real-time-safe.
#[derive(Default)]
struct OverflowStats {
    /// Total samples dropped since the last time we logged a summary.
    dropped_since_last_log: AtomicU64,
    /// Milliseconds-since-UNIX-epoch when we last emitted a warn log.
    last_log_ms: AtomicU64,
}

fn now_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn build_input_stream<E>(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    channels: usize,
    mut producer: impl ringbuf::traits::Producer<Item = f32> + Send + 'static,
    err_fn: E,
) -> anyhow::Result<cpal::Stream>
where
    E: FnMut(cpal::StreamError) + Send + 'static,
{
    let stream_config: cpal::StreamConfig = config.clone().into();
    let overflow_stats = Arc::new(OverflowStats::default());

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => {
            let stats = Arc::clone(&overflow_stats);
            device.build_input_stream(
                &stream_config,
                move |data: &[f32], _| push_mono(data, channels, &mut producer, &stats),
                err_fn,
                None,
            )?
        }
        cpal::SampleFormat::I16 => {
            let stats = Arc::clone(&overflow_stats);
            device.build_input_stream(
                &stream_config,
                move |data: &[i16], _| {
                    let converted: Vec<f32> = data
                        .iter()
                        .map(|sample| *sample as f32 / i16::MAX as f32)
                        .collect();
                    push_mono(&converted, channels, &mut producer, &stats);
                },
                err_fn,
                None,
            )?
        }
        cpal::SampleFormat::U16 => {
            let stats = Arc::clone(&overflow_stats);
            device.build_input_stream(
                &stream_config,
                move |data: &[u16], _| {
                    let converted: Vec<f32> = data
                        .iter()
                        .map(|sample| (*sample as f32 / u16::MAX as f32) * 2.0 - 1.0)
                        .collect();
                    push_mono(&converted, channels, &mut producer, &stats);
                },
                err_fn,
                None,
            )?
        }
        sample_format => anyhow::bail!("unsupported sample format: {sample_format:?}"),
    };

    Ok(stream)
}

fn push_mono(
    data: &[f32],
    channels: usize,
    producer: &mut (impl ringbuf::traits::Producer<Item = f32> + ?Sized),
    overflow_stats: &OverflowStats,
) {
    let mono: Vec<f32> = data
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect();
    let written = producer.push_slice(&mono);
    let dropped = mono.len() - written;
    if dropped == 0 {
        return;
    }

    // Aggregate drops so we don't spam the log on every cpal callback.
    let total = overflow_stats
        .dropped_since_last_log
        .fetch_add(dropped as u64, Ordering::Relaxed)
        + dropped as u64;

    let now = now_ms();
    let last = overflow_stats.last_log_ms.load(Ordering::Relaxed);
    if now.saturating_sub(last) >= OVERFLOW_LOG_INTERVAL_MS {
        // Race-free claim of the log slot: only one caller wins the CAS.
        if overflow_stats
            .last_log_ms
            .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            warn!(
                dropped = total,
                window_ms = OVERFLOW_LOG_INTERVAL_MS,
                "ring buffer overflow (aggregated) — reader thread falling behind; \
                 consider increasing RING_BUFFER_CAPACITY or reducing downstream latency"
            );
            overflow_stats
                .dropped_since_last_log
                .store(0, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_capture_default_creation() {
        let capture = AudioCapture::new();
        assert!(!capture.running.load(Ordering::SeqCst));
    }

    #[test]
    fn microphone_startup_error_mentions_permissions_and_devices() {
        let message = microphone_startup_error("failed to build input stream: device unavailable");

        assert!(message.contains("System Settings > Privacy & Security > Microphone"));
        assert!(message.contains("available input device"));
        assert!(message.contains("device unavailable"));
    }

    /// This test requires a real microphone and is skipped in CI.
    #[tokio::test]
    #[ignore = "requires microphone hardware"]
    async fn audio_capture_start_stop_real_hardware() {
        let mut capture = AudioCapture::new();
        let (tx, mut rx) = mpsc::channel(256);

        capture.start(tx).await.unwrap();

        // Wait for some audio
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        capture.stop().await.unwrap();

        let mut count = 0;
        while rx.try_recv().is_ok() {
            count += 1;
        }
        println!("Received {} audio chunks", count);
        assert!(count > 0, "Should have received at least 1 audio chunk");
    }
}
