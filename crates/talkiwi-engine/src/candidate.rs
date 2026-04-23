//! Candidate-set builder for the trace annotation engine.
//!
//! Per [docs/design/2026-04-18-trace-annotation-engine-tech-plan.md](../../docs/design/2026-04-18-trace-annotation-engine-tech-plan.md) §6,
//! each `SpeakSegment` is mapped to a trimmed set of `ActionEvent`s the
//! LLM will see while resolving coreferences. The set is the union of:
//!
//! - `T_i` — events inside the temporal window `[start-60s, end+5s]`,
//!   excluding *always-noise* types (`ClickMouse`, `WindowFocus`).
//! - `U`   — events with `curation.is_user_sourced() == true` for the
//!   whole session (toolbar/manual anchors cross time windows).
//! - `L`   — `ClickLink` / `PageCurrent` events, deduplicated by
//!   scheme+host+path so revisits to the same URL collapse to the
//!   earliest visit.
//!
//! The resulting list is capped at `MAX_PER_SEGMENT` items, sorted by
//! priority `(user_sourced DESC, type_prior DESC, |Δt| ASC)`, then
//! re-sorted back into chronological order before being emitted.

use std::collections::HashSet;

use talkiwi_core::event::{ActionEvent, ActionPayload, ActionType};
use talkiwi_core::session::SpeakSegment;
use uuid::Uuid;

use crate::payload_render::trim_for_prompt;

pub const DEFAULT_TIME_WINDOW_PRE_MS: u64 = 60_000;
pub const DEFAULT_TIME_WINDOW_POST_MS: u64 = 5_000;
pub const DEFAULT_MAX_PER_SEGMENT: usize = 60;

/// A single event selected into the candidate set for one segment.
///
/// `event_idx` is the index into the full `events: &[ActionEvent]` slice
/// held by the engine; `payload_preview` is a small textual rendering
/// the LLM can consume without the engine re-sending the entire payload.
#[derive(Debug, Clone, PartialEq)]
pub struct CandidateEvent {
    pub event_idx: usize,
    pub event_id: Uuid,
    pub session_offset_ms: u64,
    pub action_type: ActionType,
    pub user_sourced: bool,
    pub payload_preview: String,
}

/// All candidates chosen for a single `SpeakSegment`.
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentCandidates {
    pub segment_idx: usize,
    pub candidates: Vec<CandidateEvent>,
}

/// Tunables for `CandidateBuilder::build`.
#[derive(Debug, Clone)]
pub struct CandidateBuilderConfig {
    pub time_window_pre_ms: u64,
    pub time_window_post_ms: u64,
    pub max_per_segment: usize,
}

impl Default for CandidateBuilderConfig {
    fn default() -> Self {
        Self {
            time_window_pre_ms: DEFAULT_TIME_WINDOW_PRE_MS,
            time_window_post_ms: DEFAULT_TIME_WINDOW_POST_MS,
            max_per_segment: DEFAULT_MAX_PER_SEGMENT,
        }
    }
}

pub struct CandidateBuilder {
    config: CandidateBuilderConfig,
}

impl CandidateBuilder {
    pub fn new() -> Self {
        Self::with_config(CandidateBuilderConfig::default())
    }

    pub fn with_config(config: CandidateBuilderConfig) -> Self {
        Self { config }
    }

