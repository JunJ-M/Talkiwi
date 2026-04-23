//! Retrieval-surface renderer — produces one `RetrievalChunk` per
//! action event, composing:
//!
//! - natural-language payload rendering
//! - voice context (the spoken_text of every reference pointing at the
//!   event, and the reference's relation)
//! - importance score
//! - facet tags (`user_sourced`, `type:<kind>`, `app:<name>`,
//!   `domain:<host>`)
//!
//! Unlike the prompt surface, retrieval chunks are emitted for **every
//! non-deleted event** regardless of importance — embed-time filtering
//! happens at query time, not render time.
//!
//! See [docs/design/2026-04-18-trace-annotation-engine-tech-plan.md](../../docs/design/2026-04-18-trace-annotation-engine-tech-plan.md) §10.2.

use std::collections::HashMap;

use talkiwi_core::event::{ActionEvent, ActionPayload};
use talkiwi_core::output::{RefRelation, Reference, RetrievalChunk};
use talkiwi_core::session::SpeakSegment;
use uuid::Uuid;

use crate::importance::EventScore;
use crate::payload_render::narrate_for_retrieval;

const SESSION_OFFSET_FORMAT_MINUTES_CAP: u64 = 99 * 60;

/// Compose a retrieval chunk per live event. `scores` must come from
/// [`crate::importance::ImportanceScorer::score_all`] — its length
/// matches `events`. `segments` is consulted only to attach voice
/// context to chunks that are referenced.
pub fn render_chunks(
    events: &[ActionEvent],
    segments: &[SpeakSegment],
    references: &[Reference],
    scores: &[EventScore],
) -> Vec<RetrievalChunk> {
    if events.is_empty() {
        return Vec::new();
    }

    let refs_by_event = index_references_by_event(references);
    let score_by_idx: HashMap<usize, &EventScore> =
        scores.iter().map(|s| (s.event_idx, s)).collect();
    let ref_segment_idx = map_reference_to_segment(references, segments);

    events
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.curation.deleted)
        .map(|(idx, event)| {
            let importance = score_by_idx
                .get(&idx)
                .map(|s| s.score)
                .unwrap_or(0.0);
            let refs = refs_by_event
                .get(&event.id)
                .cloned()
                .unwrap_or_default();
            let referenced_by_segments =
                collect_segment_indices(&refs, references, &ref_segment_idx);
            let text = compose_text(event, &refs, references, segments, importance);
            let tags = build_tags(event);

            RetrievalChunk {
                event_id: event.id,
                session_id: event.session_id,
                session_offset_ms: event.session_offset_ms,
                action_type: event.action_type.as_str().to_string(),
                text,
                referenced_by_segments,
                importance,
                tags,
            }
        })
        .collect()
}

/// Map event_id → indices into `references` that name it.
fn index_references_by_event(references: &[Reference]) -> HashMap<Uuid, Vec<usize>> {
    let mut out: HashMap<Uuid, Vec<usize>> = HashMap::new();
    for (ri, reference) in references.iter().enumerate() {
        let ids: Vec<Uuid> = if reference.targets.is_empty() {
            reference.resolved_event_id.into_iter().collect()
        } else {
            reference.targets.iter().map(|t| t.event_id).collect()
        };
        for id in ids {
            out.entry(id).or_default().push(ri);
        }
    }
    out
}

/// For each reference, pick the segment it belongs to. Prefers the v2
/// LLM-supplied `reference.segment_idx` when present (exact); falls
/// back to a text-search heuristic for legacy references where that
/// field was not populated.
fn map_reference_to_segment(
    references: &[Reference],
    segments: &[SpeakSegment],
) -> Vec<Option<usize>> {
    references
        .iter()
        .map(|reference| segment_for_reference(reference, segments))
        .collect()
}

