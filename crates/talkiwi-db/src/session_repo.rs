use rusqlite::{params, Connection, Error as SqlError};
use serde::de::DeserializeOwned;
use talkiwi_core::event::{ActionEvent, ActionPayload, ActionType};
use talkiwi_core::output::{
    ArtifactRef, IntentCategory, IntentOutput, Reference, ReferenceStrategy, RiskLevel,
};
use talkiwi_core::session::{Session, SessionState, SessionSummary, SpeakSegment};
use talkiwi_core::telemetry::{IntentTelemetry, TraceTelemetry};
use talkiwi_core::TalkiwiError;
use uuid::Uuid;

fn parse_uuid(s: &str) -> std::result::Result<Uuid, SqlError> {
    Uuid::parse_str(s).map_err(|e| {
        SqlError::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}

/// Complete session detail for history replay.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct SessionDetail {
    pub session: Session,
    pub output: IntentOutput,
    pub segments: Vec<SpeakSegment>,
    pub events: Vec<ActionEvent>,
    /// Path to the recorded WAV file, if available.
    pub audio_path: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct QualityOverview {
    pub intent_sessions: usize,
    pub trace_sessions: usize,
    pub avg_provider_latency_ms: f32,
    pub avg_output_confidence: f32,
    pub fallback_rate: f32,
    pub degraded_trace_rate: f32,
    pub latest_intent: Option<IntentTelemetry>,
    pub latest_trace: Option<TraceTelemetry>,
}

pub struct SessionRepo<'a> {
    conn: &'a Connection,
}

impl<'a> SessionRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Save a complete session with all related data in a single transaction.
    pub fn save_session(
        &self,
        session: &Session,
        output: &IntentOutput,
        segments: &[SpeakSegment],
        events: &[ActionEvent],
    ) -> talkiwi_core::Result<()> {
        self.save_session_with_audio(session, output, segments, events, None)
    }

    /// Save a complete session with optional audio path.
    pub fn save_session_with_audio(
        &self,
        session: &Session,
        output: &IntentOutput,
        segments: &[SpeakSegment],
        events: &[ActionEvent],
        audio_path: Option<&str>,
    ) -> talkiwi_core::Result<()> {
        // unchecked_transaction: SessionRepo borrows &Connection from a Mutex<Connection>
        // in the Tauri app layer, so exclusive access is guaranteed by the mutex.
        let tx = self.conn.unchecked_transaction().map_err(map_err)?;

        // 1. INSERT session
        tx.execute(
            "INSERT INTO sessions (id, started_at, ended_at, duration_ms, state, audio_path, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                session.id.to_string(),
                session.started_at,
                session.ended_at,
                session.duration_ms,
                serialize_state(&session.state),
                audio_path,
                session.started_at.unwrap_or_else(current_time_ms),
            ],
        ).map_err(map_err)?;

        // 2. INSERT intent_output
        let constraints_json = serde_json::to_string(&output.constraints)
            .map_err(|e| TalkiwiError::Serialization(e.to_string()))?;
        let missing_json = serde_json::to_string(&output.missing_context)
            .map_err(|e| TalkiwiError::Serialization(e.to_string()))?;
        let artifacts_json = serde_json::to_string(&output.artifacts)
            .map_err(|e| TalkiwiError::Serialization(e.to_string()))?;

        tx.execute(
            "INSERT INTO intent_outputs (session_id, task, intent, intent_category, constraints, missing_context, restructured_speech, final_markdown, artifacts_json, output_confidence, risk_level) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                session.id.to_string(),
                output.task,
                output.intent,
                serialize_intent_category(&output.intent_category),
                constraints_json,
                missing_json,
                output.restructured_speech,
                output.final_markdown,
                artifacts_json,
                output.output_confidence,
                serialize_risk_level(&output.risk_level),
            ],
        ).map_err(map_err)?;

        // 3. INSERT speak_segments
        for seg in segments {
            tx.execute(
                "INSERT INTO speak_segments (session_id, text, start_ms, end_ms, confidence, is_final) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    session.id.to_string(),
                    seg.text,
                    seg.start_ms,
                    seg.end_ms,
                    seg.confidence,
                    seg.is_final as i32,
                ],
            ).map_err(map_err)?;
        }

        // 4. INSERT action_events
        //
        // Always bind the session id from the `session` argument — the
        // event's own session_id may be stale (e.g., nil placeholder from
        // captures registered before any session existed). Using
        // `evt.session_id` here would violate the FOREIGN KEY on
        // action_events.session_id.
        let session_id_str = session.id.to_string();
        for evt in events {
            let payload_json = serde_json::to_string(&evt.payload)
                .map_err(|e| TalkiwiError::Serialization(e.to_string()))?;
            let curation_json = serde_json::to_string(&evt.curation)
                .map_err(|e| TalkiwiError::Serialization(e.to_string()))?;

            tx.execute(
                "INSERT INTO action_events (id, session_id, timestamp, session_offset_ms, observed_offset_ms, duration_ms, action_type, plugin_id, payload, semantic_hint, confidence, curation) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    evt.id.to_string(),
                    session_id_str,
                    evt.timestamp,
                    evt.session_offset_ms,
                    evt.observed_offset_ms,
                    evt.duration_ms,
                    evt.action_type.as_str(),
                    evt.plugin_id,
                    payload_json,
                    evt.semantic_hint,
                    evt.confidence,
                    curation_json,
                ],
            ).map_err(map_err)?;
        }

        // 5. INSERT references
        for r in &output.references {
            tx.execute(
                "INSERT INTO references_ (session_id, spoken_text, spoken_offset, resolved_event_idx, resolved_event_id, confidence, strategy, user_confirmed) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    session.id.to_string(),
                    r.spoken_text,
                    r.spoken_offset as i64,
                    r.resolved_event_idx as i64,
                    r.resolved_event_id.map(|id| id.to_string()),
                    r.confidence,
                    serialize_strategy(&r.strategy),
                    r.user_confirmed as i32,
                ],
            ).map_err(map_err)?;
        }

        tx.commit().map_err(map_err)?;
        Ok(())
    }

    pub fn save_intent_telemetry(&self, telemetry: &IntentTelemetry) -> talkiwi_core::Result<()> {
        self.conn
            .execute(
                "INSERT INTO intent_telemetry (session_id, timestamp, provider_latency_ms, provider_success, retry_count, fallback_used, schema_valid, repair_attempted, output_confidence, reference_count, low_confidence_refs, intent_category) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    telemetry.session_id.to_string(),
                    telemetry.timestamp,
                    telemetry.provider_latency_ms,
                    telemetry.provider_success as i32,
                    telemetry.retry_count,
                    telemetry.fallback_used as i32,
                    telemetry.schema_valid as i32,
                    telemetry.repair_attempted as i32,
                    telemetry.output_confidence,
                    telemetry.reference_count as i64,
                    telemetry.low_confidence_refs as i64,
                    telemetry.intent_category,
                ],
            )
            .map_err(map_err)?;
        Ok(())
    }

    pub fn save_trace_telemetry(&self, telemetry: &TraceTelemetry) -> talkiwi_core::Result<()> {
        let capture_health_json = serde_json::to_string(&telemetry.capture_health)
            .map_err(|e| TalkiwiError::Serialization(e.to_string()))?;

        self.conn
            .execute(
                "INSERT INTO trace_telemetry (session_id, duration_ms, segment_count, event_count, capture_health_json, event_density, alignment_anomalies) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    telemetry.session_id.to_string(),
                    telemetry.duration_ms,
                    telemetry.segment_count as i64,
                    telemetry.event_count as i64,
                    capture_health_json,
                    telemetry.event_density,
                    telemetry.alignment_anomalies as i64,
                ],
            )
            .map_err(map_err)?;
        Ok(())
    }

    /// Get full session detail for history replay.
    pub fn get_session_detail(&self, session_id: &str) -> talkiwi_core::Result<SessionDetail> {
        // 1. Session + audio_path
        let (session, audio_path) = self
            .conn
            .query_row(
                "SELECT id, started_at, ended_at, duration_ms, state, audio_path FROM sessions WHERE id = ?1",
                params![session_id],
                |row| {
                    let session = Session {
                        id: parse_uuid(&row.get::<_, String>(0)?)?,
                        started_at: row.get(1)?,
                        ended_at: row.get(2)?,
                        duration_ms: row.get(3)?,
                        state: parse_state(&row.get::<_, String>(4)?)?,
                    };
                    let audio_path: Option<String> = row.get(5)?;
                    Ok((session, audio_path))
                },
            )
            .map_err(map_err)?;

        // 2. IntentOutput
        let output = self.conn.query_row(
            "SELECT task, intent, intent_category, constraints, missing_context, restructured_speech, final_markdown, artifacts_json, output_confidence, risk_level FROM intent_outputs WHERE session_id = ?1",
            params![session_id],
            |row| {
                let constraints: Vec<String> =
                    parse_json_column(&row.get::<_, String>(3)?, "intent_outputs.constraints")?;
                let missing: Vec<String> =
                    parse_json_column(&row.get::<_, String>(4)?, "intent_outputs.missing_context")?;
                let artifacts: Vec<ArtifactRef> =
                    parse_json_column(&row.get::<_, String>(7)?, "intent_outputs.artifacts_json")?;

                Ok(IntentOutput {
                    session_id: session.id,
                    task: row.get(0)?,
                    intent: row.get(1)?,
                    intent_category: parse_intent_category(&row.get::<_, String>(2)?)?,
                    constraints,
                    missing_context: missing,
                    restructured_speech: row.get(5)?,
                    final_markdown: row.get(6)?,
                    artifacts,
                    references: vec![], // filled below
                    output_confidence: row.get(8)?,
                    risk_level: parse_risk_level(&row.get::<_, String>(9)?)?,
                })
            },
        ).map_err(map_err)?;

        // 3. SpeakSegments
        let mut seg_stmt = self.conn.prepare(
            "SELECT text, start_ms, end_ms, confidence, is_final FROM speak_segments WHERE session_id = ?1 ORDER BY start_ms"
        ).map_err(map_err)?;
        let segments: Vec<SpeakSegment> = seg_stmt
            .query_map(params![session_id], |row| {
                Ok(SpeakSegment {
                    text: row.get(0)?,
                    start_ms: row.get(1)?,
                    end_ms: row.get(2)?,
                    confidence: row.get(3)?,
                    is_final: row.get::<_, i32>(4)? != 0,
                })
            })
            .map_err(map_err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(map_err)?;

        // 4. ActionEvents
        let mut evt_stmt = self.conn.prepare(
            "SELECT id, session_id, timestamp, session_offset_ms, observed_offset_ms, duration_ms, action_type, plugin_id, payload, semantic_hint, confidence, curation FROM action_events WHERE session_id = ?1 ORDER BY session_offset_ms"
        ).map_err(map_err)?;
        let events: Vec<ActionEvent> = evt_stmt
            .query_map(params![session_id], |row| {
                let payload_str: String = row.get(8)?;
                let payload: ActionPayload =
                    parse_json_column(&payload_str, "action_events.payload")?;
                let action_type_str: String = row.get(6)?;

                // `curation` is an additive column. Rows written before
                // migration 002 have the `DEFAULT '{}'` value, which
                // deserializes into `TraceCuration::default()`.
                let curation_str: String = row.get(11)?;
                let curation: talkiwi_core::event::TraceCuration =
                    parse_json_column(&curation_str, "action_events.curation")?;

                Ok(ActionEvent {
                    id: parse_uuid(&row.get::<_, String>(0)?)?,
                    session_id: parse_uuid(&row.get::<_, String>(1)?)?,
                    timestamp: row.get(2)?,
                    session_offset_ms: row.get(3)?,
                    observed_offset_ms: row.get(4)?,
                    duration_ms: row.get(5)?,
                    action_type: ActionType::from_str_name(&action_type_str),
                    plugin_id: row.get(7)?,
                    payload,
                    semantic_hint: row.get(9)?,
                    confidence: row.get(10)?,
                    curation,
                })
            })
            .map_err(map_err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(map_err)?;

        // 5. References
        let mut ref_stmt = self.conn.prepare(
            "SELECT spoken_text, spoken_offset, resolved_event_idx, resolved_event_id, confidence, strategy, user_confirmed FROM references_ WHERE session_id = ?1 ORDER BY id"
        ).map_err(map_err)?;
        let references: Vec<Reference> = ref_stmt
            .query_map(params![session_id], |row| {
                let event_id_str: Option<String> = row.get(3)?;
                Ok(Reference {
                    spoken_text: row.get(0)?,
                    spoken_offset: row.get::<_, i64>(1)? as usize,
                    resolved_event_idx: row.get::<_, i64>(2)? as usize,
                    resolved_event_id: event_id_str.and_then(|s| Uuid::parse_str(&s).ok()),
                    confidence: row.get(4)?,
                    strategy: parse_strategy(&row.get::<_, String>(5)?)?,
                    user_confirmed: row.get::<_, i32>(6)? != 0,
                })
            })
            .map_err(map_err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(map_err)?;

        let output = IntentOutput {
            references,
            ..output
        };

        Ok(SessionDetail {
            session,
            output,
            segments,
            events,
            audio_path,
        })
    }

    /// List session summaries for history view.
    pub fn list_sessions(
        &self,
        limit: usize,
        offset: usize,
    ) -> talkiwi_core::Result<Vec<SessionSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.started_at, s.duration_ms,
                    (SELECT COUNT(*) FROM speak_segments WHERE session_id = s.id) as seg_count,
                    (SELECT COUNT(*) FROM action_events WHERE session_id = s.id) as evt_count,
                    COALESCE((SELECT SUBSTR(final_markdown, 1, 200) FROM intent_outputs WHERE session_id = s.id), '')
             FROM sessions s
             ORDER BY s.created_at DESC, s.id DESC
             LIMIT ?1 OFFSET ?2"
        ).map_err(map_err)?;

        let summaries = stmt
            .query_map(params![limit as i64, offset as i64], |row| {
                Ok(SessionSummary {
                    id: parse_uuid(&row.get::<_, String>(0)?)?,
                    started_at: row.get::<_, Option<u64>>(1)?.unwrap_or(0),
                    duration_ms: row.get::<_, Option<u64>>(2)?.unwrap_or(0),
                    speak_segment_count: row.get::<_, i64>(3)? as usize,
                    action_event_count: row.get::<_, i64>(4)? as usize,
                    preview: row.get(5)?,
                })
            })
            .map_err(map_err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(map_err)?;

        Ok(summaries)
    }

    pub fn quality_overview(&self, limit: usize) -> talkiwi_core::Result<QualityOverview> {
        let intent_rows = self.load_intent_telemetry(limit)?;
        let trace_rows = self.load_trace_telemetry(limit)?;

        let avg_provider_latency_ms = if intent_rows.is_empty() {
            0.0
        } else {
            intent_rows
                .iter()
                .map(|row| row.provider_latency_ms as f32)
                .sum::<f32>()
                / intent_rows.len() as f32
        };

        let avg_output_confidence = if intent_rows.is_empty() {
            0.0
        } else {
            intent_rows
                .iter()
                .map(|row| row.output_confidence)
                .sum::<f32>()
                / intent_rows.len() as f32
        };

        let fallback_rate = if intent_rows.is_empty() {
            0.0
        } else {
            intent_rows.iter().filter(|row| row.fallback_used).count() as f32
                / intent_rows.len() as f32
        };

        let degraded_trace_rate = if trace_rows.is_empty() {
            0.0
        } else {
            trace_rows
                .iter()
                .filter(|row| {
                    row.capture_health.iter().any(|entry| {
                        matches!(
                            entry.status,
                            talkiwi_core::telemetry::CaptureStatus::PermissionDenied
                                | talkiwi_core::telemetry::CaptureStatus::Stale
                                | talkiwi_core::telemetry::CaptureStatus::Error
                        )
                    })
                })
                .count() as f32
                / trace_rows.len() as f32
        };

        Ok(QualityOverview {
            intent_sessions: intent_rows.len(),
            trace_sessions: trace_rows.len(),
            avg_provider_latency_ms,
            avg_output_confidence,
            fallback_rate,
            degraded_trace_rate,
            latest_intent: intent_rows.into_iter().next(),
            latest_trace: trace_rows.into_iter().next(),
        })
    }

    fn load_intent_telemetry(&self, limit: usize) -> talkiwi_core::Result<Vec<IntentTelemetry>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT session_id, timestamp, provider_latency_ms, provider_success, retry_count, fallback_used, schema_valid, repair_attempted, output_confidence, reference_count, low_confidence_refs, intent_category
                 FROM intent_telemetry
                 ORDER BY id DESC
                 LIMIT ?1",
            )
            .map_err(map_err)?;

        let rows = stmt
            .query_map(params![limit as i64], |row| {
                Ok(IntentTelemetry {
                    session_id: parse_uuid(&row.get::<_, String>(0)?)?,
                    timestamp: row.get(1)?,
                    provider_latency_ms: row.get(2)?,
                    provider_success: row.get::<_, i32>(3)? != 0,
                    retry_count: row.get::<_, i64>(4)? as u32,
                    fallback_used: row.get::<_, i32>(5)? != 0,
                    schema_valid: row.get::<_, i32>(6)? != 0,
                    repair_attempted: row.get::<_, i32>(7)? != 0,
                    output_confidence: row.get(8)?,
                    reference_count: row.get::<_, i64>(9)? as usize,
                    low_confidence_refs: row.get::<_, i64>(10)? as usize,
                    intent_category: row.get(11)?,
                })
            })
            .map_err(map_err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(map_err)?;

        Ok(rows)
    }

    fn load_trace_telemetry(&self, limit: usize) -> talkiwi_core::Result<Vec<TraceTelemetry>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT session_id, duration_ms, segment_count, event_count, capture_health_json, event_density, alignment_anomalies
                 FROM trace_telemetry
                 ORDER BY id DESC
                 LIMIT ?1",
            )
            .map_err(map_err)?;

        let rows = stmt
            .query_map(params![limit as i64], |row| {
                let capture_health_json: String = row.get(4)?;
                Ok(TraceTelemetry {
                    session_id: parse_uuid(&row.get::<_, String>(0)?)?,
                    duration_ms: row.get(1)?,
                    segment_count: row.get::<_, i64>(2)? as usize,
                    event_count: row.get::<_, i64>(3)? as usize,
                    capture_health: parse_json_column(
                        &capture_health_json,
                        "trace_telemetry.capture_health_json",
                    )?,
                    event_density: row.get(5)?,
                    alignment_anomalies: row.get::<_, i64>(6)? as usize,
                })
            })
            .map_err(map_err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(map_err)?;

        Ok(rows)
    }
}

