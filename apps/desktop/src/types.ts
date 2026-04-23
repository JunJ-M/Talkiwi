// Core types mirroring Rust talkiwi-core — keep in sync

export type KnownActionType =
  | "selection.text"
  | "screenshot"
  | "clipboard.change"
  | "page.current"
  | "click.link"
  | "file.attach"
  | "window.focus"
  | "click.mouse"
  | "manual.note";

// Allow plugin-defined action types while preserving known type narrowing
export type ActionType = KnownActionType | (string & {});

// Trace curation — matches talkiwi-core::event::TraceCuration
export type TraceSource = "passive" | "toolbar" | "manual";
export type TraceRole = "issue" | "target" | "expected" | "reference";

export interface TraceCuration {
  source: TraceSource;
  role?: TraceRole | null;
  user_note?: string | null;
  deleted?: boolean;
}

// Permission modules surfaced by permissions_check
export type TracePermissionModule =
  | "accessibility"
  | "screen_recording"
  | "microphone";

export type TracePermissionMatrix = Record<TracePermissionModule, boolean>;

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

export interface WindowFocusPayload {
  app_name: string;
  window_title: string;
}

export interface ClickMousePayload {
  app_name?: string | null;
  window_title?: string | null;
  button: string;
  x: number;
  y: number;
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
  observed_offset_ms?: number | null;
  duration_ms: number | null;
  action_type: TActionType;
  plugin_id: string;
  payload: TPayload;
  semantic_hint: string | null;
  confidence: number;
  /** Curation metadata — added 2026-04-16. Old snapshots may omit it. */
  curation?: TraceCuration;
}

export type KnownActionEvent =
  | BaseActionEvent<"selection.text", SelectionTextPayload>
  | BaseActionEvent<"screenshot", ScreenshotPayload>
  | BaseActionEvent<"clipboard.change", ClipboardChangePayload>
  | BaseActionEvent<"page.current", PageCurrentPayload>
  | BaseActionEvent<"click.link", ClickLinkPayload>
  | BaseActionEvent<"file.attach", FileAttachPayload>
  | BaseActionEvent<"window.focus", WindowFocusPayload>
  | BaseActionEvent<"click.mouse", ClickMousePayload>;

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
  | "user_confirmed"
  | "llm_coreference"
  | "anchor_propagation";

export type RefRelation =
  | "single"
  | "composition"
  | "contrast"
  | "subtraction"
  | "unknown";

export type TargetRole =
  | "source"
  | "style"
  | "feature"
  | "excluded_aspect"
  | "preserve_scope"
  | "user_anchor"
  | "unknown";

export interface RefTarget {
  event_id: string;
  event_idx: number;
  role?: TargetRole;
  via_anchor?: string | null;
}

export interface Reference {
  spoken_text: string;
  spoken_offset: number;
  resolved_event_idx: number;
  resolved_event_id: string | null;
  confidence: number;
  strategy: ReferenceStrategy;
  user_confirmed: boolean;
  /** Present on sessions produced by the trace annotation engine. */
  targets?: RefTarget[];
  relation?: RefRelation;
}

export interface RetrievalChunk {
  event_id: string;
  session_id: string;
  session_offset_ms: number;
  action_type: string;
  text: string;
  referenced_by_segments?: number[];
  importance: number;
  tags?: string[];
}

export interface IntentOutput {
  session_id: string;
  task: string;
  intent: string;
  intent_category:
    | "rewrite"
    | "analyze"
    | "summarize"
    | "generate"
    | "debug"
    | "query"
    | "unknown";
  constraints: string[];
  missing_context: string[];
  restructured_speech: string;
  final_markdown: string;
  artifacts: ArtifactRef[];
  references: Reference[];
  output_confidence: number;
  risk_level: "low" | "medium" | "high";
  /** Present on sessions produced by the trace annotation engine. */
  retrieval_chunks?: RetrievalChunk[];
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
  audio: {
    input_device_id: string | null;
    input_device_name: string | null;
  };
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
    input_gain_db: number;
    cloud_api_key: string | null;
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

export interface AudioInputInfo {
  id: string;
  name: string;
  is_default: boolean;
  sample_rates: number[];
  channels: number[];
}

export type CaptureStatus =
  | "active"
  | "permission_denied"
  | "not_started"
  | "stale"
  | "error";

export interface CaptureHealthEntry {
  capture_id: string;
  status: CaptureStatus;
  event_count: number;
  last_event_offset_ms?: number | null;
}

export interface WidgetActionPin {
  id: string;
  t: number;
  type: string;
  count?: number | null;
  /** Mirrors `ActionEvent.curation.source` so pins can be styled distinctly.
   * Defaults to "passive" when the backend omits the field. */
  source?: TraceSource;
}

export interface WidgetTranscriptState {
  partial_text?: string | null;
  final_segments: SpeakSegment[];
}

export interface WidgetHealthState {
  capture_status: CaptureHealthEntry[];
  degraded: boolean;
}

export interface WidgetSnapshot {
  session_state: SessionState;
  elapsed_ms: number;
  mic?: AudioInputInfo | null;
  audio_bins: number[];
  speech_bins: number[];
  action_pins: WidgetActionPin[];
  transcript: WidgetTranscriptState;
  health: WidgetHealthState;
}

export interface IntentTelemetry {
  session_id: string;
  timestamp: number;
  provider_latency_ms: number;
  provider_success: boolean;
  retry_count: number;
  fallback_used: boolean;
  schema_valid: boolean;
  repair_attempted: boolean;
  output_confidence: number;
  reference_count: number;
  low_confidence_refs: number;
  intent_category: string;
}

export interface TraceTelemetry {
  session_id: string;
  duration_ms: number;
  segment_count: number;
  event_count: number;
  capture_health: CaptureHealthEntry[];
  event_density: number;
  alignment_anomalies: number;
}

export interface QualityOverview {
  intent_sessions: number;
  trace_sessions: number;
  avg_provider_latency_ms: number;
  avg_output_confidence: number;
  fallback_rate: number;
  degraded_trace_rate: number;
  latest_intent?: IntentTelemetry | null;
  latest_trace?: TraceTelemetry | null;
}

export interface ModelStatusResponse {
  exists: boolean;
  path: string;
  size_name: string;
  file_size_bytes: number;
  download_url: string;
  expected_size_display: string;
}