    /// Build a per-segment candidate set for the given session. Runs in
    /// `O(events * segments)` which is fine for the target workload
    /// (dozens of segments, a few hundred events per session).
    pub fn build(
        &self,
        segments: &[SpeakSegment],
        events: &[ActionEvent],
    ) -> Vec<SegmentCandidates> {
        if segments.is_empty() || events.is_empty() {
            return segments
                .iter()
                .enumerate()
                .map(|(i, _)| SegmentCandidates {
                    segment_idx: i,
                    candidates: Vec::new(),
                })
                .collect();
        }

        let user_sourced = collect_user_sourced(events);
        let url_dedup = collect_url_dedup(events);

        segments
            .iter()
            .enumerate()
            .map(|(si, segment)| {
                let window_lo = segment
                    .start_ms
                    .saturating_sub(self.config.time_window_pre_ms);
                let window_hi = segment
                    .end_ms
                    .saturating_add(self.config.time_window_post_ms);

                let mut union_set: HashSet<usize> = HashSet::new();

                for (i, event) in events.iter().enumerate() {
                    if event.curation.deleted {
                        continue;
                    }
                    if is_always_noise(&event.action_type) {
                        continue;
                    }
                    if in_window(event.session_offset_ms, window_lo, window_hi) {
                        union_set.insert(i);
                    }
                }
                for &i in &user_sourced {
                    union_set.insert(i);
                }
                for &i in &url_dedup {
                    union_set.insert(i);
                }

                let segment_mid =
                    (segment.start_ms.saturating_add(segment.end_ms)) / 2;
                let mut ranked: Vec<usize> = union_set.into_iter().collect();
                ranked.sort_by(|&a, &b| compare_priority(a, b, events, segment_mid));
                ranked.truncate(self.config.max_per_segment);
                ranked.sort_by_key(|&i| events[i].session_offset_ms);

                let candidates = ranked
                    .into_iter()
                    .map(|i| materialize(events, i))
                    .collect();

                SegmentCandidates {
                    segment_idx: si,
                    candidates,
                }
            })
            .collect()
    }
}

impl Default for CandidateBuilder {
    fn default() -> Self {
        Self::new()
    }
}

fn collect_user_sourced(events: &[ActionEvent]) -> Vec<usize> {
    events
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.curation.deleted && e.curation.is_user_sourced())
        .map(|(i, _)| i)
        .collect()
}

fn collect_url_dedup(events: &[ActionEvent]) -> Vec<usize> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for (i, event) in events.iter().enumerate() {
        if event.curation.deleted {
            continue;
        }
        let Some(url) = extract_url(&event.payload) else {
            continue;
        };
        let key = match normalize_url(url) {
            Some(k) => k,
            // Unparseable URL — fall back to the raw string so we still
            // dedup stable duplicates, just without normalization.
            None => url.to_string(),
        };
        if seen.insert(key) {
            out.push(i);
        }
    }
    out
}

fn compare_priority(
    a: usize,
    b: usize,
    events: &[ActionEvent],
    segment_mid: u64,
) -> std::cmp::Ordering {
    let ea = &events[a];
    let eb = &events[b];

    let ua = ea.curation.is_user_sourced();
    let ub = eb.curation.is_user_sourced();
    if ua != ub {
        return ub.cmp(&ua);
    }

    let pa = type_prior(&ea.action_type);
    let pb = type_prior(&eb.action_type);
    if (pa - pb).abs() > f32::EPSILON {
        return pb
            .partial_cmp(&pa)
            .unwrap_or(std::cmp::Ordering::Equal);
    }

    let da = (ea.session_offset_ms as i128 - segment_mid as i128).abs();
    let db = (eb.session_offset_ms as i128 - segment_mid as i128).abs();
    da.cmp(&db)
}

fn materialize(events: &[ActionEvent], i: usize) -> CandidateEvent {
    let e = &events[i];
    CandidateEvent {
        event_idx: i,
        event_id: e.id,
        session_offset_ms: e.session_offset_ms,
        action_type: e.action_type.clone(),
        user_sourced: e.curation.is_user_sourced(),
        payload_preview: trim_for_prompt(&e.payload),
    }
}

/// Events that are never surfaced to the LLM *on their own* — the
/// coordinates of a raw click or a bare window focus are pure noise.
/// They can still reach the candidate set via `U` (user-sourced) if the
/// user explicitly pinned them via Trace Toolbar.
fn is_always_noise(t: &ActionType) -> bool {
    matches!(t, ActionType::ClickMouse | ActionType::WindowFocus)
}

fn in_window(offset: u64, lo: u64, hi: u64) -> bool {
    offset >= lo && offset <= hi
}

