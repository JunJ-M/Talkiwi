//! Trace Toolbar commands — on-demand context capture during recording.
//!
//! Every command in this module goes through `SessionManager::inject_event`
//! so the captured events flow into the same ActionTrack + preview
//! pipeline as passive captures. The only differences:
//!
//! 1. Each event is tagged with `curation.source = Toolbar` (or `Manual`),
//!    which the assembler uses to float user-captured events above
//!    passive ones (see `build_artifacts` in talkiwi-engine/assembler.rs).
//! 2. Each event carries the `trace_toolbar` plugin id so capture health
//!    attribution stays distinct from the background captures.
//!
//! All osascript / xcap calls run inside `spawn_blocking` because they
//! are synchronous and can take 100–400 ms. The active session lock
//! is released before the blocking call runs.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use tauri::State;
use uuid::Uuid;

use talkiwi_core::event::{ActionEvent, ActionPayload, ActionType, TraceCuration};

use crate::AppState;

const TOOLBAR_PLUGIN_ID: &str = "trace_toolbar";
const MANUAL_NOTE_MAX_CHARS: usize = 280;
const MANUAL_NOTE_ACTION_TYPE: &str = "manual.note";

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn truncate_for_hint(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars).collect();
    out.push('…');
    out
}

/// Capture the currently selected text on the front application.
///
/// Reuses `talkiwi_capture::selection::get_selected_text()`, which is a
/// single osascript invocation — the same path used by the passive
/// SelectionCapture poll, just triggered explicitly by a toolbar click.
#[tauri::command]
pub async fn capture_selection_text(state: State<'_, AppState>) -> Result<ActionEvent, String> {
    let sm = &state.session_manager;
    let session_id = sm
        .current_session_id()
        .await
        .ok_or_else(|| "No active session".to_string())?;
    let offset_ms = sm.elapsed_ms().await;

    let selection = tokio::task::spawn_blocking(talkiwi_capture::selection::get_selected_text)
        .await
        .map_err(|e| format!("spawn_blocking failed: {e}"))?;

    let (app_name, window_title, text) = selection.ok_or_else(|| {
        "No text selected, or accessibility permission denied".to_string()
    })?;

    if text.chars().count() == 0 {
        return Err("Selection is empty".to_string());
    }

    let char_count = text.chars().count();
    let event = ActionEvent {
        id: Uuid::new_v4(),
        session_id,
        timestamp: now_ms(),
        session_offset_ms: offset_ms,
        observed_offset_ms: Some(offset_ms),
        duration_ms: None,
        action_type: ActionType::SelectionText,
        plugin_id: TOOLBAR_PLUGIN_ID.to_string(),
        payload: ActionPayload::SelectionText {
            text: text.clone(),
            app_name,
            window_title,
            char_count,
        },
        semantic_hint: Some(format!(
            "user captured selected text via toolbar: \"{}\"",
            truncate_for_hint(&text, 60)
        )),
        confidence: 1.0,
        curation: TraceCuration::toolbar(),
    };

    sm.inject_event(event.clone())
        .await
        .map_err(|e| e.to_string())?;
    Ok(event)
}

