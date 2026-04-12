use std::collections::HashSet;

use talkiwi_core::event::{ActionEvent, ActionPayload, ClipboardContentType};
use talkiwi_core::output::{ArtifactRef, IntentOutput, Reference};
use talkiwi_core::traits::intent::IntentRaw;
use uuid::Uuid;

/// Assemble the final `IntentOutput` from LLM output, action events, and resolved references.
///
/// Two modes:
/// - **Pure voice**: no events → `final_markdown` is just the restructured speech text
/// - **Structured**: events present → markdown with 任务/上下文/约束/期望输出/注意 sections
///
/// Referenced events appear first in the artifact list (context-1, context-2, ...),
/// followed by unreferenced events. No event appears twice.
pub fn assemble(
    raw: &IntentRaw,
    events: &[ActionEvent],
    references: &[Reference],
    session_id: Uuid,
) -> IntentOutput {
    let (artifacts, markdown) = if events.is_empty() {
        (Vec::new(), assemble_pure_voice(raw))
    } else {
        let artifacts = build_artifacts(events, references);
        let markdown = assemble_structured(raw, events, &artifacts);
        (artifacts, markdown)
    };

    IntentOutput {
        session_id,
        task: raw.task.clone(),
        intent: raw.intent.clone(),
        constraints: raw.constraints.clone(),
        missing_context: raw.missing_context.clone(),
        restructured_speech: raw.restructured_speech.clone(),
        final_markdown: markdown,
        artifacts,
        references: references.to_vec(),
    }
}

fn assemble_pure_voice(raw: &IntentRaw) -> String {
    raw.restructured_speech.clone()
}

fn assemble_structured(
    raw: &IntentRaw,
    events: &[ActionEvent],
    artifacts: &[ArtifactRef],
) -> String {
    let mut md = String::new();

    // Task section
    md.push_str("## 任务\n");
    md.push_str(&raw.task);
    md.push_str("\n\n");

    // Context section
    md.push_str("## 上下文\n");
    for artifact in artifacts {
        md.push_str(&format!("### {}\n", artifact.label));
        // Find the event for this artifact
        if let Some(event) = events.iter().find(|e| e.id == artifact.event_id) {
            md.push_str(&format_payload(&event.payload));
        } else {
            md.push_str(&artifact.inline_summary);
        }
        md.push('\n');
    }
    md.push('\n');

    // Constraints section
    if !raw.constraints.is_empty() {
        md.push_str("## 约束\n");
        for c in &raw.constraints {
            md.push_str(&format!("- {}\n", c));
        }
        md.push('\n');
    }

    // Expected output section
    md.push_str("## 期望输出\n");
    md.push_str(&raw.restructured_speech);
    md.push_str("\n\n");

    // Notes section (missing context)
    if !raw.missing_context.is_empty() {
        md.push_str("## 注意\n以下信息可能需要补充：\n");
        for m in &raw.missing_context {
            md.push_str(&format!("- {}\n", m));
        }
    }

    md
}

/// Build artifacts list: referenced events first (in order), unreferenced appended.
fn build_artifacts(events: &[ActionEvent], references: &[Reference]) -> Vec<ArtifactRef> {
    let mut artifacts: Vec<ArtifactRef> = Vec::new();
    let mut used_event_ids: HashSet<Uuid> = HashSet::new();
    let mut counter = 1;

    // Referenced events first
    for reference in references {
        if let Some(event_id) = reference.resolved_event_id {
            if !used_event_ids.insert(event_id) {
                continue;
            }
            if let Some(event) = events.iter().find(|e| e.id == event_id) {
                artifacts.push(ArtifactRef {
                    event_id,
                    label: format!("context-{}", counter),
                    inline_summary: summarize_payload(&event.payload),
                });
                counter += 1;
            }
        }
    }

    // Unreferenced events appended
    for event in events {
        if !used_event_ids.insert(event.id) {
            continue;
        }
        artifacts.push(ArtifactRef {
            event_id: event.id,
            label: format!("context-{}", counter),
            inline_summary: summarize_payload(&event.payload),
        });
        counter += 1;
    }

    artifacts
}

