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
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: ActionEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, event.id);
        assert_eq!(deserialized.action_type, ActionType::Screenshot);
        assert_eq!(deserialized.confidence, 1.0);
        assert_eq!(deserialized.observed_offset_ms, Some(5000));
    }
}