/// See `docs/design/2026-04-18-trace-annotation-engine-tech-plan.md` §9.2
/// for the priors. Kept in lock-step with `importance.rs` once that
/// module lands.
pub fn type_prior(t: &ActionType) -> f32 {
    match t {
        ActionType::SelectionText => 0.90,
        ActionType::Screenshot => 0.85,
        ActionType::ClipboardChange => 0.85,
        ActionType::FileAttach => 0.85,
        ActionType::ClickLink => 0.75,
        ActionType::PageCurrent => 0.70,
        ActionType::WindowFocus => 0.15,
        ActionType::ClickMouse => 0.05,
        ActionType::Custom(s) if s == "manual.note" => 0.95,
        ActionType::Custom(_) => 0.60,
    }
}

fn extract_url(payload: &ActionPayload) -> Option<&str> {
    match payload {
        ActionPayload::ClickLink { to_url, .. } => Some(to_url.as_str()),
        ActionPayload::PageCurrent {
            url: Some(url), ..
        } => Some(url.as_str()),
        _ => None,
    }
}

/// Lightweight URL normalization — enough to dedup revisits of the same
/// resource. Strips query + fragment, lowercases scheme + host. Not a
/// full RFC 3986 parser; unparseable input returns `None` and the
/// caller should fall back to the raw string.
fn normalize_url(raw: &str) -> Option<String> {
    let (scheme, rest) = raw.split_once("://")?;
    let rest = rest.split(['?', '#']).next().unwrap_or(rest);
    let (host, path) = match rest.find('/') {
        Some(idx) => (&rest[..idx], &rest[idx..]),
        None => (rest, "/"),
    };
    if host.is_empty() {
        return None;
    }
    Some(format!(
        "{}://{}{}",
        scheme.to_lowercase(),
        host.to_lowercase(),
        path
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use talkiwi_core::event::{
        ActionPayload, ActionType, ClipboardContentType, TraceCuration,
    };

    fn make_segment(start_ms: u64, end_ms: u64) -> SpeakSegment {
        SpeakSegment {
            text: "seg".to_string(),
            start_ms,
            end_ms,
            confidence: 0.95,
            is_final: true,
        }
    }

    fn make_event(kind: ActionType, offset: u64) -> ActionEvent {
        let payload = match &kind {
            ActionType::SelectionText => ActionPayload::SelectionText {
                text: "snippet".to_string(),
                app_name: "VSCode".to_string(),
                window_title: "main.rs".to_string(),
                char_count: 7,
            },
            ActionType::Screenshot => ActionPayload::Screenshot {
                image_path: "/tmp/s.png".to_string(),
                width: 1920,
                height: 1080,
                ocr_text: Some("ocr text".to_string()),
            },
            ActionType::ClipboardChange => ActionPayload::ClipboardChange {
                content_type: ClipboardContentType::Text,
                text: Some("copied".to_string()),
                file_path: None,
                source_app: Some("Slack".to_string()),
            },
            ActionType::PageCurrent => ActionPayload::PageCurrent {
                url: Some("https://example.com/a".to_string()),
                title: "Example".to_string(),
                app_name: "Chrome".to_string(),
                bundle_id: "com.google.Chrome".to_string(),
            },
            ActionType::ClickLink => ActionPayload::ClickLink {
                from_url: None,
                to_url: "https://example.com/a".to_string(),
                title: Some("link".to_string()),
            },
            ActionType::WindowFocus => ActionPayload::WindowFocus {
                app_name: "App".to_string(),
                window_title: "Title".to_string(),
            },
            ActionType::ClickMouse => ActionPayload::ClickMouse {
                app_name: None,
                window_title: None,
                button: "left".to_string(),
                x: 10.0,
                y: 10.0,
            },
            ActionType::FileAttach => ActionPayload::FileAttach {
                file_path: "/tmp/f".to_string(),
                file_name: "f".to_string(),
                file_size: 1024,
                mime_type: "text/plain".to_string(),
                preview: None,
            },
            ActionType::Custom(_) => ActionPayload::Custom(serde_json::json!({})),
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
            payload,
            semantic_hint: None,
            confidence: 1.0,
            curation: Default::default(),
        }
    }

    fn with_toolbar(mut e: ActionEvent) -> ActionEvent {
        e.curation = TraceCuration::toolbar();
        e
    }

    #[test]
    fn empty_session_yields_no_candidates() {
        let builder = CandidateBuilder::new();
        let out = builder.build(&[], &[]);
        assert!(out.is_empty());
    }

    #[test]
    fn empty_events_yields_empty_candidate_lists_per_segment() {
        let builder = CandidateBuilder::new();
        let segments = vec![make_segment(0, 1000), make_segment(2000, 3000)];
        let out = builder.build(&segments, &[]);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|sc| sc.candidates.is_empty()));
    }

    #[test]
    fn noise_types_excluded_from_temporal_window() {
        let builder = CandidateBuilder::new();
        let segments = vec![make_segment(5_000, 6_000)];
        let events = vec![
            make_event(ActionType::ClickMouse, 5_500),
            make_event(ActionType::WindowFocus, 5_700),
            make_event(ActionType::SelectionText, 5_900),
        ];
        let out = builder.build(&segments, &events);
        assert_eq!(out[0].candidates.len(), 1);
        assert_eq!(out[0].candidates[0].action_type, ActionType::SelectionText);
    }

    #[test]
    fn noise_event_is_pulled_in_when_user_sourced() {
        let builder = CandidateBuilder::new();
        let segments = vec![make_segment(5_000, 6_000)];
        let events = vec![
            with_toolbar(make_event(ActionType::ClickMouse, 5_500)),
            make_event(ActionType::SelectionText, 5_900),
        ];
        let out = builder.build(&segments, &events);
        let types: Vec<_> = out[0]
            .candidates
            .iter()
            .map(|c| &c.action_type)
            .collect();
        assert!(types.contains(&&ActionType::ClickMouse));
        assert!(types.contains(&&ActionType::SelectionText));
    }

    #[test]
    fn user_sourced_reaches_across_time_window() {
        // Anchor at 10s, segment at 120s. Without U ∪ rule the toolbar
        // note at 10s would fall outside the temporal window and be lost.
        let builder = CandidateBuilder::new();
        let segments = vec![make_segment(120_000, 121_000)];
        let early_anchor = with_toolbar(make_event(ActionType::SelectionText, 10_000));
        let events = vec![early_anchor.clone()];
        let out = builder.build(&segments, &events);
        assert_eq!(out[0].candidates.len(), 1);
        assert_eq!(out[0].candidates[0].event_id, early_anchor.id);
        assert!(out[0].candidates[0].user_sourced);
    }

    #[test]
    fn url_dedup_keeps_earliest_of_duplicate_links() {
        let builder = CandidateBuilder::new();
        // Segment at 500s — well past the URL-dedup events at 1s and 2s.
        // Only the dedup set can pull them in.
        let segments = vec![make_segment(500_000, 501_000)];
        let first_visit = make_event(ActionType::ClickLink, 1_000);
        let second_visit = make_event(ActionType::ClickLink, 2_000);
        let events = vec![first_visit.clone(), second_visit];
        let out = builder.build(&segments, &events);
        assert_eq!(out[0].candidates.len(), 1);
        assert_eq!(out[0].candidates[0].event_id, first_visit.id);
    }

    #[test]
    fn url_dedup_normalizes_query_and_fragment() {
        let make_link = |to: &str, offset: u64| {
            let mut e = make_event(ActionType::ClickLink, offset);
            e.payload = ActionPayload::ClickLink {
                from_url: None,
                to_url: to.to_string(),
                title: None,
            };
            e
        };
        let builder = CandidateBuilder::new();
        let segments = vec![make_segment(500_000, 501_000)];
        let first = make_link("https://example.com/doc?foo=1", 1_000);
        let second = make_link("https://example.com/doc#section", 2_000);
        let third = make_link("HTTPS://Example.com/doc", 3_000);
        let events = vec![first.clone(), second, third];
        let out = builder.build(&segments, &events);
        assert_eq!(out[0].candidates.len(), 1);
        assert_eq!(out[0].candidates[0].event_id, first.id);
    }

    #[test]
    fn deleted_events_are_excluded() {
        let builder = CandidateBuilder::new();
        let segments = vec![make_segment(5_000, 6_000)];
        let mut deleted = make_event(ActionType::SelectionText, 5_500);
        deleted.curation.deleted = true;
        let live = make_event(ActionType::SelectionText, 5_800);
        let out = builder.build(&segments, &[deleted, live.clone()]);
        assert_eq!(out[0].candidates.len(), 1);
        assert_eq!(out[0].candidates[0].event_id, live.id);
    }

    #[test]
    fn cap_drops_lowest_priority_first() {
        let config = CandidateBuilderConfig {
            time_window_pre_ms: 60_000,
            time_window_post_ms: 5_000,
            max_per_segment: 2,
        };
        let builder = CandidateBuilder::with_config(config);
        let segments = vec![make_segment(10_000, 11_000)];
        // Three events — SelectionText (0.90), PageCurrent (0.70),
        // FileAttach (0.85). Max 2 → must keep SelectionText +
        // FileAttach and drop PageCurrent.
        let selection = make_event(ActionType::SelectionText, 10_200);
        let page = make_event(ActionType::PageCurrent, 10_400);
        let file = make_event(ActionType::FileAttach, 10_600);
        let events = vec![selection.clone(), page, file.clone()];
        let out = builder.build(&segments, &events);
        assert_eq!(out[0].candidates.len(), 2);
        let ids: Vec<_> = out[0].candidates.iter().map(|c| c.event_id).collect();
        assert!(ids.contains(&selection.id));
        assert!(ids.contains(&file.id));
    }

    #[test]
    fn results_are_chronologically_sorted() {
        let builder = CandidateBuilder::new();
        let segments = vec![make_segment(10_000, 11_000)];
        let late = make_event(ActionType::SelectionText, 10_800);
        let early = make_event(ActionType::SelectionText, 10_200);
        let mid = make_event(ActionType::SelectionText, 10_500);
        let out = builder.build(&segments, &[late, mid, early]);
        let offsets: Vec<_> = out[0]
            .candidates
            .iter()
            .map(|c| c.session_offset_ms)
            .collect();
        assert_eq!(offsets, vec![10_200, 10_500, 10_800]);
    }

    #[test]
    fn candidate_payload_preview_is_populated() {
        // Trust payload_render's own tests for truncation correctness —
        // here we just verify the CandidateBuilder path hands the
        // preview string through.
        let builder = CandidateBuilder::new();
        let segments = vec![make_segment(5_000, 6_000)];
        let mut long_event = make_event(ActionType::SelectionText, 5_500);
        long_event.payload = ActionPayload::SelectionText {
            text: "a".repeat(1_000),
            app_name: "VSCode".to_string(),
            window_title: "x".to_string(),
            char_count: 1_000,
        };
        let out = builder.build(&segments, &[long_event]);
        let preview = &out[0].candidates[0].payload_preview;
        assert!(preview.starts_with("selected in VSCode:"));
        assert!(preview.ends_with('…'));
    }

    #[test]
    fn multiple_segments_each_get_own_candidate_set() {
        let builder = CandidateBuilder::new();
        let segments = vec![make_segment(1_000, 2_000), make_segment(100_000, 101_000)];
        let e1 = make_event(ActionType::SelectionText, 1_500);
        let e2 = make_event(ActionType::SelectionText, 100_500);
        let out = builder.build(&segments, &[e1.clone(), e2.clone()]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].candidates.len(), 1);
        assert_eq!(out[0].candidates[0].event_id, e1.id);
        assert_eq!(out[1].candidates.len(), 1);
        assert_eq!(out[1].candidates[0].event_id, e2.id);
    }
}