/// Capture the current front app + window title as a `PageCurrent` event.
///
/// V1 deliberately does *not* extract per-browser URLs — that path
/// requires a different osascript per vendor (Chrome, Safari, Arc, ...)
/// and deferred to Phase 3. V1 populates `url: None` and `title + app_name`.
#[tauri::command]
pub async fn capture_page_context(state: State<'_, AppState>) -> Result<ActionEvent, String> {
    let sm = &state.session_manager;
    let session_id = sm
        .current_session_id()
        .await
        .ok_or_else(|| "No active session".to_string())?;
    let offset_ms = sm.elapsed_ms().await;

    // Read front app name + bundle id + window title via System Events.
    // The `try ... on error` guard around `name of front window` makes
    // this robust against apps that have no visible window at the time
    // the user clicks the toolbar button.
    let context = tokio::task::spawn_blocking(|| -> Option<(String, String, String)> {
        let script = r#"
tell application "System Events"
    set frontApp to first application process whose frontmost is true
    set appName to name of frontApp
    try
        set appId to bundle identifier of frontApp
    on error
        set appId to ""
    end try
    try
        set winTitle to name of front window of frontApp
    on error
        set winTitle to ""
    end try
end tell
return appName & "|" & appId & "|" & winTitle
"#;
        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg(script)
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let raw = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = raw.trim().splitn(3, '|').collect();
        if parts.len() < 3 {
            return None;
        }
        Some((
            parts[0].to_string(),
            parts[1].to_string(),
            parts[2].to_string(),
        ))
    })
    .await
    .map_err(|e| format!("spawn_blocking failed: {e}"))?;

    let (app_name, bundle_id, window_title) = context.ok_or_else(|| {
        "Could not read current app context (accessibility permission required)".to_string()
    })?;

    let event = ActionEvent {
        id: Uuid::new_v4(),
        session_id,
        timestamp: now_ms(),
        session_offset_ms: offset_ms,
        observed_offset_ms: Some(offset_ms),
        duration_ms: None,
        action_type: ActionType::PageCurrent,
        plugin_id: TOOLBAR_PLUGIN_ID.to_string(),
        payload: ActionPayload::PageCurrent {
            url: None,
            title: window_title.clone(),
            app_name: app_name.clone(),
            bundle_id,
        },
        semantic_hint: Some(format!(
            "user pinned current page: {app_name} — {window_title}"
        )),
        confidence: 0.9,
        curation: TraceCuration::toolbar(),
    };

    sm.inject_event(event.clone())
        .await
        .map_err(|e| e.to_string())?;
    Ok(event)
}

/// Record a short freeform note authored by the user.
///
/// Uses `ActionType::Custom("manual.note")` + a JSON payload. V1 caps at
/// `MANUAL_NOTE_MAX_CHARS` characters; longer text is rejected rather
/// than silently truncated so the user knows their note didn't fit.
#[tauri::command]
pub async fn capture_manual_note(
    state: State<'_, AppState>,
    text: String,
) -> Result<ActionEvent, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("Note is empty".to_string());
    }
    let char_count = trimmed.chars().count();
    if char_count > MANUAL_NOTE_MAX_CHARS {
        return Err(format!(
            "Note exceeds {MANUAL_NOTE_MAX_CHARS} characters (got {char_count})"
        ));
    }

    let sm = &state.session_manager;
    let session_id = sm
        .current_session_id()
        .await
        .ok_or_else(|| "No active session".to_string())?;
    let offset_ms = sm.elapsed_ms().await;

    let payload_json = serde_json::json!({
        "kind": MANUAL_NOTE_ACTION_TYPE,
        "text": trimmed,
        "length": char_count,
    });

    let event = ActionEvent {
        id: Uuid::new_v4(),
        session_id,
        timestamp: now_ms(),
        session_offset_ms: offset_ms,
        observed_offset_ms: Some(offset_ms),
        duration_ms: None,
        action_type: ActionType::Custom(MANUAL_NOTE_ACTION_TYPE.to_string()),
        plugin_id: TOOLBAR_PLUGIN_ID.to_string(),
        payload: ActionPayload::Custom(payload_json),
        semantic_hint: Some(format!("user note: {}", truncate_for_hint(trimmed, 60))),
        confidence: 1.0,
        curation: TraceCuration::manual(),
    };

    sm.inject_event(event.clone())
        .await
        .map_err(|e| e.to_string())?;
    Ok(event)
}

