use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::output::RefRelation;

/// A single reference resolved by the LLM — links a spoken phrase to one
/// or more events by index into the candidate set the LLM was shown.
///
/// The legacy `event_index` field is kept for backward compatibility with
/// providers that still emit the v1 schema. New providers should populate
/// `event_indices` (and optionally `relation` / `excluded_indices`).
///
/// See `docs/design/2026-04-18-trace-annotation-engine-tech-plan.md` §7.2
/// for the full request/response contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawReference {
    /// The spoken phrase (e.g. "这段代码", "刚才那个截图")
    pub spoken_text: String,
    /// **Deprecated v1 single-index field.** Kept `Option<usize>` with
    /// `#[serde(default)]` so strict-v2 providers that omit it
    /// entirely still deserialize — otherwise the whole restructure
    /// pipeline would fail before any v2 compat logic runs. Callers
    /// must prefer `event_indices` when non-empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_index: Option<usize>,
    /// Why the LLM believes this phrase refers to this event. Optional
    /// because v2 responses may omit it to save tokens.
    #[serde(default)]
    pub reason: String,

    /// v2: which `SpeakSegment` this deixis came from. When present,
    /// `event_index` / `event_indices` index into the *candidate set*
    /// for that segment, not the full `events` slice. Absent → legacy
    /// semantics (index into `events` directly).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segment_idx: Option<usize>,
    /// v2: multiple candidate-set indices for `Composition` / `Contrast`
    /// relations. Empty for v1 providers — callers should fall back to
    /// `vec![event_index]` in that case.
    #[serde(default)]
    pub event_indices: Vec<usize>,
    /// v2: relation type. Absent on v1 providers — treat as `Single`.
    #[serde(default)]
    pub relation: RefRelation,
    /// v2: indices that are explicitly excluded (used by `Contrast` /
    /// `Subtraction`). Empty for v1 providers.
    #[serde(default)]
    pub excluded_indices: Vec<usize>,
}

impl RawReference {
    /// Construct a v1-shape raw reference (single event, no relation)
    /// with all v2 fields left at their defaults. Use this from
    /// legacy test paths and from v1-only provider implementations.
    pub fn v1(
        spoken_text: impl Into<String>,
        event_index: usize,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            spoken_text: spoken_text.into(),
            event_index: Some(event_index),
            reason: reason.into(),
            segment_idx: None,
            event_indices: Vec::new(),
            relation: RefRelation::Single,
            excluded_indices: Vec::new(),
        }
    }

    /// Returns the effective candidate-set indices, preferring the v2
    /// `event_indices` when present, otherwise falling back to the
    /// single `event_index`. Returns an empty slice when neither is
    /// populated — downstream must drop such references.
    pub fn effective_indices(&self) -> Vec<usize> {
        if !self.event_indices.is_empty() {
            self.event_indices.clone()
        } else if let Some(idx) = self.event_index {
            vec![idx]
        } else {
            Vec::new()
        }
    }
}

/// Raw LLM output from intent restructuring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentRaw {
    pub task: String,
    pub intent: String,
    pub constraints: Vec<String>,
    pub missing_context: Vec<String>,
    pub restructured_speech: String,
    /// LLM-resolved references: which spoken phrases map to which events.
    #[serde(default)]
    pub references: Vec<RawReference>,
}

/// Lightweight view of a `SpeakSegment` for v2 requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentRef {
    pub idx: usize,
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

/// A single candidate event shown to the LLM for coreference resolution.
/// `cand_idx` is local to the segment's candidate list; `event_idx` is
/// the global index into the session's `events: &[ActionEvent]` slice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateRef {
    pub cand_idx: usize,
    pub event_idx: usize,
    pub session_offset_ms: u64,
    /// `ActionType::as_str()` — kept as a bare string so this type has
    /// no dependency on the `event` module.
    pub action_type: String,
    pub user_sourced: bool,
    pub payload_preview: String,
}

/// Per-segment candidate bundle inside [`IntentRequestV2`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentCandidatesRef {
    pub segment_idx: usize,
    pub candidates: Vec<CandidateRef>,
}

/// Regex-matched deixis prior the LLM may use while resolving.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolverHintRef {
    pub segment_idx: usize,
    pub spoken_text: String,
    pub spoken_offset_in_segment: usize,
    pub expected_types: Vec<String>,
}

/// Structured request shape for [`IntentProvider::restructure_v2`].
///
/// The v2 request ships the LLM a compact, typed candidate set per
/// segment instead of a 4096-byte summary string. Providers that do
/// not natively support v2 fall back through the default trait method
/// into `restructure`, which sees the request serialized to JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentRequestV2 {
    pub segments: Vec<SegmentRef>,
    pub candidates_per_segment: Vec<SegmentCandidatesRef>,
    #[serde(default)]
    pub hints: Vec<ResolverHintRef>,
}

