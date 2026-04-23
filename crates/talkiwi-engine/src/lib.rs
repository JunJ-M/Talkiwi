pub mod anchor;
pub mod assembler;
pub mod candidate;
pub mod convert;
pub mod importance;
pub mod ollama_provider;
pub mod payload_render;
pub mod resolver;
pub mod retrieval;

pub use talkiwi_core::traits::intent::{IntentProvider, IntentRaw, RawReference};

use talkiwi_core::event::ActionEvent;
use talkiwi_core::locale::AssemblerLabels;
use talkiwi_core::output::{IntentCategory, IntentOutput, Reference, RiskLevel};
use talkiwi_core::session::SpeakSegment;
use talkiwi_core::telemetry::IntentTelemetry;
use tracing::warn;
use uuid::Uuid;

use crate::assembler::{assemble_with_options, AssembleOptions};
use crate::resolver::Resolver;

/// The system prompt template for v1 intent restructuring.
pub const SYSTEM_PROMPT: &str = include_str!("prompts/system.txt");

/// The v2 system prompt that teaches the LLM to emit multi-target
/// references with `relation` and `segment_idx` fields. Used when the
/// engine takes the `restructure_v2` path.
pub const SYSTEM_PROMPT_V2: &str = include_str!("prompts/system_v2.txt");

/// IntentEngine: timeline + resolver + LLM restructure + assembly.
///
/// The engine always returns an `IntentOutput`. Provider failures degrade to a
/// transcript-only fallback instead of aborting the session.
pub struct IntentEngine {
    provider: Box<dyn IntentProvider>,
    resolver: Resolver,
    candidate_builder: candidate::CandidateBuilder,
    anchor_propagator: anchor::AnchorPropagator,
    importance_scorer: importance::ImportanceScorer,
    labels: AssemblerLabels,
    system_prompt: String,
}

impl IntentEngine {
    /// Build an engine using the v2 trace-annotation system prompt by
    /// default. Callers may override `system_prompt` — legacy callers
    /// that want the v1 prompt should pass `Some(SYSTEM_PROMPT.into())`.
    pub fn new(provider: Box<dyn IntentProvider>, system_prompt: Option<String>) -> Self {
        Self {
            provider,
            resolver: Resolver::new(),
            candidate_builder: candidate::CandidateBuilder::new(),
            anchor_propagator: anchor::AnchorPropagator::new(),
            importance_scorer: importance::ImportanceScorer::new(),
            labels: AssemblerLabels::default(),
            system_prompt: system_prompt.unwrap_or_else(|| SYSTEM_PROMPT_V2.to_string()),
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
            candidate_builder: candidate::CandidateBuilder::new(),
            anchor_propagator: anchor::AnchorPropagator::new(),
            importance_scorer: importance::ImportanceScorer::new(),
            labels,
            system_prompt: system_prompt.unwrap_or_else(|| SYSTEM_PROMPT_V2.to_string()),
        }
    }

