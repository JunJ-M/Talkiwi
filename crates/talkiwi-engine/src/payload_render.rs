//! Shared natural-language rendering of `ActionPayload`s.
//!
//! Two consumers need a text rendering:
//!
//! - `candidate::CandidateBuilder` — compact previews that feed the LLM
//!   prompt alongside each segment's candidate set. Optimizes for
//!   token-per-event density.
//! - `retrieval::render_chunks` — richer narrative lines stored inside
//!   each `RetrievalChunk.text`. Used for embedding + human inspection.
//!
//! Both originally kept private copies of almost-identical rendering
//! logic. This module consolidates that into two public entry points
//! (`trim_for_prompt`, `narrate_for_retrieval`) sharing internal
//! truncation helpers.

use talkiwi_core::event::{ActionPayload, ActionType};

pub const TEXT_PREVIEW_CAP: usize = 500;
pub const OCR_PREVIEW_CAP: usize = 300;
pub const FILE_PREVIEW_CAP: usize = 300;
pub const CUSTOM_PREVIEW_CAP: usize = 300;
pub const NARRATION_TEXT_CAP: usize = 300;

/// Compact preview suitable for prompting. No surrounding quotes, no
/// action-type prefix — the candidate-set JSON carries type separately.
pub fn trim_for_prompt(payload: &ActionPayload) -> String {
    match payload {
        ActionPayload::SelectionText {
            text, app_name, ..
        } => format!(
            "selected in {}: {}",
            app_name,
            truncate_chars(text, TEXT_PREVIEW_CAP)
        ),
        ActionPayload::Screenshot {
            ocr_text: Some(ocr),
            ..
        } => format!("screenshot OCR: {}", truncate_chars(ocr, OCR_PREVIEW_CAP)),
        ActionPayload::Screenshot { .. } => "screenshot".to_string(),
        ActionPayload::ClipboardChange {
            text: Some(t),
            source_app,
            ..
        } => {
            let src = source_app
                .as_ref()
                .map(|a| format!(" from {}", a))
                .unwrap_or_default();
            format!("copied{}: {}", src, truncate_chars(t, TEXT_PREVIEW_CAP))
        }
        ActionPayload::ClipboardChange { content_type, .. } => {
            format!("clipboard ({:?})", content_type)
        }
        ActionPayload::PageCurrent {
            url,
            title,
            app_name,
            ..
        } => match url {
            Some(u) => format!("page \"{}\" ({}) in {}", title, u, app_name),
            None => format!("page \"{}\" in {}", title, app_name),
        },
        ActionPayload::ClickLink { to_url, title, .. } => match title {
            Some(t) => format!("link \"{}\" -> {}", t, to_url),
            None => format!("link -> {}", to_url),
        },
        ActionPayload::FileAttach {
            file_name,
            mime_type,
            preview,
            ..
        } => {
            let p = preview
                .as_ref()
                .map(|x| format!(" preview: {}", truncate_chars(x, FILE_PREVIEW_CAP)))
                .unwrap_or_default();
            format!("file {} ({}){}", file_name, mime_type, p)
        }
        ActionPayload::ClickMouse { .. } => "click".to_string(),
        ActionPayload::WindowFocus {
            app_name,
            window_title,
        } => format!("focus {} — {}", app_name, window_title),
        ActionPayload::Custom(v) => {
            format!("custom: {}", truncate_chars(&v.to_string(), CUSTOM_PREVIEW_CAP))
        }
    }
}