fn segment_for_reference(
    reference: &Reference,
    segments: &[SpeakSegment],
) -> Option<usize> {
    if segments.is_empty() {
        return None;
    }
    // Prefer the exact `segment_idx` the LLM tagged the reference
    // with. This avoids miscrediting repeated deixis forms like
    // "这个链接" that would otherwise collide in text search.
    if let Some(idx) = reference.segment_idx {
        if idx < segments.len() {
            return Some(idx);
        }
    }
    // Legacy fallback (references built before Phase 3 never carry
    // segment_idx): scan for the segment whose text contains the
    // spoken phrase. Last-resort fallback is segment 0.
    for (idx, segment) in segments.iter().enumerate() {
        if segment.text.contains(&reference.spoken_text) {
            return Some(idx);
        }
    }
    Some(0)
}

fn collect_segment_indices(
    ref_indices: &[usize],
    references: &[Reference],
    ref_to_segment: &[Option<usize>],
) -> Vec<usize> {
    let mut seen: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
    for &ri in ref_indices {
        if ri >= references.len() {
            continue;
        }
        if let Some(Some(si)) = ref_to_segment.get(ri) {
            seen.insert(*si);
        }
    }
    seen.into_iter().collect()
}

fn compose_text(
    event: &ActionEvent,
    ref_indices: &[usize],
    references: &[Reference],
    segments: &[SpeakSegment],
    importance: f32,
) -> String {
    let offset = format_offset(event.session_offset_ms);
    let payload_line = narrate_for_retrieval(&event.payload, &event.action_type);
    let user_note_line = event
        .curation
        .user_note
        .as_ref()
        .map(|note| format!("User pinned: \"{}\".", note));

    let mut voice_lines: Vec<String> = Vec::new();
    for &ri in ref_indices {
        let Some(reference) = references.get(ri) else {
            continue;
        };
        let relation_hint = relation_phrase(reference.relation);
        let seg_ctx = segment_for_reference(reference, segments)
            .and_then(|idx| segments.get(idx))
            .map(|s| format!(" (at {})", format_offset(s.start_ms)))
            .unwrap_or_default();
        voice_lines.push(format!(
            "narrator said \"{}\"{}{}",
            reference.spoken_text, seg_ctx, relation_hint
        ));
    }

    let mut out = String::new();
    out.push_str(&format!("At {}: {}.", offset, payload_line));
    if let Some(note) = user_note_line {
        out.push(' ');
        out.push_str(&note);
    }
    if !voice_lines.is_empty() {
        out.push_str(" Voice context: ");
        out.push_str(&voice_lines.join("; "));
        out.push('.');
    }
    out.push_str(&format!(" Importance: {:.2}.", importance));
    out
}

fn relation_phrase(relation: RefRelation) -> &'static str {
    match relation {
        RefRelation::Single | RefRelation::Unknown => "",
        RefRelation::Composition => " [composition]",
        RefRelation::Contrast => " [contrast reference]",
        RefRelation::Subtraction => " [excluded by user]",
    }
}

fn build_tags(event: &ActionEvent) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    tags.push(format!("type:{}", event.action_type.as_str()));
    if event.curation.is_user_sourced() {
        tags.push("user_sourced".to_string());
    }
    if let Some(app) = app_name(&event.payload) {
        tags.push(format!("app:{}", app));
    }
    if let Some(host) = url_host(&event.payload) {
        tags.push(format!("domain:{}", host));
    }
    tags
}

fn app_name(payload: &ActionPayload) -> Option<&str> {
    match payload {
        ActionPayload::SelectionText { app_name, .. } => Some(app_name.as_str()),
        ActionPayload::PageCurrent { app_name, .. } => Some(app_name.as_str()),
        ActionPayload::WindowFocus { app_name, .. } => Some(app_name.as_str()),
        ActionPayload::ClickMouse {
            app_name: Some(app),
            ..
        } => Some(app.as_str()),
        ActionPayload::ClipboardChange {
            source_app: Some(app),
            ..
        } => Some(app.as_str()),
        _ => None,
    }
}

