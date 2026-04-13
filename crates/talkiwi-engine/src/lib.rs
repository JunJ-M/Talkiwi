pub mod assembler;
pub mod ollama_provider;

pub use talkiwi_core::traits::intent::{IntentProvider, IntentRaw, RawReference};

use talkiwi_core::event::ActionEvent;
use talkiwi_core::locale::AssemblerLabels;
use talkiwi_core::output::{IntentOutput, Reference, ReferenceStrategy};
use talkiwi_core::session::SpeakSegment;
use uuid::Uuid;

use crate::assembler::assemble;

/// The system prompt template for intent restructuring.
pub const SYSTEM_PROMPT: &str = include_str!("prompts/system.txt");

/// IntentEngine: 3-step pipeline for intent recognition.
///
/// 1. Timeline alignment (via talkiwi_core::timeline)
/// 2. LLM intent restructuring + reference resolution (IntentProvider)
/// 3. Markdown assembly (Assembler)
pub struct IntentEngine {
    provider: Box<dyn IntentProvider>,
    labels: AssemblerLabels,
    system_prompt: String,
}

impl IntentEngine {
    pub fn new(provider: Box<dyn IntentProvider>, system_prompt: Option<String>) -> Self {
        Self {
            provider,
            labels: AssemblerLabels::default(),
            system_prompt: system_prompt.unwrap_or_else(|| SYSTEM_PROMPT.to_string()),
        }
    }

    /// Create an IntentEngine with custom assembler labels.
    pub fn with_labels(
        provider: Box<dyn IntentProvider>,
        labels: AssemblerLabels,
        system_prompt: Option<String>,
    ) -> Self {
        Self {
            provider,
            labels,
            system_prompt: system_prompt.unwrap_or_else(|| SYSTEM_PROMPT.to_string()),
        }
    }

    /// Process segments and events through the 3-step pipeline.
    pub async fn process(
        &self,
        segments: &[SpeakSegment],
        events: &[ActionEvent],
        session_id: Uuid,
    ) -> anyhow::Result<IntentOutput> {
        // Step 1: Timeline alignment
        let timeline = talkiwi_core::timeline::align_timeline(segments, events);
        let summary = talkiwi_core::timeline::timeline_to_summary(&timeline);

        // Step 2: LLM intent restructuring (includes reference resolution)
        let transcript: String = segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        let raw = self
            .provider
            .restructure(&transcript, &summary, &self.system_prompt)
            .await?;

        // Step 3: Convert LLM references + assemble markdown
        let references = convert_raw_references(&raw.references, events);
        let output = assemble(&raw, events, &references, session_id, &self.labels);

        Ok(output)
    }
}

