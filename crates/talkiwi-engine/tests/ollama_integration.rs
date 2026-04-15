//! Integration tests for OllamaProvider with a running Ollama instance.
//!
//! These tests require Ollama to be running locally with a model pulled.
//!
//! To run:
//! ```bash
//! # Start Ollama
//! ollama serve &
//!
//! # Pull a small model
//! ollama pull qwen2.5:1.5b
//!
//! # Run integration tests
//! OLLAMA_MODEL=qwen2.5:1.5b cargo test -p talkiwi-engine -- --ignored ollama
//! ```

use talkiwi_engine::ollama_provider::OllamaProvider;
use talkiwi_engine::{IntentEngine, IntentProvider, SYSTEM_PROMPT};

fn get_model() -> String {
    std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "qwen2.5:1.5b".to_string())
}

#[tokio::test]
#[ignore = "requires running Ollama instance"]
async fn ollama_is_available() {
    let provider = OllamaProvider::default_local();
    assert!(
        provider.is_available().await,
        "Ollama should be running at localhost:11434"
    );
}

#[tokio::test]
#[ignore = "requires running Ollama instance with a model"]
async fn ollama_restructure_simple_task() {
    let model = get_model();
    let provider = OllamaProvider::new("http://localhost:11434", Some(model.clone()));

    let transcript = "帮我重写这段代码，用 Rust 实现";
    let events_summary =
        "[0ms] selection.text: fn main() { println!(\"hello\"); } (VSCode, main.rs)";

    let result = provider
        .restructure(transcript, events_summary, SYSTEM_PROMPT)
        .await;

    match result {
        Ok(raw) => {
            println!("=== Ollama Restructure Result (model: {}) ===", model);
            println!("  task: {}", raw.task);
            println!("  intent: {}", raw.intent);
            println!("  constraints: {:?}", raw.constraints);
            println!("  missing_context: {:?}", raw.missing_context);
            println!("  restructured: {}", raw.restructured_speech);

            assert!(!raw.task.is_empty(), "task should not be empty");
            assert!(!raw.intent.is_empty(), "intent should not be empty");
            assert!(
                !raw.restructured_speech.is_empty(),
                "restructured_speech should not be empty"
            );
        }
        Err(e) => {
            panic!("Ollama restructure failed: {e}");
        }
    }
}

#[tokio::test]
#[ignore = "requires running Ollama instance with a model"]
async fn ollama_full_engine_pipeline() {
    use talkiwi_core::event::{ActionEvent, ActionPayload, ActionType};
    use talkiwi_core::session::SpeakSegment;
    use uuid::Uuid;

    let model = get_model();
    let provider = OllamaProvider::new("http://localhost:11434", Some(model.clone()));
    let engine = IntentEngine::new(Box::new(provider), None);

    let segments = vec![SpeakSegment {
        text: "帮我看一下这段代码有什么问题，然后修复一下".to_string(),
        start_ms: 2000,
        end_ms: 5000,
        confidence: 0.95,
        is_final: true,
    }];

    let events = vec![ActionEvent {
        id: Uuid::new_v4(),
        session_id: Uuid::new_v4(),
        timestamp: 1712900000000,
        session_offset_ms: 1000,
        observed_offset_ms: Some(1000),
        duration_ms: None,
        action_type: ActionType::SelectionText,
        plugin_id: "builtin".to_string(),
        payload: ActionPayload::SelectionText {
            text: "fn add(a: i32, b: i32) -> i32 {\n    a - b  // bug: should be a + b\n}"
                .to_string(),
            app_name: "VSCode".to_string(),
            window_title: "math.rs".to_string(),
            char_count: 65,
        },
        semantic_hint: None,
        confidence: 1.0,
        curation: Default::default(),
    }];

    let session_id = Uuid::new_v4();
    let result = engine.process(&segments, &events, session_id).await;

    match result {
        Ok(output) => {
            println!("=== Full Engine Pipeline Result (model: {}) ===", model);
            println!("  session_id: {}", output.session_id);
            println!("  task: {}", output.task);
            println!("  intent: {}", output.intent);
            println!("  references: {} found", output.references.len());
            println!("  artifacts: {} found", output.artifacts.len());
            println!("  --- final markdown ---");
            println!("{}", output.final_markdown);
            println!("  --- end ---");

            assert_eq!(output.session_id, session_id);
            assert!(!output.task.is_empty());
            assert!(!output.final_markdown.is_empty());
            // Should have resolved "这段代码" reference
            assert!(
                !output.references.is_empty(),
                "Should resolve deictic reference '这段代码'"
            );
        }
        Err(e) => {
            panic!("Full pipeline failed: {e}");
        }
    }
}
