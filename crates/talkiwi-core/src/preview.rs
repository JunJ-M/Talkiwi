use serde::{Deserialize, Serialize};

use crate::event::TraceSource;
use crate::session::{SessionState, SpeakSegment};
use crate::telemetry::CaptureHealthEntry;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AudioInputInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sample_rates: Vec<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channels: Vec<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WidgetActionPin {
    pub id: String,
    pub t: u64,
    #[serde(rename = "type")]
    pub action_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
    /// Mirrors `ActionEvent.curation.source`. Lets the widget style
    /// user-captured pins (toolbar/manual) distinctly from passive ones.
    #[serde(default)]
    pub source: TraceSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WidgetTranscriptState {
    pub partial_text: Option<String>,
    pub final_segments: Vec<SpeakSegment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WidgetHealthState {
    pub capture_status: Vec<CaptureHealthEntry>,
    pub degraded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WidgetSnapshot {
    pub session_state: SessionState,
    pub elapsed_ms: u64,
    pub mic: Option<AudioInputInfo>,
    pub audio_bins: Vec<f32>,
    pub speech_bins: Vec<f32>,
    pub action_pins: Vec<WidgetActionPin>,
    pub transcript: WidgetTranscriptState,
    pub health: WidgetHealthState,
}

#[derive(Debug, Clone)]
pub enum PreviewEvent {
    SessionStateChanged(SessionState),
    MicSelected(Option<AudioInputInfo>),
    AudioLevel {
        offset_ms: u64,
        rms: f32,
        peak: f32,
        vad_active: bool,
    },
    TranscriptPartial {
        start_ms: u64,
        end_ms: u64,
        text: String,
    },
    TranscriptFinal(SpeakSegment),
    ActionOccurred {
        id: String,
        offset_ms: u64,
        action_type: String,
        source: TraceSource,
    },
    /// User soft-deleted an event from the widget timeline. The preview
    /// pipeline drops the matching pin; the underlying event stays in
    /// ActionTrack tagged as `curation.deleted = true`.
    ActionRemoved {
        id: String,
    },
    CaptureHealthUpdated(Vec<CaptureHealthEntry>),
    Reset,
}