/// Convert LLM-produced `RawReference`s (index-based) into `Reference`s (ID-based).
///
/// Invalid indices are silently skipped — the LLM may hallucinate indices.
fn convert_raw_references(raw_refs: &[RawReference], events: &[ActionEvent]) -> Vec<Reference> {
    raw_refs
        .iter()
        .filter_map(|raw| {
            events.get(raw.event_index).map(|event| Reference {
                spoken_text: raw.spoken_text.clone(),
                spoken_offset: 0,
                resolved_event_idx: raw.event_index,
                resolved_event_id: Some(event.id),
                confidence: 1.0,
                strategy: ReferenceStrategy::SemanticSimilarity,
                user_confirmed: false,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use talkiwi_core::event::{ActionPayload, ActionType};

    /// Mock IntentProvider for testing — returns a fixed IntentRaw.
    struct MockIntentProvider {
        response: IntentRaw,
    }

    impl MockIntentProvider {
        fn new(response: IntentRaw) -> Self {
            Self { response }
        }
    }

    #[async_trait::async_trait]
    impl IntentProvider for MockIntentProvider {
        fn id(&self) -> &str {
            "mock"
        }
        fn name(&self) -> &str {
            "Mock Provider"
        }
        fn requires_network(&self) -> bool {
            false
        }
        async fn is_available(&self) -> bool {
            true
        }
        async fn restructure(
            &self,
            _transcript: &str,
            _events_summary: &str,
            _system_prompt: &str,
        ) -> anyhow::Result<IntentRaw> {
            Ok(self.response.clone())
        }
    }

    fn make_segment(text: &str, start_ms: u64, end_ms: u64) -> SpeakSegment {
        SpeakSegment {
            text: text.to_string(),
            start_ms,
            end_ms,
            confidence: 0.95,
            is_final: true,
        }
    }

    fn make_event(action_type: ActionType, offset_ms: u64) -> ActionEvent {
        ActionEvent {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            timestamp: 1712900000000,
            session_offset_ms: offset_ms,
            duration_ms: None,
            action_type,
            plugin_id: "builtin".to_string(),
            payload: ActionPayload::SelectionText {
                text: "fn main() {}".to_string(),
                app_name: "VSCode".to_string(),
                window_title: "main.rs".to_string(),
                char_count: 12,
            },
            semantic_hint: None,
            confidence: 1.0,
        }
    }

    fn default_raw() -> IntentRaw {
        IntentRaw {
            task: "重写选中的函数".to_string(),
            intent: "rewrite".to_string(),
            constraints: vec!["使用 Rust".to_string()],
            missing_context: vec![],
            restructured_speech: "请帮我重写选中的代码函数".to_string(),
            references: vec![RawReference {
                spoken_text: "这段代码".to_string(),
                event_index: 0,
                reason: "用户提到代码，关联选中文字事件".to_string(),
            }],
        }
    }

    #[tokio::test]
    async fn intent_engine_full_pipeline_mock_provider() {
        let engine = IntentEngine::new(Box::new(MockIntentProvider::new(default_raw())), None);

        let segments = vec![make_segment("帮我重写这段代码", 5000, 7000)];
        let events = vec![make_event(ActionType::SelectionText, 3000)];
        let session_id = Uuid::new_v4();

        let output = engine
            .process(&segments, &events, session_id)
            .await
            .unwrap();

        assert_eq!(output.session_id, session_id);
        assert_eq!(output.task, "重写选中的函数");
        assert_eq!(output.intent, "rewrite");
        // Should have resolved the reference "这段代码" → SelectionText event
        assert!(!output.references.is_empty());
        // Structured mode (has events)
        assert!(output.final_markdown.contains("## 任务"));
        assert!(output.final_markdown.contains("## 上下文"));
    }

    #[tokio::test]
    async fn intent_engine_pure_voice_no_events() {
        let engine = IntentEngine::new(Box::new(MockIntentProvider::new(default_raw())), None);

        let segments = vec![make_segment("帮我写一个 Rust 函数", 1000, 3000)];
        let session_id = Uuid::new_v4();

        let output = engine.process(&segments, &[], session_id).await.unwrap();

        // Pure voice mode — just the restructured speech
        assert_eq!(output.final_markdown, "请帮我重写选中的代码函数");
        assert!(output.artifacts.is_empty());
    }

    #[tokio::test]
    async fn intent_engine_no_speech() {
        let raw = IntentRaw {
            task: "分析截图内容".to_string(),
            intent: "analyze".to_string(),
            constraints: vec![],
            missing_context: vec!["需要更多上下文".to_string()],
            restructured_speech: "分析提供的截图".to_string(),
            references: vec![],
        };
        let engine = IntentEngine::new(Box::new(MockIntentProvider::new(raw)), None);

        let events = vec![make_event(ActionType::SelectionText, 1000)];
        let session_id = Uuid::new_v4();

        let output = engine.process(&[], &events, session_id).await.unwrap();

        // Has events but no speech → structured mode, no references
        assert!(output.final_markdown.contains("## 任务"));
        assert!(output.references.is_empty());
    }
}
