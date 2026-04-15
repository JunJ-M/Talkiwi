use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

/// Action type enum — unified dotted name for serde, as_str(), and DB storage.
///
/// Serialization uses `as_str()` as the single source of truth. Known variants
/// map to fixed dotted names; unknown strings become `Custom`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ActionType {
    SelectionText,
    Screenshot,
    ClipboardChange,
    PageCurrent,
    ClickLink,
    WindowFocus,
    ClickMouse,
    FileAttach,
    /// V1.5: plugin-registered custom action types.
    Custom(String),
}

impl Serialize for ActionType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ActionType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(Self::from_str_name(&s))
    }
}

impl ActionType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::SelectionText => "selection.text",
            Self::Screenshot => "screenshot",
            Self::ClipboardChange => "clipboard.change",
            Self::PageCurrent => "page.current",
            Self::ClickLink => "click.link",
            Self::WindowFocus => "window.focus",
            Self::ClickMouse => "click.mouse",
            Self::FileAttach => "file.attach",
            Self::Custom(s) => s.as_str(),
        }
    }

    /// Parse from dotted name string.
    pub fn from_str_name(s: &str) -> Self {
        match s {
            "selection.text" => Self::SelectionText,
            "screenshot" => Self::Screenshot,
            "clipboard.change" => Self::ClipboardChange,
            "page.current" => Self::PageCurrent,
            "click.link" => Self::ClickLink,
            "window.focus" => Self::WindowFocus,
            "click.mouse" => Self::ClickMouse,
            "file.attach" => Self::FileAttach,
            other => Self::Custom(other.to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardContentType {
    Text,
    Image,
    File,
}

/// How this event entered the timeline.
///
/// `Passive` — captured automatically by a background capture plugin.
/// `Toolbar` — user explicitly clicked a Trace Toolbar button.
/// `Manual`  — user authored the event (e.g. a short note).
///
/// Defaults to `Passive` so old JSON blobs (pre-2026-04-16) deserialize
/// into the correct bucket without a DB migration on the `payload` column.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TraceSource {
    #[default]
    Passive,
    Toolbar,
    Manual,
}

/// The user's semantic role for a captured event inside the current intent.
///
/// Field exists in V1 schema for forward-compatibility; the chip-based
/// tagging UI ships in V2. V1 code paths read `Option<TraceRole>` as
/// `None` everywhere and must not break when the value is missing.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TraceRole {
    /// 问题现场
    Issue,
    /// 目标位置
    Target,
    /// 期望结果
    Expected,
    /// 参考上下文
    Reference,
}

/// Trace curation metadata — tells the prompt pipeline *why* an event
/// matters and *how* it entered the timeline.
///
/// Stored alongside (not inside) `ActionPayload` to avoid serde ambiguity
/// on the untagged payload enum.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct TraceCuration {
    #[serde(default)]
    pub source: TraceSource,

    /// `None` = not yet tagged. V1 always reads as `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<TraceRole>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_note: Option<String>,

    /// Soft-deleted by the user from the widget timeline.
    /// Deleted events stay on disk for audit but never enter the prompt.
    #[serde(default)]
    pub deleted: bool,
}

impl TraceCuration {
    /// Convenience constructor for toolbar-sourced events.
    pub fn toolbar() -> Self {
        Self {
            source: TraceSource::Toolbar,
            ..Default::default()
        }
    }

    /// Convenience constructor for user-authored (manual) events.
    pub fn manual() -> Self {
        Self {
            source: TraceSource::Manual,
            ..Default::default()
        }
    }

    /// True if this event was explicitly surfaced by the user (toolbar
    /// button or manual note). Passive capture events return `false`.
    pub fn is_user_sourced(&self) -> bool {
        matches!(self.source, TraceSource::Toolbar | TraceSource::Manual)
    }
}

/// Action event payload — untagged, discriminated by ActionEvent.action_type.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ActionPayload {
    SelectionText {
        text: String,
        app_name: String,
        window_title: String,
        char_count: usize,
    },
    Screenshot {
        image_path: String,
        width: u32,
        height: u32,
        ocr_text: Option<String>,
    },
    ClipboardChange {
        content_type: ClipboardContentType,
        text: Option<String>,
        file_path: Option<String>,
        source_app: Option<String>,
    },
    PageCurrent {
        url: Option<String>,
        title: String,
        app_name: String,
        bundle_id: String,
    },
    ClickLink {
        from_url: Option<String>,
        to_url: String,
        title: Option<String>,
    },
    WindowFocus {
        app_name: String,
        window_title: String,
    },
    ClickMouse {
        app_name: Option<String>,
        window_title: Option<String>,
        button: String,
        x: f64,
        y: f64,
    },
    FileAttach {
        file_path: String,
        file_name: String,
        file_size: u64,
        mime_type: String,
        preview: Option<String>,
    },
    Custom(serde_json::Value),
}

