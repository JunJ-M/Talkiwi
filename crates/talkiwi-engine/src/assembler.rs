use std::collections::HashSet;

use talkiwi_core::event::{ActionEvent, ActionPayload, ClipboardContentType};
use talkiwi_core::locale::AssemblerLabels;
use talkiwi_core::output::{ArtifactRef, IntentCategory, IntentOutput, Reference, RiskLevel};
use talkiwi_core::traits::intent::IntentRaw;
use uuid::Uuid;

use crate::importance::{EventScore, DEFAULT_PROMPT_THRESHOLD};

/// Optional inputs that gate how `assemble` filters the prompt surface.
/// When `importance_scores` is `None`, every live event is eligible for
/// the artifact list (legacy behavior). When it is `Some`, unreferenced
/// non-user-sourced events scoring below `prompt_threshold` are dropped
/// from artifacts / markdown — they continue to appear on the
/// retrieval surface separately.
#[derive(Debug, Clone)]
pub struct AssembleOptions<'a> {
    pub importance_scores: Option<&'a [EventScore]>,
    pub prompt_threshold: f32,
}

impl Default for AssembleOptions<'_> {
    fn default() -> Self {
        Self {
            importance_scores: None,
            prompt_threshold: DEFAULT_PROMPT_THRESHOLD,
        }
    }
}

/// Assemble the final `IntentOutput` from LLM output, action events, and resolved references.
///
/// Two modes:
/// - **Pure voice**: no events → `final_markdown` is just the restructured speech text
/// - **Structured**: events present → markdown with locale-defined section headers
///
/// Referenced events appear first in the artifact list (context-1, context-2, ...),
/// followed by unreferenced events. No event appears twice.
pub fn assemble(
    raw: &IntentRaw,
    events: &[ActionEvent],
    references: &[Reference],
    session_id: Uuid,
    labels: &AssemblerLabels,
) -> IntentOutput {
    assemble_with_options(
        raw,
        events,
        references,
        session_id,
        labels,
        AssembleOptions::default(),
    )
}

/// Like [`assemble`] but honors an optional importance filter so low
/// value passive events drop out of the prompt surface while still
/// flowing through to the retrieval surface.
pub fn assemble_with_options(
    raw: &IntentRaw,
    events: &[ActionEvent],
    references: &[Reference],
    session_id: Uuid,
    labels: &AssemblerLabels,
    options: AssembleOptions<'_>,
) -> IntentOutput {
    let (artifacts, markdown) = if events.is_empty() {
        (Vec::new(), assemble_pure_voice(raw))
    } else {
        let artifacts = build_artifacts(events, references, labels, &options);
        let markdown = assemble_structured(raw, events, &artifacts, labels);
        (artifacts, markdown)
    };

    IntentOutput {
        session_id,
        task: raw.task.clone(),
        intent: raw.intent.clone(),
        intent_category: IntentCategory::from_llm_output(&raw.intent),
        output_confidence: if references.is_empty() { 0.45 } else { 0.7 },
        risk_level: if references.is_empty() {
            RiskLevel::High
        } else {
            RiskLevel::Medium
        },
        constraints: raw.constraints.clone(),
        missing_context: raw.missing_context.clone(),
        restructured_speech: raw.restructured_speech.clone(),
        final_markdown: markdown,
        artifacts,
        references: references.to_vec(),
        retrieval_chunks: vec![],
    }
}

fn assemble_pure_voice(raw: &IntentRaw) -> String {
    raw.restructured_speech.clone()
}

