//! Anchor propagation — transparently expands references that land on a
//! user-written anchor (a `TraceCuration.user_note`) to also cover the
//! adjacent raw event the user was actually pointing at.
//!
//! Motivating example (see
//! [docs/design/2026-04-18-trace-annotation-engine-tech-plan.md](../../docs/design/2026-04-18-trace-annotation-engine-tech-plan.md) §0
//! bullet 6):
//!
//! The user pins a toolbar note "堆栈在这" at `t=32s`, but the actual
//! stack-trace clipboard change happened at `t=28s`. When voice
//! references "刚才那个堆栈" later and the LLM resolves it to the note
//! event, `AnchorPropagator` appends a second `RefTarget` pointing at
//! the nearby `ClipboardChange` so prompt/retrieval consumers see both.
//!
//! The added target carries `TargetRole::UserAnchor` and
//! `via_anchor = Some(note_id)` so downstream renderers can explain
//! *how* the link was made.

use std::collections::HashMap;

use talkiwi_core::event::ActionEvent;
use talkiwi_core::output::{RefTarget, Reference, TargetRole};
use uuid::Uuid;

/// Maximum temporal distance for anchor → raw-event propagation.
/// 10 seconds matches the PRD for Trace Toolbar: users typically tap
/// the toolbar within a few seconds of the event they want to pin.
pub const DEFAULT_ANCHOR_WINDOW_MS: u64 = 10_000;

pub struct AnchorPropagator {
    window_ms: u64,
}

impl AnchorPropagator {
    pub fn new() -> Self {
        Self {
            window_ms: DEFAULT_ANCHOR_WINDOW_MS,
        }
    }

    pub fn with_window_ms(window_ms: u64) -> Self {
        Self { window_ms }
    }

    /// Expand each reference: if any target is an anchor event (one with
    /// `curation.user_note` set) and a nearby non-anchor raw event
    /// exists within `window_ms`, append a `UserAnchor` target pointing
    /// at the raw event.
    ///
    /// References with no anchor targets pass through unchanged.
    pub fn propagate(
        &self,
        mut references: Vec<Reference>,
        events: &[ActionEvent],
    ) -> Vec<Reference> {
        if references.is_empty() || events.is_empty() {
            return references;
        }

        let anchors = build_anchor_index(events, self.window_ms);
        if anchors.is_empty() {
            return references;
        }

        for reference in &mut references {
            expand_targets(reference, events, &anchors);
        }
        references
    }
}

impl Default for AnchorPropagator {
    fn default() -> Self {
        Self::new()
    }
}

/// Map from anchor-event-id to the index of the nearest non-anchor,
/// non-deleted event within `window_ms`. Anchors without a nearby
/// event are absent from the map.
fn build_anchor_index(events: &[ActionEvent], window_ms: u64) -> HashMap<Uuid, usize> {
    let mut index: HashMap<Uuid, usize> = HashMap::new();

    for (anchor_idx, anchor) in events.iter().enumerate() {
        if !is_anchor(anchor) {
            continue;
        }
        let anchor_offset = anchor.session_offset_ms;

        let mut best: Option<(usize, u64)> = None;
        for (cand_idx, cand) in events.iter().enumerate() {
            if cand_idx == anchor_idx {
                continue;
            }
            if cand.curation.deleted {
                continue;
            }
            if is_anchor(cand) {
                continue;
            }
            let distance = distance_ms(anchor_offset, cand.session_offset_ms);
            if distance > window_ms {
                continue;
            }
            if best.map(|(_, d)| distance < d).unwrap_or(true) {
                best = Some((cand_idx, distance));
            }
        }

        if let Some((target_idx, _)) = best {
            index.insert(anchor.id, target_idx);
        }
    }

    index
}

/// An anchor event is one the user hand-annotated with `user_note`.
/// Source must be `Toolbar` or `Manual` (never `Passive`) so passive
/// captures that happen to carry a note string never masquerade as
/// anchors.
fn is_anchor(event: &ActionEvent) -> bool {
    event.curation.is_user_sourced() && event.curation.user_note.is_some()
}

fn distance_ms(a: u64, b: u64) -> u64 {
    a.abs_diff(b)
}