    pub async fn process_with_telemetry(
        &self,
        segments: &[SpeakSegment],
        events: &[ActionEvent],
        session_id: Uuid,
    ) -> anyhow::Result<(IntentOutput, IntentTelemetry)> {
        let started = std::time::Instant::now();

        // ---------- v2 candidate + hints construction -----------------
        let candidate_sets = self.candidate_builder.build(segments, events);
        let hints = self.resolver.boost_hints(segments);
        let request = build_request_v2(segments, &candidate_sets, &hints);
        let candidate_size_stats = candidate_size_percentiles(&candidate_sets);

        // In the v2 path the regex `Resolver` is *demoted* — it only
        // contributes hints. References come exclusively from the LLM.
        // See tech plan §4.1 Pipeline diagram and §15 Phase 2 Go/No-go.
        let transcript = request.transcript();

        let mut retry_count = 0;
        let mut fallback_used = false;
        let mut schema_valid = false;

        let raw = match self
            .try_restructure_v2_with_retry(
                &request,
                events.len(),
                &mut retry_count,
                &mut schema_valid,
            )
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
                    candidate_set_size_p50: candidate_size_stats.0,
                    candidate_set_size_p95: candidate_size_stats.1,
                    references_by_relation: Default::default(),
                    anchor_propagations: 0,
                    importance_filtered_events: 0,
                    retrieval_chunk_count: 0,
                };
                return Ok((output, telemetry));
            }
        };

        let llm_refs = convert::convert_raw_references_v2(
            &raw.references,
            &request.candidates_per_segment,
            events,
        );
        let merged = dedup_references(llm_refs);
        let anchor_baseline = count_anchor_targets(&merged);
        let references = self.anchor_propagator.propagate(merged, events);
        let anchor_propagations =
            count_anchor_targets(&references).saturating_sub(anchor_baseline);

        let output_confidence = compute_output_confidence(segments, &references, &raw, events);
        let risk_level = RiskLevel::from_confidence(output_confidence);

        // Importance must be computed *before* assembly so the prompt
        // surface can filter low-score passive events. Retrieval uses
        // the same scores without any threshold (tech plan §9.3).
        let scores = self.importance_scorer.score_all(events, &references);
        let importance_filtered_events = scores
            .iter()
            .filter(|s| s.score < importance::DEFAULT_PROMPT_THRESHOLD)
            .count();

        let mut output = assemble_with_options(
            &raw,
            events,
            &references,
            session_id,
            &self.labels,
            AssembleOptions {
                importance_scores: Some(&scores),
                prompt_threshold: importance::DEFAULT_PROMPT_THRESHOLD,
            },
        );
        output.intent_category = IntentCategory::from_llm_output(&raw.intent);
        output.output_confidence = output_confidence;
        output.risk_level = risk_level;

        // Retrieval surface: always emit one chunk per live event,
        // regardless of the prompt-importance threshold. Embed-time
        // ranking happens at query time.
        output.retrieval_chunks =
            retrieval::render_chunks(events, segments, &references, &scores);

        let references_by_relation = group_references_by_relation(&references);
        let retrieval_chunk_count = output.retrieval_chunks.len();

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
            candidate_set_size_p50: candidate_size_stats.0,
            candidate_set_size_p95: candidate_size_stats.1,
            references_by_relation,
            anchor_propagations,
            importance_filtered_events,
            retrieval_chunk_count,
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

    async fn try_restructure_v2_with_retry(
        &self,
        request: &talkiwi_core::traits::intent::IntentRequestV2,
        events_len: usize,
        retry_count: &mut u32,
        schema_valid: &mut bool,
    ) -> anyhow::Result<IntentRaw> {
        let mut last_error: Option<anyhow::Error> = None;
        let mut last_raw: Option<IntentRaw> = None;

        for attempt in 0..=1 {
            match self
                .provider
                .restructure_v2(request, &self.system_prompt)
                .await
            {
                Ok(raw) => {
                    if validate_intent_raw_v2(&raw, request, events_len) {
                        *schema_valid = true;
                        return Ok(raw);
                    }
                    last_raw = Some(raw);
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

        // Partial degradation (tech plan §7.4): if the provider produced a
        // payload whose core fields are valid but whose references failed
        // v2 schema checks, keep the structured output and strip the
        // malformed reference list. `schema_valid` stays `false` so the
        // downstream telemetry still flags the incident.
        if let Some(mut raw) = last_raw {
            if validate_intent_raw_basic(&raw) {
                raw.references.clear();
                return Ok(raw);
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
            retrieval_chunks: vec![],
        }
    }
}

/// Assemble the v2 structured LLM request from engine outputs.
fn build_request_v2(
    segments: &[SpeakSegment],
    candidate_sets: &[candidate::SegmentCandidates],
    hints: &[resolver::ResolverHint],
) -> talkiwi_core::traits::intent::IntentRequestV2 {
    use talkiwi_core::traits::intent::{
        CandidateRef, IntentRequestV2, ResolverHintRef, SegmentCandidatesRef, SegmentRef,
    };

    let segment_refs = segments
        .iter()
        .enumerate()
        .map(|(idx, s)| SegmentRef {
            idx,
            start_ms: s.start_ms,
            end_ms: s.end_ms,
            text: s.text.clone(),
        })
        .collect();

    let candidates_per_segment = candidate_sets
        .iter()
        .map(|bundle| SegmentCandidatesRef {
            segment_idx: bundle.segment_idx,
            candidates: bundle
                .candidates
                .iter()
                .enumerate()
                .map(|(cand_idx, c)| CandidateRef {
                    cand_idx,
                    event_idx: c.event_idx,
                    session_offset_ms: c.session_offset_ms,
                    action_type: c.action_type.as_str().to_string(),
                    user_sourced: c.user_sourced,
                    payload_preview: c.payload_preview.clone(),
                })
                .collect(),
        })
        .collect();

    let hint_refs = hints
        .iter()
        .map(|h| ResolverHintRef {
            segment_idx: h.segment_idx,
            spoken_text: h.spoken_text.clone(),
            spoken_offset_in_segment: h.spoken_offset_in_segment,
            expected_types: h
                .expected_types
                .iter()
                .map(|t| t.as_str().to_string())
                .collect(),
        })
        .collect();

    IntentRequestV2 {
        segments: segment_refs,
        candidates_per_segment,
        hints: hint_refs,
    }
}

/// p50 / p95 candidate-set sizes across all segments.
fn candidate_size_percentiles(sets: &[candidate::SegmentCandidates]) -> (usize, usize) {
    if sets.is_empty() {
        return (0, 0);
    }
    let mut sizes: Vec<usize> = sets.iter().map(|s| s.candidates.len()).collect();
    sizes.sort_unstable();
    let p50 = sizes[sizes.len() / 2];
    let p95_idx = ((sizes.len() as f32) * 0.95).ceil() as usize;
    let p95 = sizes[p95_idx.saturating_sub(1).min(sizes.len() - 1)];
    (p50, p95)
}

/// Count `TargetRole::UserAnchor` targets across all references. Used
/// to estimate how many anchor-propagations the AnchorPropagator made
/// by diffing before/after.
fn count_anchor_targets(references: &[Reference]) -> usize {
    use talkiwi_core::output::TargetRole;
    references
        .iter()
        .flat_map(|r| r.targets.iter())
        .filter(|t| t.role == TargetRole::UserAnchor)
        .count()
}

fn group_references_by_relation(references: &[Reference]) -> std::collections::HashMap<String, usize> {
    use talkiwi_core::output::RefRelation;
    let mut out: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for reference in references {
        let key = match reference.relation {
            RefRelation::Single => "single",
            RefRelation::Composition => "composition",
            RefRelation::Contrast => "contrast",
            RefRelation::Subtraction => "subtraction",
            RefRelation::Unknown => "unknown",
        };
        *out.entry(key.to_string()).or_insert(0) += 1;
    }
    out
}

/// Baseline validity check that gates whether an LLM payload is worth
/// keeping at all. When this fails the engine has to go all the way to
/// `build_fallback_output`. When it passes we can at least preserve
/// `task` / `intent` / `restructured_speech` even if the stricter v2
/// reference contract is violated.
fn validate_intent_raw_basic(raw: &IntentRaw) -> bool {
    !raw.task.trim().is_empty() && !raw.restructured_speech.trim().is_empty()
}

/// Strict v2 schema validator (tech plan §7.3). Gates `schema_valid` in
/// telemetry and feeds the partial-degradation path — any failure here
/// causes [`try_restructure_v2_with_retry`] to retry once, and on a
/// second failure to salvage the structured output while clearing
/// `references`.
///
/// Checks per reference:
/// - effective indices (`event_indices` ∪ `event_index`) non-empty
/// - `segment_idx`, when present, indexes a real segment and all
///   effective / excluded indices stay within that segment's candidate
///   bundle
/// - `segment_idx` absent (legacy v1-shape) → indices must stay within
///   the global `events` slice so out-of-range hallucinations don't
///   slip through validation to be silently dropped in conversion
/// - relation-specific shape:
///     - `Composition` → ≥ 2 effective indices
///     - `Contrast` → non-empty `excluded_indices` (Subtraction allows
///       empty — see `system_v2.txt`: "excluded_indices 可空")
///
/// `Single` / `Subtraction` / `Unknown` impose no extra shape
/// constraints — they fall through as long as the index checks above
/// succeed.
fn validate_intent_raw_v2(
    raw: &IntentRaw,
    request: &talkiwi_core::traits::intent::IntentRequestV2,
    events_len: usize,
) -> bool {
    use talkiwi_core::output::RefRelation;

    if !validate_intent_raw_basic(raw) {
        return false;
    }
    let segments_len = request.segments.len();
    for reference in &raw.references {
        let effective = reference.effective_indices();
        if effective.is_empty() {
            return false;
        }
        if let Some(seg_idx) = reference.segment_idx {
            if seg_idx >= segments_len {
                return false;
            }
            let Some(bundle) = request
                .candidates_per_segment
                .iter()
                .find(|b| b.segment_idx == seg_idx)
            else {
                return false;
            };
            let cand_len = bundle.candidates.len();
            if effective.iter().any(|i| *i >= cand_len) {
                return false;
            }
            if reference
                .excluded_indices
                .iter()
                .any(|i| *i >= cand_len)
            {
                return false;
            }
        } else {
            // Legacy v1-shape reference: indices point into the global
            // events slice, so bounds-check against events_len.
            if effective.iter().any(|i| *i >= events_len) {
                return false;
            }
            if reference
                .excluded_indices
                .iter()
                .any(|i| *i >= events_len)
            {
                return false;
            }
        }
        match reference.relation {
            RefRelation::Composition => {
                if effective.len() < 2 {
                    return false;
                }
            }
            RefRelation::Contrast => {
                if reference.excluded_indices.is_empty() {
                    return false;
                }
            }
            RefRelation::Subtraction
            | RefRelation::Single
            | RefRelation::Unknown => {}
        }
    }
    true
}

/// Remove duplicate references that share the same spoken text,
/// segment, relation, AND full target set — keeps the highest
/// confidence copy. Two `Composition` references that share a primary
/// target but differ on a secondary target are *not* duplicates and
/// must both survive.
fn dedup_references(references: Vec<Reference>) -> Vec<Reference> {
    let mut merged: Vec<Reference> = Vec::new();
    for candidate in references {
        if let Some(existing) = merged
            .iter_mut()
            .find(|reference| references_are_duplicates(reference, &candidate))
        {
            if candidate.confidence > existing.confidence {
                *existing = candidate;
            }
        } else {
            merged.push(candidate);
        }
    }
    merged
}

fn references_are_duplicates(a: &Reference, b: &Reference) -> bool {
    if a.spoken_text != b.spoken_text
        || a.relation != b.relation
        || a.segment_idx != b.segment_idx
    {
        return false;
    }
    // Compare the full target id-set. Use the legacy primary id as a
    // fallback key when `targets` is empty on either side.
    let a_ids = reference_event_id_set(a);
    let b_ids = reference_event_id_set(b);
    a_ids == b_ids
}

fn reference_event_id_set(reference: &Reference) -> std::collections::BTreeSet<Uuid> {
    if reference.targets.is_empty() {
        reference
            .resolved_event_id
            .into_iter()
            .collect()
    } else {
        reference
            .targets
            .iter()
            .map(|t| t.event_id)
            .collect()
    }
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
            references: vec![RawReference::v1(
                "这段代码",
                0,
                "用户提到代码，关联选中文字事件",
            )],
        }
    }

    #[tokio::test]
    async fn intent_engine_full_pipeline_mock_provider() {
        let engine = IntentEngine::new(Box::new(MockIntentProvider::new(default_raw())), None);

        let segments = vec![make_segment("帮我重写这段代码", 5000, 7000)];
        let events = vec![make_event(ActionType::SelectionText, 3000)];
        let session_id = Uuid::new_v4();

        let (output, telemetry) = engine
            .process_with_telemetry(&segments, &events, session_id)
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

        // Annotation engine guarantees:
        //   - retrieval_chunks holds one chunk per non-deleted event
        //   - candidate set size telemetry is populated (non-zero p50
        //     because the single segment has ≥ 1 candidate)
        //   - relation counters see the Single reference produced by
        //     the v1-fallback mock
        assert_eq!(output.retrieval_chunks.len(), events.len());
        assert_eq!(output.retrieval_chunks[0].event_id, events[0].id);
        assert!(telemetry.candidate_set_size_p50 >= 1);
        assert_eq!(
            telemetry.references_by_relation.get("single").copied(),
            Some(output.references.len()),
        );
        assert_eq!(telemetry.retrieval_chunk_count, output.retrieval_chunks.len());
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
        // Retrieval chunks must still be emitted even with 0 segments:
        // the annotation engine's contract is "every live event gets a
        // chunk" regardless of whether anything referenced it.
        assert_eq!(output.retrieval_chunks.len(), events.len());
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

    use talkiwi_core::output::{RefRelation, RefTarget, Reference, ReferenceStrategy, TargetRole};

    fn ref_with_targets(
        spoken: &str,
        segment: Option<usize>,
        relation: RefRelation,
        targets: &[(usize, Uuid)],
        confidence: f32,
    ) -> Reference {
        Reference {
            spoken_text: spoken.to_string(),
            spoken_offset: 0,
            resolved_event_idx: targets.first().map(|t| t.0).unwrap_or(0),
            resolved_event_id: targets.first().map(|t| t.1),
            confidence,
            strategy: ReferenceStrategy::LlmCoreference,
            user_confirmed: false,
            targets: targets
                .iter()
                .map(|(idx, id)| RefTarget {
                    event_id: *id,
                    event_idx: *idx,
                    role: TargetRole::Source,
                    via_anchor: None,
                })
                .collect(),
            relation,
            segment_idx: segment,
        }
    }

    #[test]
    fn dedup_keeps_distinct_composition_refs_with_same_primary() {
        // Two composition refs share the first target (id_a) but
        // differ on the second target (id_b vs id_c). Old dedup used
        // (spoken_text, primary_event_idx) and collapsed them, losing
        // the secondary target. The new key uses the whole target set.
        let id_a = Uuid::new_v4();
        let id_b = Uuid::new_v4();
        let id_c = Uuid::new_v4();
        let refs = vec![
            ref_with_targets(
                "这个和那个",
                Some(0),
                RefRelation::Composition,
                &[(0, id_a), (1, id_b)],
                0.8,
            ),
            ref_with_targets(
                "这个和那个",
                Some(0),
                RefRelation::Composition,
                &[(0, id_a), (2, id_c)],
                0.85,
            ),
        ];
        let out = dedup_references(refs);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn dedup_collapses_true_duplicates_keeping_higher_confidence() {
        let id = Uuid::new_v4();
        let refs = vec![
            ref_with_targets(
                "x",
                Some(0),
                RefRelation::Single,
                &[(0, id)],
                0.5,
            ),
            ref_with_targets(
                "x",
                Some(0),
                RefRelation::Single,
                &[(0, id)],
                0.9,
            ),
        ];
        let out = dedup_references(refs);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].confidence, 0.9);
    }

    #[tokio::test]
    async fn low_importance_passive_events_drop_out_of_prompt_surface() {
        // A pure-noise `ClickMouse` event scores well below the
        // 0.35 threshold. The pipeline must drop it from artifacts
        // while still emitting a retrieval chunk for it.
        let raw = IntentRaw {
            task: "分析".to_string(),
            intent: "analyze".to_string(),
            constraints: vec![],
            missing_context: vec![],
            restructured_speech: "分析".to_string(),
            references: vec![],
        };
        let engine = IntentEngine::new(Box::new(MockIntentProvider::new(raw)), None);

        let click = ActionEvent {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            timestamp: 1_712_900_000_000,
            session_offset_ms: 5_000,
            observed_offset_ms: Some(5_000),
            duration_ms: None,
            action_type: ActionType::ClickMouse,
            plugin_id: "builtin".to_string(),
            payload: ActionPayload::ClickMouse {
                app_name: None,
                window_title: None,
                button: "left".to_string(),
                x: 10.0,
                y: 10.0,
            },
            semantic_hint: None,
            confidence: 1.0,
            curation: Default::default(),
        };
        // user-sourced selection clears the threshold via β term even
        // without a voice reference, giving us a clean contrast:
        //   click:     score ≈ 0.08  (below threshold)
        //   selection: score ≈ 0.53  (above threshold)
        let mut selection = make_event(ActionType::SelectionText, 6_000);
        selection.curation = talkiwi_core::event::TraceCuration::toolbar();

        let output = engine
            .process(&[], &[click.clone(), selection.clone()], Uuid::new_v4())
            .await
            .unwrap();

        // Artifacts include the high-score selection but not the
        // low-score click.
        let artifact_ids: std::collections::HashSet<_> =
            output.artifacts.iter().map(|a| a.event_id).collect();
        assert!(artifact_ids.contains(&selection.id));
        assert!(!artifact_ids.contains(&click.id));

        // Retrieval keeps both.
        let chunk_ids: std::collections::HashSet<_> =
            output.retrieval_chunks.iter().map(|c| c.event_id).collect();
        assert!(chunk_ids.contains(&selection.id));
        assert!(chunk_ids.contains(&click.id));
    }

    #[test]
    fn dedup_treats_different_segments_as_distinct() {
        let id = Uuid::new_v4();
        let refs = vec![
            ref_with_targets("x", Some(0), RefRelation::Single, &[(0, id)], 0.5),
            ref_with_targets("x", Some(1), RefRelation::Single, &[(0, id)], 0.5),
        ];
        let out = dedup_references(refs);
        assert_eq!(out.len(), 2);
    }

    // ---------- v2 schema validator + partial-degradation tests -----

    fn sample_request_v2() -> talkiwi_core::traits::intent::IntentRequestV2 {
        use talkiwi_core::traits::intent::{
            CandidateRef, IntentRequestV2, SegmentCandidatesRef, SegmentRef,
        };
        IntentRequestV2 {
            segments: vec![SegmentRef {
                idx: 0,
                start_ms: 0,
                end_ms: 1_000,
                text: "这段代码".to_string(),
            }],
            candidates_per_segment: vec![SegmentCandidatesRef {
                segment_idx: 0,
                candidates: vec![
                    CandidateRef {
                        cand_idx: 0,
                        event_idx: 0,
                        session_offset_ms: 500,
                        action_type: "selection.text".to_string(),
                        user_sourced: false,
                        payload_preview: String::new(),
                    },
                    CandidateRef {
                        cand_idx: 1,
                        event_idx: 1,
                        session_offset_ms: 800,
                        action_type: "selection.text".to_string(),
                        user_sourced: false,
                        payload_preview: String::new(),
                    },
                ],
            }],
            hints: vec![],
        }
    }

    fn raw_with_refs(references: Vec<RawReference>) -> IntentRaw {
        IntentRaw {
            task: "task".to_string(),
            intent: "rewrite".to_string(),
            constraints: vec![],
            missing_context: vec![],
            restructured_speech: "speech".to_string(),
            references,
        }
    }

    /// `sample_request_v2()` references 2 distinct global events
    /// (event_idx 0 and 1), so every test below passes `events_len = 2`
    /// unless it specifically probes the bounds check.
    const SAMPLE_EVENTS_LEN: usize = 2;

    #[test]
    fn validate_v2_accepts_well_formed_single_ref() {
        let raw = raw_with_refs(vec![RawReference {
            spoken_text: "这段代码".to_string(),
            event_index: None,
            reason: String::new(),
            segment_idx: Some(0),
            event_indices: vec![0],
            relation: RefRelation::Single,
            excluded_indices: vec![],
        }]);
        assert!(validate_intent_raw_v2(
            &raw,
            &sample_request_v2(),
            SAMPLE_EVENTS_LEN,
        ));
    }

    #[test]
    fn validate_v2_rejects_segment_idx_out_of_range() {
        let raw = raw_with_refs(vec![RawReference {
            spoken_text: "这段代码".to_string(),
            event_index: None,
            reason: String::new(),
            segment_idx: Some(5), // only 1 segment in request
            event_indices: vec![0],
            relation: RefRelation::Single,
            excluded_indices: vec![],
        }]);
        assert!(!validate_intent_raw_v2(
            &raw,
            &sample_request_v2(),
            SAMPLE_EVENTS_LEN,
        ));
    }

    #[test]
    fn validate_v2_rejects_candidate_idx_out_of_range() {
        let raw = raw_with_refs(vec![RawReference {
            spoken_text: "这段代码".to_string(),
            event_index: None,
            reason: String::new(),
            segment_idx: Some(0),
            event_indices: vec![7], // bundle has 2 candidates
            relation: RefRelation::Single,
            excluded_indices: vec![],
        }]);
        assert!(!validate_intent_raw_v2(
            &raw,
            &sample_request_v2(),
            SAMPLE_EVENTS_LEN,
        ));
    }

    #[test]
    fn validate_v2_rejects_composition_with_single_target() {
        let raw = raw_with_refs(vec![RawReference {
            spoken_text: "A 和 B".to_string(),
            event_index: None,
            reason: String::new(),
            segment_idx: Some(0),
            event_indices: vec![0], // composition needs ≥ 2
            relation: RefRelation::Composition,
            excluded_indices: vec![],
        }]);
        assert!(!validate_intent_raw_v2(
            &raw,
            &sample_request_v2(),
            SAMPLE_EVENTS_LEN,
        ));
    }

    #[test]
    fn validate_v2_rejects_contrast_without_excluded() {
        let raw = raw_with_refs(vec![RawReference {
            spoken_text: "像 X 但不要 Y".to_string(),
            event_index: None,
            reason: String::new(),
            segment_idx: Some(0),
            event_indices: vec![0],
            relation: RefRelation::Contrast,
            excluded_indices: vec![], // contrast needs excluded set
        }]);
        assert!(!validate_intent_raw_v2(
            &raw,
            &sample_request_v2(),
            SAMPLE_EVENTS_LEN,
        ));
    }

    #[test]
    fn validate_v2_accepts_subtraction_with_empty_excluded() {
        // system_v2.txt:48 — "subtraction: excluded_indices 可空".
        // The validator must not reject a well-formed Subtraction that
        // leaves excluded_indices empty (only the preserved scope matters).
        let raw = raw_with_refs(vec![RawReference {
            spoken_text: "别动选中部分的其他逻辑".to_string(),
            event_index: None,
            reason: String::new(),
            segment_idx: Some(0),
            event_indices: vec![0],
            relation: RefRelation::Subtraction,
            excluded_indices: vec![],
        }]);
        assert!(validate_intent_raw_v2(
            &raw,
            &sample_request_v2(),
            SAMPLE_EVENTS_LEN,
        ));
    }

    #[test]
    fn validate_v2_rejects_empty_indices() {
        let raw = raw_with_refs(vec![RawReference {
            spoken_text: "x".to_string(),
            event_index: None,
            reason: String::new(),
            segment_idx: Some(0),
            event_indices: vec![],
            relation: RefRelation::Single,
            excluded_indices: vec![],
        }]);
        assert!(!validate_intent_raw_v2(
            &raw,
            &sample_request_v2(),
            SAMPLE_EVENTS_LEN,
        ));
    }

    #[test]
    fn validate_v2_accepts_legacy_ref_within_events_bounds() {
        // v1 provider path: event_index set, segment_idx absent. The
        // validator bounds-checks against `events_len` since the index
        // references the global events slice in this shape.
        let raw = raw_with_refs(vec![RawReference::v1("这段代码", 0, "reason")]);
        assert!(validate_intent_raw_v2(
            &raw,
            &sample_request_v2(),
            SAMPLE_EVENTS_LEN,
        ));
    }

    #[test]
    fn validate_v2_rejects_legacy_ref_out_of_events_bounds() {
        // v1-shape reference with a hallucinated global index should no
        // longer pass validation to be silently dropped in conversion.
        let raw = raw_with_refs(vec![RawReference::v1("这段代码", 9999, "reason")]);
        assert!(!validate_intent_raw_v2(
            &raw,
            &sample_request_v2(),
            SAMPLE_EVENTS_LEN,
        ));
    }

    #[tokio::test]
    async fn partial_degradation_preserves_structured_output_when_refs_fail_schema() {
        // Regression for the 2026-04-19 review: a bad reference list
        // used to abort the whole call and force build_fallback_output,
        // losing the provider's task/intent/restructured_speech. The
        // new contract keeps those fields and only clears `references`.
        let raw = IntentRaw {
            task: "重写选中代码".to_string(),
            intent: "rewrite".to_string(),
            constraints: vec!["保留注释".to_string()],
            missing_context: vec![],
            restructured_speech: "帮我重写".to_string(),
            references: vec![RawReference {
                spoken_text: "这段".to_string(),
                event_index: None,
                reason: String::new(),
                // segment 42 does not exist — will fail v2 validation.
                segment_idx: Some(42),
                event_indices: vec![0],
                relation: RefRelation::Single,
                excluded_indices: vec![],
            }],
        };
        let engine = IntentEngine::new(Box::new(MockIntentProvider::new(raw)), None);
        let segments = vec![make_segment("重写这段代码", 0, 1_000)];
        let events = vec![make_event(ActionType::SelectionText, 500)];

        let (output, telemetry) = engine
            .process_with_telemetry(&segments, &events, Uuid::new_v4())
            .await
            .unwrap();

        // Structured fields survived.
        assert_eq!(output.task, "重写选中代码");
        assert_eq!(output.intent, "rewrite");
        assert_eq!(output.restructured_speech, "帮我重写");
        assert_eq!(output.constraints, vec!["保留注释".to_string()]);
        // References were dropped.
        assert!(output.references.is_empty());
        // Not a full fallback — schema_valid flags the incident.
        assert!(!telemetry.fallback_used);
        assert!(!telemetry.schema_valid);
        // Retry was attempted before salvaging.
        assert_eq!(telemetry.retry_count, 1);
    }
}
