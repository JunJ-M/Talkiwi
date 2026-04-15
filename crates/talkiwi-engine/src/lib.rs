pub mod assembler;
pub mod ollama_provider;
pub mod resolver;

pub use talkiwi_core::traits::intent::{IntentProvider, IntentRaw, RawReference};

use talkiwi_core::event::ActionEvent;
use talkiwi_core::locale::AssemblerLabels;
use talkiwi_core::output::{IntentCategory, IntentOutput, Reference, ReferenceStrategy, RiskLevel};
use talkiwi_core::session::SpeakSegment;
use talkiwi_core::telemetry::IntentTelemetry;
use tracing::warn;
use uuid::Uuid;

use crate::assembler::assemble;
use crate::resolver::Resolver;

/// The system prompt template for intent restructuring.
pub const SYSTEM_PROMPT: &str = include_str!("prompts/system.txt");

/// IntentEngine: timeline + resolver + LLM restructure + assembly.
///
/// The engine always returns an `IntentOutput`. Provider failures degrade to a
/// transcript-only fallback instead of aborting the session.
pub struct IntentEngine {
    provider: Box<dyn IntentProvider>,
    resolver: Resolver,
    labels: AssemblerLabels,
    system_prompt: String,
}

impl IntentEngine {
    pub fn new(provider: Box<dyn IntentProvider>, system_prompt: Option<String>) -> Self {
        Self {
            provider,
            resolver: Resolver::new(),
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
            resolver: Resolver::new(),
            labels,
            system_prompt: system_prompt.unwrap_or_else(|| SYSTEM_PROMPT.to_string()),
        }
    }

    pub async fn process_with_telemetry(
        &self,
        segments: &[SpeakSegment],
        events: &[ActionEvent],
        session_id: Uuid,
    ) -> anyhow::Result<(IntentOutput, IntentTelemetry)> {
        let started = std::time::Instant::now();
        let timeline = talkiwi_core::timeline::align_timeline(segments, events);
        let summary = talkiwi_core::timeline::timeline_to_summary_bounded(&timeline, 4_096);
        let transcript = build_transcript(segments);
        let resolver_refs = self.resolver.resolve(segments, events);

        let mut retry_count = 0;
        let mut fallback_used = false;
        let mut schema_valid = false;

        let raw = match self
            .try_restructure_with_retry(&transcript, &summary, &mut retry_count, &mut schema_valid)
            .await
        {
            Ok(raw) => raw,
            Err(err) => {
                warn!(error = %err, "intent restructure failed, falling back to transcript-only output");
                fallback_used = true;
                let repair_attempted = retry_count > 0;
                let output = self.build_fallback_output(&transcript, session_id);
                let telemetry = IntentTelemetry {
                    session_id,
                    timestamp: current_time_ms(),
                    provider_latency_ms: started.elapsed().as_millis() as u64,
                    provider_success: false,
                    retry_count,
                    fallback_used,
                    schema_valid,
                    repair_attempted,
                    output_confidence: output.output_confidence,
                    reference_count: 0,
                    low_confidence_refs: 0,
                    intent_category: format!("{:?}", output.intent_category).to_lowercase(),
                };
                return Ok((output, telemetry));
            }
        };

        let llm_refs = convert_raw_references(&raw.references, events);
        let references = merge_references(resolver_refs, llm_refs);
        let output_confidence = compute_output_confidence(segments, &references, &raw, events);
        let risk_level = RiskLevel::from_confidence(output_confidence);

        let mut output = assemble(&raw, events, &references, session_id, &self.labels);
        output.intent_category = IntentCategory::from_llm_output(&raw.intent);
        output.output_confidence = output_confidence;
        output.risk_level = risk_level;

        let repair_attempted = retry_count > 0;
        let telemetry = IntentTelemetry {
            session_id,
            timestamp: current_time_ms(),
            provider_latency_ms: started.elapsed().as_millis() as u64,
            provider_success: true,
            retry_count,
            fallback_used,
            schema_valid,
            repair_attempted,
            output_confidence,
            reference_count: output.references.len(),
            low_confidence_refs: output
                .references
                .iter()
                .filter(|reference| reference.confidence < 0.5)
                .count(),
            intent_category: format!("{:?}", output.intent_category).to_lowercase(),
        };

        Ok((output, telemetry))
    }

    pub async fn process(
        &self,
        segments: &[SpeakSegment],
        events: &[ActionEvent],
        session_id: Uuid,
    ) -> anyhow::Result<IntentOutput> {
        self.process_with_telemetry(segments, events, session_id)
            .await
            .map(|(output, _telemetry)| output)
    }

