use crate::event::{ActionEvent, TraceSource};
use crate::session::SpeakSegment;

/// A unified timeline entry — either a speech segment or an action event.
///
/// `Action` is boxed because `ActionEvent` is ~296 bytes while `Speak`
/// is ~48 bytes — without indirection every enum instance pays the full
/// cost, wasting memory on event-heavy timelines.
#[derive(Debug, Clone)]
pub enum TimelineEntry {
    Speak(SpeakSegment),
    Action(Box<ActionEvent>),
}

impl TimelineEntry {
    /// Get the start time (ms) of this entry for sorting.
    pub fn start_ms(&self) -> u64 {
        match self {
            Self::Speak(s) => s.start_ms,
            Self::Action(a) => a.session_offset_ms,
        }
    }
}

/// Merge-sort align speak segments and action events by time.
pub fn align_timeline(segments: &[SpeakSegment], events: &[ActionEvent]) -> Vec<TimelineEntry> {
    let mut sorted_segments = segments.to_vec();
    sorted_segments.sort_by_key(|segment| segment.start_ms);

    let mut sorted_events = events.to_vec();
    sorted_events.sort_by_key(|event| event.session_offset_ms);

    let mut result = Vec::with_capacity(segments.len() + events.len());
    let mut si = 0;
    let mut ei = 0;

    while si < sorted_segments.len() && ei < sorted_events.len() {
        if sorted_segments[si].start_ms <= sorted_events[ei].session_offset_ms {
            result.push(TimelineEntry::Speak(sorted_segments[si].clone()));
            si += 1;
        } else {
            result.push(TimelineEntry::Action(Box::new(sorted_events[ei].clone())));
            ei += 1;
        }
    }

    while si < sorted_segments.len() {
        result.push(TimelineEntry::Speak(sorted_segments[si].clone()));
        si += 1;
    }

    while ei < sorted_events.len() {
        result.push(TimelineEntry::Action(Box::new(sorted_events[ei].clone())));
        ei += 1;
    }

    result
}

/// Generate a structured text summary from a timeline for LLM input.
///
/// Output separates speech and events into two sections. Events are numbered
/// from 0 so the LLM can reference them by index in its `references` output.
pub fn timeline_to_summary(timeline: &[TimelineEntry]) -> String {
    let mut speech_lines = Vec::new();
    let mut event_lines = Vec::new();
    let mut event_idx: usize = 0;

    for entry in timeline {
        match entry {
            TimelineEntry::Speak(s) => {
                speech_lines.push(format!("[{}-{}ms] {}", s.start_ms, s.end_ms, s.text));
            }
            TimelineEntry::Action(a) => {
                // Soft-deleted events never enter the prompt summary.
                if a.curation.deleted {
                    continue;
                }
                let summary = format_action_summary(&a.payload);
                // Prefix user-sourced events so the LLM can identify them
                // as higher-priority signal than passive captures.
                //   `★ ` — toolbar-captured
                //   `✎ ` — manual note
                //   (none) — passive capture
                let prefix = match a.curation.source {
                    TraceSource::Toolbar => "★ ",
                    TraceSource::Manual => "✎ ",
                    TraceSource::Passive => "",
                };
                event_lines.push(format!(
                    "[{}] {}{} @ {}ms — {}",
                    event_idx,
                    prefix,
                    a.action_type.as_str(),
                    a.session_offset_ms,
                    summary,
                ));
                event_idx += 1;
            }
        }
    }

    let mut result = String::new();
    if !speech_lines.is_empty() {
        result.push_str("口语转录:\n");
        result.push_str(&speech_lines.join("\n"));
    }
    if !event_lines.is_empty() {
        if !result.is_empty() {
            result.push_str("\n\n");
        }
        result.push_str("操作事件:\n");
        result.push_str(&event_lines.join("\n"));
    }
    result
}

