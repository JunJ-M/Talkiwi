use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CaptureStatus {
    Active,
    PermissionDenied,
    NotStarted,
    Stale,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CaptureHealthEntry {
    pub capture_id: String,
    pub status: CaptureStatus,
    pub event_count: usize,
    pub last_event_offset_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentTelemetry {
    pub session_id: Uuid,
    pub timestamp: u64,
    pub provider_latency_ms: u64,
    pub provider_success: bool,
    pub retry_count: u32,
    pub fallback_used: bool,
    pub schema_valid: bool,
    pub repair_attempted: bool,
    pub output_confidence: f32,
    pub reference_count: usize,
    pub low_confidence_refs: usize,
    pub intent_category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceTelemetry {
    pub session_id: Uuid,
    pub duration_ms: u64,
    pub segment_count: usize,
    pub event_count: usize,
    pub capture_health: Vec<CaptureHealthEntry>,
    pub event_density: f32,
    pub alignment_anomalies: usize,
}
