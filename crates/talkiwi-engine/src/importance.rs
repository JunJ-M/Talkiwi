//! Importance scoring for action events.
//!
//! Per [docs/design/2026-04-18-trace-annotation-engine-tech-plan.md](../../docs/design/2026-04-18-trace-annotation-engine-tech-plan.md) §9,
//! every event receives a derived score
//!
//! ```text
//! score(e) = α · ref_count_norm(e)
//!          + β · is_user_sourced(e)
//!          + γ · type_prior(e.action_type)
//!          + δ · recency_norm(e, session)
//! ```
//!
//! with defaults `α=0.45, β=0.25, γ=0.20, δ=0.10`. Scores are used to
//! filter the prompt surface (score >= `prompt_threshold` enters
//! `artifacts`) while retrieval chunks are emitted for *every* event
//! regardless of score — see the tech plan §9.3.

use std::collections::HashMap;

use talkiwi_core::event::ActionEvent;
use talkiwi_core::output::Reference;
use uuid::Uuid;

use crate::candidate::type_prior;

pub const DEFAULT_ALPHA_REF_COUNT: f32 = 0.45;
pub const DEFAULT_BETA_USER_SOURCED: f32 = 0.25;
pub const DEFAULT_GAMMA_TYPE_PRIOR: f32 = 0.20;
pub const DEFAULT_DELTA_RECENCY: f32 = 0.10;

/// Saturation ceiling for `ref_count_norm`. A single voice reference
/// earns ~0.33; three or more saturate at 1.0. Matches the tech plan.
pub const REF_COUNT_SATURATION: f32 = 3.0;

/// Lower bound on `recency_norm`. Even the oldest event in a session
/// gets at least this fraction so recency never fully zeros out the
/// remaining score.
pub const RECENCY_FLOOR: f32 = 0.3;

pub const DEFAULT_PROMPT_THRESHOLD: f32 = 0.35;

#[derive(Debug, Clone)]
pub struct ScoringWeights {
    pub alpha_ref_count: f32,
    pub beta_user_sourced: f32,
    pub gamma_type_prior: f32,
    pub delta_recency: f32,
}