fn url_host(payload: &ActionPayload) -> Option<String> {
    let raw = match payload {
        ActionPayload::ClickLink { to_url, .. } => to_url.as_str(),
        ActionPayload::PageCurrent {
            url: Some(url), ..
        } => url.as_str(),
        _ => return None,
    };
    let (_, rest) = raw.split_once("://")?;
    let rest = rest.split(['?', '#']).next().unwrap_or(rest);
    let host = match rest.find('/') {
        Some(i) => &rest[..i],
        None => rest,
    };
    if host.is_empty() {
        None
    } else {
        Some(host.to_lowercase())
    }
}

/// Short MM:SS offset rendering, capped at 99:59 for readability.
fn format_offset(offset_ms: u64) -> String {
    let seconds = offset_ms / 1000;
    let capped = seconds.min(SESSION_OFFSET_FORMAT_MINUTES_CAP + 59);
    let minutes = capped / 60;
    let secs = capped % 60;
    format!("{:02}:{:02}", minutes, secs)
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

    use crate::importance::ImportanceScorer;

    fn make_segment(text: &str, start: u64, end: u64) -> SpeakSegment {
        SpeakSegment {
            text: text.to_string(),
            start_ms: start,
            end_ms: end,
            confidence: 0.95,
            is_final: true,
        }
    }

    fn make_clipboard(offset: u64, text: &str, source: Option<&str>) -> ActionEvent {
        ActionEvent {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            timestamp: 1_712_900_000_000,
            session_offset_ms: offset,
            observed_offset_ms: Some(offset),
            duration_ms: None,
            action_type: ActionType::ClipboardChange,
            plugin_id: "builtin".to_string(),
            payload: ActionPayload::ClipboardChange {
                content_type: ClipboardContentType::Text,
                text: Some(text.to_string()),
                file_path: None,
                source_app: source.map(|s| s.to_string()),
            },
            semantic_hint: None,
            confidence: 1.0,
            curation: Default::default(),
        }
    }

    fn make_link(offset: u64, to_url: &str) -> ActionEvent {
        ActionEvent {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            timestamp: 1_712_900_000_000,
            session_offset_ms: offset,
            observed_offset_ms: Some(offset),
            duration_ms: None,
            action_type: ActionType::ClickLink,
            plugin_id: "builtin".to_string(),
            payload: ActionPayload::ClickLink {
                from_url: None,
                to_url: to_url.to_string(),
                title: None,
            },
            semantic_hint: None,
            confidence: 1.0,
            curation: Default::default(),
        }
    }

    #[test]
    fn empty_events_produce_no_chunks() {
        let chunks = render_chunks(&[], &[], &[], &[]);
        assert!(chunks.is_empty());
    }

    #[test]
    fn repeated_deixis_is_attributed_by_segment_idx_not_text_search() {
        // Two segments both contain "这个链接" — a legacy text-search
        // heuristic would wrongly attribute both references to seg 0.
        // The v2 pipeline sets `reference.segment_idx`, which must win.
        let a = make_link(5_000, "https://x.io/a");
        let b = make_link(80_000, "https://x.io/b");
        let events = vec![a.clone(), b.clone()];
        let segments = vec![
            make_segment("打开这个链接看一下", 6_000, 8_000),
            make_segment("回头这个链接要再查", 82_000, 84_000),
        ];
        let ref_seg0 = Reference {
            spoken_text: "这个链接".to_string(),
            spoken_offset: 0,
            resolved_event_idx: 0,
            resolved_event_id: Some(a.id),
            confidence: 0.9,
            strategy: ReferenceStrategy::LlmCoreference,
            user_confirmed: false,
            targets: vec![RefTarget {
                event_id: a.id,
                event_idx: 0,
                role: TargetRole::Source,
                via_anchor: None,
            }],
            relation: RefRelation::Single,
            segment_idx: Some(0),
        };
        let ref_seg1 = Reference {
            spoken_text: "这个链接".to_string(),
            spoken_offset: 0,
            resolved_event_idx: 1,
            resolved_event_id: Some(b.id),
            confidence: 0.9,
            strategy: ReferenceStrategy::LlmCoreference,
            user_confirmed: false,
            targets: vec![RefTarget {
                event_id: b.id,
                event_idx: 1,
                role: TargetRole::Source,
                via_anchor: None,
            }],
            relation: RefRelation::Single,
            segment_idx: Some(1),
        };
        let refs = vec![ref_seg0, ref_seg1];
        let scores = ImportanceScorer::new().score_all(&events, &refs);
        let chunks = render_chunks(&events, &segments, &refs, &scores);

        let chunk_a = chunks.iter().find(|c| c.event_id == a.id).unwrap();
        let chunk_b = chunks.iter().find(|c| c.event_id == b.id).unwrap();
        assert_eq!(chunk_a.referenced_by_segments, vec![0]);
        assert_eq!(chunk_b.referenced_by_segments, vec![1]);
        // Compose_text must also pull the seg timestamp matching the
        // reference's segment_idx, not the first text match.
        assert!(chunk_b.text.contains("at 01:22"));
    }

    #[test]
    fn noise_events_still_emit_retrieval_chunks() {
        // Contract: retrieval surface is exhaustive over live events.
        // Even `ClickMouse` / `WindowFocus` noise that the prompt
        // surface would drop must get a chunk so the knowledge base
        // stays complete.
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
            curation: TraceCuration::default(),
        };
        let focus = ActionEvent {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            timestamp: 1_712_900_001_000,
            session_offset_ms: 6_000,
            observed_offset_ms: Some(6_000),
            duration_ms: None,
            action_type: ActionType::WindowFocus,
            plugin_id: "builtin".to_string(),
            payload: ActionPayload::WindowFocus {
                app_name: "Chrome".to_string(),
                window_title: "Docs".to_string(),
            },
            semantic_hint: None,
            confidence: 1.0,
            curation: TraceCuration::default(),
        };
        let events = vec![click.clone(), focus.clone()];
        let scores = ImportanceScorer::new().score_all(&events, &[]);
        let chunks = render_chunks(&events, &[], &[], &scores);
        assert_eq!(chunks.len(), 2);
        let types: std::collections::HashSet<_> =
            chunks.iter().map(|c| c.action_type.clone()).collect();
        assert!(types.contains("click.mouse"));
        assert!(types.contains("window.focus"));
    }

    #[test]
    fn one_chunk_per_live_event() {
        let clipboard = make_clipboard(5_000, "hello", Some("Slack"));
        let mut deleted = make_clipboard(6_000, "deleted", None);
        deleted.curation.deleted = true;
        let link = make_link(7_000, "https://example.com/a");
        let events = vec![clipboard.clone(), deleted, link.clone()];

        let scores = ImportanceScorer::new().score_all(&events, &[]);
        let chunks = render_chunks(&events, &[], &[], &scores);
        assert_eq!(chunks.len(), 2);
        let ids: Vec<_> = chunks.iter().map(|c| c.event_id).collect();
        assert!(ids.contains(&clipboard.id));
        assert!(ids.contains(&link.id));
    }

    #[test]
    fn chunk_text_includes_payload_and_offset() {
        let clipboard = make_clipboard(65_000, "panic: oops", Some("Slack"));
        let events = vec![clipboard.clone()];
        let scores = ImportanceScorer::new().score_all(&events, &[]);
        let chunks = render_chunks(&events, &[], &[], &scores);
        let text = &chunks[0].text;
        assert!(text.starts_with("At 01:05:"));
        assert!(text.contains("copied from Slack"));
        assert!(text.contains("panic: oops"));
        assert!(text.contains("Importance:"));
    }

    #[test]
    fn chunk_text_includes_voice_context_when_referenced() {
        let clipboard = make_clipboard(28_000, "stack trace", Some("Slack"));
        let events = vec![clipboard.clone()];
        let segments = vec![make_segment("看这个堆栈真的很长", 35_000, 40_000)];
        let reference = Reference {
            spoken_text: "这个堆栈".to_string(),
            spoken_offset: 0,
            resolved_event_idx: 0,
            resolved_event_id: Some(clipboard.id),
            confidence: 0.9,
            strategy: ReferenceStrategy::LlmCoreference,
            user_confirmed: false,
            targets: vec![RefTarget {
                event_id: clipboard.id,
                event_idx: 0,
                role: TargetRole::Source,
                via_anchor: None,
            }],
            relation: RefRelation::Single,
            segment_idx: Some(0),
        };
        let refs = vec![reference];
        let scores = ImportanceScorer::new().score_all(&events, &refs);
        let chunks = render_chunks(&events, &segments, &refs, &scores);
        assert!(chunks[0].text.contains("narrator said \"这个堆栈\""));
        assert!(chunks[0].text.contains("at 00:35"));
        assert_eq!(chunks[0].referenced_by_segments, vec![0]);
    }

    #[test]
    fn tags_include_type_and_optional_facets() {
        let mut clipboard = make_clipboard(5_000, "x", Some("Slack"));
        clipboard.curation = TraceCuration::toolbar();
        let link = make_link(6_000, "https://github.com/org/repo/pull/1");
        let events = vec![clipboard.clone(), link.clone()];
        let scores = ImportanceScorer::new().score_all(&events, &[]);
        let chunks = render_chunks(&events, &[], &[], &scores);

        let cb_tags = &chunks[0].tags;
        assert!(cb_tags.contains(&"type:clipboard.change".to_string()));
        assert!(cb_tags.contains(&"user_sourced".to_string()));
        assert!(cb_tags.contains(&"app:Slack".to_string()));

        let link_tags = &chunks[1].tags;
        assert!(link_tags.contains(&"type:click.link".to_string()));
        assert!(link_tags.contains(&"domain:github.com".to_string()));
    }

    #[test]
    fn user_note_is_surfaced_in_chunk_text() {
        let mut note_event = make_clipboard(32_000, "", None);
        note_event.curation = TraceCuration {
            source: talkiwi_core::event::TraceSource::Toolbar,
            role: None,
            user_note: Some("堆栈在这".to_string()),
            deleted: false,
        };
        let events = vec![note_event];
        let scores = ImportanceScorer::new().score_all(&events, &[]);
        let chunks = render_chunks(&events, &[], &[], &scores);
        assert!(chunks[0].text.contains("User pinned: \"堆栈在这\""));
    }

    #[test]
    fn multi_target_reference_contributes_to_each_chunk() {
        let a = make_clipboard(5_000, "A", None);
        let b = make_link(7_000, "https://x.io/y");
        let events = vec![a.clone(), b.clone()];
        let segments = vec![make_segment("同时说到 A 和 B", 10_000, 11_000)];
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
            segment_idx: Some(0),
        };
        let refs = vec![reference];
        let scores = ImportanceScorer::new().score_all(&events, &refs);
        let chunks = render_chunks(&events, &segments, &refs, &scores);
        for chunk in &chunks {
            assert!(chunk.text.contains("composition"));
            assert_eq!(chunk.referenced_by_segments, vec![0]);
        }
    }

    #[test]
    fn format_offset_handles_hour_plus_gracefully() {
        assert_eq!(format_offset(0), "00:00");
        assert_eq!(format_offset(65_000), "01:05");
        assert_eq!(format_offset(3_600_000), "60:00");
        // Beyond 99:59 is capped (retrieval readability, not a hard invariant).
        let long = format_offset(10_000_000);
        assert!(long.contains(':'));
    }
}