fn expand_targets(
    reference: &mut Reference,
    events: &[ActionEvent],
    anchors: &HashMap<Uuid, usize>,
) {
    if reference.targets.is_empty() {
        // Legacy-shape reference — when the legacy primary points at an
        // anchor, promote the reference into the new multi-target shape
        // by seeding `targets` with the original primary (so
        // `primary_event_id()` still returns the note the LLM chose)
        // and appending a `UserAnchor` target for the propagated event.
        let Some(primary_id) = reference.resolved_event_id else {
            return;
        };
        let Some(&target_idx) = anchors.get(&primary_id) else {
            return;
        };
        reference.targets.push(RefTarget {
            event_id: primary_id,
            event_idx: reference.resolved_event_idx,
            role: TargetRole::Source,
            via_anchor: None,
        });
        reference.targets.push(RefTarget {
            event_id: events[target_idx].id,
            event_idx: target_idx,
            role: TargetRole::UserAnchor,
            via_anchor: Some(primary_id),
        });
        return;
    }

    // Collect additions up front so we can extend without interleaving
    // mutations during iteration.
    let mut additions: Vec<RefTarget> = Vec::new();
    let mut already_present: std::collections::HashSet<Uuid> =
        reference.targets.iter().map(|t| t.event_id).collect();

    for target in &reference.targets {
        let Some(&propagated_idx) = anchors.get(&target.event_id) else {
            continue;
        };
        let propagated_id = events[propagated_idx].id;
        if !already_present.insert(propagated_id) {
            continue;
        }
        additions.push(RefTarget {
            event_id: propagated_id,
            event_idx: propagated_idx,
            role: TargetRole::UserAnchor,
            via_anchor: Some(target.event_id),
        });
    }
    reference.targets.extend(additions);
}

#[cfg(test)]
mod tests {
    use super::*;
    use talkiwi_core::event::{
        ActionEvent, ActionPayload, ActionType, ClipboardContentType, TraceCuration,
    };
    use talkiwi_core::output::{RefRelation, Reference, ReferenceStrategy};

    fn make_event(
        kind: ActionType,
        offset: u64,
        payload: ActionPayload,
        curation: TraceCuration,
    ) -> ActionEvent {
        ActionEvent {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            timestamp: 1_712_900_000_000,
            session_offset_ms: offset,
            observed_offset_ms: Some(offset),
            duration_ms: None,
            action_type: kind,
            plugin_id: "builtin".to_string(),
            payload,
            semantic_hint: None,
            confidence: 1.0,
            curation,
        }
    }

    fn clipboard_event(offset: u64) -> ActionEvent {
        make_event(
            ActionType::ClipboardChange,
            offset,
            ActionPayload::ClipboardChange {
                content_type: ClipboardContentType::Text,
                text: Some("panic: ...".to_string()),
                file_path: None,
                source_app: None,
            },
            TraceCuration::default(),
        )
    }

    fn toolbar_note(offset: u64, note: &str) -> ActionEvent {
        let curation = TraceCuration {
            source: talkiwi_core::event::TraceSource::Toolbar,
            role: None,
            user_note: Some(note.to_string()),
            deleted: false,
        };
        make_event(
            ActionType::Custom("manual.note".to_string()),
            offset,
            ActionPayload::Custom(serde_json::json!({ "note": note })),
            curation,
        )
    }

    fn make_reference_with_target(target_event: &ActionEvent, target_idx: usize) -> Reference {
        Reference {
            spoken_text: "刚才那个堆栈".to_string(),
            spoken_offset: 0,
            resolved_event_idx: target_idx,
            resolved_event_id: Some(target_event.id),
            confidence: 0.72,
            strategy: ReferenceStrategy::LlmCoreference,
            user_confirmed: false,
            targets: vec![RefTarget {
                event_id: target_event.id,
                event_idx: target_idx,
                role: TargetRole::Source,
                via_anchor: None,
            }],
            relation: RefRelation::Single,
            segment_idx: None,
        }
    }