fn map_err(e: rusqlite::Error) -> TalkiwiError {
    TalkiwiError::Storage(e.to_string())
}

fn serialize_state(state: &SessionState) -> String {
    match state {
        SessionState::Idle => "idle".to_string(),
        SessionState::Recording => "recording".to_string(),
        SessionState::Processing => "processing".to_string(),
        SessionState::Ready => "ready".to_string(),
        SessionState::Error(msg) => format!("error:{}", msg),
    }
}

fn parse_state(s: &str) -> std::result::Result<SessionState, SqlError> {
    match s {
        "idle" => Ok(SessionState::Idle),
        "recording" => Ok(SessionState::Recording),
        "processing" => Ok(SessionState::Processing),
        "ready" => Ok(SessionState::Ready),
        other if other.starts_with("error:") => Ok(SessionState::Error(other[6..].to_string())),
        other => Err(invalid_data_err(format!(
            "invalid sessions.state value: {}",
            other
        ))),
    }
}

fn serialize_strategy(s: &ReferenceStrategy) -> &'static str {
    match s {
        ReferenceStrategy::TemporalProximity => "temporal_proximity",
        ReferenceStrategy::SemanticSimilarity => "semantic_similarity",
        ReferenceStrategy::UserConfirmed => "user_confirmed",
    }
}