    async fn try_restructure_with_retry(
        &self,
        transcript: &str,
        summary: &str,
        retry_count: &mut u32,
        schema_valid: &mut bool,
    ) -> anyhow::Result<IntentRaw> {
        let mut last_error: Option<anyhow::Error> = None;

        for attempt in 0..=1 {
            match self
                .provider
                .restructure(transcript, summary, &self.system_prompt)
                .await
            {
                Ok(raw) => {
                    if validate_intent_raw(&raw) {
                        *schema_valid = true;
                        return Ok(raw);
                    }
                    last_error = Some(anyhow::anyhow!("intent raw schema invalid"));
                }
                Err(err) => {
                    last_error = Some(err);
                }
            }

            if attempt == 0 {
                *retry_count += 1;
                tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("intent restructure failed")))
    }

    fn build_fallback_output(&self, transcript: &str, session_id: Uuid) -> IntentOutput {
        IntentOutput {
            session_id,
            task: transcript.to_string(),
            intent: "query".to_string(),
            intent_category: IntentCategory::Unknown,
            constraints: vec![],
            missing_context: vec!["LLM 服务不可用，结果退化为原始转录".to_string()],
            restructured_speech: transcript.to_string(),
            final_markdown: transcript.to_string(),
            artifacts: vec![],
            references: vec![],
            output_confidence: 0.0,
            risk_level: RiskLevel::High,
        }
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
                confidence: 0.85,
                strategy: ReferenceStrategy::SemanticSimilarity,
                user_confirmed: false,
            })
        })
        .collect()
}

fn build_transcript(segments: &[SpeakSegment]) -> String {
    segments
        .iter()
        .map(|segment| segment.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

fn validate_intent_raw(raw: &IntentRaw) -> bool {
    !raw.task.trim().is_empty() && !raw.restructured_speech.trim().is_empty()
}

fn merge_references(resolver_refs: Vec<Reference>, llm_refs: Vec<Reference>) -> Vec<Reference> {
    let mut merged: Vec<Reference> = Vec::new();
    for candidate in resolver_refs.into_iter().chain(llm_refs.into_iter()) {
        if let Some(existing) = merged.iter_mut().find(|reference| {
            reference.spoken_text == candidate.spoken_text
                && reference.resolved_event_idx == candidate.resolved_event_idx
        }) {
            if candidate.confidence > existing.confidence {
                *existing = candidate;
            }
        } else {
            merged.push(candidate);
        }
    }
    merged
}

fn compute_output_confidence(
    segments: &[SpeakSegment],
    references: &[Reference],
    raw: &IntentRaw,
    events: &[ActionEvent],
) -> f32 {
    let segment_score = if segments.is_empty() {
        0.5
    } else {
        segments
            .iter()
            .map(|segment| segment.confidence)
            .sum::<f32>()
            / segments.len() as f32
    };
    let reference_score = if references.is_empty() {
        if events.is_empty() {
            0.8
        } else {
            0.45
        }
    } else {
        references
            .iter()
            .map(|reference| reference.confidence)
            .sum::<f32>()
            / references.len() as f32
    };
    let intent_score = match IntentCategory::from_llm_output(&raw.intent) {
        IntentCategory::Unknown => 0.35,
        _ => 0.9,
    };

    ((segment_score * 0.4) + (reference_score * 0.35) + (intent_score * 0.25)).clamp(0.0, 1.0)
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
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
            observed_offset_ms: Some(offset_ms),
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
            curation: Default::default(),
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
        assert_eq!(output.intent_category, IntentCategory::Rewrite);
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

    #[tokio::test]
    async fn process_with_telemetry_falls_back_when_provider_fails() {
        struct FailingProvider;

        #[async_trait::async_trait]
        impl IntentProvider for FailingProvider {
            fn id(&self) -> &str {
                "failing"
            }
            fn name(&self) -> &str {
                "Failing"
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
                anyhow::bail!("boom")
            }
        }

        let engine = IntentEngine::new(Box::new(FailingProvider), None);
        let segments = vec![make_segment("帮我做个总结", 0, 1000)];
        let (output, telemetry) = engine
            .process_with_telemetry(&segments, &[], Uuid::new_v4())
            .await
            .unwrap();
        assert!(telemetry.fallback_used);
        assert_eq!(output.intent_category, IntentCategory::Unknown);
        assert_eq!(output.risk_level, RiskLevel::High);
    }
}
