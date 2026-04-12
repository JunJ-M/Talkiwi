use regex::Regex;
use talkiwi_core::event::{ActionEvent, ActionType};
use talkiwi_core::output::{Reference, ReferenceStrategy};
use talkiwi_core::session::SpeakSegment;

/// A compiled pattern mapping spoken Chinese references to expected ActionTypes.
struct PatternEntry {
    regex: Regex,
    expected_types: Vec<ActionType>,
}

/// Reference resolver: maps spoken Chinese deictic expressions (e.g. "这段代码",
/// "这个报错") to `ActionEvent`s using regex pattern matching + temporal proximity scoring.
///
/// Patterns are ordered by specificity — longer patterns match first, preventing
/// the wildcard "这个/那个" from consuming matches meant for more specific patterns.
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
                vec![ActionType::PageCurrent, ActionType::ClickLink],
            ),
            (r"这张截图|截图|这个截屏", vec![ActionType::Screenshot]),
            (r"复制的|剪贴板", vec![ActionType::ClipboardChange]),
            (r"这个文件|附件", vec![ActionType::FileAttach]),
            // Wildcard — matches any type (empty vec signals no type filter)
            (r"这个|那个", vec![]),
        ];

        let patterns = patterns
            .into_iter()
            .map(|(pat, types)| PatternEntry {
                regex: Regex::new(pat).expect("invalid pattern"),
                expected_types: types,
            })
            .collect();

        Self { patterns }
    }

    /// Resolve spoken references to ActionEvents.
    ///
    /// Returns a list of References linking spoken text to the temporally
    /// closest matching ActionEvent.
    pub fn resolve(&self, segments: &[SpeakSegment], events: &[ActionEvent]) -> Vec<Reference> {
        if segments.is_empty() || events.is_empty() {
            return Vec::new();
        }

        // Collect all matches: (spoken_text, spoken_offset_in_concat, speech_time_ms, expected_types)
        // Patterns are ordered by specificity (longer/more specific first, wildcard last).
        // Each byte position can only be matched once.
        let mut matches: Vec<(String, usize, u64, &[ActionType])> = Vec::new();
        let mut concat_offset = 0usize;

        for segment in segments {
            let mut matched_ranges: Vec<(usize, usize)> = Vec::new();

            for entry in &self.patterns {
                for mat in entry.regex.find_iter(&segment.text) {
                    // Skip if this range overlaps with an already-matched range
                    let overlaps = matched_ranges
                        .iter()
                        .any(|(s, e)| mat.start() < *e && mat.end() > *s);
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
            concat_offset += segment.text.len() + 1; // +1 for space separator
        }

        // Deduplicate: track which event indices are already used
        let mut used_event_indices: Vec<bool> = vec![false; events.len()];
        let mut references = Vec::new();

        for (spoken_text, spoken_offset, speech_time_ms, expected_types) in &matches {
            if let Some((event_idx, confidence)) =
                find_best_event(events, *speech_time_ms, expected_types, &used_event_indices)
            {
                used_event_indices[event_idx] = true;
                references.push(Reference {
                    spoken_text: spoken_text.clone(),
                    spoken_offset: *spoken_offset,
                    resolved_event_idx: event_idx,
                    resolved_event_id: Some(events[event_idx].id),
                    confidence,
                    strategy: ReferenceStrategy::TemporalProximity,
                    user_confirmed: false,
                });
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

/// Find the best unused event matching the expected types, scored by temporal proximity.
///
/// Events before the speech time are preferred. Events after get a 3x distance penalty.
fn find_best_event(
    events: &[ActionEvent],
    speech_time_ms: u64,
    expected_types: &[ActionType],
    used: &[bool],
) -> Option<(usize, f32)> {
    let mut best: Option<(usize, f32, u64)> = None; // (idx, confidence, effective_distance)

    for (i, event) in events.iter().enumerate() {
        if used[i] {
            continue;
        }

        // Type filter: empty expected_types means wildcard (any type)
        if !expected_types.is_empty() && !expected_types.contains(&event.action_type) {
            continue;
        }

        let event_time = event.session_offset_ms;
        let effective_distance = if event_time <= speech_time_ms {
            speech_time_ms - event_time
        } else {
            // Post-speech events get 3x penalty
            (event_time - speech_time_ms).saturating_mul(3)
        };

        let confidence = distance_to_confidence(effective_distance);

        match &best {
            Some((_, _, best_dist)) if effective_distance < *best_dist => {
                best = Some((i, confidence, effective_distance));
            }
            None => {
                best = Some((i, confidence, effective_distance));
            }
            _ => {}
        }
    }

    best.map(|(idx, conf, _)| (idx, conf))
}

/// Map effective temporal distance to confidence score.
fn distance_to_confidence(distance_ms: u64) -> f32 {
    match distance_ms {
        0..=2999 => 0.9,
        3000..=9999 => 0.7,
        10000..=29999 => 0.5,
        _ => 0.3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use talkiwi_core::event::ActionPayload;
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
                content_type: talkiwi_core::event::ClipboardContentType::Text,
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
            ActionType::FileAttach => ActionPayload::FileAttach {
                file_path: "/tmp/test.rs".to_string(),
                file_name: "test.rs".to_string(),
                file_size: 1024,
                mime_type: "text/x-rust".to_string(),
                preview: None,
            },
            _ => ActionPayload::Custom(serde_json::json!({})),
        };

        ActionEvent {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            timestamp: 1712900000000,
            session_offset_ms,
            duration_ms: None,
            action_type,
            plugin_id: "builtin".to_string(),
            payload,
            semantic_hint: None,
            confidence: 1.0,
        }
    }

    #[test]
    fn resolve_zhege_daima_matches_selection_text() {
        let resolver = Resolver::new();
        let segments = vec![make_segment("帮我重写这段代码", 5000, 7000)];
        let events = vec![make_event(ActionType::SelectionText, 3000)];

        let refs = resolver.resolve(&segments, &events);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].spoken_text, "这段代码");
        assert_eq!(refs[0].resolved_event_idx, 0);
        assert_eq!(refs[0].confidence, 0.9); // 5000 - 3000 = 2000ms < 3s
        assert_eq!(refs[0].strategy, ReferenceStrategy::TemporalProximity);
    }

    #[test]
    fn resolve_zhege_baocuo_matches_screenshot_or_selection() {
        let resolver = Resolver::new();
        let segments = vec![make_segment("看看这个报错", 5000, 7000)];
        let events = vec![
            make_event(ActionType::Screenshot, 4000),
            make_event(ActionType::SelectionText, 4500),
        ];

        let refs = resolver.resolve(&segments, &events);
        assert_eq!(refs.len(), 1);
        // Should match the closer SelectionText at 4500ms (distance=500ms)
        assert_eq!(refs[0].resolved_event_idx, 1);
    }

    #[test]
    fn resolve_temporal_scoring() {
        let resolver = Resolver::new();

        // Test each distance bucket
        let test_cases = vec![
            (5000u64, 3500u64, 0.9f32), // 1500ms distance → 0.9
            (5000, 0, 0.7),             // 5000ms distance → 0.7
            (25000, 5000, 0.5),         // 20000ms distance → 0.5
            (50000, 5000, 0.3),         // 45000ms distance → 0.3
        ];

        for (speech_ms, event_ms, expected_conf) in test_cases {
            let segments = vec![make_segment("这段代码", speech_ms, speech_ms + 2000)];
            let events = vec![make_event(ActionType::SelectionText, event_ms)];
            let refs = resolver.resolve(&segments, &events);
            assert_eq!(refs.len(), 1);
            assert_eq!(
                refs[0].confidence, expected_conf,
                "speech_ms={}, event_ms={}, expected={}",
                speech_ms, event_ms, expected_conf
            );
        }
    }

    #[test]
    fn resolve_prefers_before_over_after() {
        let resolver = Resolver::new();
        let segments = vec![make_segment("这段代码", 5000, 7000)];

        // Event at 4000 (before, distance=1000) vs event at 5500 (after, effective_distance=1500)
        let events = vec![
            make_event(ActionType::SelectionText, 4000),
            make_event(ActionType::SelectionText, 5500),
        ];

        let refs = resolver.resolve(&segments, &events);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].resolved_event_idx, 0); // Before is preferred
    }

    #[test]
    fn resolve_post_speech_3x_penalty() {
        let resolver = Resolver::new();
        let segments = vec![make_segment("这段代码", 5000, 7000)];

        // Event at 3000 (before, distance=2000ms)
        // Event at 5500 (after, real_distance=500ms, effective=1500ms)
        // Before event wins because 2000 > 1500
        let events = vec![
            make_event(ActionType::SelectionText, 3000),
            make_event(ActionType::SelectionText, 5500),
        ];

        let refs = resolver.resolve(&segments, &events);
        assert_eq!(refs.len(), 1);
        // After event at 5500: effective = 500*3 = 1500ms < 2000ms before event
        assert_eq!(refs[0].resolved_event_idx, 1);
    }

    #[test]
    fn resolve_no_duplicate_events() {
        let resolver = Resolver::new();
        // Two references in one segment, but only one matching event
        let segments = vec![make_segment("这段代码和这个报错", 5000, 7000)];
        let events = vec![
            make_event(ActionType::SelectionText, 4000),
            make_event(ActionType::Screenshot, 3000),
        ];

        let refs = resolver.resolve(&segments, &events);
        assert_eq!(refs.len(), 2);
        // Each reference should resolve to a different event
        let resolved_indices: Vec<usize> = refs.iter().map(|r| r.resolved_event_idx).collect();
        assert!(resolved_indices.contains(&0));
        assert!(resolved_indices.contains(&1));
    }

    #[test]
    fn resolve_wildcard_zhege_matches_any() {
        let resolver = Resolver::new();
        let segments = vec![make_segment("帮我看看这个", 5000, 7000)];
        let events = vec![make_event(ActionType::FileAttach, 4000)];

        let refs = resolver.resolve(&segments, &events);
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].resolved_event_idx, 0);
    }

    #[test]
    fn resolve_empty_inputs() {
        let resolver = Resolver::new();

        // No segments
        assert!(resolver
            .resolve(&[], &[make_event(ActionType::Screenshot, 1000)])
            .is_empty());

        // No events
        assert!(resolver
            .resolve(&[make_segment("这段代码", 5000, 7000)], &[])
            .is_empty());

        // Both empty
        assert!(resolver.resolve(&[], &[]).is_empty());
    }
}