fn parse_strategy(s: &str) -> std::result::Result<ReferenceStrategy, SqlError> {
    match s {
        "temporal_proximity" => Ok(ReferenceStrategy::TemporalProximity),
        "semantic_similarity" => Ok(ReferenceStrategy::SemanticSimilarity),
        "user_confirmed" => Ok(ReferenceStrategy::UserConfirmed),
        other => Err(invalid_data_err(format!(
            "invalid references_.strategy value: {}",
            other
        ))),
    }
}

fn serialize_intent_category(category: &IntentCategory) -> &'static str {
    match category {
        IntentCategory::Rewrite => "rewrite",
        IntentCategory::Analyze => "analyze",
        IntentCategory::Summarize => "summarize",
        IntentCategory::Generate => "generate",
        IntentCategory::Debug => "debug",
        IntentCategory::Query => "query",
        IntentCategory::Unknown => "unknown",
    }
}

fn parse_intent_category(s: &str) -> std::result::Result<IntentCategory, SqlError> {
    match s {
        "rewrite" => Ok(IntentCategory::Rewrite),
        "analyze" => Ok(IntentCategory::Analyze),
        "summarize" => Ok(IntentCategory::Summarize),
        "generate" => Ok(IntentCategory::Generate),
        "debug" => Ok(IntentCategory::Debug),
        "query" => Ok(IntentCategory::Query),
        "unknown" => Ok(IntentCategory::Unknown),
        other => Err(invalid_data_err(format!(
            "invalid intent_outputs.intent_category value: {}",
            other
        ))),
    }
}