/// Rectangular crop specified by the frontend after the user drags a
/// region in the widget. Coordinates are in physical pixels.
#[derive(Debug, Clone, Deserialize)]
pub struct RegionRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Capture a screenshot — either full-screen or a user-specified region.
///
/// V1 always runs through `xcap::Monitor::capture_image` (same path as
/// the existing `capture_screenshot` command). When `region` is provided,
/// the full frame is cropped via `image::imageops::crop_imm` before it
/// hits disk. Native overlay + drag-to-select lives in Phase 3.
///
/// Events are tagged with `TraceSource::Toolbar` so the assembler treats
/// them as user-captured signal.
#[tauri::command]
pub async fn capture_screenshot_region(
    state: State<'_, AppState>,
    region: Option<RegionRect>,
) -> Result<ActionEvent, String> {
    let sm = &state.session_manager;
    let session_id = sm
        .current_session_id()
        .await
        .ok_or_else(|| "No active session".to_string())?;
    let offset_ms = sm.elapsed_ms().await;

    let session_dir = state.output_dir.join(session_id.to_string());
    std::fs::create_dir_all(&session_dir).map_err(|e| e.to_string())?;

    let file_name = if region.is_some() {
        format!("region-{}.png", offset_ms)
    } else {
        format!("screenshot-{}.png", offset_ms)
    };
    let screenshot_path = session_dir.join(file_name);

    let path = screenshot_path.clone();
    let region_c = region.clone();
    let (width, height) =
        tokio::task::spawn_blocking(move || -> Result<(u32, u32), String> {
            let screens = xcap::Monitor::all().map_err(|e| e.to_string())?;
            let screen = screens.first().ok_or_else(|| "No monitors found".to_string())?;
            let image = screen.capture_image().map_err(|e| e.to_string())?;

            match region_c {
                Some(r) => {
                    // Clamp to image bounds — frontend-computed rectangles
                    // can occasionally spill over the screen edge on
                    // multi-monitor / high-DPI setups, and a spill that
                    // `crop_imm` can't satisfy panics the thread.
                    let max_w = image.width().saturating_sub(r.x);
                    let max_h = image.height().saturating_sub(r.y);
                    let w = r.width.min(max_w);
                    let h = r.height.min(max_h);
                    if w == 0 || h == 0 {
                        return Err("Region is outside the screen bounds".to_string());
                    }
                    let cropped =
                        image::imageops::crop_imm(&image, r.x, r.y, w, h).to_image();
                    let dims = (cropped.width(), cropped.height());
                    cropped.save(&path).map_err(|e| e.to_string())?;
                    Ok(dims)
                }
                None => {
                    let dims = (image.width(), image.height());
                    image.save(&path).map_err(|e| e.to_string())?;
                    Ok(dims)
                }
            }
        })
        .await
        .map_err(|e| e.to_string())??;

    let hint = if region.is_some() {
        "user took a region screenshot via toolbar".to_string()
    } else {
        "user took a screenshot via toolbar".to_string()
    };

    let event = ActionEvent {
        id: Uuid::new_v4(),
        session_id,
        timestamp: now_ms(),
        session_offset_ms: offset_ms,
        observed_offset_ms: Some(offset_ms),
        duration_ms: None,
        action_type: ActionType::Screenshot,
        plugin_id: TOOLBAR_PLUGIN_ID.to_string(),
        payload: ActionPayload::Screenshot {
            image_path: screenshot_path.to_string_lossy().to_string(),
            width,
            height,
            ocr_text: None,
        },
        semantic_hint: Some(hint),
        confidence: 1.0,
        curation: TraceCuration::toolbar(),
    };

    sm.inject_event(event.clone())
        .await
        .map_err(|e| e.to_string())?;
    Ok(event)
}

/// Soft-delete a captured event from the active session's timeline.
///
/// Marks the event's `curation.deleted = true` in-memory so downstream
/// timeline summaries and prompt assembly skip it, and emits a
/// `PreviewEvent::ActionRemoved` so the widget pin disappears from the
/// track. The event itself stays in ActionTrack's vector so a
/// subsequent `stop()` still persists it to disk — soft-delete is an
/// intent filter, not a data-loss operation.
#[tauri::command]
pub async fn widget_trace_delete_event(
    state: State<'_, AppState>,
    event_id: String,
) -> Result<bool, String> {
    let uuid = Uuid::parse_str(&event_id).map_err(|e| format!("invalid event id: {e}"))?;
    state
        .session_manager
        .soft_delete_event(uuid)
        .await
        .map_err(|e| e.to_string())
}
