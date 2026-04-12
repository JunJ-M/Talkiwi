use crate::event::ActionEvent;
use crate::session::SpeakSegment;

/// A unified timeline entry — either a speech segment or an action event.
#[derive(Debug, Clone)]
pub enum TimelineEntry {
    Speak(SpeakSegment),
    Action(ActionEvent),
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
    let mut result = Vec::with_capacity(segments.len() + events.len());
    let mut si = 0;
    let mut ei = 0;

    while si < segments.len() && ei < events.len() {
        if segments[si].start_ms <= events[ei].session_offset_ms {
            result.push(TimelineEntry::Speak(segments[si].clone()));
            si += 1;
        } else {
            result.push(TimelineEntry::Action(events[ei].clone()));
            ei += 1;
        }
    }

    while si < segments.len() {
        result.push(TimelineEntry::Speak(segments[si].clone()));
        si += 1;
    }

    while ei < events.len() {
        result.push(TimelineEntry::Action(events[ei].clone()));
        ei += 1;
    }

    result
}

/// Generate a text summary from a timeline for LLM input.
pub fn timeline_to_summary(timeline: &[TimelineEntry]) -> String {
    let mut lines = Vec::new();

    for entry in timeline {
        match entry {
            TimelineEntry::Speak(s) => {
                lines.push(format!("[{}-{}ms] SPEAK: {}", s.start_ms, s.end_ms, s.text));
            }
            TimelineEntry::Action(a) => {
                let payload_summary = match &a.payload {
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
                    crate::event::ActionPayload::Custom(v) => {
                        format!("Custom: {}", v)
                    }
                };
                lines.push(format!(
                    "[{}ms] ACTION({}): {}",
                    a.session_offset_ms,
                    a.action_type.as_str(),
                    payload_summary
                ));
            }
        }
    }

    lines.join("\n")
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

        assert!(summary.contains("[0-1000ms] SPEAK: rewrite this"));
        assert!(summary.contains("[500ms] ACTION(selection.text):"));
    }

    #[test]
    fn truncate_handles_unicode() {
        let long_str = "a".repeat(200);
        assert_eq!(truncate(&long_str, 100).len(), 100);
        assert_eq!(truncate("short", 100), "short");
    }
}
