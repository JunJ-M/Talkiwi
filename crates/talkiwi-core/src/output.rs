use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IntentCategory {
    Rewrite,
    Analyze,
    Summarize,
    Generate,
    Debug,
    Query,
    Unknown,
}

impl IntentCategory {
    pub fn from_llm_output(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "rewrite" | "重写" => Self::Rewrite,
            "analyze" | "分析" => Self::Analyze,
            "summarize" | "总结" | "概述" => Self::Summarize,
            "generate" | "生成" | "创建" => Self::Generate,
            "debug" | "调试" | "修复" => Self::Debug,
            "query" | "查询" | "问答" => Self::Query,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

impl RiskLevel {
    pub fn from_confidence(confidence: f32) -> Self {
        if confidence >= 0.8 {
            Self::Low
        } else if confidence >= 0.5 {
            Self::Medium
        } else {
            Self::High
        }
    }
}

/// How a spoken-phrase reference was resolved.
///
/// New variants (`LlmCoreference`, `AnchorPropagation`) are added for the
/// trace annotation engine introduced 2026-04-18. Legacy values still
/// deserialize unchanged.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceStrategy {
    TemporalProximity,
    SemanticSimilarity,
    UserConfirmed,
    /// LLM solved the deixis in the end-of-session coreference pass.
    LlmCoreference,
    /// Resolved via user-written anchor (e.g. toolbar note "堆栈在这"
    /// pulling the reference onto the adjacent clipboard event).
    AnchorPropagation,
}

/// The relation a spoken phrase has with its resolved targets.
///
/// - `Single`: "这个链接" → one target.
/// - `Composition`: "A 的 X + B 的 Y" → multiple positive targets.
/// - `Contrast`: "像 X 但不要 Y" → some targets are sources, some are
///   `ExcludedAspect` and feed constraints.
/// - `Subtraction`: "别动 X" → all targets are pure exclusions; no
///   artifact is produced.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RefRelation {
    #[default]
    Single,
    Composition,
    Contrast,
    Subtraction,
    /// Forward-compatibility bucket for unknown relation strings from a
    /// future LLM response. Treat as `Single` at consumption time.
    #[serde(other)]
    Unknown,
}

/// The role a target plays inside a reference.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TargetRole {
    /// Generic forward pointer.
    #[default]
    Source,
    /// Visual/style reference ("那种样式").
    Style,
    /// Feature reference ("那个功能").
    Feature,
    /// Aspect explicitly excluded in a `Contrast` reference.
    ExcludedAspect,
    /// Preserve boundary ("别动这部分").
    PreserveScope,
    /// Target reached by propagating from a user-written anchor.
    UserAnchor,
    /// Forward-compatibility bucket.
    #[serde(other)]
    Unknown,
}

/// A single target inside a multi-target reference.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RefTarget {
    pub event_id: Uuid,
    /// Index into the full `ActionEvent` slice. Redundant with `event_id`
    /// but cheap to carry so downstream rendering doesn't re-scan.
    pub event_idx: usize,
    #[serde(default)]
    pub role: TargetRole,
    /// If this target was reached via a user-note anchor, record the
    /// original anchor event id so the retrieval surface can surface it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via_anchor: Option<Uuid>,
}

/// A resolved reference linking spoken text to one or more action events.
///
/// ## Writing new code
///
/// - Construct via [`Reference::new_single`] (Single relation) or by
///   building a `targets: Vec<RefTarget>` and filling `relation`.
/// - Read via [`Reference::primary_event_id`] /
///   [`Reference::primary_event_idx`] — they transparently fall back
///   to the legacy fields when `targets` is empty.
///
/// ## Legacy fields
///
/// `resolved_event_idx` and `resolved_event_id` predate the trace
/// annotation engine (see `docs/design/2026-04-18-trace-annotation-engine-tech-plan.md`).
/// They remain `pub` and required-on-construction because:
///
/// 1. The `references_` SQL table declares `resolved_event_idx INTEGER
///    NOT NULL` — removing the fields requires a DB migration (tech
///    plan §15 Phase 6).
/// 2. External consumers (`apps/desktop/src/types.ts`) still read them.
///
/// Do **not** read them directly in new Rust code — use
/// `primary_event_*`. Writes should go through `Reference::new_single`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Reference {
    pub spoken_text: String,
    pub spoken_offset: usize,
    /// Legacy — do not read in new code; call `primary_event_idx()`.
    pub resolved_event_idx: usize,
    /// Legacy — do not read in new code; call `primary_event_id()`.
    pub resolved_event_id: Option<Uuid>,
    pub confidence: f32,
    pub strategy: ReferenceStrategy,
    pub user_confirmed: bool,

    /// Multi-target resolution produced by the annotation engine. Empty
    /// for references built by legacy code paths — callers should treat
    /// an empty `targets` as a `Single` target described by the legacy
    /// fields. See `primary_event_id()` / `primary_event_idx()`.
    #[serde(default)]
    pub targets: Vec<RefTarget>,
    #[serde(default)]
    pub relation: RefRelation,
    /// Which `SpeakSegment` this reference originated from, when the v2
    /// LLM response tagged it with `segment_idx`. Retrieval uses this
    /// to attribute `referenced_by_segments` precisely instead of
    /// guessing via text search — needed when the same surface form
    /// (e.g. "这个链接") appears in multiple segments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segment_idx: Option<usize>,
}