fn serialize_risk_level(level: &RiskLevel) -> &'static str {
    match level {
        RiskLevel::Low => "low",
        RiskLevel::Medium => "medium",
        RiskLevel::High => "high",
    }
}

fn parse_risk_level(s: &str) -> std::result::Result<RiskLevel, SqlError> {
    match s {
        "low" => Ok(RiskLevel::Low),
        "medium" => Ok(RiskLevel::Medium),
        "high" => Ok(RiskLevel::High),
        other => Err(invalid_data_err(format!(
            "invalid intent_outputs.risk_level value: {}",
            other
        ))),
    }
}

fn parse_json_column<T>(raw: &str, column: &'static str) -> std::result::Result<T, SqlError>
where
    T: DeserializeOwned,
{
    serde_json::from_str(raw)
        .map_err(|e| invalid_data_err(format!("invalid JSON in {}: {}", column, e)))
}

fn invalid_data_err(message: String) -> SqlError {
    SqlError::FromSqlConversionFailure(
        0,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message,
        )),
    )
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use talkiwi_core::event::ActionPayload;
    use talkiwi_core::output::ArtifactRef;

    fn setup() -> Connection {
        crate::init_database_memory().unwrap()
    }

    fn make_test_session() -> (Session, IntentOutput, Vec<SpeakSegment>, Vec<ActionEvent>) {
        let session_id = Uuid::new_v4();
        let event_id = Uuid::new_v4();

        let session = Session {
            id: session_id,
            state: SessionState::Ready,
            started_at: Some(1712900000000),
            ended_at: Some(1712900010000),
            duration_ms: Some(10000),
        };

        let segments = vec![
            SpeakSegment {
                text: "rewrite this function".to_string(),
                start_ms: 0,
                end_ms: 2000,
                confidence: 0.95,
                is_final: true,
            },
            SpeakSegment {
                text: "use Rust".to_string(),
                start_ms: 2500,
                end_ms: 3500,
                confidence: 0.88,
                is_final: true,
            },
        ];

        let events = vec![ActionEvent {
            id: event_id,
            session_id,
            timestamp: 1712900001000,
            session_offset_ms: 1000,
            observed_offset_ms: Some(1000),
            duration_ms: None,
            action_type: ActionType::SelectionText,
            plugin_id: "builtin".to_string(),
            payload: ActionPayload::SelectionText {
                text: "fn old_function() {}".to_string(),
                app_name: "VSCode".to_string(),
                window_title: "main.rs".to_string(),
                char_count: 20,
            },
            semantic_hint: Some("selected code".to_string()),
            confidence: 1.0,
            curation: Default::default(),
        }];

        let output = IntentOutput {
            session_id,
            task: "Rewrite the function in Rust".to_string(),
            intent: "rewrite".to_string(),
            intent_category: IntentCategory::Rewrite,
            constraints: vec!["use Rust".to_string(), "keep the same API".to_string()],
            missing_context: vec!["which specific function".to_string()],
            restructured_speech: "Please rewrite the selected function using Rust".to_string(),
            final_markdown: "## Task\nRewrite the function in Rust\n\n## Context\n### context-1\nSelected code in VSCode".to_string(),
            artifacts: vec![ArtifactRef {
                event_id,
                label: "context-1".to_string(),
                inline_summary: "Selected code in VSCode: fn old_function() {}".to_string(),
            }],
            references: vec![Reference {
                spoken_text: "this function".to_string(),
                spoken_offset: 8,
                resolved_event_idx: 0,
                resolved_event_id: Some(event_id),
                confidence: 0.9,
                strategy: ReferenceStrategy::TemporalProximity,
                user_confirmed: false,
            }],
            output_confidence: 0.87,
            risk_level: RiskLevel::Low,
        };

        (session, output, segments, events)
    }

    #[test]
    fn schema_creates_all_tables() {
        let conn = setup();
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect();

        assert!(tables.contains(&"sessions".to_string()));
        assert!(tables.contains(&"intent_outputs".to_string()));
        assert!(tables.contains(&"speak_segments".to_string()));
        assert!(tables.contains(&"action_events".to_string()));
        assert!(tables.contains(&"references_".to_string()));
        assert!(tables.contains(&"intent_telemetry".to_string()));
        assert!(tables.contains(&"trace_telemetry".to_string()));
    }

    #[test]
    fn full_round_trip() {
        let conn = setup();
        let repo = SessionRepo::new(&conn);
        let (session, output, segments, events) = make_test_session();

        repo.save_session(&session, &output, &segments, &events)
            .unwrap();

        let detail = repo.get_session_detail(&session.id.to_string()).unwrap();

        // Session
        assert_eq!(detail.session.id, session.id);
        assert_eq!(detail.session.state, SessionState::Ready);
        assert_eq!(detail.session.started_at, Some(1712900000000));
        assert_eq!(detail.session.duration_ms, Some(10000));

        // IntentOutput
        assert_eq!(detail.output.task, "Rewrite the function in Rust");
        assert_eq!(detail.output.intent, "rewrite");
        assert_eq!(detail.output.intent_category, IntentCategory::Rewrite);
        assert_eq!(detail.output.constraints.len(), 2);
        assert_eq!(detail.output.missing_context.len(), 1);
        assert!(detail.output.final_markdown.contains("## Task"));
        assert!((detail.output.output_confidence - 0.87).abs() < 0.001);
        assert_eq!(detail.output.risk_level, RiskLevel::Low);

        // Artifacts round-trip
        assert_eq!(detail.output.artifacts.len(), 1);
        assert_eq!(detail.output.artifacts[0].label, "context-1");
        assert_eq!(detail.output.artifacts[0].event_id, events[0].id);

        // SpeakSegments
        assert_eq!(detail.segments.len(), 2);
        assert_eq!(detail.segments[0].text, "rewrite this function");
        assert_eq!(detail.segments[0].start_ms, 0);
        assert_eq!(detail.segments[1].text, "use Rust");
        assert!(detail.segments[1].is_final);

        // ActionEvents - UUID round-trip
        assert_eq!(detail.events.len(), 1);
        assert_eq!(detail.events[0].id, events[0].id);
        assert_eq!(detail.events[0].action_type, ActionType::SelectionText);
        assert_eq!(detail.events[0].plugin_id, "builtin");
        assert_eq!(detail.events[0].session_offset_ms, 1000);
        assert_eq!(detail.events[0].observed_offset_ms, Some(1000));

        // ActionPayload JSON round-trip
        if let ActionPayload::SelectionText {
            text, char_count, ..
        } = &detail.events[0].payload
        {
            assert_eq!(text, "fn old_function() {}");
            assert_eq!(*char_count, 20);
        } else {
            panic!("Expected SelectionText payload");
        }

        // References - full field round-trip
        assert_eq!(detail.output.references.len(), 1);
        let r = &detail.output.references[0];
        assert_eq!(r.spoken_text, "this function");
        assert_eq!(r.spoken_offset, 8);
        assert_eq!(r.resolved_event_idx, 0);
        assert_eq!(r.resolved_event_id, Some(events[0].id));
        assert!((r.confidence - 0.9).abs() < 0.01);
        assert_eq!(r.strategy, ReferenceStrategy::TemporalProximity);
        assert!(!r.user_confirmed);
    }

    #[test]
    fn curation_round_trips_through_db() {
        use talkiwi_core::event::{TraceCuration, TraceRole, TraceSource};

        let conn = setup();
        let repo = SessionRepo::new(&conn);
        let (mut session, mut output, segments, mut events) = make_test_session();
        session.id = Uuid::new_v4();
        output.session_id = session.id;
        for evt in &mut events {
            evt.session_id = session.id;
            evt.curation = TraceCuration {
                source: TraceSource::Toolbar,
                role: Some(TraceRole::Issue),
                user_note: Some("ring any bell?".to_string()),
                deleted: false,
            };
        }

        repo.save_session(&session, &output, &segments, &events)
            .unwrap();

        let detail = repo.get_session_detail(&session.id.to_string()).unwrap();
        assert_eq!(detail.events.len(), 1);
        assert_eq!(detail.events[0].curation.source, TraceSource::Toolbar);
        assert_eq!(detail.events[0].curation.role, Some(TraceRole::Issue));
        assert_eq!(
            detail.events[0].curation.user_note.as_deref(),
            Some("ring any bell?"),
        );
        assert!(!detail.events[0].curation.deleted);
    }

    #[test]
    fn list_sessions_pagination() {
        let conn = setup();
        let repo = SessionRepo::new(&conn);

        // Insert 3 sessions
        for i in 0..3 {
            let mut data = make_test_session();
            data.0.id = Uuid::new_v4();
            data.0.started_at = Some(1712900000000 + i * 1000);
            data.1.session_id = data.0.id;
            for seg in &mut data.2 {
                // segments don't have session_id, nothing to update
                let _ = seg;
            }
            for evt in &mut data.3 {
                evt.session_id = data.0.id;
                evt.id = Uuid::new_v4();
            }
            data.1.artifacts = vec![];
            data.1.references = vec![];
            repo.save_session(&data.0, &data.1, &data.2, &data.3)
                .unwrap();
        }

        let all = repo.list_sessions(10, 0).unwrap();
        assert_eq!(all.len(), 3);

        let page1 = repo.list_sessions(2, 0).unwrap();
        assert_eq!(page1.len(), 2);

        let page2 = repo.list_sessions(2, 2).unwrap();
        assert_eq!(page2.len(), 1);
    }

    #[test]
    fn list_sessions_includes_counts_and_preview() {
        let conn = setup();
        let repo = SessionRepo::new(&conn);
        let (session, output, segments, events) = make_test_session();

        repo.save_session(&session, &output, &segments, &events)
            .unwrap();

        let summaries = repo.list_sessions(10, 0).unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].speak_segment_count, 2);
        assert_eq!(summaries[0].action_event_count, 1);
        assert!(!summaries[0].preview.is_empty());
    }

    #[test]
    fn telemetry_tables_accept_rows() {
        let conn = setup();
        let repo = SessionRepo::new(&conn);
        let (session, output, segments, events) = make_test_session();

        repo.save_session(&session, &output, &segments, &events)
            .unwrap();
        repo.save_intent_telemetry(&IntentTelemetry {
            session_id: session.id,
            timestamp: 123,
            provider_latency_ms: 456,
            provider_success: true,
            retry_count: 1,
            fallback_used: false,
            schema_valid: true,
            repair_attempted: true,
            output_confidence: 0.88,
            reference_count: 1,
            low_confidence_refs: 0,
            intent_category: "rewrite".to_string(),
        })
        .unwrap();
        repo.save_trace_telemetry(&TraceTelemetry {
            session_id: session.id,
            duration_ms: 1_000,
            segment_count: 2,
            event_count: 1,
            capture_health: vec![],
            event_density: 0.1,
            alignment_anomalies: 0,
        })
        .unwrap();

        let intent_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM intent_telemetry", [], |row| {
                row.get(0)
            })
            .unwrap();
        let trace_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM trace_telemetry", [], |row| row.get(0))
            .unwrap();

        assert_eq!(intent_count, 1);
        assert_eq!(trace_count, 1);
    }

    #[test]
    fn quality_overview_aggregates_recent_rows() {
        let conn = setup();
        let repo = SessionRepo::new(&conn);
        let (session, output, segments, events) = make_test_session();

        repo.save_session(&session, &output, &segments, &events)
            .unwrap();
        repo.save_intent_telemetry(&IntentTelemetry {
            session_id: session.id,
            timestamp: 100,
            provider_latency_ms: 800,
            provider_success: true,
            retry_count: 1,
            fallback_used: false,
            schema_valid: true,
            repair_attempted: true,
            output_confidence: 0.8,
            reference_count: 1,
            low_confidence_refs: 0,
            intent_category: "rewrite".to_string(),
        })
        .unwrap();
        repo.save_trace_telemetry(&TraceTelemetry {
            session_id: session.id,
            duration_ms: 2_000,
            segment_count: 2,
            event_count: 1,
            capture_health: vec![talkiwi_core::telemetry::CaptureHealthEntry {
                capture_id: "builtin.focus".to_string(),
                status: talkiwi_core::telemetry::CaptureStatus::PermissionDenied,
                event_count: 0,
                last_event_offset_ms: None,
            }],
            event_density: 0.5,
            alignment_anomalies: 0,
        })
        .unwrap();

        let overview = repo.quality_overview(10).unwrap();
        assert_eq!(overview.intent_sessions, 1);
        assert_eq!(overview.trace_sessions, 1);
        assert!((overview.avg_provider_latency_ms - 800.0).abs() < f32::EPSILON);
        assert!((overview.avg_output_confidence - 0.8).abs() < f32::EPSILON);
        assert_eq!(overview.fallback_rate, 0.0);
        assert_eq!(overview.degraded_trace_rate, 1.0);
        assert!(overview.latest_intent.is_some());
        assert!(overview.latest_trace.is_some());
    }

    #[test]
    fn session_state_round_trip() {
        assert_eq!(
            parse_state(&serialize_state(&SessionState::Idle)).unwrap(),
            SessionState::Idle
        );
        assert_eq!(
            parse_state(&serialize_state(&SessionState::Recording)).unwrap(),
            SessionState::Recording
        );
        assert_eq!(
            parse_state(&serialize_state(&SessionState::Ready)).unwrap(),
            SessionState::Ready
        );
        assert_eq!(
            parse_state(&serialize_state(&SessionState::Error("test".to_string()))).unwrap(),
            SessionState::Error("test".to_string())
        );
    }

    #[test]
    fn history_detail_fails_on_invalid_json() {
        let conn = setup();
        let repo = SessionRepo::new(&conn);
        let (session, output, segments, events) = make_test_session();

        repo.save_session(&session, &output, &segments, &events)
            .unwrap();
        conn.execute(
            "UPDATE intent_outputs SET constraints = 'not-json' WHERE session_id = ?1",
            params![session.id.to_string()],
        )
        .unwrap();

        let err = repo
            .get_session_detail(&session.id.to_string())
            .unwrap_err();
        match err {
            TalkiwiError::Storage(message) => {
                assert!(message.contains("intent_outputs.constraints"))
            }
            other => panic!("unexpected error: {:?}", other),
        }
    }

    #[test]
    fn list_sessions_order_is_stable_for_equal_created_at() {
        let conn = setup();
        let repo = SessionRepo::new(&conn);
        let base_started_at = Some(1712900000000);

        for (session_id, suffix) in [
            ("00000000-0000-0000-0000-000000000001", "one"),
            ("00000000-0000-0000-0000-000000000002", "two"),
        ] {
            let mut data = make_test_session();
            data.0.id = Uuid::parse_str(session_id).unwrap();
            data.0.started_at = base_started_at;
            data.1.session_id = data.0.id;
            data.1.final_markdown = format!("session {}", suffix);
            for evt in &mut data.3 {
                evt.session_id = data.0.id;
                evt.id = Uuid::new_v4();
            }
            data.1.artifacts = vec![];
            data.1.references = vec![];
            repo.save_session(&data.0, &data.1, &data.2, &data.3)
                .unwrap();
        }

        let summaries = repo.list_sessions(10, 0).unwrap();
        let ids: Vec<String> = summaries.iter().map(|s| s.id.to_string()).collect();
        assert_eq!(
            ids,
            vec![
                "00000000-0000-0000-0000-000000000002".to_string(),
                "00000000-0000-0000-0000-000000000001".to_string(),
            ]
        );
    }
}
