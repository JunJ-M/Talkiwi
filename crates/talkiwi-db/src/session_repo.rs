use rusqlite::{params, Connection, Error as SqlError};
use serde::de::DeserializeOwned;
use talkiwi_core::event::{ActionEvent, ActionPayload, ActionType};
use talkiwi_core::output::{ArtifactRef, IntentOutput, Reference, ReferenceStrategy};
use talkiwi_core::session::{Session, SessionState, SessionSummary, SpeakSegment};
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
            "INSERT INTO intent_outputs (session_id, task, intent, constraints, missing_context, restructured_speech, final_markdown, artifacts_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                session.id.to_string(),
                output.task,
                output.intent,
                constraints_json,
                missing_json,
                output.restructured_speech,
                output.final_markdown,
                artifacts_json,
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
        for evt in events {
            let payload_json = serde_json::to_string(&evt.payload)
                .map_err(|e| TalkiwiError::Serialization(e.to_string()))?;

            tx.execute(
                "INSERT INTO action_events (id, session_id, timestamp, session_offset_ms, duration_ms, action_type, plugin_id, payload, semantic_hint, confidence) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    evt.id.to_string(),
                    evt.session_id.to_string(),
                    evt.timestamp,
                    evt.session_offset_ms,
                    evt.duration_ms,
                    evt.action_type.as_str(),
                    evt.plugin_id,
                    payload_json,
                    evt.semantic_hint,
                    evt.confidence,
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
            "SELECT task, intent, constraints, missing_context, restructured_speech, final_markdown, artifacts_json FROM intent_outputs WHERE session_id = ?1",
            params![session_id],
            |row| {
                let constraints: Vec<String> =
                    parse_json_column(&row.get::<_, String>(2)?, "intent_outputs.constraints")?;
                let missing: Vec<String> =
                    parse_json_column(&row.get::<_, String>(3)?, "intent_outputs.missing_context")?;
                let artifacts: Vec<ArtifactRef> =
                    parse_json_column(&row.get::<_, String>(6)?, "intent_outputs.artifacts_json")?;

                Ok(IntentOutput {
                    session_id: session.id,
                    task: row.get(0)?,
                    intent: row.get(1)?,
                    constraints,
                    missing_context: missing,
                    restructured_speech: row.get(4)?,
                    final_markdown: row.get(5)?,
                    artifacts,
                    references: vec![], // filled below
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
            "SELECT id, session_id, timestamp, session_offset_ms, duration_ms, action_type, plugin_id, payload, semantic_hint, confidence FROM action_events WHERE session_id = ?1 ORDER BY session_offset_ms"
        ).map_err(map_err)?;
        let events: Vec<ActionEvent> = evt_stmt
            .query_map(params![session_id], |row| {
                let payload_str: String = row.get(7)?;
                let payload: ActionPayload =
                    parse_json_column(&payload_str, "action_events.payload")?;
                let action_type_str: String = row.get(5)?;

                Ok(ActionEvent {
                    id: parse_uuid(&row.get::<_, String>(0)?)?,
                    session_id: parse_uuid(&row.get::<_, String>(1)?)?,
                    timestamp: row.get(2)?,
                    session_offset_ms: row.get(3)?,
                    duration_ms: row.get(4)?,
                    action_type: ActionType::from_str_name(&action_type_str),
                    plugin_id: row.get(6)?,
                    payload,
                    semantic_hint: row.get(8)?,
                    confidence: row.get(9)?,
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
        }];

        let output = IntentOutput {
            session_id,
            task: "Rewrite the function in Rust".to_string(),
            intent: "rewrite".to_string(),
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
        assert_eq!(detail.output.constraints.len(), 2);
        assert_eq!(detail.output.missing_context.len(), 1);
        assert!(detail.output.final_markdown.contains("## Task"));

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