impl Default for ScoringWeights {
    fn default() -> Self {
        Self {
            alpha_ref_count: DEFAULT_ALPHA_REF_COUNT,
            beta_user_sourced: DEFAULT_BETA_USER_SOURCED,
            gamma_type_prior: DEFAULT_GAMMA_TYPE_PRIOR,
            delta_recency: DEFAULT_DELTA_RECENCY,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EventScore {
    pub event_idx: usize,
    pub score: f32,
    pub ref_count: u32,
}

pub struct ImportanceScorer {
    weights: ScoringWeights,
}

impl ImportanceScorer {
    pub fn new() -> Self {
        Self::with_weights(ScoringWeights::default())
    }

    pub fn with_weights(weights: ScoringWeights) -> Self {
        Self { weights }
    }

    /// Score every event in `events` given the voice-produced
    /// `references`. Output indices match `events`.
    pub fn score_all(
        &self,
        events: &[ActionEvent],
        references: &[Reference],
    ) -> Vec<EventScore> {
        let ref_counts = count_references_per_event(references);
        let (session_start, session_end) = session_extent(events);

        events
            .iter()
            .enumerate()
            .map(|(idx, event)| {
                let ref_count = *ref_counts.get(&event.id).unwrap_or(&0);
                let score = self.compose_score(event, ref_count, session_start, session_end);
                EventScore {
                    event_idx: idx,
                    score,
                    ref_count,
                }
            })
            .collect()
    }

    fn compose_score(
        &self,
        event: &ActionEvent,
        ref_count: u32,
        session_start: u64,
        session_end: u64,
    ) -> f32 {
        let ref_component =
            self.weights.alpha_ref_count * ref_count_norm(ref_count);
        let user_component = if event.curation.is_user_sourced() {
            self.weights.beta_user_sourced
        } else {
            0.0
        };
        let type_component =
            self.weights.gamma_type_prior * type_prior(&event.action_type);
        let recency_component = self.weights.delta_recency
            * recency_norm(event.session_offset_ms, session_start, session_end);

        let raw = ref_component + user_component + type_component + recency_component;
        raw.clamp(0.0, 1.0)
    }
}

impl Default for ImportanceScorer {
    fn default() -> Self {
        Self::new()
    }
}

/// Count how many references land on each event, following both the
/// new `targets` list and the legacy `resolved_event_id` fallback.
fn count_references_per_event(references: &[Reference]) -> HashMap<Uuid, u32> {
    let mut counts: HashMap<Uuid, u32> = HashMap::new();
    for reference in references {
        if !reference.targets.is_empty() {
            for target in &reference.targets {
                *counts.entry(target.event_id).or_insert(0) += 1;
            }
        } else if let Some(id) = reference.resolved_event_id {
            *counts.entry(id).or_insert(0) += 1;
        }
    }
    counts
}

fn ref_count_norm(ref_count: u32) -> f32 {
    (ref_count as f32 / REF_COUNT_SATURATION).min(1.0)
}

/// Normalized recency in [RECENCY_FLOOR, 1.0]. An event at session
/// end returns 1.0; the session start approaches `RECENCY_FLOOR`.
fn recency_norm(offset_ms: u64, session_start: u64, session_end: u64) -> f32 {
    if session_end <= session_start {
        return 1.0;
    }
    let duration = (session_end - session_start) as f32;
    let elapsed = offset_ms.saturating_sub(session_start) as f32;
    let raw = elapsed / duration;
    raw.clamp(RECENCY_FLOOR, 1.0)
}

fn session_extent(events: &[ActionEvent]) -> (u64, u64) {
    if events.is_empty() {
        return (0, 0);
    }
    let mut min = u64::MAX;
    let mut max = 0u64;
    for event in events {
        if event.session_offset_ms < min {
            min = event.session_offset_ms;
        }
        if event.session_offset_ms > max {
            max = event.session_offset_ms;
        }
    }
    (min, max)
}

/// Default prompt-surface filter: keep only events scoring at or above
/// `threshold`. Returns the subset of `scores` that pass.
pub fn filter_for_prompt(scores: &[EventScore], threshold: f32) -> Vec<EventScore> {
    scores
        .iter()
        .copied()
        .filter(|s| s.score >= threshold)
        .collect()
}

/// True if the event type is one the prompt surface should always show
/// when user-sourced — matches the prompt filtering rationale in the
/// tech plan: user-sourced anchors should never be silently dropped
/// even if their `type_prior` is low.
#[allow(dead_code)]
pub fn is_always_keep(event: &ActionEvent) -> bool {
    event.curation.is_user_sourced()
}

#[cfg(test)]
mod tests {
    use super::*;
    use talkiwi_core::event::{
        ActionEvent, ActionPayload, ActionType, ClipboardContentType, TraceCuration,
    };
    use talkiwi_core::output::{
        RefRelation, RefTarget, Reference, ReferenceStrategy, TargetRole,
    };

    fn make_event(kind: ActionType, offset: u64, user_sourced: bool) -> ActionEvent {
        let curation = if user_sourced {
            TraceCuration::toolbar()
        } else {
            TraceCuration::default()
        };
        ActionEvent {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            timestamp: 1_712_900_000_000,
            session_offset_ms: offset,
            observed_offset_ms: Some(offset),
            duration_ms: None,
            action_type: kind,
            plugin_id: "builtin".to_string(),
            payload: ActionPayload::ClipboardChange {
                content_type: ClipboardContentType::Text,
                text: Some("x".to_string()),
                file_path: None,
                source_app: None,
            },
            semantic_hint: None,
            confidence: 1.0,
            curation,
        }
    }

    fn make_ref_targeting(event_id: Uuid) -> Reference {
        Reference {
            spoken_text: "x".to_string(),
            spoken_offset: 0,
            resolved_event_idx: 0,
            resolved_event_id: Some(event_id),
            confidence: 0.85,
            strategy: ReferenceStrategy::LlmCoreference,
            user_confirmed: false,
            targets: vec![RefTarget {
                event_id,
                event_idx: 0,
                role: TargetRole::Source,
                via_anchor: None,
            }],
            relation: RefRelation::Single,
            segment_idx: None,
        }
    }

    #[test]
    fn empty_events_produce_empty_scores() {
        let scorer = ImportanceScorer::new();
        let out = scorer.score_all(&[], &[]);
        assert!(out.is_empty());
    }

    #[test]
    fn ref_count_norm_saturates() {
        assert!((ref_count_norm(0) - 0.0).abs() < 1e-6);
        assert!((ref_count_norm(1) - 1.0 / 3.0).abs() < 1e-6);
        assert!((ref_count_norm(3) - 1.0).abs() < 1e-6);
        assert!((ref_count_norm(10) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn recency_norm_has_floor() {
        // Event at session start gets the floor value.
        assert!((recency_norm(0, 0, 10_000) - RECENCY_FLOOR).abs() < 1e-6);
        // Event at session end gets 1.0.
        assert!((recency_norm(10_000, 0, 10_000) - 1.0).abs() < 1e-6);
        // Zero duration: always 1.0.
        assert!((recency_norm(0, 0, 0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn user_sourced_event_outscores_passive_of_same_type() {
        let scorer = ImportanceScorer::new();
        let passive = make_event(ActionType::SelectionText, 5_000, false);
        let pinned = make_event(ActionType::SelectionText, 5_000, true);
        let events = vec![passive, pinned];
        let scores = scorer.score_all(&events, &[]);
        assert!(scores[1].score > scores[0].score);
    }

    #[test]
    fn referenced_event_outscores_unreferenced_of_same_type() {
        let scorer = ImportanceScorer::new();
        let plain = make_event(ActionType::SelectionText, 5_000, false);
        let referenced = make_event(ActionType::SelectionText, 5_000, false);
        let events = vec![plain, referenced.clone()];
        let references = vec![make_ref_targeting(referenced.id)];
        let scores = scorer.score_all(&events, &references);
        assert!(scores[1].score > scores[0].score);
        assert_eq!(scores[1].ref_count, 1);
    }

    #[test]
    fn click_mouse_scores_below_prompt_threshold_by_default() {
        let scorer = ImportanceScorer::new();
        let click = make_event(ActionType::ClickMouse, 5_000, false);
        let scores = scorer.score_all(&[click], &[]);
        assert!(scores[0].score < DEFAULT_PROMPT_THRESHOLD);
    }

    #[test]
    fn selection_text_with_reference_passes_prompt_threshold() {
        let scorer = ImportanceScorer::new();
        let selection = make_event(ActionType::SelectionText, 5_000, false);
        let references = vec![make_ref_targeting(selection.id)];
        let scores = scorer.score_all(&[selection], &references);
        assert!(scores[0].score >= DEFAULT_PROMPT_THRESHOLD);
    }

    #[test]
    fn filter_for_prompt_drops_below_threshold() {
        let scores = vec![
            EventScore {
                event_idx: 0,
                score: 0.10,
                ref_count: 0,
            },
            EventScore {
                event_idx: 1,
                score: 0.40,
                ref_count: 1,
            },
            EventScore {
                event_idx: 2,
                score: 0.90,
                ref_count: 2,
            },
        ];
        let kept = filter_for_prompt(&scores, DEFAULT_PROMPT_THRESHOLD);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].event_idx, 1);
        assert_eq!(kept[1].event_idx, 2);
    }

    #[test]
    fn legacy_reference_without_targets_still_counts() {
        let scorer = ImportanceScorer::new();
        let event = make_event(ActionType::SelectionText, 5_000, false);
        let legacy_ref = Reference {
            spoken_text: "x".to_string(),
            spoken_offset: 0,
            resolved_event_idx: 0,
            resolved_event_id: Some(event.id),
            confidence: 0.7,
            strategy: ReferenceStrategy::TemporalProximity,
            user_confirmed: false,
            targets: Vec::new(),
            relation: RefRelation::Single,
            segment_idx: None,
        };
        let scores = scorer.score_all(&[event], &[legacy_ref]);
        assert_eq!(scores[0].ref_count, 1);
    }

    #[test]
    fn multi_target_reference_counts_each_target_once() {
        let scorer = ImportanceScorer::new();
        let a = make_event(ActionType::SelectionText, 5_000, false);
        let b = make_event(ActionType::ClickLink, 6_000, false);
        let reference = Reference {
            spoken_text: "A 和 B".to_string(),
            spoken_offset: 0,
            resolved_event_idx: 0,
            resolved_event_id: Some(a.id),
            confidence: 0.88,
            strategy: ReferenceStrategy::LlmCoreference,
            user_confirmed: false,
            targets: vec![
                RefTarget {
                    event_id: a.id,
                    event_idx: 0,
                    role: TargetRole::Source,
                    via_anchor: None,
                },
                RefTarget {
                    event_id: b.id,
                    event_idx: 1,
                    role: TargetRole::Source,
                    via_anchor: None,
                },
            ],
            relation: RefRelation::Composition,
            segment_idx: None,
        };
        let scores = scorer.score_all(&[a, b], &[reference]);
        assert_eq!(scores[0].ref_count, 1);
        assert_eq!(scores[1].ref_count, 1);
    }
}