impl Reference {
    /// Construct a `Single`-relation reference with a populated `targets`
    /// list kept in sync with the legacy fields. Prefer this over manual
    /// struct literals in new code so the two shapes never drift.
    pub fn new_single(
        spoken_text: impl Into<String>,
        spoken_offset: usize,
        event_idx: usize,
        event_id: Uuid,
        confidence: f32,
        strategy: ReferenceStrategy,
    ) -> Self {
        Self {
            spoken_text: spoken_text.into(),
            spoken_offset,
            resolved_event_idx: event_idx,
            resolved_event_id: Some(event_id),
            confidence,
            strategy,
            user_confirmed: false,
            targets: vec![RefTarget {
                event_id,
                event_idx,
                role: TargetRole::Source,
                via_anchor: None,
            }],
            relation: RefRelation::Single,
            segment_idx: None,
        }
    }

    /// The first target's event id, falling back to the legacy
    /// `resolved_event_id` field. Returns `None` only when both the
    /// new and legacy shapes are empty.
    pub fn primary_event_id(&self) -> Option<Uuid> {
        self.targets
            .first()
            .map(|t| t.event_id)
            .or(self.resolved_event_id)
    }

    /// The first target's event index, falling back to the legacy
    /// `resolved_event_idx` field.
    pub fn primary_event_idx(&self) -> usize {
        self.targets
            .first()
            .map(|t| t.event_idx)
            .unwrap_or(self.resolved_event_idx)
    }
}

/// An artifact reference for the final output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub event_id: Uuid,
    pub label: String,
    pub inline_summary: String,
}

/// Retrieval-surface chunk — one natural-language blob per action event.
///
/// Unlike `artifacts`, retrieval chunks are NOT filtered by importance:
/// every non-deleted event in the session produces a chunk so the
/// retrieval index stays complete. The `importance` score is carried
/// so downstream retrieval can rank / weight.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RetrievalChunk {
    pub event_id: Uuid,
    pub session_id: Uuid,
    pub session_offset_ms: u64,
    /// `ActionType::as_str()` — kept as a bare string so this type has
    /// no dependency on the `event` module.
    pub action_type: String,
    /// Natural-language rendering ready to embed.
    pub text: String,
    /// Indices into the session's `SpeakSegment` list for segments that
    /// reference this event. Indices — not ids — because `SpeakSegment`
    /// does not carry a stable id; within one session the index is the
    /// natural stable handle.
    #[serde(default)]
    pub referenced_by_segments: Vec<usize>,
    pub importance: f32,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Intent output — the final structured result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentOutput {
    pub session_id: Uuid,
    pub task: String,
    pub intent: String,
    #[serde(default = "default_intent_category")]
    pub intent_category: IntentCategory,
    pub constraints: Vec<String>,
    pub missing_context: Vec<String>,
    pub restructured_speech: String,
    pub final_markdown: String,
    pub artifacts: Vec<ArtifactRef>,
    pub references: Vec<Reference>,
    #[serde(default)]
    pub output_confidence: f32,
    #[serde(default = "default_risk_level")]
    pub risk_level: RiskLevel,

    /// Retrieval-surface chunks, one per action event. Defaults to empty
    /// so sessions produced before the annotation engine deserialize
    /// without migration.
    #[serde(default)]
    pub retrieval_chunks: Vec<RetrievalChunk>,
}

