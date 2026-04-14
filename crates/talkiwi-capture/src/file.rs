use std::fs;
use std::path::Path;

use talkiwi_core::clock::SessionClock;
use talkiwi_core::event::{ActionEvent, ActionPayload, ActionType};
use uuid::Uuid;

/// Supported code/text extensions that get preview content.
const CODE_EXTENSIONS: &[&str] = &[
    "ts", "js", "py", "rs", "go", "java", "swift", "c", "cpp", "tsx", "jsx", "json", "yaml",
    "toml", "md", "txt",
];

/// Maximum characters for file preview.
const MAX_PREVIEW_CHARS: usize = 500;

/// Process a dropped file into an ActionEvent.
///
/// Reads file metadata, guesses MIME type, and extracts preview text
/// for code/text files.
///
/// # Trust model
/// The `file_path` comes from a Tauri drag-drop event initiated by the user.
/// The user explicitly dragged the file into the application, so any readable
/// path is considered intentional. No path restriction is applied.
pub fn process_dropped_file(
    file_path: &str,
    session_id: Uuid,
    session_offset_ms: u64,
) -> anyhow::Result<ActionEvent> {
    let path = Path::new(file_path);

    let metadata = fs::metadata(path)
        .map_err(|e| anyhow::anyhow!("failed to read file metadata for {}: {}", file_path, e))?;

    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| file_path.to_string());

    let mime_type = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();

    let preview = extract_preview(path);

    let payload = ActionPayload::FileAttach {
        file_path: file_path.to_string(),
        file_name,
        file_size: metadata.len(),
        mime_type,
        preview,
    };

    Ok(ActionEvent {
        id: Uuid::new_v4(),
        session_id,
        timestamp: u64::try_from(chrono::Utc::now().timestamp_millis()).unwrap_or(0),
        session_offset_ms,
        observed_offset_ms: Some(session_offset_ms),
        duration_ms: None,
        action_type: ActionType::FileAttach,
        plugin_id: "builtin".to_string(),
        payload,
        semantic_hint: None,
        confidence: 1.0,
    })
}

/// Extract preview text for code/text files.
/// Returns None for non-text files or on read errors.
fn extract_preview(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?.to_lowercase();

    if !CODE_EXTENSIONS.contains(&ext.as_str()) {
        return None;
    }

    let content = fs::read_to_string(path).ok()?;
    if content.len() <= MAX_PREVIEW_CHARS {
        Some(content)
    } else {
        // Truncate at char boundary
        // Find a safe char boundary at or before MAX_PREVIEW_CHARS
        let mut end = MAX_PREVIEW_CHARS;
        while end > 0 && !content.is_char_boundary(end) {
            end -= 1;
        }
        Some(content[..end].to_string())
    }
}

/// FileCapture implements ActionCapture for file drag-drop events.
/// Since files are injected via Tauri events, start/stop are no-ops.
pub struct FileCapture;

impl talkiwi_core::traits::capture::ActionCapture for FileCapture {
    fn id(&self) -> &str {
        "builtin.file"
    }

    fn action_types(&self) -> &[ActionType] {
        &[ActionType::FileAttach]
    }

    fn start(
        &mut self,
        _tx: tokio::sync::mpsc::Sender<ActionEvent>,
        _clock: SessionClock,
    ) -> anyhow::Result<()> {
        // File events are injected externally via ActionTrack::inject_event
        Ok(())
    }

    fn stop(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn check_permission(&self) -> talkiwi_core::traits::capture::PermissionStatus {
        talkiwi_core::traits::capture::PermissionStatus::NotRequired
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_temp_file(ext: &str, content: &str) -> NamedTempFile {
        let suffix = format!(".{}", ext);
        let mut file = tempfile::Builder::new().suffix(&suffix).tempfile().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file.flush().unwrap();
        file
    }

    #[test]
    fn file_capture_rust_file() {
        let content = "fn main() {\n    println!(\"hello\");\n}";
        let file = create_temp_file("rs", content);
        let path = file.path().to_str().unwrap();

        let event = process_dropped_file(path, Uuid::new_v4(), 1000).unwrap();

        assert_eq!(event.action_type, ActionType::FileAttach);
        if let ActionPayload::FileAttach {
            file_name,
            mime_type,
            preview,
            file_size,
            ..
        } = &event.payload
        {
            assert!(file_name.ends_with(".rs"));
            assert_eq!(mime_type, "text/x-rust");
            assert_eq!(preview.as_deref(), Some(content));
            assert_eq!(*file_size, content.len() as u64);
        } else {
            panic!("Expected FileAttach payload");
        }
    }

    #[test]
    fn file_capture_image_file() {
        // Create a fake PNG file (just needs the extension)
        let file = create_temp_file("png", "PNG fake image data");
        let path = file.path().to_str().unwrap();

        let event = process_dropped_file(path, Uuid::new_v4(), 1000).unwrap();

        if let ActionPayload::FileAttach {
            mime_type, preview, ..
        } = &event.payload
        {
            assert!(mime_type.starts_with("image/"));
            assert!(preview.is_none()); // No preview for images
        } else {
            panic!("Expected FileAttach payload");
        }
    }

    #[test]
    fn file_capture_large_text_preview_truncated() {
        let content: String = "x".repeat(1000);
        let file = create_temp_file("txt", &content);
        let path = file.path().to_str().unwrap();

        let event = process_dropped_file(path, Uuid::new_v4(), 1000).unwrap();

        if let ActionPayload::FileAttach { preview, .. } = &event.payload {
            let preview = preview.as_ref().unwrap();
            assert_eq!(preview.len(), MAX_PREVIEW_CHARS);
        } else {
            panic!("Expected FileAttach payload");
        }
    }

    #[test]
    fn file_capture_nonexistent_file() {
        let result = process_dropped_file("/nonexistent/file.rs", Uuid::new_v4(), 0);
        assert!(result.is_err());
    }

    #[test]
    fn file_capture_supported_code_extensions() {
        for ext in CODE_EXTENSIONS {
            let content = "test content";
            let file = create_temp_file(ext, content);
            let path = file.path().to_str().unwrap();

            let event = process_dropped_file(path, Uuid::new_v4(), 0).unwrap();

            if let ActionPayload::FileAttach { preview, .. } = &event.payload {
                assert!(
                    preview.is_some(),
                    "Extension {} should produce preview",
                    ext
                );
            }
        }
    }

    #[test]
    fn file_capture_permission_not_required() {
        use talkiwi_core::traits::capture::{ActionCapture, PermissionStatus};
        let capture = FileCapture;
        assert_eq!(capture.check_permission(), PermissionStatus::NotRequired);
    }
}
