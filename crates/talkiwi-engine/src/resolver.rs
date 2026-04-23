use regex::Regex;
use talkiwi_core::event::{ActionEvent, ActionType};
use talkiwi_core::output::{Reference, ReferenceStrategy};
use talkiwi_core::session::SpeakSegment;

/// A regex-matched deixis surface in a single `SpeakSegment`. Unlike
/// `Reference`, hints carry *no* resolved event — they are a prior the
/// LLM sees alongside the candidate set, indicating "this spoken
/// phrase smells like one of these event types".
///
/// Produced by [`Resolver::boost_hints`] once per non-overlapping regex
/// match. `expected_types` may be empty for bare "这个/那个" surfaces,
/// which signal "there is a deixis here" without type preference.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolverHint {
    pub segment_idx: usize,
    pub spoken_text: String,
    pub spoken_offset_in_segment: usize,
    pub expected_types: Vec<ActionType>,
}

const CONFIDENCE_HIGH: f32 = 0.9;
const CONFIDENCE_MEDIUM: f32 = 0.7;
const CONFIDENCE_LOW: f32 = 0.5;
const CONFIDENCE_MINIMAL: f32 = 0.3;

const DISTANCE_CLOSE_MS: u64 = 3_000;
const DISTANCE_MEDIUM_MS: u64 = 10_000;
const DISTANCE_FAR_MS: u64 = 30_000;
const POST_SPEECH_PENALTY_FACTOR: u64 = 3;
const POST_SPEECH_PENALTY_MS: u64 = 3_000;

struct PatternEntry {
    regex: Regex,
    expected_types: Vec<ActionType>,
}

pub struct Resolver {
    patterns: Vec<PatternEntry>,
}

impl Resolver {
    pub fn new() -> Self {
        let patterns = vec![
            (
                r"这段代码|这些代码|选中的代码",
                vec![ActionType::SelectionText],
            ),
            (
                r"这个报错|这个错误",
                vec![ActionType::Screenshot, ActionType::SelectionText],
            ),
            (
                r"这个页面|这个网页|当前页面",
                vec![
                    ActionType::PageCurrent,
                    ActionType::ClickLink,
                    ActionType::WindowFocus,
                ],
            ),
            (r"这张截图|截图|这个截屏", vec![ActionType::Screenshot]),
            (r"复制的|剪贴板", vec![ActionType::ClipboardChange]),
            (r"这个文件|附件", vec![ActionType::FileAttach]),
            (r"这个窗口|当前窗口", vec![ActionType::WindowFocus]),
            (r"刚才点的|刚才点击的", vec![ActionType::ClickMouse]),
            (r"这个|那个", vec![]),
        ];

        let patterns = patterns
            .into_iter()
            .map(|(pattern, expected_types)| PatternEntry {
                regex: Regex::new(pattern).expect("invalid resolver regex"),
                expected_types,
            })
            .collect();

        Self { patterns }
    }

    /// Emit per-segment deixis hints. Unlike [`Self::resolve`] this does
    /// not require `events` — it is a pure regex pass over each
    /// segment's text. Consumed by the trace annotation engine to feed
    /// the LLM's candidate set as a type prior. See
    /// `docs/design/2026-04-18-trace-annotation-engine-tech-plan.md` §7.2.
    pub fn boost_hints(&self, segments: &[SpeakSegment]) -> Vec<ResolverHint> {
        let mut hints = Vec::new();
        for (segment_idx, segment) in segments.iter().enumerate() {
            let mut matched_ranges: Vec<(usize, usize)> = Vec::new();
            for entry in &self.patterns {
                for mat in entry.regex.find_iter(&segment.text) {
                    let overlaps = matched_ranges
                        .iter()
                        .any(|(start, end)| mat.start() < *end && mat.end() > *start);
                    if overlaps {
                        continue;
                    }
                    matched_ranges.push((mat.start(), mat.end()));
                    hints.push(ResolverHint {
                        segment_idx,
                        spoken_text: mat.as_str().to_string(),
                        spoken_offset_in_segment: mat.start(),
                        expected_types: entry.expected_types.clone(),
                    });
                }
            }
        }
        hints
    }

