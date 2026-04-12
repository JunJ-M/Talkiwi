// Core types mirroring Rust talkiwi-core — keep in sync

export type KnownActionType =
  | "selection.text"
  | "screenshot"
  | "clipboard.change"
  | "page.current"
  | "click.link"
  | "file.attach";

// Allow plugin-defined action types while preserving known type narrowing
export type ActionType = KnownActionType | (string & {});

export type ClipboardContentType = "text" | "image" | "file";

export type SessionState =
  | "idle"
  | "recording"
  | "processing"
  | "ready"
  | { error: string };

export interface SpeakSegment {
  text: string;
  start_ms: number;
  end_ms: number;
  confidence: number;
  is_final: boolean;
}

export interface SelectionTextPayload {
  text: string;
  app_name: string;
  window_title: string;
  char_count: number;
}

export interface ScreenshotPayload {
  image_path: string;
  width: number;
  height: number;
  ocr_text?: string | null;
}

export interface ClipboardChangePayload {
  content_type: ClipboardContentType;
  text?: string | null;
  file_path?: string | null;
  source_app?: string | null;
}

export interface PageCurrentPayload {
  url?: string | null;
  title: string;
  app_name: string;
  bundle_id: string;
}

export interface ClickLinkPayload {
  from_url?: string | null;
  to_url: string;
  title?: string | null;
}

export interface FileAttachPayload {
  file_path: string;
  file_name: string;
  file_size: number;
  mime_type: string;
  preview?: string | null;
}

interface BaseActionEvent<TActionType extends ActionType, TPayload> {
  id: string;
  session_id: string;
  timestamp: number;
  session_offset_ms: number;
  duration_ms: number | null;
  action_type: TActionType;
  plugin_id: string;
  payload: TPayload;
  semantic_hint: string | null;
  confidence: number;
}

export type KnownActionEvent =
  | BaseActionEvent<"selection.text", SelectionTextPayload>
  | BaseActionEvent<"screenshot", ScreenshotPayload>
  | BaseActionEvent<"clipboard.change", ClipboardChangePayload>
  | BaseActionEvent<"page.current", PageCurrentPayload>
  | BaseActionEvent<"click.link", ClickLinkPayload>
  | BaseActionEvent<"file.attach", FileAttachPayload>;

export type CustomActionEvent = BaseActionEvent<
  Exclude<ActionType, KnownActionType>,
  Record<string, unknown>
>;

export type ActionEvent = KnownActionEvent | CustomActionEvent;

export interface ArtifactRef {
  event_id: string;
  label: string;
  inline_summary: string;
}

export type ReferenceStrategy =
  | "temporal_proximity"
  | "semantic_similarity"
  | "user_confirmed";

export interface Reference {
  spoken_text: string;
  spoken_offset: number;
  resolved_event_idx: number;
  resolved_event_id: string | null;
  confidence: number;
  strategy: ReferenceStrategy;
  user_confirmed: boolean;
}

export interface IntentOutput {
  session_id: string;
  task: string;
  intent: string;
  constraints: string[];
  missing_context: string[];
  restructured_speech: string;
  final_markdown: string;
  artifacts: ArtifactRef[];
  references: Reference[];
}

export interface SessionSummary {
  id: string;
  started_at: number;
  duration_ms: number;
  speak_segment_count: number;
  action_event_count: number;
  preview: string;
}

export interface Session {
  id: string;
  state: SessionState;
  started_at: number | null;
  ended_at: number | null;
  duration_ms: number | null;
}

export interface SessionDetail {
  session: Session;
  output: IntentOutput;
  segments: SpeakSegment[];
  events: ActionEvent[];
  audio_path: string | null;
}

export interface PermissionEntry {
  module: string;
  granted: boolean;
  description: string;
}

export interface PermissionReport {
  entries: PermissionEntry[];
}

export interface AppConfig {
  asr: {
    active_provider: string;
    whisper_model_path: string | null;
    whisper_model_size: string | null;
    language: string | null;
    beam_size: number;
    condition_on_previous_text: boolean;
    initial_prompt: string | null;
    vad_enabled: boolean;
    vad_threshold: number;
    vad_silence_timeout_ms: number;
    vad_min_speech_duration_ms: number;
    max_segment_ms: number;
  };
  intent: {
    active_provider: string;
    ollama_url: string;
    ollama_model: string;
    cloud_api_key: string | null;
  };
  capture: {
    selection_enabled: boolean;
    screenshot_enabled: boolean;
    clipboard_enabled: boolean;
    page_enabled: boolean;
    link_enabled: boolean;
    file_enabled: boolean;
    selection_poll_interval_ms: number;
    clipboard_poll_interval_ms: number;
    selection_min_chars: number;
  };
  ui: {
    panel_width: number;
    panel_side: string;
  };
  storage: {
    output_dir: string;
    db_path: string;
  };
}

export interface ModelStatusResponse {
  exists: boolean;
  path: string;
  size_name: string;
  file_size_bytes: number;
  download_url: string;
  expected_size_display: string;
}