fn default_intent_category() -> IntentCategory {
    IntentCategory::Unknown
}

fn default_risk_level() -> RiskLevel {
    RiskLevel::High
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_strategy_serde() {
        let strategies = vec![
            ReferenceStrategy::TemporalProximity,
            ReferenceStrategy::SemanticSimilarity,
            ReferenceStrategy::UserConfirmed,
            ReferenceStrategy::LlmCoreference,
            ReferenceStrategy::AnchorPropagation,
        ];
        for s in &strategies {
            let json = serde_json::to_string(s).unwrap();
            let deserialized: ReferenceStrategy = serde_json::from_str(&json).unwrap();
            assert_eq!(&deserialized, s);
        }
    }

    #[test]
    fn ref_relation_round_trip_and_unknown() {
        for r in [
            RefRelation::Single,
            RefRelation::Composition,
            RefRelation::Contrast,
            RefRelation::Subtraction,
        ] {
            let json = serde_json::to_string(&r).unwrap();
            let back: RefRelation = serde_json::from_str(&json).unwrap();
            assert_eq!(back, r);
        }
        // Unknown relation from a future LLM response must degrade, not error.
        let unknown: RefRelation = serde_json::from_str("\"tangent\"").unwrap();
        assert_eq!(unknown, RefRelation::Unknown);
    }

    #[test]
    fn target_role_round_trip_and_unknown() {
        for role in [
            TargetRole::Source,
            TargetRole::Style,
            TargetRole::Feature,
            TargetRole::ExcludedAspect,
            TargetRole::PreserveScope,
            TargetRole::UserAnchor,
        ] {
            let json = serde_json::to_string(&role).unwrap();
            let back: TargetRole = serde_json::from_str(&json).unwrap();
            assert_eq!(back, role);
        }
        let unknown: TargetRole = serde_json::from_str("\"antagonist\"").unwrap();
        assert_eq!(unknown, TargetRole::Unknown);
    }

    #[test]
    fn reference_round_trip_legacy_shape() {
        // Legacy shape — targets empty, relation absent.
        let legacy = Reference {
            spoken_text: "this code".to_string(),
            spoken_offset: 42,
            resolved_event_idx: 0,
            resolved_event_id: Some(Uuid::new_v4()),
            confidence: 0.9,
            strategy: ReferenceStrategy::TemporalProximity,
            user_confirmed: false,
            targets: Vec::new(),
            relation: RefRelation::Single,
            segment_idx: None,
        };
        let json = serde_json::to_string(&legacy).unwrap();
        let back: Reference = serde_json::from_str(&json).unwrap();
        assert_eq!(back.resolved_event_idx, 0);
        assert!(back.targets.is_empty());
        assert_eq!(back.relation, RefRelation::Single);
    }

    #[test]
    fn reference_pre_annotation_json_deserializes() {
        // JSON written before the annotation engine introduced
        // `targets` / `relation`. Must deserialize with defaults.
        let legacy_json = r#"{
            "spoken_text": "这段代码",
            "spoken_offset": 0,
            "resolved_event_idx": 3,
            "resolved_event_id": null,
            "confidence": 0.7,
            "strategy": "temporal_proximity",
            "user_confirmed": false
        }"#;
        let r: Reference = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(r.resolved_event_idx, 3);
        assert!(r.targets.is_empty());
        assert_eq!(r.relation, RefRelation::Single);
        assert_eq!(r.primary_event_idx(), 3);
    }

    #[test]
    fn reference_new_single_keeps_shapes_in_sync() {
        let id = Uuid::new_v4();
        let r = Reference::new_single(
            "这段代码",
            7,
            2,
            id,
            0.88,
            ReferenceStrategy::LlmCoreference,
        );
        assert_eq!(r.resolved_event_idx, 2);
        assert_eq!(r.resolved_event_id, Some(id));
        assert_eq!(r.targets.len(), 1);
        assert_eq!(r.targets[0].event_id, id);
        assert_eq!(r.targets[0].event_idx, 2);
        assert_eq!(r.relation, RefRelation::Single);
        assert_eq!(r.primary_event_id(), Some(id));
        assert_eq!(r.primary_event_idx(), 2);
    }

    #[test]
    fn primary_event_id_prefers_targets_over_legacy() {
        let legacy_id = Uuid::new_v4();
        let target_id = Uuid::new_v4();
        let r = Reference {
            spoken_text: "x".to_string(),
            spoken_offset: 0,
            resolved_event_idx: 0,
            resolved_event_id: Some(legacy_id),
            confidence: 0.5,
            strategy: ReferenceStrategy::LlmCoreference,
            user_confirmed: false,
            targets: vec![RefTarget {
                event_id: target_id,
                event_idx: 5,
                role: TargetRole::Source,
                via_anchor: None,
            }],
            relation: RefRelation::Single,
            segment_idx: None,
        };
        assert_eq!(r.primary_event_id(), Some(target_id));
        assert_eq!(r.primary_event_idx(), 5);
    }

    #[test]
    fn composition_reference_round_trip() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let r = Reference {
            spoken_text: "rewind 那种风格 + loom 的锚点".to_string(),
            spoken_offset: 0,
            resolved_event_idx: 3,
            resolved_event_id: Some(a),
            confidence: 0.88,
            strategy: ReferenceStrategy::LlmCoreference,
            user_confirmed: false,
            targets: vec![
                RefTarget {
                    event_id: a,
                    event_idx: 3,
                    role: TargetRole::Style,
                    via_anchor: None,
                },
                RefTarget {
                    event_id: b,
                    event_idx: 8,
                    role: TargetRole::Feature,
                    via_anchor: None,
                },
            ],
            relation: RefRelation::Composition,
            segment_idx: None,
        };
        let json = serde_json::to_string(&r).unwrap();
        let back: Reference = serde_json::from_str(&json).unwrap();
        assert_eq!(back.relation, RefRelation::Composition);
        assert_eq!(back.targets.len(), 2);
        assert_eq!(back.targets[0].role, TargetRole::Style);
        assert_eq!(back.targets[1].role, TargetRole::Feature);
    }

    #[test]
    fn retrieval_chunk_round_trip() {
        let chunk = RetrievalChunk {
            event_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            session_offset_ms: 28_000,
            action_type: "clipboard.change".to_string(),
            text: "At 00:28 user copied stack trace 'panic: ...'".to_string(),
            referenced_by_segments: vec![1, 4],
            importance: 0.92,
            tags: vec!["user_sourced".to_string(), "app:Slack".to_string()],
        };
        let json = serde_json::to_string(&chunk).unwrap();
        let back: RetrievalChunk = serde_json::from_str(&json).unwrap();
        assert_eq!(back.session_offset_ms, 28_000);
        assert_eq!(back.referenced_by_segments, vec![1, 4]);
        assert_eq!(back.tags[0], "user_sourced");
    }

    #[test]
    fn intent_output_round_trip() {
        let output = IntentOutput {
            session_id: Uuid::new_v4(),
            task: "Rewrite the function".to_string(),
            intent: "rewrite".to_string(),
            intent_category: IntentCategory::Rewrite,
            constraints: vec!["use Rust".to_string()],
            missing_context: vec!["which function".to_string()],
            restructured_speech: "Please rewrite the selected function using Rust".to_string(),
            final_markdown: "## Task\nRewrite the function".to_string(),
            artifacts: vec![ArtifactRef {
                event_id: Uuid::new_v4(),
                label: "context-1".to_string(),
                inline_summary: "Selected code in VSCode".to_string(),
            }],
            references: vec![],
            output_confidence: 0.88,
            risk_level: RiskLevel::Low,
            retrieval_chunks: vec![],
        };

        let json = serde_json::to_string(&output).unwrap();
        let deserialized: IntentOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.task, "Rewrite the function");
        assert_eq!(deserialized.artifacts.len(), 1);
        assert_eq!(deserialized.artifacts[0].label, "context-1");
        assert_eq!(deserialized.intent_category, IntentCategory::Rewrite);
        assert_eq!(deserialized.risk_level, RiskLevel::Low);
        assert!(deserialized.retrieval_chunks.is_empty());
    }

    #[test]
    fn intent_output_pre_annotation_json_deserializes() {
        // IntentOutput JSON written before retrieval_chunks existed —
        // must deserialize cleanly with an empty vec.
        let legacy_json = r#"{
            "session_id": "00000000-0000-0000-0000-000000000001",
            "task": "t",
            "intent": "debug",
            "constraints": [],
            "missing_context": [],
            "restructured_speech": "s",
            "final_markdown": "m",
            "artifacts": [],
            "references": []
        }"#;
        let out: IntentOutput = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(out.task, "t");
        assert!(out.retrieval_chunks.is_empty());
    }
}