/// Format an ActionPayload into markdown for the context section.
fn format_payload(payload: &ActionPayload) -> String {
    match payload {
        ActionPayload::SelectionText {
            text,
            app_name,
            window_title,
            ..
        } => {
            format!(
                "**来源**: {} 中选中的文字 ({})\n```\n{}\n```\n",
                app_name, window_title, text
            )
        }
        ActionPayload::Screenshot {
            image_path,
            width,
            height,
            ocr_text,
        } => {
            let mut s = format!("**截图** ({}x{}): {}\n", width, height, image_path);
            if let Some(ocr) = ocr_text {
                s.push_str(&format!("OCR 文本:\n```\n{}\n```\n", ocr));
            }
            s
        }
        ActionPayload::ClipboardChange {
            content_type, text, ..
        } => {
            let type_str = match content_type {
                ClipboardContentType::Text => "文本",
                ClipboardContentType::Image => "图片",
                ClipboardContentType::File => "文件",
            };
            let mut s = format!("**剪贴板内容** ({}):\n", type_str);
            if let Some(t) = text {
                s.push_str(&format!("```\n{}\n```\n", t));
            }
            s
        }
        ActionPayload::PageCurrent {
            url,
            title,
            app_name,
            ..
        } => {
            let link = match url {
                Some(u) => format!("[{}]({})", title, u),
                None => title.clone(),
            };
            format!("**当前页面**: {} ({})\n", link, app_name)
        }
        ActionPayload::ClickLink { to_url, title, .. } => {
            let title_str = title.as_deref().unwrap_or(to_url.as_str());
            format!("**导航到**: [{}]({})\n", title_str, to_url)
        }
        ActionPayload::FileAttach {
            file_name,
            file_size,
            mime_type,
            preview,
            ..
        } => {
            let size_str = format_file_size(*file_size);
            let mut s = format!("**附件**: {} ({}, {})\n", file_name, size_str, mime_type);
            if let Some(p) = preview {
                s.push_str(&format!("```\n{}\n```\n", p));
            }
            s
        }
        ActionPayload::Custom(val) => {
            format!("**自定义事件**: {}\n", val)
        }
    }
}

/// Short summary of a payload for artifact inline_summary.
fn summarize_payload(payload: &ActionPayload) -> String {
    match payload {
        ActionPayload::SelectionText {
            app_name,
            char_count,
            ..
        } => {
            format!("{} 中选中的文字 ({} 字符)", app_name, char_count)
        }
        ActionPayload::Screenshot { width, height, .. } => {
            format!("截图 ({}x{})", width, height)
        }
        ActionPayload::ClipboardChange { content_type, .. } => {
            let type_str = match content_type {
                ClipboardContentType::Text => "文本",
                ClipboardContentType::Image => "图片",
                ClipboardContentType::File => "文件",
            };
            format!("剪贴板{}", type_str)
        }
        ActionPayload::PageCurrent { title, .. } => {
            format!("当前页面: {}", title)
        }
        ActionPayload::ClickLink { to_url, .. } => {
            format!("导航到: {}", to_url)
        }
        ActionPayload::FileAttach { file_name, .. } => {
            format!("附件: {}", file_name)
        }
        ActionPayload::Custom(_) => "自定义事件".to_string(),
    }
}

fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use talkiwi_core::event::{ActionType, ClipboardContentType};
    use talkiwi_core::output::ReferenceStrategy;

    fn make_raw(task: &str, constraints: Vec<&str>, missing: Vec<&str>) -> IntentRaw {
        IntentRaw {
            task: task.to_string(),
            intent: "rewrite".to_string(),
            constraints: constraints.into_iter().map(|s| s.to_string()).collect(),
            missing_context: missing.into_iter().map(|s| s.to_string()).collect(),
            restructured_speech: "请重写选中的函数".to_string(),
        }
    }

    fn make_event_with_id(
        id: Uuid,
        action_type: ActionType,
        payload: ActionPayload,
    ) -> ActionEvent {
        ActionEvent {
            id,
            session_id: Uuid::new_v4(),
            timestamp: 1712900000000,
            session_offset_ms: 3000,
            duration_ms: None,
            action_type,
            plugin_id: "builtin".to_string(),
            payload,
            semantic_hint: None,
            confidence: 1.0,
        }
    }

    fn make_reference(spoken_text: &str, event_id: Uuid, event_idx: usize) -> Reference {
        Reference {
            spoken_text: spoken_text.to_string(),
            spoken_offset: 0,
            resolved_event_idx: event_idx,
            resolved_event_id: Some(event_id),
            confidence: 0.9,
            strategy: ReferenceStrategy::TemporalProximity,
            user_confirmed: false,
        }
    }

    #[test]
    fn assemble_pure_voice_mode() {
        let raw = make_raw("重写函数", vec![], vec![]);
        let output = assemble(&raw, &[], &[], Uuid::new_v4());

        assert_eq!(output.final_markdown, "请重写选中的函数");
        assert!(output.artifacts.is_empty());
        assert!(output.references.is_empty());
    }

    #[test]
    fn assemble_structured_mode_with_all_sections() {
        let raw = make_raw(
            "重写选中的函数",
            vec!["使用 Rust", "保持接口不变"],
            vec!["具体是哪个函数"],
        );
        let event_id = Uuid::new_v4();
        let events = vec![make_event_with_id(
            event_id,
            ActionType::SelectionText,
            ActionPayload::SelectionText {
                text: "fn old() {}".to_string(),
                app_name: "VSCode".to_string(),
                window_title: "main.rs".to_string(),
                char_count: 11,
            },
        )];
        let refs = vec![make_reference("这段代码", event_id, 0)];

        let output = assemble(&raw, &events, &refs, Uuid::new_v4());

        assert!(output.final_markdown.contains("## 任务"));
        assert!(output.final_markdown.contains("## 上下文"));
        assert!(output.final_markdown.contains("## 约束"));
        assert!(output.final_markdown.contains("## 期望输出"));
        assert!(output.final_markdown.contains("## 注意"));
        assert!(output.final_markdown.contains("重写选中的函数"));
        assert!(output.final_markdown.contains("使用 Rust"));
        assert!(output.final_markdown.contains("具体是哪个函数"));
    }

    #[test]
    fn assemble_context_selection_text_format() {
        let raw = make_raw("分析代码", vec![], vec![]);
        let event_id = Uuid::new_v4();
        let events = vec![make_event_with_id(
            event_id,
            ActionType::SelectionText,
            ActionPayload::SelectionText {
                text: "fn main() { println!(\"hello\"); }".to_string(),
                app_name: "VSCode".to_string(),
                window_title: "main.rs".to_string(),
                char_count: 33,
            },
        )];

        let output = assemble(&raw, &events, &[], Uuid::new_v4());
        assert!(output.final_markdown.contains("**来源**: VSCode"));
        assert!(output.final_markdown.contains("```\nfn main()"));
    }

    #[test]
    fn assemble_context_screenshot_format() {
        let raw = make_raw("分析截图", vec![], vec![]);
        let event_id = Uuid::new_v4();
        let events = vec![make_event_with_id(
            event_id,
            ActionType::Screenshot,
            ActionPayload::Screenshot {
                image_path: "/sessions/abc/shot.png".to_string(),
                width: 1920,
                height: 1080,
                ocr_text: Some("Error: undefined".to_string()),
            },
        )];

        let output = assemble(&raw, &events, &[], Uuid::new_v4());
        assert!(output.final_markdown.contains("**截图** (1920x1080)"));
        assert!(output.final_markdown.contains("OCR 文本"));
        assert!(output.final_markdown.contains("Error: undefined"));
    }

    #[test]
    fn assemble_context_clipboard_format() {
        let raw = make_raw("分析内容", vec![], vec![]);
        let event_id = Uuid::new_v4();
        let events = vec![make_event_with_id(
            event_id,
            ActionType::ClipboardChange,
            ActionPayload::ClipboardChange {
                content_type: ClipboardContentType::Text,
                text: Some("copied content here".to_string()),
                file_path: None,
                source_app: None,
            },
        )];

        let output = assemble(&raw, &events, &[], Uuid::new_v4());
        assert!(output.final_markdown.contains("**剪贴板内容** (文本)"));
        assert!(output.final_markdown.contains("copied content here"));
    }

    #[test]
    fn assemble_context_page_format() {
        let raw = make_raw("总结页面", vec![], vec![]);
        let event_id = Uuid::new_v4();
        let events = vec![make_event_with_id(
            event_id,
            ActionType::PageCurrent,
            ActionPayload::PageCurrent {
                url: Some("https://docs.rs".to_string()),
                title: "Rust Documentation".to_string(),
                app_name: "Chrome".to_string(),
                bundle_id: "com.google.Chrome".to_string(),
            },
        )];

        let output = assemble(&raw, &events, &[], Uuid::new_v4());
        assert!(output
            .final_markdown
            .contains("[Rust Documentation](https://docs.rs)"));
        assert!(output.final_markdown.contains("Chrome"));
    }

    #[test]
    fn assemble_context_file_format() {
        let raw = make_raw("分析文件", vec![], vec![]);
        let event_id = Uuid::new_v4();
        let events = vec![make_event_with_id(
            event_id,
            ActionType::FileAttach,
            ActionPayload::FileAttach {
                file_path: "/tmp/test.rs".to_string(),
                file_name: "test.rs".to_string(),
                file_size: 2048,
                mime_type: "text/x-rust".to_string(),
                preview: Some("fn test() {}".to_string()),
            },
        )];

        let output = assemble(&raw, &events, &[], Uuid::new_v4());
        assert!(output.final_markdown.contains("**附件**: test.rs"));
        assert!(output.final_markdown.contains("2.0 KB"));
        assert!(output.final_markdown.contains("fn test() {}"));
    }

    #[test]
    fn assemble_artifact_ordering() {
        let raw = make_raw("处理", vec![], vec![]);
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let id3 = Uuid::new_v4();

        let events = vec![
            make_event_with_id(
                id1,
                ActionType::SelectionText,
                ActionPayload::SelectionText {
                    text: "first".to_string(),
                    app_name: "A".to_string(),
                    window_title: "W".to_string(),
                    char_count: 5,
                },
            ),
            make_event_with_id(
                id2,
                ActionType::Screenshot,
                ActionPayload::Screenshot {
                    image_path: "/shot.png".to_string(),
                    width: 800,
                    height: 600,
                    ocr_text: None,
                },
            ),
            make_event_with_id(
                id3,
                ActionType::ClipboardChange,
                ActionPayload::ClipboardChange {
                    content_type: ClipboardContentType::Text,
                    text: Some("clip".to_string()),
                    file_path: None,
                    source_app: None,
                },
            ),
        ];

        // Reference points to event id2 (second event, index 1)
        let refs = vec![make_reference("截图", id2, 1)];

        let output = assemble(&raw, &events, &refs, Uuid::new_v4());

        // Referenced event (id2) should be first
        assert_eq!(output.artifacts[0].event_id, id2);
        assert_eq!(output.artifacts[0].label, "context-1");
        // Unreferenced events appended in order
        assert_eq!(output.artifacts[1].event_id, id1);
        assert_eq!(output.artifacts[1].label, "context-2");
        assert_eq!(output.artifacts[2].event_id, id3);
        assert_eq!(output.artifacts[2].label, "context-3");
        // No duplicates
        assert_eq!(output.artifacts.len(), 3);
    }

    #[test]
    fn assemble_returns_valid_intent_output() {
        let session_id = Uuid::new_v4();
        let raw = IntentRaw {
            task: "Debug the issue".to_string(),
            intent: "debug".to_string(),
            constraints: vec!["no breaking changes".to_string()],
            missing_context: vec!["stack trace".to_string()],
            restructured_speech: "帮我调试这个问题".to_string(),
        };

        let output = assemble(&raw, &[], &[], session_id);

        assert_eq!(output.session_id, session_id);
        assert_eq!(output.task, "Debug the issue");
        assert_eq!(output.intent, "debug");
        assert_eq!(output.constraints, vec!["no breaking changes"]);
        assert_eq!(output.missing_context, vec!["stack trace"]);
        assert_eq!(output.restructured_speech, "帮我调试这个问题");
    }
}
