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

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use ringbuf::traits::{Consumer, Producer, Split};
use ringbuf::HeapRb;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use talkiwi_core::traits::asr::AudioChunk;

use crate::audio_source::AudioSource;

/// Target sample rate for ASR (Whisper expects 16kHz).
const TARGET_SAMPLE_RATE: u32 = 16000;

/// Ring buffer capacity in samples (2 seconds at max expected rate).
const RING_BUFFER_CAPACITY: usize = 96000; // 48kHz * 2s

/// How often the reader thread reads from the ring buffer.
const READ_INTERVAL_MS: u64 = 100;

/// AudioCapture reads from the default microphone input using cpal,
/// resamples to 16kHz mono via rubato, and sends AudioChunks.
///
/// All cpal operations are managed on a dedicated std::thread since
/// cpal::Stream is not Send+Sync on macOS.
pub struct AudioCapture {
    running: Arc<AtomicBool>,
    audio_thread: Option<thread::JoinHandle<()>>,
}

impl AudioCapture {
    pub fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            audio_thread: None,
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

        // Use a oneshot to report startup errors from the audio thread
        let (err_tx, err_rx) = tokio::sync::oneshot::channel::<Option<String>>();

        let handle = thread::spawn(move || {
            // All cpal operations happen here, on this dedicated thread
            let host = cpal::default_host();
            let device = match host.default_input_device() {
                Some(d) => d,
                None => {
                    let _ = err_tx.send(Some("no default audio input device found".to_string()));
                    return;
                }
            };

            let config = match device.default_input_config() {
                Ok(c) => c,
                Err(e) => {
                    let _ = err_tx.send(Some(format!("failed to get input config: {e}")));
                    return;
                }
            };

            let device_sample_rate = config.sample_rate().0;
            let device_channels = config.channels() as usize;

            info!(
                device = device.name().unwrap_or_default(),
                sample_rate = device_sample_rate,
                channels = device_channels,
                "audio capture starting"
            );

            // Create lock-free ring buffer
            let rb = HeapRb::<f32>::new(RING_BUFFER_CAPACITY);
            let (mut producer, mut consumer) = rb.split();

            // Build cpal stream
            let channels = device_channels;
            let stream = match device.build_input_stream(
                &config.into(),
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    let mono: Vec<f32> = data
                        .chunks(channels)
                        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                        .collect();
                    let written = producer.push_slice(&mono);
                    if written < mono.len() {
                        debug!(dropped = mono.len() - written, "ring buffer overflow");
                    }
                },
                move |err| {
                    error!(error = %err, "cpal input stream error");
                },
                None,
            ) {
                Ok(s) => s,
                Err(e) => {
                    let _ = err_tx.send(Some(format!("failed to build input stream: {e}")));
                    return;
                }
            };

            if let Err(e) = stream.play() {
                let _ = err_tx.send(Some(format!("failed to play stream: {e}")));
                return;
            }

            // Signal success
            let _ = err_tx.send(None);

            // Setup resampler if needed
            let needs_resample = device_sample_rate != TARGET_SAMPLE_RATE;
            let mut resampler = if needs_resample {
                match rubato::SincFixedIn::<f32>::new(
                    TARGET_SAMPLE_RATE as f64 / device_sample_rate as f64,
                    2.0,
                    rubato::SincInterpolationParameters {
                        sinc_len: 256,
                        f_cutoff: 0.95,
                        oversampling_factor: 256,
                        interpolation: rubato::SincInterpolationType::Linear,
                        window: rubato::WindowFunction::BlackmanHarris2,
                    },
                    1024,
                    1,
                ) {
                    Ok(r) => Some(r),
                    Err(e) => {
                        warn!(error = %e, "failed to create resampler");
                        None
                    }
                }
            } else {
                None
            };

            // Read loop: pull from ring buffer, resample, send as AudioChunk
            let mut offset_ms: u64 = 0;
            let mut read_buf = vec![0.0f32; RING_BUFFER_CAPACITY];

            while running.load(Ordering::SeqCst) {
                thread::sleep(std::time::Duration::from_millis(READ_INTERVAL_MS));

                let count = consumer.pop_slice(&mut read_buf);
                if count == 0 {
                    continue;
                }

                let raw_samples = &read_buf[..count];

                let output_samples = if let Some(ref mut resampler) = resampler {
                    match resample_chunk(resampler, raw_samples) {
                        Ok(samples) => samples,
                        Err(e) => {
                            warn!(error = %e, "resample error");
                            raw_samples.to_vec()
                        }
                    }
                } else {
                    raw_samples.to_vec()
                };

                if output_samples.is_empty() {
                    continue;
                }

                let duration_ms = (count as u64 * 1000) / device_sample_rate as u64;
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

/// Resample a chunk using rubato SincFixedIn.
fn resample_chunk(
    resampler: &mut rubato::SincFixedIn<f32>,
    input: &[f32],
) -> anyhow::Result<Vec<f32>> {
    use rubato::Resampler;

    let chunk_size = resampler.input_frames_max();
    let mut output = Vec::new();

    for chunk in input.chunks(chunk_size) {
        if chunk.len() < resampler.input_frames_next() {
            break;
        }
        let input_vec = vec![chunk.to_vec()];
        match resampler.process(&input_vec, None) {
            Ok(result) => {
                if let Some(channel) = result.first() {
                    output.extend_from_slice(channel);
                }
            }
            Err(e) => {
                warn!(error = %e, "rubato resample error");
            }
        }
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_capture_default_creation() {
        let capture = AudioCapture::new();
        assert!(!capture.running.load(Ordering::SeqCst));
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
