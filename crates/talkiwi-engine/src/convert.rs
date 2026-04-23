//! Convert LLM-produced `RawReference` payloads into multi-target
//! `Reference` objects the rest of the pipeline consumes.
//!
//! The v2 shape ([RawReference] with `segment_idx` +
//! `event_indices` + `relation` + `excluded_indices`) lets a single
//! spoken phrase point at several events with differentiated roles.
//! See [docs/design/2026-04-18-trace-annotation-engine-tech-plan.md](../../docs/design/2026-04-18-trace-annotation-engine-tech-plan.md) §8.
//!
//! Out-of-bounds indices (the LLM occasionally hallucinates) are
//! silently skipped — a reference with **zero** resolvable targets
//! after filtering is dropped entirely.

use talkiwi_core::event::ActionEvent;
use talkiwi_core::output::{RefRelation, RefTarget, Reference, ReferenceStrategy, TargetRole};
use talkiwi_core::traits::intent::{RawReference, SegmentCandidatesRef};

/// Confidence assigned to LLM-coreference references when the provider
/// doesn't ship a per-reference confidence. Matches the v1 constant.
pub const DEFAULT_LLM_CONFIDENCE: f32 = 0.85;

/// Convert the v2 LLM output into `Reference`s, translating per-segment
/// candidate indices back to global event indices via
/// `candidates_per_segment`. Falls back to the legacy v1 semantic when
/// a `RawReference` has no `segment_idx`.
pub fn convert_raw_references_v2(
    raw_refs: &[RawReference],
    candidates_per_segment: &[SegmentCandidatesRef],
    events: &[ActionEvent],
) -> Vec<Reference> {
    raw_refs
        .iter()
        .filter_map(|raw| convert_single(raw, candidates_per_segment, events))
        .collect()
}

fn convert_single(
    raw: &RawReference,
    candidates_per_segment: &[SegmentCandidatesRef],
    events: &[ActionEvent],
) -> Option<Reference> {
    let positive_indices = collect_positive_indices(raw);
    let excluded_indices = raw.excluded_indices.clone();

    let positive_targets = resolve_targets(
        &positive_indices,
        raw.segment_idx,
        candidates_per_segment,
        events,
        default_positive_role(raw.relation),
    );
    let excluded_targets = resolve_targets(
        &excluded_indices,
        raw.segment_idx,
        candidates_per_segment,
        events,
        TargetRole::ExcludedAspect,
    );

    // For `Subtraction`, all positive targets are repurposed as
    // PreserveScope exclusions — there is no constructive target.
    let mut targets: Vec<RefTarget> = match raw.relation {
        RefRelation::Subtraction => positive_targets
            .into_iter()
            .map(|mut t| {
                t.role = TargetRole::PreserveScope;
                t
            })
            .chain(excluded_targets)
            .collect(),
        _ => positive_targets
            .into_iter()
            .chain(excluded_targets)
            .collect(),
    };
    dedup_targets(&mut targets);

    if targets.is_empty() {
        return None;
    }

    let primary = targets.first().unwrap();
    let resolved_event_idx = primary.event_idx;
    let resolved_event_id = Some(primary.event_id);
    let relation = normalize_relation(raw.relation);

    Some(Reference {
        spoken_text: raw.spoken_text.clone(),
        spoken_offset: 0,
        resolved_event_idx,
        resolved_event_id,
        confidence: DEFAULT_LLM_CONFIDENCE,
        strategy: ReferenceStrategy::LlmCoreference,
        user_confirmed: false,
        targets,
        relation,
        segment_idx: raw.segment_idx,
    })
}

fn collect_positive_indices(raw: &RawReference) -> Vec<usize> {
    raw.effective_indices()
}

fn default_positive_role(relation: RefRelation) -> TargetRole {
    match relation {
        RefRelation::Subtraction => TargetRole::PreserveScope,
        _ => TargetRole::Source,
    }
}

/// Unknown relations from a future LLM response degrade to `Single` at
/// consumption time — we never forward `Unknown` downstream since
/// assembler / retrieval only branch on the concrete four cases.
fn normalize_relation(relation: RefRelation) -> RefRelation {
    match relation {
        RefRelation::Unknown => RefRelation::Single,
        other => other,
    }
}

fn resolve_targets(
    indices: &[usize],
    segment_idx: Option<usize>,
    candidates_per_segment: &[SegmentCandidatesRef],
    events: &[ActionEvent],
    role: TargetRole,
) -> Vec<RefTarget> {
    indices
        .iter()
        .filter_map(|&idx| {
            let event_idx = resolve_global_event_idx(idx, segment_idx, candidates_per_segment)?;
            let event = events.get(event_idx)?;
            Some(RefTarget {
                event_id: event.id,
                event_idx,
                role,
                via_anchor: None,
            })
        })
        .collect()
}