fn assemble_structured(
    raw: &IntentRaw,
    events: &[ActionEvent],
    artifacts: &[ArtifactRef],
    labels: &AssemblerLabels,
) -> String {
    let mut md = String::new();

    // Task section
    md.push_str(&labels.section_task);
    md.push('\n');
    md.push_str(&raw.task);
    md.push_str("\n\n");

    // Context section
    md.push_str(&labels.section_context);
    md.push('\n');
    for artifact in artifacts {
        md.push_str(&format!("### {}\n", artifact.label));
        if let Some(event) = events.iter().find(|e| e.id == artifact.event_id) {
            md.push_str(&format_payload(&event.payload, labels));
        } else {
            md.push_str(&artifact.inline_summary);
        }
        md.push('\n');
    }
    md.push('\n');

    // Constraints section
    if !raw.constraints.is_empty() {
        md.push_str(&labels.section_constraints);
        md.push('\n');
        for c in &raw.constraints {
            md.push_str(&format!("- {}\n", c));
        }
        md.push('\n');
    }

    // Expected output section
    md.push_str(&labels.section_expected);
    md.push('\n');
    md.push_str(&raw.restructured_speech);
    md.push_str("\n\n");

    // Notes section (missing context)
    if !raw.missing_context.is_empty() {
        md.push_str(&labels.section_notes);
        md.push('\n');
        md.push_str(&labels.notes_preamble);
        md.push('\n');
        for m in &raw.missing_context {
            md.push_str(&format!("- {}\n", m));
        }
    }

    md
}

/// Maximum number of *unreferenced* user-sourced events (toolbar / manual)
/// allowed in the prompt. Prevents an over-enthusiastic user from drowning
/// their own transcript with low-value captures.
const TOOLBAR_UNREF_BUDGET: usize = 4;

/// Build artifacts list with two-tier ordering.
///
/// The reference resolver's semantics are preserved — referenced events
/// still come before unreferenced. Within each tier, events that the user
/// explicitly surfaced via the Trace Toolbar or a manual note float to the
/// top, so the prompt pipeline sees them first.
///
/// Tier A — referenced events:
///   A1. referenced ∧ curation.source ∈ {Toolbar, Manual}
///   A2. referenced ∧ curation.source == Passive
///
/// Tier B — unreferenced events:
///   B1. unreferenced ∧ curation.source ∈ {Toolbar, Manual}  (capped)
///   B2. unreferenced ∧ curation.source == Passive
///
/// Events with `curation.deleted == true` are excluded from the list
/// entirely — the user soft-deleted them from the widget timeline.
fn build_artifacts(
    events: &[ActionEvent],
    references: &[Reference],
    labels: &AssemblerLabels,
    options: &AssembleOptions<'_>,
) -> Vec<ArtifactRef> {
    let mut artifacts: Vec<ArtifactRef> = Vec::new();
    let mut used_event_ids: HashSet<Uuid> = HashSet::new();
    let mut counter = 1usize;

    // Collect referenced event ids in the order the resolver produced
    // them — including *every* target of a multi-target reference, not
    // just the primary. For `Composition`/`Contrast` this lifts every
    // referenced event into the artifact tier instead of silently
    // demoting secondary targets to the unreferenced pile.
    let mut referenced_ids: Vec<Uuid> = Vec::new();
    let mut seen: HashSet<Uuid> = HashSet::new();
    for reference in references {
        if reference.targets.is_empty() {
            if let Some(id) = reference.resolved_event_id {
                if seen.insert(id) {
                    referenced_ids.push(id);
                }
            }
        } else {
            for target in &reference.targets {
                if seen.insert(target.event_id) {
                    referenced_ids.push(target.event_id);
                }
            }
        }
    }

    let is_user_sourced = |event: &ActionEvent| event.curation.is_user_sourced();
    let is_live = |event: &ActionEvent| !event.curation.deleted;

    let push = |event: &ActionEvent,
                artifacts: &mut Vec<ArtifactRef>,
                counter: &mut usize| {
        artifacts.push(ArtifactRef {
            event_id: event.id,
            label: format!("context-{}", *counter),
            inline_summary: summarize_payload(&event.payload, labels),
        });
        *counter += 1;
    };

    // --- Tier A1: referenced + user-sourced ---
    for id in &referenced_ids {
        if used_event_ids.contains(id) {
            continue;
        }
        if let Some(event) = events.iter().find(|e| e.id == *id) {
            if is_live(event) && is_user_sourced(event) {
                used_event_ids.insert(*id);
                push(event, &mut artifacts, &mut counter);
            }
        }
    }

    // --- Tier A2: referenced + passive ---
    for id in &referenced_ids {
        if used_event_ids.contains(id) {
            continue;
        }
        if let Some(event) = events.iter().find(|e| e.id == *id) {
            if is_live(event) && !is_user_sourced(event) {
                used_event_ids.insert(*id);
                push(event, &mut artifacts, &mut counter);
            }
        }
    }

    // --- Tier B1: unreferenced + user-sourced (capped) ---
    let mut toolbar_budget = TOOLBAR_UNREF_BUDGET;
    for event in events {
        if toolbar_budget == 0 {
            break;
        }
        if used_event_ids.contains(&event.id) {
            continue;
        }
        if is_live(event) && is_user_sourced(event) {
            used_event_ids.insert(event.id);
            push(event, &mut artifacts, &mut counter);
            toolbar_budget -= 1;
        }
    }

    // --- Tier B2: unreferenced + passive ---
    // When an importance filter is supplied, events scoring below the
    // threshold drop out of the prompt surface here (tech plan §9.3).
    // Referenced events and user-sourced events have already been
    // placed by earlier tiers and are not affected by this filter.
    let passes_importance = |event: &ActionEvent| -> bool {
        let Some(scores) = options.importance_scores else {
            return true;
        };
        let Some(score) = scores.iter().find(|s| {
            events.get(s.event_idx).map(|e| e.id) == Some(event.id)
        }) else {
            return true; // score missing = don't over-filter
        };
        score.score >= options.prompt_threshold
    };

    for event in events {
        if used_event_ids.contains(&event.id) {
            continue;
        }
        if is_live(event) && !is_user_sourced(event) && passes_importance(event) {
            used_event_ids.insert(event.id);
            push(event, &mut artifacts, &mut counter);
        }
    }

    artifacts
}

