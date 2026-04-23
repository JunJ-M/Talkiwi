use rusqlite::Connection;

const INIT_SQL: &str = include_str!("001_init.sql");

pub fn run(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(INIT_SQL)?;
    ensure_column(
        conn,
        "intent_outputs",
        "intent_category",
        "TEXT NOT NULL DEFAULT 'unknown'",
    )?;
    ensure_column(
        conn,
        "intent_outputs",
        "output_confidence",
        "REAL NOT NULL DEFAULT 0.0",
    )?;
    ensure_column(
        conn,
        "intent_outputs",
        "risk_level",
        "TEXT NOT NULL DEFAULT 'high'",
    )?;
    ensure_column(conn, "action_events", "observed_offset_ms", "INTEGER")?;
    // 2026-04-16: trace curation metadata. JSON blob column so future
    // fields (weight, notes, etc.) don't require another migration.
    // Old rows default to `{}` which deserializes into
    // `TraceCuration::default()` via serde(default).
    ensure_column(
        conn,
        "action_events",
        "curation",
        "TEXT NOT NULL DEFAULT '{}'",
    )?;
    // 2026-04-18: trace annotation engine fields. JSON / scalar columns
    // are additive so v1 sessions rehydrate cleanly (empty arrays /
    // default enum strings / NULL segment_idx).
    ensure_column(
        conn,
        "intent_outputs",
        "retrieval_chunks_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )?;
    ensure_column(
        conn,
        "references_",
        "targets_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )?;
    ensure_column(
        conn,
        "references_",
        "relation",
        "TEXT NOT NULL DEFAULT 'single'",
    )?;
    ensure_column(conn, "references_", "segment_idx", "INTEGER")?;
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS intent_telemetry (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL REFERENCES sessions(id),
            timestamp INTEGER NOT NULL,
            provider_latency_ms INTEGER NOT NULL,
            provider_success INTEGER NOT NULL,
            retry_count INTEGER NOT NULL DEFAULT 0,
            fallback_used INTEGER NOT NULL DEFAULT 0,
            schema_valid INTEGER NOT NULL DEFAULT 1,
            repair_attempted INTEGER NOT NULL DEFAULT 0,
            output_confidence REAL NOT NULL DEFAULT 0.0,
            reference_count INTEGER NOT NULL DEFAULT 0,
            low_confidence_refs INTEGER NOT NULL DEFAULT 0,
            intent_category TEXT NOT NULL DEFAULT 'unknown'
        );
        CREATE TABLE IF NOT EXISTS trace_telemetry (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL REFERENCES sessions(id),
            duration_ms INTEGER NOT NULL DEFAULT 0,
            segment_count INTEGER NOT NULL DEFAULT 0,
            event_count INTEGER NOT NULL DEFAULT 0,
            capture_health_json TEXT NOT NULL DEFAULT '[]',
            event_density REAL NOT NULL DEFAULT 0.0,
            alignment_anomalies INTEGER NOT NULL DEFAULT 0
        );
        CREATE INDEX IF NOT EXISTS idx_intent_telemetry_session ON intent_telemetry(session_id);
        CREATE INDEX IF NOT EXISTS idx_trace_telemetry_session ON trace_telemetry(session_id);
        "#,
    )?;
    // 2026-04-19: annotation-engine telemetry columns. Additive so older
    // DBs rehydrate cleanly — missing rows deserialize into the
    // `#[serde(default)]` zero values on the IntentTelemetry struct.
    ensure_column(
        conn,
        "intent_telemetry",
        "candidate_set_size_p50",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        conn,
        "intent_telemetry",
        "candidate_set_size_p95",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        conn,
        "intent_telemetry",
        "references_by_relation_json",
        "TEXT NOT NULL DEFAULT '{}'",
    )?;
    ensure_column(
        conn,
        "intent_telemetry",
        "anchor_propagations",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        conn,
        "intent_telemetry",
        "importance_filtered_events",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    ensure_column(
        conn,
        "intent_telemetry",
        "retrieval_chunk_count",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    Ok(())
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> anyhow::Result<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let exists = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .iter()
        .any(|name| name == column);

    if !exists {
        conn.execute(
            &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
            [],
        )?;
    }

    Ok(())
}
