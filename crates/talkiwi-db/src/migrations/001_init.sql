-- Sessions table
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    started_at INTEGER,
    ended_at INTEGER,
    duration_ms INTEGER,
    state TEXT NOT NULL DEFAULT 'idle',
    audio_path TEXT,
    created_at INTEGER NOT NULL DEFAULT (
        CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER)
    )
);

-- Intent outputs (v2 new, v3 added artifacts)
CREATE TABLE IF NOT EXISTS intent_outputs (
    session_id TEXT PRIMARY KEY REFERENCES sessions(id),
    task TEXT NOT NULL,
    intent TEXT NOT NULL,
    intent_category TEXT NOT NULL DEFAULT 'unknown',
    constraints TEXT NOT NULL DEFAULT '[]',
    missing_context TEXT NOT NULL DEFAULT '[]',
    restructured_speech TEXT NOT NULL,
    final_markdown TEXT NOT NULL,
    artifacts_json TEXT NOT NULL DEFAULT '[]',
    output_confidence REAL NOT NULL DEFAULT 0.0,
    risk_level TEXT NOT NULL DEFAULT 'high'
);

-- Speech segments
CREATE TABLE IF NOT EXISTS speak_segments (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    text TEXT NOT NULL,
    start_ms INTEGER NOT NULL,
    end_ms INTEGER NOT NULL,
    confidence REAL NOT NULL,
    is_final INTEGER NOT NULL DEFAULT 1
);

-- Action events (v2: id is UUID TEXT)
CREATE TABLE IF NOT EXISTS action_events (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    timestamp INTEGER NOT NULL,
    session_offset_ms INTEGER NOT NULL,
    observed_offset_ms INTEGER,
    duration_ms INTEGER,
    action_type TEXT NOT NULL,
    plugin_id TEXT NOT NULL DEFAULT 'builtin',
    payload TEXT NOT NULL,
    semantic_hint TEXT,
    confidence REAL NOT NULL DEFAULT 1.0
);

-- References (v2: full field set)
CREATE TABLE IF NOT EXISTS references_ (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    spoken_text TEXT NOT NULL,
    spoken_offset INTEGER NOT NULL,
    resolved_event_idx INTEGER NOT NULL,
    resolved_event_id TEXT,
    confidence REAL NOT NULL,
    strategy TEXT NOT NULL DEFAULT 'temporal_proximity',
    user_confirmed INTEGER NOT NULL DEFAULT 0
);

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

-- Indexes
CREATE INDEX IF NOT EXISTS idx_segments_session ON speak_segments(session_id);
CREATE INDEX IF NOT EXISTS idx_events_session ON action_events(session_id);
CREATE INDEX IF NOT EXISTS idx_references_session ON references_(session_id);
CREATE INDEX IF NOT EXISTS idx_sessions_created ON sessions(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_intent_telemetry_session ON intent_telemetry(session_id);
CREATE INDEX IF NOT EXISTS idx_trace_telemetry_session ON trace_telemetry(session_id);