/// Bound the timeline summary for LLM input.
///
/// Keeps the most recent speech lines within `max_chars`, then appends the
/// event section if space remains. This preserves recent spoken intent first.
pub fn timeline_to_summary_bounded(timeline: &[TimelineEntry], max_chars: usize) -> String {
    let full = timeline_to_summary(timeline);
    if full.len() <= max_chars {
        return full;
    }

    let mut sections = full.split("\n\n");
    let speech_section = sections.next().unwrap_or_default();
    let events_section = sections.next().unwrap_or_default();

    let mut speech_lines: Vec<&str> = speech_section.lines().collect();
    let header = if !speech_lines.is_empty() {
        speech_lines.remove(0)
    } else {
        "口语转录:"
    };

    let mut kept_speech: Vec<String> = Vec::new();
    let mut budget = max_chars.saturating_sub(header.len() + 1);
    for line in speech_lines.iter().rev() {
        let line_cost = line.len() + 1;
        if line_cost > budget {
            if kept_speech.is_empty() && budget > 0 {
                kept_speech.push(truncate_spoken_content(line, budget));
            }
            break;
        } else {
            kept_speech.push((*line).to_string());
            budget = budget.saturating_sub(line_cost);
        }
    }
    kept_speech.reverse();

    let mut result = String::new();
    result.push_str(header);
    if !kept_speech.is_empty() {
        result.push('\n');
        result.push_str(&kept_speech.join("\n"));
    }

    if !events_section.is_empty() {
        let separator_cost = 2;
        if result.len() + separator_cost < max_chars {
            let remaining = max_chars - result.len() - separator_cost;
            let mut event_lines: Vec<&str> = events_section.lines().collect();
            let event_header = if !event_lines.is_empty() {
                event_lines.remove(0)
            } else {
                "操作事件:"
            };
            let mut event_text = event_header.to_string();
            for line in event_lines {
                if event_text.len() + line.len() + 1 > remaining {
                    break;
                }
                event_text.push('\n');
                event_text.push_str(line);
            }

            result.push_str("\n\n");
            result.push_str(&event_text);
        }
    }

    result
}

/// Format a single action payload into a short summary for the LLM.
fn format_action_summary(payload: &crate::event::ActionPayload) -> String {
    match payload {
        crate::event::ActionPayload::SelectionText { text, app_name, .. } => {
            format!("Selected text in {}: \"{}\"", app_name, truncate(text, 100))
        }
        crate::event::ActionPayload::Screenshot { image_path, .. } => {
            format!("Screenshot: {}", image_path)
        }
        crate::event::ActionPayload::ClipboardChange { text, .. } => {
            let preview = text.as_deref().unwrap_or("<non-text>");
            format!("Clipboard: \"{}\"", truncate(preview, 100))
        }
        crate::event::ActionPayload::PageCurrent { title, url, .. } => {
            let url_str = url.as_deref().unwrap_or("N/A");
            format!("Page: {} ({})", title, url_str)
        }
        crate::event::ActionPayload::ClickLink { to_url, .. } => {
            format!("Navigate to: {}", to_url)
        }
        crate::event::ActionPayload::FileAttach { file_name, .. } => {
            format!("File attached: {}", file_name)
        }
        crate::event::ActionPayload::WindowFocus {
            app_name,
            window_title,
        } => {
            format!("Window focus: {} ({})", window_title, app_name)
        }
        crate::event::ActionPayload::ClickMouse {
            app_name,
            window_title,
            button,
            x,
            y,
        } => {
            let app = app_name.as_deref().unwrap_or("unknown");
            let title = window_title.as_deref().unwrap_or("unknown");
            format!(
                "Mouse click: {} at ({:.0}, {:.0}) in {} ({})",
                button, x, y, title, app
            )
        }
        crate::event::ActionPayload::Custom(v) => {
            format!("Custom: {}", v)
        }
    }
}

/// Truncate a string to at most `max_len` bytes on a char boundary.
/// Returns `""` only when the first character is wider than `max_len`.
fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        return s;
    }
    // Find the last char boundary that fits within max_len
    let mut end = max_len;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