    #[test]
    fn reference_pointing_at_anchor_gains_propagated_target() {
        let clipboard = clipboard_event(28_000);
        let note = toolbar_note(32_000, "堆栈在这");
        let events = vec![clipboard.clone(), note.clone()];

        let propagator = AnchorPropagator::new();
        let input = vec![make_reference_with_target(&note, 1)];

        let out = propagator.propagate(input, &events);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].targets.len(), 2);

        let anchor_target = &out[0].targets[0];
        assert_eq!(anchor_target.event_id, note.id);
        assert_eq!(anchor_target.role, TargetRole::Source);

        let propagated = &out[0].targets[1];
        assert_eq!(propagated.event_id, clipboard.id);
        assert_eq!(propagated.role, TargetRole::UserAnchor);
        assert_eq!(propagated.via_anchor, Some(note.id));
    }

    #[test]
    fn reference_unrelated_to_anchors_passes_through() {
        let clipboard = clipboard_event(28_000);
        let note = toolbar_note(32_000, "堆栈在这");
        let other = clipboard_event(80_000);
        let events = vec![clipboard, note, other.clone()];

        let propagator = AnchorPropagator::new();
        let input = vec![make_reference_with_target(&other, 2)];

        let out = propagator.propagate(input, &events);
        assert_eq!(out[0].targets.len(), 1);
        assert_eq!(out[0].targets[0].event_id, other.id);
    }

    #[test]
    fn propagation_skips_anchor_without_nearby_event() {
        // Note at 30s, nothing within 10s window.
        let note = toolbar_note(30_000, "孤立的标签");
        let far = clipboard_event(90_000);
        let events = vec![note.clone(), far];

        let propagator = AnchorPropagator::new();
        let input = vec![make_reference_with_target(&note, 0)];

        let out = propagator.propagate(input, &events);
        assert_eq!(out[0].targets.len(), 1);
    }

    #[test]
    fn propagation_honors_window_config() {
        let clipboard = clipboard_event(20_000);
        let note = toolbar_note(35_000, "太远"); // 15s gap > default 10s
        let events = vec![clipboard, note.clone()];

        let default_propagator = AnchorPropagator::new();
        let input = vec![make_reference_with_target(&note, 1)];
        let out_default = default_propagator.propagate(input.clone(), &events);
        assert_eq!(out_default[0].targets.len(), 1);

        let wide_propagator = AnchorPropagator::with_window_ms(20_000);
        let out_wide = wide_propagator.propagate(input, &events);
        assert_eq!(out_wide[0].targets.len(), 2);
    }

    #[test]
    fn propagation_does_not_duplicate_existing_target() {
        // The reference already names both the note and the clipboard.
        // Anchor propagation must not add a duplicate target.
        let clipboard = clipboard_event(28_000);
        let note = toolbar_note(32_000, "堆栈在这");
        let events = vec![clipboard.clone(), note.clone()];

        let mut reference = make_reference_with_target(&note, 1);
        reference.targets.push(RefTarget {
            event_id: clipboard.id,
            event_idx: 0,
            role: TargetRole::Source,
            via_anchor: None,
        });
        reference.relation = RefRelation::Composition;

        let out = AnchorPropagator::new().propagate(vec![reference], &events);
        assert_eq!(out[0].targets.len(), 2);
    }

    #[test]
    fn propagation_skips_deleted_anchor_neighbors() {
        let mut clipboard = clipboard_event(28_000);
        clipboard.curation.deleted = true;
        let later_live = clipboard_event(33_000);
        let note = toolbar_note(32_000, "堆栈在这");
        let events = vec![clipboard, later_live.clone(), note.clone()];

        let out = AnchorPropagator::new()
            .propagate(vec![make_reference_with_target(&note, 2)], &events);
        // Should pick `later_live`, not the deleted clipboard.
        assert_eq!(out[0].targets.len(), 2);
        assert_eq!(out[0].targets[1].event_id, later_live.id);
    }

    #[test]
    fn legacy_reference_without_targets_still_propagates() {
        // Reference written before Phase 1 — targets empty, legacy id set.
        // Anchor propagation must *both* seed a primary `Source` target
        // (so `primary_event_id()` still names the note the LLM chose)
        // and append a `UserAnchor` target for the propagated clipboard.
        let clipboard = clipboard_event(28_000);
        let note = toolbar_note(32_000, "堆栈在这");
        let events = vec![clipboard.clone(), note.clone()];

        let reference = Reference {
            spoken_text: "堆栈".to_string(),
            spoken_offset: 0,
            resolved_event_idx: 1,
            resolved_event_id: Some(note.id),
            confidence: 0.5,
            strategy: ReferenceStrategy::TemporalProximity,
            user_confirmed: false,
            targets: Vec::new(),
            relation: RefRelation::Single,
            segment_idx: None,
        };

        let out = AnchorPropagator::new().propagate(vec![reference], &events);
        assert_eq!(out[0].targets.len(), 2);
        assert_eq!(out[0].targets[0].event_id, note.id);
        assert_eq!(out[0].targets[0].role, TargetRole::Source);
        assert_eq!(out[0].targets[1].role, TargetRole::UserAnchor);
        assert_eq!(out[0].targets[1].via_anchor, Some(note.id));
        assert_eq!(out[0].targets[1].event_id, clipboard.id);
        assert_eq!(out[0].primary_event_id(), Some(note.id));
    }

    #[test]
    fn anchor_requires_both_user_sourced_and_user_note() {
        // Passive event with a note string in curation — should NOT
        // count as an anchor (source must be Toolbar/Manual).
        let mut passive_with_note = clipboard_event(28_000);
        passive_with_note.curation.user_note = Some("ignored".to_string());
        let clipboard = clipboard_event(35_000);
        let events = vec![passive_with_note.clone(), clipboard];

        // Build reference pointing at the passive event — no anchor
        // behavior expected.
        let reference = make_reference_with_target(&passive_with_note, 0);
        let out = AnchorPropagator::new().propagate(vec![reference], &events);
        assert_eq!(out[0].targets.len(), 1);
    }
}
