//! Talkiwi Desktop — Tauri v2 application shell.
//! Phase 3: SessionManager, commands, config integration.

mod commands;
pub mod session_manager;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use tauri::{AppHandle, Manager, RunEvent, WebviewWindow};

use talkiwi_capture::{ClipboardCapture, PageCapture, SelectionCapture};
use talkiwi_core::config::AppConfig;
use talkiwi_core::traits::asr::AsrProvider;
use talkiwi_engine::IntentEngine;
use talkiwi_track::{ActionTrack, SpeakTrack};

use crate::session_manager::SessionManager;

/// Shared application state managed by Tauri.
///
/// `db` is the single SQLite connection shared between SessionManager and
/// history/config commands. Uses `std::sync::Mutex` because `rusqlite::Connection`
/// is `!Send` and DB operations are synchronous.
pub(crate) struct AppState {
    pub(crate) session_manager: SessionManager,
    pub(crate) db: Arc<Mutex<Connection>>,
    pub(crate) data_dir: PathBuf,
    pub(crate) output_dir: PathBuf,
    pub(crate) config_path: PathBuf,
    pub(crate) config: Mutex<AppConfig>,
    pub(crate) app_handle: AppHandle,
}

/// Load settings.json, falling back to defaults if missing.
fn load_settings(config_path: &Path) -> anyhow::Result<AppConfig> {
    if config_path.exists() {
        let content = std::fs::read_to_string(config_path)?;
        Ok(serde_json::from_str(&content)?)
    } else {
        let config = AppConfig::default();
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&config)?;
        std::fs::write(config_path, json)?;
        Ok(config)
    }
}

/// Create ASR provider from config.
///
/// Uses `model_manager::resolve_model_path` for consistent path resolution
/// across the app (same logic as `model_status` command).
fn init_asr_provider(config: &AppConfig, data_dir: &Path) -> anyhow::Result<Box<dyn AsrProvider>> {
    match config.asr.active_provider.as_str() {
        "whisper-local" => {
            let size_name = config.asr.whisper_model_size.as_deref().unwrap_or("small");

            let model_path = talkiwi_asr::resolve_model_path(
                config.asr.whisper_model_path.as_deref(),
                size_name,
                data_dir,
            );

            let runtime_config = talkiwi_asr::WhisperRuntimeConfig::from(&config.asr);
            let provider = talkiwi_asr::WhisperLocalProvider::with_config(
                model_path.to_string_lossy().to_string(),
                runtime_config,
            );
            Ok(Box::new(provider))
        }
        #[cfg(feature = "openai")]
        "openai-whisper" => {
            let api_key = config
                .asr
                .cloud_api_key
                .as_deref()
                .filter(|k| !k.is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!("OpenAI Whisper requires asr.cloud_api_key to be set")
                })?;

            let mut openai_config = talkiwi_asr::OpenAiWhisperConfig::new(api_key);
            openai_config.language = config.asr.language.clone();

            if let Some(prompt) = config.asr.initial_prompt.as_deref() {
                openai_config.prompt = Some(prompt.to_string());
            }

            openai_config.max_segment_ms = config.asr.max_segment_ms;
            openai_config.vad_enabled = config.asr.vad_enabled;
            openai_config.vad_threshold = config.asr.vad_threshold;
            openai_config.vad_silence_timeout_ms = config.asr.vad_silence_timeout_ms;
            openai_config.vad_min_speech_duration_ms = config.asr.vad_min_speech_duration_ms;

            let provider = talkiwi_asr::OpenAiWhisperProvider::new(openai_config);
            Ok(Box::new(provider))
        }
        other => Err(anyhow::anyhow!("Unknown ASR provider: {}", other)),
    }
}

/// Create ASR provider from cached config in AppState.
pub(crate) fn init_asr_provider_from_state(
    state: &AppState,
) -> anyhow::Result<Box<dyn AsrProvider>> {
    let config = state
        .config
        .lock()
        .map_err(|e| anyhow::anyhow!("config lock poisoned: {}", e))?;
    init_asr_provider(&config, &state.data_dir)
}

fn resolve_configured_path(raw: &str, app_data_dir: &Path) -> PathBuf {
    let expanded = if raw == "~" {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| app_data_dir.to_path_buf())
    } else if raw.starts_with("~/") {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| app_data_dir.to_path_buf())
            .join(raw.trim_start_matches("~/"))
    } else {
        PathBuf::from(raw)
    };

    if expanded.is_absolute() {
        expanded
    } else {
        app_data_dir.join(expanded)
    }
}

