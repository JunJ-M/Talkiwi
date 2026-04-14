pub mod audio_capture;
pub mod audio_input;
pub mod audio_source;
pub mod model_manager;
#[cfg(feature = "openai")]
pub mod openai_provider;
pub mod vad;
pub mod wav_writer;
pub mod whisper_provider;

pub use audio_capture::AudioCapture;
pub use audio_input::{AudioInputManager, SelectedAudioInput};
pub use audio_source::AudioSource;
pub use model_manager::{check_model_status, resolve_model_path, ModelSize, ModelStatus};
#[cfg(feature = "openai")]
pub use openai_provider::{OpenAiWhisperConfig, OpenAiWhisperProvider};
pub use talkiwi_core::traits::asr::{AsrProvider, AudioChunk};
pub use wav_writer::WavWriter;
pub use whisper_provider::{WhisperLocalProvider, WhisperRuntimeConfig};
