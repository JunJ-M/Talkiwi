//! Voice Activity Detection (VAD) using RMS energy threshold.
//!
//! State machine: Silent → Speaking → SilenceWait → Silent
//! V1: simple RMS energy. V1.5: upgrade to Silero VAD ONNX.

/// VAD configuration.
///
/// Controls the sensitivity and timing of voice activity detection.
#[derive(Debug, Clone)]
pub struct VadConfig {
    /// RMS energy threshold for speech detection. Default: 0.02
    pub threshold: f32,
    /// How long silence must persist to end a speech segment (ms). Default: 800
    pub silence_timeout_ms: u64,
    /// Minimum speech duration to emit (ms). Shorter bursts are discarded. Default: 300
    pub min_speech_duration_ms: u64,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            threshold: 0.02,
            silence_timeout_ms: 800,
            min_speech_duration_ms: 300,
        }
    }
}

/// Events emitted by the VAD state machine.
#[derive(Debug, Clone, PartialEq)]
pub enum VadEvent {
    /// Speech started at the given offset.
    SpeechStart { start_ms: u64 },
    /// Speech ended. Contains start and end offsets.
    SpeechEnd { start_ms: u64, end_ms: u64 },
}

#[derive(Debug, Clone, PartialEq)]
enum VadState {
    Silent,
    Speaking,
    SilenceWait,
}

/// Voice Activity Detector using RMS energy state machine.
pub struct VoiceActivityDetector {
    config: VadConfig,
    state: VadState,
    speech_start_ms: u64,
    silence_start_ms: u64,
}

impl VoiceActivityDetector {
    pub fn new(config: VadConfig) -> Self {
        Self {
            config,
            state: VadState::Silent,
            speech_start_ms: 0,
            silence_start_ms: 0,
        }
    }

    /// Process a chunk of audio samples and return any state transition event.
    ///
    /// `chunk_offset_ms` is the timestamp of this chunk relative to session start.
    pub fn process_chunk(&mut self, samples: &[f32], chunk_offset_ms: u64) -> Option<VadEvent> {
        let energy = rms_energy(samples);
        let is_speech = energy > self.config.threshold;

        match self.state {
            VadState::Silent => {
                if is_speech {
                    self.state = VadState::Speaking;
                    self.speech_start_ms = chunk_offset_ms;
                    Some(VadEvent::SpeechStart {
                        start_ms: chunk_offset_ms,
                    })
                } else {
                    None
                }
            }
            VadState::Speaking => {
                if !is_speech {
                    self.state = VadState::SilenceWait;
                    self.silence_start_ms = chunk_offset_ms;
                }
                None
            }
            VadState::SilenceWait => {
                if is_speech {
                    // Speech resumed — go back to Speaking
                    self.state = VadState::Speaking;
                    None
                } else {
                    // Check if silence has lasted long enough
                    let silence_duration = chunk_offset_ms.saturating_sub(self.silence_start_ms);
                    if silence_duration >= self.config.silence_timeout_ms {
                        self.state = VadState::Silent;
                        let speech_duration = chunk_offset_ms.saturating_sub(self.speech_start_ms);
                        if speech_duration >= self.config.min_speech_duration_ms {
                            Some(VadEvent::SpeechEnd {
                                start_ms: self.speech_start_ms,
                                end_ms: chunk_offset_ms,
                            })
                        } else {
                            // Too short — discard
                            None
                        }
                    } else {
                        None
                    }
                }
            }
        }
    }

    /// Get current state (for testing/debugging).
    pub fn is_speaking(&self) -> bool {
        matches!(self.state, VadState::Speaking | VadState::SilenceWait)
    }
}