/// Narrative line for retrieval chunks. Uses past-tense verbs and
/// quoted text so the blob reads naturally as a sentence.
pub fn narrate_for_retrieval(payload: &ActionPayload, action_type: &ActionType) -> String {
    match payload {
        ActionPayload::SelectionText {
            text, app_name, ..
        } => format!(
            "selected in {}: \"{}\"",
            app_name,
            truncate_chars(text, NARRATION_TEXT_CAP)
        ),
        ActionPayload::Screenshot {
            ocr_text: Some(ocr),
            ..
        } => format!(
            "screenshot OCR: \"{}\"",
            truncate_chars(ocr, NARRATION_TEXT_CAP)
        ),
        ActionPayload::Screenshot { .. } => "took a screenshot".to_string(),
        ActionPayload::ClipboardChange {
            text: Some(t),
            source_app,
            ..
        } => {
            let from = source_app
                .as_ref()
                .map(|a| format!(" from {}", a))
                .unwrap_or_default();
            format!(
                "copied{}: \"{}\"",
                from,
                truncate_chars(t, NARRATION_TEXT_CAP)
            )
        }
        ActionPayload::ClipboardChange { content_type, .. } => {
            format!("clipboard change ({:?})", content_type)
        }
        ActionPayload::PageCurrent {
            url,
            title,
            app_name,
            ..
        } => match url {
            Some(u) => format!("visited \"{}\" ({}) in {}", title, u, app_name),
            None => format!("visited \"{}\" in {}", title, app_name),
        },
        ActionPayload::ClickLink { to_url, title, .. } => match title {
            Some(t) => format!("clicked link \"{}\" -> {}", t, to_url),
            None => format!("clicked link -> {}", to_url),
        },
        ActionPayload::FileAttach {
            file_name,
            mime_type,
            preview,
            ..
        } => {
            let prev = preview
                .as_ref()
                .map(|p| format!(" preview: \"{}\"", truncate_chars(p, FILE_PREVIEW_CAP)))
                .unwrap_or_default();
            format!(
                "attached file \"{}\" ({}){}",
                file_name, mime_type, prev
            )
        }
        ActionPayload::ClickMouse { app_name, .. } => match app_name {
            Some(a) => format!("clicked in {}", a),
            None => "clicked".to_string(),
        },
        ActionPayload::WindowFocus {
            app_name,
            window_title,
        } => format!("focused \"{}\" in {}", window_title, app_name),
        ActionPayload::Custom(v) => format!(
            "{} event: {}",
            action_type.as_str(),
            truncate_chars(&v.to_string(), CUSTOM_PREVIEW_CAP.min(200))
        ),
    }
}

/// Char-boundary safe truncation. Never splits a multi-byte UTF-8
/// scalar. Appends an ellipsis when truncation occurs so callers can
/// visibly tell the string was shortened.
pub fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use talkiwi_core::event::{ActionPayload, ClipboardContentType};

    #[test]
    fn truncate_respects_char_boundaries() {
        let mixed = "一二三abc一二三";
        assert_eq!(truncate_chars(mixed, 4), "一二三a…");
    }

    #[test]
    fn trim_for_prompt_clipboard_with_source() {
        let payload = ActionPayload::ClipboardChange {
            content_type: ClipboardContentType::Text,
            text: Some("hello".to_string()),
            file_path: None,
            source_app: Some("Slack".to_string()),
        };
        assert_eq!(trim_for_prompt(&payload), "copied from Slack: hello");
    }

    #[test]
    fn narrate_for_retrieval_clipboard_quotes_text() {
        let payload = ActionPayload::ClipboardChange {
            content_type: ClipboardContentType::Text,
            text: Some("panic: oops".to_string()),
            file_path: None,
            source_app: Some("Slack".to_string()),
        };
        let line =
            narrate_for_retrieval(&payload, &talkiwi_core::event::ActionType::ClipboardChange);
        assert_eq!(line, "copied from Slack: \"panic: oops\"");
    }

    #[test]
    fn trim_for_prompt_page_with_url() {
        let payload = ActionPayload::PageCurrent {
            url: Some("https://example.com/doc".to_string()),
            title: "Docs".to_string(),
            app_name: "Chrome".to_string(),
            bundle_id: "com.google.Chrome".to_string(),
        };
        assert_eq!(
            trim_for_prompt(&payload),
            "page \"Docs\" (https://example.com/doc) in Chrome"
        );
    }

    #[test]
    fn trim_for_prompt_truncates_long_selection() {
        let payload = ActionPayload::SelectionText {
            text: "a".repeat(1_000),
            app_name: "VSCode".to_string(),
            window_title: "f.rs".to_string(),
            char_count: 1_000,
        };
        let out = trim_for_prompt(&payload);
        assert!(out.ends_with('…'));
        assert!(out.chars().count() < 600);
    }
}
