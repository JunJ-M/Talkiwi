use serde::{Deserialize, Serialize};

fn default_whisper_language() -> Option<String> {
    Some("zh".to_string())
}

fn default_whisper_beam_size() -> u32 {
    5
}

fn default_condition_on_previous_text() -> bool {
    true
}

fn default_initial_prompt() -> Option<String> {
    Some(
        "以下是普通话中文口述，混合英文术语。常见英文词：code, function, API, bug, debug, error, deploy, commit, PR, merge, build, test, React, TypeScript, Rust, Python。请准确转写中英混合内容，保持英文原文不翻译。"
            .to_string(),
    )
}

fn default_vad_enabled() -> bool {
    true
}

fn default_vad_threshold() -> f32 {
    0.02
}

fn default_vad_silence_timeout_ms() -> u64 {
    600
}

fn default_vad_min_speech_duration_ms() -> u64 {
    250
}

fn default_max_segment_ms() -> u64 {
    10_000
}

fn default_temperature() -> f32 {
    0.0
}

fn default_input_gain_db() -> f32 {
    8.0
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AudioConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_device_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_device_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AsrConfig {
    pub active_provider: String,
    pub whisper_model_path: Option<String>,
    pub whisper_model_size: Option<String>,
    pub language: Option<String>,
    pub beam_size: u32,
    pub condition_on_previous_text: bool,
    pub initial_prompt: Option<String>,
    pub vad_enabled: bool,
    pub vad_threshold: f32,
    pub vad_silence_timeout_ms: u64,
    pub vad_min_speech_duration_ms: u64,
    pub max_segment_ms: u64,
    pub input_gain_db: f32,
    /// Decoding temperature (0.0 = greedy/deterministic, higher = more creative).
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    /// API key for cloud ASR providers (e.g., OpenAI Whisper API).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud_api_key: Option<String>,
}

impl Default for AsrConfig {
    fn default() -> Self {
        Self {
            active_provider: "whisper-local".to_string(),
            whisper_model_path: None,
            whisper_model_size: Some("medium".to_string()),
            language: default_whisper_language(),
            beam_size: default_whisper_beam_size(),
            condition_on_previous_text: default_condition_on_previous_text(),
            initial_prompt: default_initial_prompt(),
            vad_enabled: default_vad_enabled(),
            vad_threshold: default_vad_threshold(),
            vad_silence_timeout_ms: default_vad_silence_timeout_ms(),
            vad_min_speech_duration_ms: default_vad_min_speech_duration_ms(),
            max_segment_ms: default_max_segment_ms(),
            input_gain_db: default_input_gain_db(),
            temperature: default_temperature(),
            cloud_api_key: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentConfig {
    pub active_provider: String,
    pub ollama_url: String,
    pub ollama_model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud_api_key: Option<String>,
}

impl Default for IntentConfig {
    fn default() -> Self {
        Self {
            active_provider: "ollama".to_string(),
            ollama_url: "http://localhost:11434".to_string(),
            ollama_model: "qwen2.5:7b".to_string(),
            cloud_api_key: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaptureConfig {
    pub selection_enabled: bool,
    pub screenshot_enabled: bool,
    pub clipboard_enabled: bool,
    pub page_enabled: bool,
    pub link_enabled: bool,
    pub file_enabled: bool,
    pub selection_poll_interval_ms: u64,
    pub clipboard_poll_interval_ms: u64,
    pub selection_min_chars: usize,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            selection_enabled: true,
            screenshot_enabled: true,
            clipboard_enabled: true,
            page_enabled: true,
            link_enabled: true,
            file_enabled: true,
            selection_poll_interval_ms: 200,
            clipboard_poll_interval_ms: 500,
            selection_min_chars: 3,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    pub panel_width: u32,
    pub panel_side: String,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            panel_width: 360,
            panel_side: "right".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub output_dir: String,
    pub db_path: String,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            output_dir: "~/Talkiwi/sessions".to_string(),
            db_path: "~/Talkiwi/data/talkiwi.db".to_string(),
        }
    }
}

/// Application configuration — single source of truth (settings.json).
/// Each sub-config falls back to its Default when missing from JSON.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub audio: AudioConfig,
    #[serde(default)]
    pub asr: AsrConfig,
    #[serde(default)]
    pub intent: IntentConfig,
    #[serde(default)]
    pub capture: CaptureConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub storage: StorageConfig,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_config_default_round_trip() {
        let config = AppConfig::default();
        let json = serde_json::to_string_pretty(&config).unwrap();
        let deserialized: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.asr.active_provider, "whisper-local");
        assert_eq!(deserialized.intent.ollama_model, "qwen2.5:7b");
        assert_eq!(deserialized.asr.language, Some("zh".to_string()));
        assert_eq!(deserialized.asr.beam_size, 5);
        assert!(deserialized.asr.vad_enabled);
        assert_eq!(deserialized.asr.input_gain_db, 8.0);
        assert_eq!(deserialized.audio.input_device_id, None);
        assert_eq!(deserialized.capture.selection_poll_interval_ms, 200);
        assert_eq!(deserialized.ui.panel_width, 360);
    }

    #[test]
    fn app_config_partial_json_all_keys() {
        let json = r#"{
            "asr": { "active_provider": "openai-whisper", "language": "zh" },
            "intent": { "active_provider": "ollama", "ollama_url": "http://localhost:11434", "ollama_model": "qwen2.5:7b" },
            "audio": { "input_device_id": "built-in", "input_device_name": "Built-in Mic" },
            "capture": {
                "selection_enabled": false, "screenshot_enabled": true, "clipboard_enabled": true,
                "page_enabled": true, "link_enabled": true, "file_enabled": true,
                "selection_poll_interval_ms": 200, "clipboard_poll_interval_ms": 500,
                "selection_min_chars": 3
            },
            "ui": { "panel_width": 400, "panel_side": "left" },
            "storage": { "output_dir": "~/Talkiwi/sessions", "db_path": "~/Talkiwi/data/talkiwi.db" }
        }"#;

        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.asr.active_provider, "openai-whisper");
        assert_eq!(config.asr.language, Some("zh".to_string()));
        assert_eq!(config.asr.beam_size, 5);
        assert!(config.asr.condition_on_previous_text);
        assert_eq!(config.asr.input_gain_db, 8.0);
        assert_eq!(config.audio.input_device_id.as_deref(), Some("built-in"));
        assert!(!config.capture.selection_enabled);
        assert_eq!(config.ui.panel_side, "left");
    }

    #[test]
    fn app_config_missing_sub_configs_use_defaults() {
        // Only asr provided — all other sub-configs should fall back to Default
        let json = r#"{ "asr": { "active_provider": "openai-whisper" } }"#;
        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.asr.active_provider, "openai-whisper");
        assert_eq!(config.intent.ollama_model, "qwen2.5:7b"); // default
        assert_eq!(config.asr.language, Some("zh".to_string()));
        assert_eq!(config.asr.max_segment_ms, 10_000);
        assert_eq!(config.asr.input_gain_db, 8.0);
        assert_eq!(config.audio.input_device_name, None);
        assert!(config.capture.selection_enabled); // default
        assert_eq!(config.ui.panel_width, 360); // default
    }

    #[test]
    fn app_config_empty_json_uses_all_defaults() {
        let config: AppConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(config.asr.active_provider, "whisper-local");
        assert_eq!(config.intent.active_provider, "ollama");
        assert_eq!(config.asr.initial_prompt, default_initial_prompt());
    }

    #[test]
    fn asr_config_missing_new_fields_uses_defaults() {
        let json = r#"{
            "active_provider": "whisper-local",
            "whisper_model_size": "tiny"
        }"#;

        let config: AsrConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.whisper_model_size, Some("tiny".to_string()));
        assert_eq!(config.language, Some("zh".to_string()));
        assert_eq!(config.beam_size, 5);
        assert!(config.vad_enabled);
        assert_eq!(config.max_segment_ms, 10_000);
        assert_eq!(config.input_gain_db, 8.0);
        assert_eq!(config.temperature, 0.0);
    }
}