/// Calculate RMS (Root Mean Square) energy of audio samples.
pub fn rms_energy(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
    (sum_sq / samples.len() as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn silence(len: usize) -> Vec<f32> {
        vec![0.0; len]
    }

    fn loud_signal(len: usize, amplitude: f32) -> Vec<f32> {
        // Sine wave at 440Hz, 16kHz sample rate
        (0..len)
            .map(|i| amplitude * (2.0 * PI * 440.0 * i as f32 / 16000.0).sin())
            .collect()
    }

    #[test]
    fn vad_rms_energy_calculation() {
        // All zeros → 0.0
        assert_eq!(rms_energy(&silence(100)), 0.0);

        // Sine wave of amplitude 1.0 → RMS ≈ 0.707
        let sine: Vec<f32> = (0..16000)
            .map(|i| (2.0 * PI * 440.0 * i as f32 / 16000.0).sin())
            .collect();
        let rms = rms_energy(&sine);
        assert!((rms - 0.707).abs() < 0.01, "RMS was {}", rms);

        // Empty → 0.0
        assert_eq!(rms_energy(&[]), 0.0);
    }

    #[test]
    fn vad_silent_to_speaking_transition() {
        let mut vad = VoiceActivityDetector::new(VadConfig::default());

        // Silent chunk — no event
        let event = vad.process_chunk(&silence(1600), 0);
        assert_eq!(event, None);
        assert!(!vad.is_speaking());

        // Loud chunk — SpeechStart
        let event = vad.process_chunk(&loud_signal(1600, 0.1), 100);
        assert_eq!(event, Some(VadEvent::SpeechStart { start_ms: 100 }));
        assert!(vad.is_speaking());
    }

    #[test]
    fn vad_speaking_to_silence_wait_to_silent() {
        let mut vad = VoiceActivityDetector::new(VadConfig::default());

        // Start speaking
        vad.process_chunk(&loud_signal(1600, 0.1), 0);
        assert!(vad.is_speaking());

        // Go quiet — enters SilenceWait
        vad.process_chunk(&silence(1600), 100);
        assert!(vad.is_speaking()); // Still "speaking" (in wait)

        // Stay quiet past timeout (800ms)
        let event = vad.process_chunk(&silence(1600), 1000);
        assert_eq!(
            event,
            Some(VadEvent::SpeechEnd {
                start_ms: 0,
                end_ms: 1000,
            })
        );
        assert!(!vad.is_speaking());
    }

    #[test]
    fn vad_min_speech_duration_filters_short_bursts() {
        let config = VadConfig {
            min_speech_duration_ms: 300,
            silence_timeout_ms: 100, // short timeout for testing
            ..Default::default()
        };
        let mut vad = VoiceActivityDetector::new(config);

        // Very short burst: speak at 0ms, silence at 50ms, wait until 200ms
        vad.process_chunk(&loud_signal(1600, 0.1), 0);
        vad.process_chunk(&silence(1600), 50);
        let event = vad.process_chunk(&silence(1600), 200);

        // Duration is 200ms < 300ms min_speech → discarded (None)
        assert_eq!(event, None);
        assert!(!vad.is_speaking());
    }

    #[test]
    fn vad_continuous_speech_stays_speaking() {
        let mut vad = VoiceActivityDetector::new(VadConfig::default());

        // Start speaking
        vad.process_chunk(&loud_signal(1600, 0.1), 0);

        // Continue speaking for many chunks
        for i in 1..20 {
            let event = vad.process_chunk(&loud_signal(1600, 0.1), i * 100);
            assert_eq!(event, None);
            assert!(vad.is_speaking());
        }
    }

    #[test]
    fn vad_silence_wait_resets_on_new_speech() {
        let mut vad = VoiceActivityDetector::new(VadConfig::default());

        // Start speaking
        vad.process_chunk(&loud_signal(1600, 0.1), 0);

        // Go quiet (enters SilenceWait)
        vad.process_chunk(&silence(1600), 100);

        // Before timeout: speak again
        let event = vad.process_chunk(&loud_signal(1600, 0.1), 400);
        assert_eq!(event, None); // No SpeechEnd — speech continued
        assert!(vad.is_speaking());

        // Now go quiet again and wait past timeout
        vad.process_chunk(&silence(1600), 500);
        let event = vad.process_chunk(&silence(1600), 1400);
        assert_eq!(
            event,
            Some(VadEvent::SpeechEnd {
                start_ms: 0,
                end_ms: 1400,
            })
        );
    }
}