impl IntentRequestV2 {
    /// Build a transcript string by joining segment texts — used by the
    /// default `restructure_v2` fallback impl to feed v1 providers.
    pub fn transcript(&self) -> String {
        self.segments
            .iter()
            .map(|s| s.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Intent provider trait — implemented by Ollama, cloud LLM providers, etc.
///
/// Providers implement the v1 `restructure` method (required). v2
/// coreference capability is opt-in via `restructure_v2`; the default
/// impl degrades gracefully by serializing the structured request to
/// JSON and calling v1.
#[async_trait::async_trait]
pub trait IntentProvider: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn requires_network(&self) -> bool;
    async fn is_available(&self) -> bool;
    async fn restructure(
        &self,
        transcript: &str,
        events_summary: &str,
        system_prompt: &str,
    ) -> anyhow::Result<IntentRaw>;

    /// v2 structured-request entry point. Default impl serializes the
    /// request and delegates to `restructure`, so providers that only
    /// speak v1 keep working without code changes.
    ///
    /// Providers that natively support multi-target coreference
    /// (populating `RawReference.event_indices` / `relation`) should
    /// override this method.
    async fn restructure_v2(
        &self,
        request: &IntentRequestV2,
        system_prompt: &str,
    ) -> anyhow::Result<IntentRaw> {
        let transcript = request.transcript();
        let events_summary = serde_json::to_string(request)
            .context("failed to serialize IntentRequestV2 for v1 fallback")?;
        self.restructure(&transcript, &events_summary, system_prompt)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_reference_v1_json_deserializes() {
        // v1 JSON — no event_indices / relation / excluded_indices.
        let json = r#"{
            "spoken_text": "这段代码",
            "event_index": 4,
            "reason": "近邻选中"
        }"#;
        let r: RawReference = serde_json::from_str(json).unwrap();
        assert_eq!(r.event_index, Some(4));
        assert!(r.event_indices.is_empty());
        assert_eq!(r.relation, RefRelation::Single);
        assert!(r.excluded_indices.is_empty());
        assert_eq!(r.effective_indices(), vec![4]);
    }

    #[test]
    fn strict_v2_json_without_event_index_deserializes() {
        // A v2 provider that follows `system_v2.txt` strictly will omit
        // the legacy `event_index` field entirely. This must still
        // deserialize — otherwise the whole restructure call fails and
        // the pipeline never gets to run the v2 conversion.
        let json = r#"{
            "spoken_text": "刚才那个堆栈",
            "segment_idx": 4,
            "event_indices": [2],
            "relation": "single"
        }"#;
        let r: RawReference = serde_json::from_str(json).unwrap();
        assert_eq!(r.event_index, None);
        assert_eq!(r.segment_idx, Some(4));
        assert_eq!(r.effective_indices(), vec![2]);
    }

    #[test]
    fn raw_reference_effective_indices_is_empty_when_both_missing() {
        let json = r#"{
            "spoken_text": "x"
        }"#;
        let r: RawReference = serde_json::from_str(json).unwrap();
        assert!(r.effective_indices().is_empty());
    }

    #[test]
    fn raw_reference_v2_composition_round_trip() {
        let r = RawReference {
            spoken_text: "A 和 B".to_string(),
            event_index: None,
            reason: "two references".to_string(),
            segment_idx: Some(4),
            event_indices: vec![2, 5],
            relation: RefRelation::Composition,
            excluded_indices: vec![],
        };
        let json = serde_json::to_string(&r).unwrap();
        let back: RawReference = serde_json::from_str(&json).unwrap();
        assert_eq!(back.relation, RefRelation::Composition);
        assert_eq!(back.effective_indices(), vec![2, 5]);
        assert_eq!(back.segment_idx, Some(4));
    }

    #[test]
    fn raw_reference_v1_helper_builds_legacy_shape() {
        let r = RawReference::v1("这段代码", 3, "近邻选中");
        assert_eq!(r.event_index, Some(3));
        assert!(r.segment_idx.is_none());
        assert!(r.event_indices.is_empty());
        assert_eq!(r.relation, RefRelation::Single);
    }

    #[test]
    fn raw_reference_effective_indices_prefers_v2() {
        // v2 populated — must ignore legacy event_index entirely.
        let r = RawReference {
            spoken_text: "x".to_string(),
            event_index: Some(99),
            reason: String::new(),
            segment_idx: Some(0),
            event_indices: vec![1, 2],
            relation: RefRelation::Composition,
            excluded_indices: vec![],
        };
        assert_eq!(r.effective_indices(), vec![1, 2]);
    }

    #[test]
    fn intent_request_v2_transcript_joins_segments() {
        let req = IntentRequestV2 {
            segments: vec![
                SegmentRef {
                    idx: 0,
                    start_ms: 0,
                    end_ms: 1000,
                    text: "帮我".to_string(),
                },
                SegmentRef {
                    idx: 1,
                    start_ms: 1000,
                    end_ms: 2000,
                    text: "重写这段".to_string(),
                },
            ],
            candidates_per_segment: Vec::new(),
            hints: Vec::new(),
        };
        assert_eq!(req.transcript(), "帮我 重写这段");
    }

    #[test]
    fn intent_request_v2_round_trip() {
        let req = IntentRequestV2 {
            segments: vec![SegmentRef {
                idx: 0,
                start_ms: 0,
                end_ms: 1000,
                text: "这段代码".to_string(),
            }],
            candidates_per_segment: vec![SegmentCandidatesRef {
                segment_idx: 0,
                candidates: vec![CandidateRef {
                    cand_idx: 0,
                    event_idx: 7,
                    session_offset_ms: 800,
                    action_type: "selection.text".to_string(),
                    user_sourced: false,
                    payload_preview: "selected in VSCode: fn main() {}".to_string(),
                }],
            }],
            hints: vec![ResolverHintRef {
                segment_idx: 0,
                spoken_text: "这段代码".to_string(),
                spoken_offset_in_segment: 0,
                expected_types: vec!["selection.text".to_string()],
            }],
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: IntentRequestV2 = serde_json::from_str(&json).unwrap();
        assert_eq!(back.candidates_per_segment[0].candidates[0].event_idx, 7);
        assert_eq!(back.hints[0].spoken_text, "这段代码");
    }
}