    pub fn resolve(&self, segments: &[SpeakSegment], events: &[ActionEvent]) -> Vec<Reference> {
        if segments.is_empty() || events.is_empty() {
            return Vec::new();
        }

        let mut matches: Vec<(String, usize, u64, &[ActionType])> = Vec::new();
        let mut concat_offset = 0usize;

        for segment in segments {
            let mut matched_ranges: Vec<(usize, usize)> = Vec::new();
            for entry in &self.patterns {
                for mat in entry.regex.find_iter(&segment.text) {
                    let overlaps = matched_ranges
                        .iter()
                        .any(|(start, end)| mat.start() < *end && mat.end() > *start);
                    if overlaps {
                        continue;
                    }
                    matched_ranges.push((mat.start(), mat.end()));
                    matches.push((
                        mat.as_str().to_string(),
                        concat_offset + mat.start(),
                        segment.start_ms,
                        &entry.expected_types,
                    ));
                }
            }
            concat_offset += segment.text.len() + 1;
        }

        let mut used_event_indices = vec![false; events.len()];
        let mut references = Vec::new();
        for (spoken_text, spoken_offset, speech_time_ms, expected_types) in matches {
            if let Some((event_idx, confidence)) =
                find_best_event(events, speech_time_ms, expected_types, &used_event_indices)
            {
                used_event_indices[event_idx] = true;
                references.push(Reference::new_single(
                    spoken_text,
                    spoken_offset,
                    event_idx,
                    events[event_idx].id,
                    confidence,
                    ReferenceStrategy::TemporalProximity,
                ));
            }
        }

        references
    }
}

impl Default for Resolver {
    fn default() -> Self {
        Self::new()
    }
}

fn find_best_event(
    events: &[ActionEvent],
    speech_time_ms: u64,
    expected_types: &[ActionType],
    used: &[bool],
) -> Option<(usize, f32)> {
    let mut best: Option<(usize, f32, u64)> = None;

    for (i, event) in events.iter().enumerate() {
        if used[i] {
            continue;
        }
        if !expected_types.is_empty() && !expected_types.contains(&event.action_type) {
            continue;
        }

        let event_time = event.session_offset_ms;
        let effective_distance = if event_time <= speech_time_ms {
            speech_time_ms - event_time
        } else {
            (event_time - speech_time_ms)
                .saturating_mul(POST_SPEECH_PENALTY_FACTOR)
                .saturating_add(POST_SPEECH_PENALTY_MS)
        };
        let confidence = distance_to_confidence(effective_distance);
        match best {
            Some((_, _, best_distance)) if effective_distance >= best_distance => {}
            _ => best = Some((i, confidence, effective_distance)),
        }
    }

    best.map(|(idx, confidence, _)| (idx, confidence))
}

