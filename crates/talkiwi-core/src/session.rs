use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Session state machine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Idle,
    Recording,
    Processing,
    Ready,
    Error(String),
}

/// Session metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: Uuid,
    pub state: SessionState,
    pub started_at: Option<u64>,
    pub ended_at: Option<u64>,
    pub duration_ms: Option<u64>,
}

/// Speech segment from ASR.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpeakSegment {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub confidence: f32,
    pub is_final: bool,
}

/// Session summary for history list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: Uuid,
    pub started_at: u64,
    pub duration_ms: u64,
    pub speak_segment_count: usize,
    pub action_event_count: usize,
    pub preview: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_state_serde_round_trip() {
        let states = vec![
            SessionState::Idle,
            SessionState::Recording,
            SessionState::Processing,
            SessionState::Ready,
            SessionState::Error("test error".to_string()),
        ];

        for state in &states {
            let json = serde_json::to_string(state).unwrap();
            let deserialized: SessionState = serde_json::from_str(&json).unwrap();
            assert_eq!(&deserialized, state);
        }
    }

    #[test]
    fn session_serde_round_trip() {
        let session = Session {
            id: Uuid::new_v4(),
            state: SessionState::Recording,
            started_at: Some(1712900000000),
            ended_at: None,
            duration_ms: None,
        };

        let json = serde_json::to_string(&session).unwrap();
        let deserialized: Session = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, session.id);
        assert_eq!(deserialized.state, SessionState::Recording);
    }

    #[test]
    fn speak_segment_serde_round_trip() {
        let segment = SpeakSegment {
            text: "hello world".to_string(),
            start_ms: 1000,
            end_ms: 2500,
            confidence: 0.95,
            is_final: true,
        };

        let json = serde_json::to_string(&segment).unwrap();
        let deserialized: SpeakSegment = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.text, "hello world");
        assert_eq!(deserialized.start_ms, 1000);
        assert!(deserialized.is_final);
    }
}
