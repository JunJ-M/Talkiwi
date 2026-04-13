use serde::{Deserialize, Serialize};

/// Labels used by the Assembler to build markdown output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssemblerLabels {
    pub section_task: String,
    pub section_context: String,
    pub section_constraints: String,
    pub section_expected: String,
    pub section_notes: String,
    pub notes_preamble: String,
    // format_payload labels
    pub source_label: String,
    pub screenshot_label: String,
    pub ocr_text_label: String,
    pub clipboard_label: String,
    pub clipboard_type_text: String,
    pub clipboard_type_image: String,
    pub clipboard_type_file: String,
    pub current_page_label: String,
    pub navigate_to_label: String,
    pub attachment_label: String,
    pub custom_event_label: String,
    // summarize_payload labels
    pub selected_text_summary: String,
    pub screenshot_summary: String,
    pub clipboard_summary: String,
    pub current_page_summary: String,
    pub navigate_to_summary: String,
    pub attachment_summary: String,
    pub custom_event_summary: String,
}

/// Locale configuration for the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocaleConfig {
    pub assembler: AssemblerLabels,
}

impl Default for AssemblerLabels {
    fn default() -> Self {
        Self {
            section_task: "## 任务".to_string(),
            section_context: "## 上下文".to_string(),
            section_constraints: "## 约束".to_string(),
            section_expected: "## 期望输出".to_string(),
            section_notes: "## 注意".to_string(),
            notes_preamble: "以下信息可能需要补充：".to_string(),
            source_label: "来源".to_string(),
            screenshot_label: "截图".to_string(),
            ocr_text_label: "OCR 文本".to_string(),
            clipboard_label: "剪贴板内容".to_string(),
            clipboard_type_text: "文本".to_string(),
            clipboard_type_image: "图片".to_string(),
            clipboard_type_file: "文件".to_string(),
            current_page_label: "当前页面".to_string(),
            navigate_to_label: "导航到".to_string(),
            attachment_label: "附件".to_string(),
            custom_event_label: "自定义事件".to_string(),
            selected_text_summary: "{app} 中选中的文字 ({chars} 字符)".to_string(),
            screenshot_summary: "截图 ({w}x{h})".to_string(),
            clipboard_summary: "剪贴板{type}".to_string(),
            current_page_summary: "当前页面: {title}".to_string(),
            navigate_to_summary: "导航到: {url}".to_string(),
            attachment_summary: "附件: {name}".to_string(),
            custom_event_summary: "自定义事件".to_string(),
        }
    }
}

impl Default for LocaleConfig {
    fn default() -> Self {
        Self {
            assembler: AssemblerLabels::default(),
        }
    }
}