/// Action event — the basic unit of Action Track.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionEvent {
    pub id: Uuid,
    pub session_id: Uuid,
    pub timestamp: u64,
    pub session_offset_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observed_offset_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub action_type: ActionType,
    pub plugin_id: String,
    pub payload: ActionPayload,
    pub semantic_hint: Option<String>,
    pub confidence: f32,
    /// Curation metadata — added 2026-04-16. Old events deserialize with
    /// `TraceCuration::default()` (source=Passive, role=None, deleted=false).
    #[serde(default)]
    pub curation: TraceCuration,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_type_serde_consistency() {
        let types = vec![
            (ActionType::SelectionText, "selection.text"),
            (ActionType::Screenshot, "screenshot"),
            (ActionType::ClipboardChange, "clipboard.change"),
            (ActionType::PageCurrent, "page.current"),
            (ActionType::ClickLink, "click.link"),
            (ActionType::WindowFocus, "window.focus"),
            (ActionType::ClickMouse, "click.mouse"),
            (ActionType::FileAttach, "file.attach"),
        ];

        for (action_type, expected_str) in &types {
            assert_eq!(action_type.as_str(), *expected_str);

            let json = serde_json::to_string(action_type).unwrap();
            assert_eq!(json, format!("\"{}\"", expected_str));

            let deserialized: ActionType = serde_json::from_str(&json).unwrap();
            assert_eq!(&deserialized, action_type);
        }
    }

    #[test]
    fn action_type_custom_round_trip() {
        let custom = ActionType::Custom("plugin.my_action".to_string());
        assert_eq!(custom.as_str(), "plugin.my_action");

        let json = serde_json::to_string(&custom).unwrap();
        let deserialized: ActionType = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.as_str(), "plugin.my_action");
    }

    #[test]
    fn action_type_from_str_name() {
        assert_eq!(
            ActionType::from_str_name("screenshot"),
            ActionType::Screenshot
        );
        assert_eq!(
            ActionType::from_str_name("plugin.custom"),
            ActionType::Custom("plugin.custom".to_string())
        );
    }

    #[test]
    fn action_payload_selection_text_round_trip() {
        let payload = ActionPayload::SelectionText {
            text: "hello world".to_string(),
            app_name: "VSCode".to_string(),
            window_title: "main.rs".to_string(),
            char_count: 11,
        };
        let json = serde_json::to_string(&payload).unwrap();
        let deserialized: ActionPayload = serde_json::from_str(&json).unwrap();
        if let ActionPayload::SelectionText {
            text, char_count, ..
        } = deserialized
        {
            assert_eq!(text, "hello world");
            assert_eq!(char_count, 11);
        } else {
            panic!("Expected SelectionText variant");
        }
    }

    #[test]
    fn action_event_full_round_trip() {
        let event = ActionEvent {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            timestamp: 1712900000000,
            session_offset_ms: 5000,
            observed_offset_ms: Some(5000),
            duration_ms: Some(100),
            action_type: ActionType::Screenshot,
            plugin_id: "builtin".to_string(),
            payload: ActionPayload::Screenshot {
                image_path: "/tmp/screenshot.png".to_string(),
                width: 1920,
                height: 1080,
                ocr_text: None,
            },
            semantic_hint: Some("user took a screenshot".to_string()),
            confidence: 1.0,
            curation: TraceCuration::default(),
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: ActionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, event.id);
        assert_eq!(deserialized.action_type, ActionType::Screenshot);
        assert_eq!(deserialized.confidence, 1.0);
        assert_eq!(deserialized.observed_offset_ms, Some(5000));
        assert_eq!(deserialized.curation.source, TraceSource::Passive);
    }

    #[test]
    fn trace_curation_defaults_to_passive() {
        let cur = TraceCuration::default();
        assert_eq!(cur.source, TraceSource::Passive);
        assert!(cur.role.is_none());
        assert!(!cur.deleted);
        assert!(!cur.is_user_sourced());
    }

    #[test]
    fn trace_curation_toolbar_constructor_is_user_sourced() {
        assert!(TraceCuration::toolbar().is_user_sourced());
        assert!(TraceCuration::manual().is_user_sourced());
        assert!(!TraceCuration::default().is_user_sourced());
    }

    #[test]
    fn old_action_event_json_without_curation_deserializes() {
        // Simulates an event persisted before 2026-04-16 — the curation
        // field is absent from the JSON. serde(default) must fill it in.
        let legacy_json = r#"{
            "id": "00000000-0000-0000-0000-000000000001",
            "session_id": "00000000-0000-0000-0000-000000000002",
            "timestamp": 1712900000000,
            "session_offset_ms": 100,
            "duration_ms": null,
            "action_type": "click.mouse",
            "plugin_id": "builtin.click",
            "payload": {
                "app_name": null,
                "window_title": null,
                "button": "left",
                "x": 10.0,
                "y": 20.0
            },
            "semantic_hint": null,
            "confidence": 0.9
        }"#;
        let event: ActionEvent = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(event.curation.source, TraceSource::Passive);
        assert!(!event.curation.deleted);
    }

    #[test]
    fn toolbar_sourced_event_round_trips_source_field() {
        let event = ActionEvent {
            id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            timestamp: 1712900000000,
            session_offset_ms: 5000,
            observed_offset_ms: Some(5000),
            duration_ms: None,
            action_type: ActionType::SelectionText,
            plugin_id: "trace_toolbar".to_string(),
            payload: ActionPayload::SelectionText {
                text: "hello".to_string(),
                app_name: "VSCode".to_string(),
                window_title: "main.rs".to_string(),
                char_count: 5,
            },
            semantic_hint: None,
            confidence: 1.0,
            curation: TraceCuration::toolbar(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("\"source\":\"toolbar\""));

        let back: ActionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back.curation.source, TraceSource::Toolbar);
    }
}