/// Given a candidate-set index, translate it to the global `events`
/// index. When `segment_idx` is absent, the index is already global
/// (legacy v1 semantic).
fn resolve_global_event_idx(
    raw_idx: usize,
    segment_idx: Option<usize>,
    candidates_per_segment: &[SegmentCandidatesRef],
) -> Option<usize> {
    let Some(seg) = segment_idx else {
        return Some(raw_idx);
    };
    let bundle = candidates_per_segment.iter().find(|b| b.segment_idx == seg)?;
    let candidate = bundle.candidates.get(raw_idx)?;
    Some(candidate.event_idx)
}

fn dedup_targets(targets: &mut Vec<RefTarget>) {
    let mut seen: std::collections::HashSet<(uuid::Uuid, TargetRole)> = Default::default();
    targets.retain(|t| seen.insert((t.event_id, t.role)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use talkiwi_core::event::{
        ActionEvent, ActionPayload, ActionType, ClipboardContentType, TraceCuration,
    };
    use talkiwi_core::traits::intent::{CandidateRef, SegmentCandidatesRef};
    use uuid::Uuid;

    fn event(offset: u64, kind: ActionType) -> ActionEvent {
        let payload = match kind {
            ActionType::SelectionText => ActionPayload::SelectionText {
                text: "x".to_string(),
                app_name: "VSCode".to_string(),
                window_title: "f.rs".to_string(),
                char_count: 1,
            },
            ActionType::ClickLink => ActionPayload::ClickLink {
                from_url: None,
                to_url: "https://x/".to_string(),
                title: None,
            },
            _ => ActionPayload::ClipboardChange {
                content_type: ClipboardContentType::Text,
                text: Some("c".to_string()),
                file_path: None,
                source_app: None,
            },
        };
        ActionEvent {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            timestamp: 1_712_900_000_000,
            session_offset_ms: offset,
            observed_offset_ms: Some(offset),
            duration_ms: None,
            action_type: kind,
            plugin_id: "b".to_string(),
            payload,
            semantic_hint: None,
            confidence: 1.0,
            curation: TraceCuration::default(),
        }
    }

    fn bundle(seg: usize, pairs: &[(usize, usize)]) -> SegmentCandidatesRef {
        SegmentCandidatesRef {
            segment_idx: seg,
            candidates: pairs
                .iter()
                .map(|&(cand_idx, event_idx)| CandidateRef {
                    cand_idx,
                    event_idx,
                    session_offset_ms: 0,
                    action_type: "x".to_string(),
                    user_sourced: false,
                    payload_preview: String::new(),
                })
                .collect(),
        }
    }

    #[test]
    fn v1_raw_reference_without_segment_idx_uses_global_indices() {
        let events = vec![
            event(1_000, ActionType::ClipboardChange),
            event(2_000, ActionType::SelectionText),
        ];
        let raw = RawReference::v1("这段代码", 1, "...");
        let out = convert_raw_references_v2(&[raw], &[], &events);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].primary_event_id(), Some(events[1].id));
        assert_eq!(out[0].relation, RefRelation::Single);
        assert_eq!(out[0].targets.len(), 1);
        assert_eq!(out[0].strategy, ReferenceStrategy::LlmCoreference);
    }

    #[test]
    fn v2_reference_translates_candidate_idx_to_global() {
        // Events 0,1,2. Segment 0's candidates: cand 0 → event 2.
        let events = vec![
            event(1_000, ActionType::ClickLink),
            event(2_000, ActionType::ClipboardChange),
            event(3_000, ActionType::SelectionText),
        ];
        let candidates = vec![bundle(0, &[(0, 2)])];
        let raw = RawReference {
            spoken_text: "这段代码".to_string(),
            event_index: Some(0),
            reason: String::new(),
            segment_idx: Some(0),
            event_indices: vec![0],
            relation: RefRelation::Single,
            excluded_indices: Vec::new(),
        };
        let out = convert_raw_references_v2(&[raw], &candidates, &events);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].primary_event_id(), Some(events[2].id));
    }

    #[test]
    fn composition_produces_multi_target_reference() {
        let events = vec![
            event(1_000, ActionType::Screenshot),
            event(2_000, ActionType::ClickLink),
            event(3_000, ActionType::Screenshot),
        ];
        let candidates = vec![bundle(0, &[(0, 0), (1, 1), (2, 2)])];
        let raw = RawReference {
            spoken_text: "rewind + loom".to_string(),
            event_index: Some(0),
            reason: String::new(),
            segment_idx: Some(0),
            event_indices: vec![0, 2],
            relation: RefRelation::Composition,
            excluded_indices: Vec::new(),
        };
        let out = convert_raw_references_v2(&[raw], &candidates, &events);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].relation, RefRelation::Composition);
        assert_eq!(out[0].targets.len(), 2);
        assert_eq!(out[0].targets[0].event_id, events[0].id);
        assert_eq!(out[0].targets[1].event_id, events[2].id);
    }

    #[test]
    fn contrast_splits_positive_and_excluded_roles() {
        let events = vec![
            event(1_000, ActionType::ClickLink),
            event(2_000, ActionType::Screenshot),
        ];
        let candidates = vec![bundle(0, &[(0, 0), (1, 1)])];
        let raw = RawReference {
            spoken_text: "像 loom 但不要 thumbnail".to_string(),
            event_index: Some(0),
            reason: String::new(),
            segment_idx: Some(0),
            event_indices: vec![0],
            relation: RefRelation::Contrast,
            excluded_indices: vec![1],
        };
        let out = convert_raw_references_v2(&[raw], &candidates, &events);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].relation, RefRelation::Contrast);
        assert_eq!(out[0].targets.len(), 2);
        let source = out[0]
            .targets
            .iter()
            .find(|t| t.event_id == events[0].id)
            .unwrap();
        let excluded = out[0]
            .targets
            .iter()
            .find(|t| t.event_id == events[1].id)
            .unwrap();
        assert_eq!(source.role, TargetRole::Source);
        assert_eq!(excluded.role, TargetRole::ExcludedAspect);
    }

    #[test]
    fn subtraction_marks_all_positive_as_preserve_scope() {
        let events = vec![event(1_000, ActionType::SelectionText)];
        let candidates = vec![bundle(0, &[(0, 0)])];
        let raw = RawReference {
            spoken_text: "别动这部分".to_string(),
            event_index: Some(0),
            reason: String::new(),
            segment_idx: Some(0),
            event_indices: vec![0],
            relation: RefRelation::Subtraction,
            excluded_indices: Vec::new(),
        };
        let out = convert_raw_references_v2(&[raw], &candidates, &events);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].relation, RefRelation::Subtraction);
        assert_eq!(out[0].targets[0].role, TargetRole::PreserveScope);
    }

    #[test]
    fn out_of_bounds_indices_are_silently_dropped() {
        let events = vec![event(1_000, ActionType::SelectionText)];
        let candidates = vec![bundle(0, &[(0, 0)])];
        let raw = RawReference {
            spoken_text: "x".to_string(),
            event_index: Some(0),
            reason: String::new(),
            segment_idx: Some(0),
            // cand_idx 5 doesn't exist in the segment's candidate list.
            event_indices: vec![5],
            relation: RefRelation::Single,
            excluded_indices: Vec::new(),
        };
        let out = convert_raw_references_v2(&[raw], &candidates, &events);
        assert!(out.is_empty());
    }

    #[test]
    fn missing_segment_in_bundle_drops_reference() {
        let events = vec![event(1_000, ActionType::SelectionText)];
        let candidates: Vec<SegmentCandidatesRef> = Vec::new();
        let raw = RawReference {
            spoken_text: "x".to_string(),
            event_index: Some(0),
            reason: String::new(),
            segment_idx: Some(3),
            event_indices: vec![0],
            relation: RefRelation::Single,
            excluded_indices: Vec::new(),
        };
        let out = convert_raw_references_v2(&[raw], &candidates, &events);
        assert!(out.is_empty());
    }

    #[test]
    fn unknown_relation_degrades_to_single() {
        let events = vec![event(1_000, ActionType::SelectionText)];
        let candidates = vec![bundle(0, &[(0, 0)])];
        let raw = RawReference {
            spoken_text: "x".to_string(),
            event_index: Some(0),
            reason: String::new(),
            segment_idx: Some(0),
            event_indices: vec![0],
            relation: RefRelation::Unknown,
            excluded_indices: Vec::new(),
        };
        let out = convert_raw_references_v2(&[raw], &candidates, &events);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].relation, RefRelation::Single);
    }

    #[test]
    fn duplicate_positive_and_excluded_indices_dedup() {
        let events = vec![event(1_000, ActionType::SelectionText)];
        let candidates = vec![bundle(0, &[(0, 0)])];
        let raw = RawReference {
            spoken_text: "x".to_string(),
            event_index: Some(0),
            reason: String::new(),
            segment_idx: Some(0),
            // Same idx appears in both positive and excluded — the
            // second should win under the stable Source → Excluded
            // order, but dedup keys on (id, role) so both survive.
            event_indices: vec![0, 0],
            relation: RefRelation::Contrast,
            excluded_indices: vec![0],
        };
        let out = convert_raw_references_v2(&[raw], &candidates, &events);
        assert_eq!(out.len(), 1);
        // One Source + one ExcludedAspect for the same event_id.
        assert_eq!(out[0].targets.len(), 2);
    }
}