/// Format an ActionPayload into markdown for the context section.
fn format_payload(payload: &ActionPayload, labels: &AssemblerLabels) -> String {
    match payload {
        ActionPayload::SelectionText {
            text,
            app_name,
            window_title,
            ..
        } => {
            format!(
                "**{}**: {} ({})  \n```\n{}\n```\n",
                labels.source_label, app_name, window_title, text
            )
        }
        ActionPayload::Screenshot {
            image_path,
            width,
            height,
            ocr_text,
        } => {
            let mut s = format!(
                "**{}** ({}x{}): {}\n",
                labels.screenshot_label, width, height, image_path
            );
            if let Some(ocr) = ocr_text {
                s.push_str(&format!("{}:\n```\n{}\n```\n", labels.ocr_text_label, ocr));
            }
            s
        }
        ActionPayload::ClipboardChange {
            content_type, text, ..
        } => {
            let type_str = match content_type {
                ClipboardContentType::Text => &labels.clipboard_type_text,
                ClipboardContentType::Image => &labels.clipboard_type_image,
                ClipboardContentType::File => &labels.clipboard_type_file,
            };
            let mut s = format!("**{}** ({}):\n", labels.clipboard_label, type_str);
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
            format!(
                "**{}**: {} ({})\n",
                labels.current_page_label, link, app_name
            )
        }
        ActionPayload::ClickLink { to_url, title, .. } => {
            let title_str = title.as_deref().unwrap_or(to_url.as_str());
            format!(
                "**{}**: [{}]({})\n",
                labels.navigate_to_label, title_str, to_url
            )
        }
        ActionPayload::FileAttach {
            file_name,
            file_size,
            mime_type,
            preview,
            ..
        } => {
            let size_str = format_file_size(*file_size);
            let mut s = format!(
                "**{}**: {} ({}, {})\n",
                labels.attachment_label, file_name, size_str, mime_type
            );
            if let Some(p) = preview {
                s.push_str(&format!("```\n{}\n```\n", p));
            }
            s
        }
        ActionPayload::WindowFocus {
            app_name,
            window_title,
        } => {
            format!("**窗口焦点**: {} ({})\n", window_title, app_name)
        }
        ActionPayload::ClickMouse {
            app_name,
            window_title,
            button,
            x,
            y,
        } => {
            let app_name = app_name.as_deref().unwrap_or("unknown");
            let window_title = window_title.as_deref().unwrap_or("unknown");
            format!(
                "**鼠标点击**: {} ({:.0}, {:.0}) [{} / {}]\n",
                button, x, y, app_name, window_title
            )
        }
        ActionPayload::Custom(val) => {
            format!("**{}**: {}\n", labels.custom_event_label, val)
        }
    }
}