/// Configure the ball window for transparent background on macOS.
///
/// The ball renders a circular SiriWave canvas on a fully transparent window.
/// Vibrancy is intentionally NOT applied — it would fill the window with
/// system blur material, defeating the circular transparent look.
fn setup_ball_window(_window: &WebviewWindow) {
    // Transparent background is configured via tauri.conf.json:
    //   "transparent": true + "decorations": false
    // Combined with CSS: html, body { background: transparent }
    // and macOSPrivateApi: true in the app config.
    //
    // No additional Rust-side setup needed — the window is already transparent.
}

/// Application entry point.
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let config_path = data_dir.join("config").join("settings.json");

            let config = load_settings(&config_path)?;
            let output_dir = resolve_configured_path(&config.storage.output_dir, &data_dir);
            std::fs::create_dir_all(&output_dir)?;

            // Init DB — single connection shared across the entire app
            let db_path = resolve_configured_path(&config.storage.db_path, &data_dir);
            if let Some(parent) = db_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let db = talkiwi_db::init_database(&db_path)?;
            let db = Arc::new(Mutex::new(db));

            // Construct audio source + SpeakTrack
            let audio_capture = talkiwi_asr::AudioCapture::new();
            let speak_track = SpeakTrack::new(Box::new(audio_capture));

            // Construct ActionTrack with registered captures.
            // Captures take a placeholder session_id; the real session_id is set
            // per-event when ActionTrack starts a new session.
            let placeholder_id = uuid::Uuid::nil();
            let mut action_track = ActionTrack::new();
            if config.capture.selection_enabled {
                action_track.register(Box::new(SelectionCapture::new(placeholder_id)));
            }
            if config.capture.clipboard_enabled {
                action_track.register(Box::new(ClipboardCapture::new(placeholder_id)));
            }
            if config.capture.page_enabled {
                action_track.register(Box::new(PageCapture::new(placeholder_id)));
            }

            // Construct IntentEngine
            let intent_provider: Box<dyn talkiwi_engine::IntentProvider> =
                match config.intent.active_provider.as_str() {
                    "ollama" => Box::new(talkiwi_engine::ollama_provider::OllamaProvider::new(
                        &config.intent.ollama_url,
                        Some(config.intent.ollama_model.clone()),
                    )),
                    other => {
                        return Err(anyhow::anyhow!("Unknown intent provider: {}", other).into());
                    }
                };
            let engine = IntentEngine::new(intent_provider, None);

            // Construct SessionManager — shares the same DB connection
            let session_manager =
                SessionManager::new(speak_track, action_track, engine, Arc::clone(&db));

            app.manage(AppState {
                session_manager,
                db,
                data_dir,
                output_dir,
                config_path,
                config: Mutex::new(config),
                app_handle: app.handle().clone(),
            });

            // Configure ball window (transparency via tauri.conf.json)
            if let Some(ball_window) = app.get_webview_window("ball") {
                setup_ball_window(&ball_window);
            }

            // Ensure editor starts hidden
            if let Some(editor_window) = app.get_webview_window("editor") {
                let _ = editor_window.hide();
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::session::session_start,
            commands::session::session_stop,
            commands::session::session_state,
            commands::session::session_regenerate,
            commands::capture::capture_screenshot,
            commands::history::history_list,
            commands::history::history_detail,
            commands::config::config_get,
            commands::config::config_update,
            commands::config::config_update_many,
            commands::permissions::permissions_check,
            commands::permissions::permissions_request,
            commands::model::model_status,
        ])
        .build(tauri::generate_context!())
        .expect("Failed to build Talkiwi")
        .run(|_app, event| {
            // Prevent app exit when the editor window is closed.
            // The ball window is the primary — only quit when explicitly requested.
            if let RunEvent::ExitRequested { api, .. } = &event {
                api.prevent_exit();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::resolve_configured_path;
    use std::path::{Path, PathBuf};

    #[test]
    fn resolve_configured_path_expands_home() {
        let home = std::env::var("HOME").expect("HOME should be set in test env");
        let resolved = resolve_configured_path("~/Talkiwi/data/talkiwi.db", Path::new("/tmp/app"));
        assert_eq!(
            resolved,
            PathBuf::from(home).join("Talkiwi/data/talkiwi.db")
        );
    }

    #[test]
    fn resolve_configured_path_keeps_absolute_paths() {
        let resolved = resolve_configured_path("/var/tmp/talkiwi.db", Path::new("/tmp/app"));
        assert_eq!(resolved, PathBuf::from("/var/tmp/talkiwi.db"));
    }

    #[test]
    fn resolve_configured_path_resolves_relative_paths_from_app_data_dir() {
        let resolved = resolve_configured_path("data/talkiwi.db", Path::new("/tmp/app"));
        assert_eq!(resolved, PathBuf::from("/tmp/app/data/talkiwi.db"));
    }
}