fn truncate_spoken_content(line: &str, max_len: usize) -> String {
    if let Some((_, spoken)) = line.split_once("] ") {
        let spoken = spoken.trim();
        return truncate(spoken, max_len).to_string();
    }

    truncate(line, max_len).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::*;
    use uuid::Uuid;

    fn make_segment(text: &str, start: u64, end: u64) -> SpeakSegment {
        SpeakSegment {
            text: text.to_string(),
            start_ms: start,
            end_ms: end,
            confidence: 0.95,
            is_final: true,
        }
    }

    fn make_event(offset: u64, action_type: ActionType) -> ActionEvent {
        ActionEvent {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            timestamp: 1712900000000 + offset,
            session_offset_ms: offset,
            observed_offset_ms: Some(offset),
            duration_ms: None,
            action_type,
            plugin_id: "builtin".to_string(),
            payload: ActionPayload::SelectionText {
                text: "test".to_string(),
                app_name: "VSCode".to_string(),
                window_title: "main.rs".to_string(),
                char_count: 4,
            },
            semantic_hint: None,
            confidence: 1.0,
            curation: Default::default(),
        }
    }

    #[test]
    fn align_timeline_merge_sort() {
        let segments = vec![
            make_segment("hello", 0, 1000),
            make_segment("world", 2000, 3000),
        ];
        let events = vec![
            make_event(500, ActionType::SelectionText),
            make_event(2500, ActionType::Screenshot),
        ];

        let timeline = align_timeline(&segments, &events);
        assert_eq!(timeline.len(), 4);

        let times: Vec<u64> = timeline.iter().map(|e| e.start_ms()).collect();
        assert_eq!(times, vec![0, 500, 2000, 2500]);
    }

    #[test]
    fn align_timeline_sorts_unsorted_inputs() {
        let segments = vec![
            make_segment("world", 2000, 3000),
            make_segment("hello", 0, 1000),
        ];
        let events = vec![
            make_event(2500, ActionType::Screenshot),
            make_event(500, ActionType::SelectionText),
        ];

        let timeline = align_timeline(&segments, &events);
        let times: Vec<u64> = timeline.iter().map(|e| e.start_ms()).collect();
        assert_eq!(times, vec![0, 500, 2000, 2500]);
    }

    #[test]
    fn align_timeline_empty_inputs() {
        let empty_segments: Vec<SpeakSegment> = vec![];
        let empty_events: Vec<ActionEvent> = vec![];

        assert!(align_timeline(&empty_segments, &empty_events).is_empty());

        let segments = vec![make_segment("hello", 0, 1000)];
        let timeline = align_timeline(&segments, &empty_events);
        assert_eq!(timeline.len(), 1);

        let events = vec![make_event(500, ActionType::Screenshot)];
        let timeline = align_timeline(&empty_segments, &events);
        assert_eq!(timeline.len(), 1);
    }

    #[test]
    fn timeline_to_summary_format() {
        let segments = vec![make_segment("rewrite this", 0, 1000)];
        let events = vec![make_event(500, ActionType::SelectionText)];
        let timeline = align_timeline(&segments, &events);
        let summary = timeline_to_summary(&timeline);

        assert!(summary.contains("口语转录:\n[0-1000ms] rewrite this"));
        assert!(summary.contains("操作事件:\n[0] selection.text @ 500ms"));
    }

    #[test]
    fn truncate_handles_unicode() {
        let long_str = "a".repeat(200);
        assert_eq!(truncate(&long_str, 100).len(), 100);
        assert_eq!(truncate("short", 100), "short");
    }

    #[test]
    fn timeline_to_summary_bounded_keeps_recent_speech() {
        let segments = vec![
            make_segment("第一句", 0, 1000),
            make_segment("第二句", 2000, 3000),
            make_segment("第三句", 4000, 5000),
        ];
        let timeline = align_timeline(&segments, &[]);
        let summary = timeline_to_summary_bounded(&timeline, 32);
        assert!(summary.contains("第三句"));
    }

    #[test]
    fn timeline_summary_prefixes_user_sourced_events() {
        use crate::event::{TraceCuration, TraceSource};

        let mut toolbar_event = make_event(500, ActionType::Screenshot);
        toolbar_event.curation = TraceCuration {
            source: TraceSource::Toolbar,
            ..Default::default()
        };
        let mut manual_event = make_event(1500, ActionType::SelectionText);
        manual_event.curation = TraceCuration {
            source: TraceSource::Manual,
            ..Default::default()
        };
        let passive_event = make_event(2500, ActionType::SelectionText);

        let timeline = align_timeline(
            &[],
            &[toolbar_event, manual_event, passive_event],
        );
        let summary = timeline_to_summary(&timeline);

        assert!(summary.contains("★ screenshot @ 500ms"));
        assert!(summary.contains("✎ selection.text @ 1500ms"));
        // Passive event has no prefix before `selection.text`.
        assert!(summary.contains("] selection.text @ 2500ms"));
        assert!(!summary.contains("★ selection.text @ 2500ms"));
    }

    #[test]
    fn timeline_summary_drops_deleted_events() {
        use crate::event::{TraceCuration, TraceSource};

        let mut deleted = make_event(500, ActionType::Screenshot);
        deleted.curation = TraceCuration {
            source: TraceSource::Toolbar,
            deleted: true,
            ..Default::default()
        };
        let kept = make_event(1500, ActionType::SelectionText);

        let timeline = align_timeline(&[], &[deleted, kept]);
        let summary = timeline_to_summary(&timeline);
        assert!(!summary.contains("screenshot"));
        assert!(summary.contains("selection.text @ 1500ms"));
    }
}