/// Short summary of a payload for artifact inline_summary.
fn summarize_payload(payload: &ActionPayload, labels: &AssemblerLabels) -> String {
    match payload {
        ActionPayload::SelectionText {
            app_name,
            char_count,
            ..
        } => labels
            .selected_text_summary
            .replace("{app}", app_name)
            .replace("{chars}", &char_count.to_string()),
        ActionPayload::Screenshot { width, height, .. } => labels
            .screenshot_summary
            .replace("{w}", &width.to_string())
            .replace("{h}", &height.to_string()),
        ActionPayload::ClipboardChange { content_type, .. } => {
            let type_str = match content_type {
                ClipboardContentType::Text => &labels.clipboard_type_text,
                ClipboardContentType::Image => &labels.clipboard_type_image,
                ClipboardContentType::File => &labels.clipboard_type_file,
            };
            labels.clipboard_summary.replace("{type}", type_str)
        }
        ActionPayload::PageCurrent { title, .. } => {
            labels.current_page_summary.replace("{title}", title)
        }
        ActionPayload::ClickLink { to_url, .. } => {
            labels.navigate_to_summary.replace("{url}", to_url)
        }
        ActionPayload::FileAttach { file_name, .. } => {
            labels.attachment_summary.replace("{name}", file_name)
        }
        ActionPayload::WindowFocus {
            app_name,
            window_title,
        } => format!("Focused {} ({})", window_title, app_name),
        ActionPayload::ClickMouse {
            app_name,
            window_title,
            button,
            ..
        } => format!(
            "Mouse click {} in {} ({})",
            button,
            window_title.as_deref().unwrap_or("unknown"),
            app_name.as_deref().unwrap_or("unknown")
        ),
        ActionPayload::Custom(_) => labels.custom_event_summary.clone(),
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
    use talkiwi_core::locale::AssemblerLabels;
    use talkiwi_core::output::ReferenceStrategy;

    fn labels() -> AssemblerLabels {
        AssemblerLabels::default()
    }

    fn make_raw(task: &str, constraints: Vec<&str>, missing: Vec<&str>) -> IntentRaw {
        IntentRaw {
            task: task.to_string(),
            intent: "rewrite".to_string(),
            constraints: constraints.into_iter().map(|s| s.to_string()).collect(),
            missing_context: missing.into_iter().map(|s| s.to_string()).collect(),
            restructured_speech: "请重写选中的函数".to_string(),
            references: vec![],
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
            observed_offset_ms: Some(3000),
            duration_ms: None,
            action_type,
            plugin_id: "builtin".to_string(),
            payload,
            semantic_hint: None,
            confidence: 1.0,
            curation: Default::default(),
        }
    }

    fn make_reference(spoken_text: &str, event_id: Uuid, event_idx: usize) -> Reference {
        Reference::new_single(
            spoken_text.to_string(),
            0,
            event_idx,
            event_id,
            0.9,
            ReferenceStrategy::TemporalProximity,
        )
    }

    #[test]
    fn assemble_pure_voice_mode() {
        let raw = make_raw("重写函数", vec![], vec![]);
        let output = assemble(&raw, &[], &[], Uuid::new_v4(), &labels());

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

        let output = assemble(&raw, &events, &refs, Uuid::new_v4(), &labels());

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

        let output = assemble(&raw, &events, &[], Uuid::new_v4(), &labels());
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

        let output = assemble(&raw, &events, &[], Uuid::new_v4(), &labels());
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

        let output = assemble(&raw, &events, &[], Uuid::new_v4(), &labels());
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

        let output = assemble(&raw, &events, &[], Uuid::new_v4(), &labels());
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

        let output = assemble(&raw, &events, &[], Uuid::new_v4(), &labels());
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

        let output = assemble(&raw, &events, &refs, Uuid::new_v4(), &labels());

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
    fn composition_reference_lifts_all_targets_to_referenced_tier() {
        use talkiwi_core::output::{RefRelation, RefTarget, TargetRole};

        let raw = make_raw("组合", vec![], vec![]);
        let id_a = Uuid::new_v4();
        let id_b = Uuid::new_v4();
        let id_passive = Uuid::new_v4();
        let events = vec![
            make_event_with_id(
                id_a,
                ActionType::SelectionText,
                ActionPayload::SelectionText {
                    text: "a".to_string(),
                    app_name: "A".to_string(),
                    window_title: "w".to_string(),
                    char_count: 1,
                },
            ),
            make_event_with_id(
                id_b,
                ActionType::ClickLink,
                ActionPayload::ClickLink {
                    from_url: None,
                    to_url: "https://x/".to_string(),
                    title: None,
                },
            ),
            make_event_with_id(
                id_passive,
                ActionType::Screenshot,
                ActionPayload::Screenshot {
                    image_path: "/p.png".to_string(),
                    width: 1,
                    height: 1,
                    ocr_text: None,
                },
            ),
        ];
        // One Composition reference hitting both id_a and id_b.
        let composition = Reference {
            spoken_text: "A 和 B".to_string(),
            spoken_offset: 0,
            resolved_event_idx: 0,
            resolved_event_id: Some(id_a),
            confidence: 0.9,
            strategy: ReferenceStrategy::LlmCoreference,
            user_confirmed: false,
            targets: vec![
                RefTarget {
                    event_id: id_a,
                    event_idx: 0,
                    role: TargetRole::Source,
                    via_anchor: None,
                },
                RefTarget {
                    event_id: id_b,
                    event_idx: 1,
                    role: TargetRole::Source,
                    via_anchor: None,
                },
            ],
            relation: RefRelation::Composition,
            segment_idx: Some(0),
        };
        let output = assemble(&raw, &events, &[composition], Uuid::new_v4(), &labels());

        // Both composition targets land in the referenced tier. The
        // passive screenshot is appended after.
        let first_two: std::collections::HashSet<_> = output
            .artifacts
            .iter()
            .take(2)
            .map(|a| a.event_id)
            .collect();
        assert!(first_two.contains(&id_a));
        assert!(first_two.contains(&id_b));
        assert_eq!(output.artifacts[2].event_id, id_passive);
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
            references: vec![],
        };

        let output = assemble(&raw, &[], &[], session_id, &labels());

        assert_eq!(output.session_id, session_id);
        assert_eq!(output.task, "Debug the issue");
        assert_eq!(output.intent, "debug");
        assert_eq!(output.constraints, vec!["no breaking changes"]);
        assert_eq!(output.missing_context, vec!["stack trace"]);
        assert_eq!(output.restructured_speech, "帮我调试这个问题");
    }

    // ── Two-tier curation sort tests ───────────────────────────────────

    fn make_event_sourced(source: talkiwi_core::event::TraceSource) -> ActionEvent {
        let mut event = make_event_with_id(
            Uuid::new_v4(),
            ActionType::SelectionText,
            ActionPayload::SelectionText {
                text: "snippet".to_string(),
                app_name: "App".to_string(),
                window_title: "Win".to_string(),
                char_count: 7,
            },
        );
        event.curation = talkiwi_core::event::TraceCuration {
            source,
            ..Default::default()
        };
        event
    }

    #[test]
    fn toolbar_events_float_above_passive_within_referenced_tier() {
        use talkiwi_core::event::TraceSource;

        let passive_ref = make_event_sourced(TraceSource::Passive);
        let toolbar_ref = make_event_sourced(TraceSource::Toolbar);
        let events = vec![passive_ref.clone(), toolbar_ref.clone()];
        // Resolver hits passive first (order matches original event order).
        let refs = vec![
            make_reference("A", passive_ref.id, 0),
            make_reference("B", toolbar_ref.id, 1),
        ];

        let output = assemble(
            &make_raw("任务", vec![], vec![]),
            &events,
            &refs,
            Uuid::new_v4(),
            &labels(),
        );

        // Within the referenced tier, toolbar jumps ahead of passive.
        assert_eq!(output.artifacts.len(), 2);
        assert_eq!(output.artifacts[0].event_id, toolbar_ref.id);
        assert_eq!(output.artifacts[1].event_id, passive_ref.id);
    }

    #[test]
    fn referenced_tier_always_beats_unreferenced_tier() {
        use talkiwi_core::event::TraceSource;

        // Toolbar event is *not* referenced. Passive event IS referenced.
        // The referenced-passive event must still outrank the
        // unreferenced-toolbar event — we never let a toolbar capture
        // unseat the reference-resolver's semantic anchor.
        let passive_ref = make_event_sourced(TraceSource::Passive);
        let toolbar_unref = make_event_sourced(TraceSource::Toolbar);
        let events = vec![toolbar_unref.clone(), passive_ref.clone()];
        let refs = vec![make_reference("A", passive_ref.id, 1)];

        let output = assemble(
            &make_raw("任务", vec![], vec![]),
            &events,
            &refs,
            Uuid::new_v4(),
            &labels(),
        );

        assert_eq!(output.artifacts.len(), 2);
        assert_eq!(output.artifacts[0].event_id, passive_ref.id);
        assert_eq!(output.artifacts[1].event_id, toolbar_unref.id);
    }

    #[test]
    fn unreferenced_toolbar_events_are_capped_by_budget() {
        use talkiwi_core::event::TraceSource;

        // 6 toolbar events, none referenced. Only 4 should make it in.
        let toolbar_events: Vec<ActionEvent> = (0..6)
            .map(|_| make_event_sourced(TraceSource::Toolbar))
            .collect();

        let output = assemble(
            &make_raw("任务", vec![], vec![]),
            &toolbar_events,
            &[],
            Uuid::new_v4(),
            &labels(),
        );

        // 4 toolbar events capped + 0 passive = 4 total. The 2 overflow
        // toolbar events are dropped entirely (not demoted to the passive
        // tier, because they *are* user-sourced — we only want user
        // signal to dominate, not flood).
        assert_eq!(output.artifacts.len(), 4);
    }

    #[test]
    fn deleted_events_are_excluded_from_artifacts() {
        use talkiwi_core::event::{TraceCuration, TraceSource};

        let mut deleted = make_event_sourced(TraceSource::Toolbar);
        deleted.curation = TraceCuration {
            source: TraceSource::Toolbar,
            deleted: true,
            ..Default::default()
        };
        let live = make_event_sourced(TraceSource::Passive);

        let output = assemble(
            &make_raw("任务", vec![], vec![]),
            &[deleted.clone(), live.clone()],
            &[make_reference("X", deleted.id, 0)],
            Uuid::new_v4(),
            &labels(),
        );

        // Deleted event excluded even though it was referenced.
        assert_eq!(output.artifacts.len(), 1);
        assert_eq!(output.artifacts[0].event_id, live.id);
    }
}