fn distance_to_confidence(distance_ms: u64) -> f32 {
    if distance_ms < DISTANCE_CLOSE_MS {
        CONFIDENCE_HIGH
    } else if distance_ms < DISTANCE_MEDIUM_MS {
        CONFIDENCE_MEDIUM
    } else if distance_ms < DISTANCE_FAR_MS {
        CONFIDENCE_LOW
    } else {
        CONFIDENCE_MINIMAL
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use talkiwi_core::event::{ActionPayload, ClipboardContentType};
    use uuid::Uuid;

    fn make_segment(text: &str, start_ms: u64, end_ms: u64) -> SpeakSegment {
        SpeakSegment {
            text: text.to_string(),
            start_ms,
            end_ms,
            confidence: 0.95,
            is_final: true,
        }
    }

    fn make_event(action_type: ActionType, session_offset_ms: u64) -> ActionEvent {
        let payload = match &action_type {
            ActionType::SelectionText => ActionPayload::SelectionText {
                text: "fn main() {}".to_string(),
                app_name: "VSCode".to_string(),
                window_title: "main.rs".to_string(),
                char_count: 12,
            },
            ActionType::Screenshot => ActionPayload::Screenshot {
                image_path: "/tmp/shot.png".to_string(),
                width: 1920,
                height: 1080,
                ocr_text: None,
            },
            ActionType::ClipboardChange => ActionPayload::ClipboardChange {
                content_type: ClipboardContentType::Text,
                text: Some("copied text".to_string()),
                file_path: None,
                source_app: None,
            },
            ActionType::PageCurrent => ActionPayload::PageCurrent {
                url: Some("https://example.com".to_string()),
                title: "Example".to_string(),
                app_name: "Chrome".to_string(),
                bundle_id: "com.google.Chrome".to_string(),
            },
            ActionType::WindowFocus => ActionPayload::WindowFocus {
                app_name: "Chrome".to_string(),
                window_title: "Docs".to_string(),
            },
            ActionType::ClickMouse => ActionPayload::ClickMouse {
                app_name: Some("Chrome".to_string()),
                window_title: Some("Docs".to_string()),
                button: "left".to_string(),
                x: 100.0,
                y: 220.0,
            },
            ActionType::FileAttach => ActionPayload::FileAttach {
                file_path: "/tmp/test.rs".to_string(),
                file_name: "test.rs".to_string(),
                file_size: 1024,
                mime_type: "text/x-rust".to_string(),
                preview: None,
            },
            ActionType::ClickLink => ActionPayload::ClickLink {
                from_url: None,
                to_url: "https://example.com/next".to_string(),
                title: Some("Next".to_string()),
            },
            ActionType::Custom(_) => ActionPayload::Custom(serde_json::json!({})),
        };

        ActionEvent {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            timestamp: 1712900000000,
            session_offset_ms,
            observed_offset_ms: Some(session_offset_ms),
            duration_ms: None,
            action_type,
            plugin_id: "builtin".to_string(),
            payload,
            semantic_hint: None,
            confidence: 1.0,
            curation: Default::default(),
        }
    }

    #[test]
    fn resolve_zhege_daima_matches_selection_text() {
        let resolver = Resolver::new();
        let segments = vec![make_segment("帮我重写这段代码", 5000, 7000)];
        let events = vec![make_event(ActionType::SelectionText, 3000)];
        let refs = resolver.resolve(&segments, &events);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].resolved_event_idx, 0);
        assert_eq!(refs[0].confidence, 0.9);
    }

    #[test]
    fn resolve_prefers_before_over_after() {
        let resolver = Resolver::new();
        let segments = vec![make_segment("这个页面", 5000, 7000)];
        let events = vec![
            make_event(ActionType::PageCurrent, 4000),
            make_event(ActionType::WindowFocus, 5100),
        ];
        let refs = resolver.resolve(&segments, &events);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].resolved_event_idx, 0);
    }

    #[test]
    fn boost_hints_finds_typed_deixis() {
        let resolver = Resolver::new();
        let segments = vec![make_segment("帮我重写这段代码，看这张截图", 0, 1000)];
        let hints = resolver.boost_hints(&segments);
        assert_eq!(hints.len(), 2);
        let surfaces: Vec<_> = hints.iter().map(|h| h.spoken_text.as_str()).collect();
        assert!(surfaces.contains(&"这段代码"));
        assert!(surfaces.contains(&"这张截图"));
        for hint in &hints {
            assert_eq!(hint.segment_idx, 0);
        }
    }

    #[test]
    fn boost_hints_reports_expected_types_per_pattern() {
        let resolver = Resolver::new();
        let segments = vec![make_segment("这段代码", 0, 1000)];
        let hints = resolver.boost_hints(&segments);
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].expected_types, vec![ActionType::SelectionText]);
        assert_eq!(hints[0].spoken_offset_in_segment, 0);
    }

    #[test]
    fn boost_hints_does_not_overlap_matches() {
        // The `这个|那个` fallback regex must not also match inside
        // "这个报错" since the longer pattern covers that range.
        let resolver = Resolver::new();
        let segments = vec![make_segment("看这个报错", 0, 1000)];
        let hints = resolver.boost_hints(&segments);
        assert_eq!(hints.len(), 1);
        assert_eq!(hints[0].spoken_text, "这个报错");
    }

    #[test]
    fn boost_hints_empty_segments_returns_empty() {
        let resolver = Resolver::new();
        assert!(resolver.boost_hints(&[]).is_empty());
    }

    #[test]
    fn boost_hints_works_without_events() {
        // Unlike resolve(), boost_hints does not consult events at all —
        // it is a pure regex pass over segment text.
        let resolver = Resolver::new();
        let segments = vec![
            make_segment("第一段：这个链接", 0, 1_000),
            make_segment("第二段：这张截图", 2_000, 3_000),
        ];
        let hints = resolver.boost_hints(&segments);
        assert_eq!(hints.len(), 2);
        assert_eq!(hints[0].segment_idx, 0);
        assert_eq!(hints[1].segment_idx, 1);
    }
}
