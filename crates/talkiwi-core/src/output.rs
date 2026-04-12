use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Reference resolution strategy.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceStrategy {
    TemporalProximity,
    SemanticSimilarity,
    UserConfirmed,
}

/// A resolved reference linking spoken text to an action event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reference {
    pub spoken_text: String,
    pub spoken_offset: usize,
    pub resolved_event_idx: usize,
    pub resolved_event_id: Option<Uuid>,
    pub confidence: f32,
    pub strategy: ReferenceStrategy,
    pub user_confirmed: bool,
}

/// An artifact reference for the final output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub event_id: Uuid,
    pub label: String,
    pub inline_summary: String,
}

/// Intent output — the final structured result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentOutput {
    pub session_id: Uuid,
    pub task: String,
    pub intent: String,
    pub constraints: Vec<String>,
    pub missing_context: Vec<String>,
    pub restructured_speech: String,
    pub final_markdown: String,
    pub artifacts: Vec<ArtifactRef>,
    pub references: Vec<Reference>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_strategy_serde() {
        let strategies = vec![
            ReferenceStrategy::TemporalProximity,
            ReferenceStrategy::SemanticSimilarity,
            ReferenceStrategy::UserConfirmed,
        ];
        for s in &strategies {
            let json = serde_json::to_string(s).unwrap();
            let deserialized: ReferenceStrategy = serde_json::from_str(&json).unwrap();
            assert_eq!(&deserialized, s);
        }
    }

    #[test]
    fn reference_round_trip() {
        let reference = Reference {
            spoken_text: "this code".to_string(),
            spoken_offset: 42,
            resolved_event_idx: 0,
            resolved_event_id: Some(Uuid::new_v4()),
            confidence: 0.9,
            strategy: ReferenceStrategy::TemporalProximity,
            user_confirmed: false,
        };

        let json = serde_json::to_string(&reference).unwrap();
        let deserialized: Reference = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.spoken_text, "this code");
        assert_eq!(deserialized.spoken_offset, 42);
        assert_eq!(deserialized.resolved_event_idx, 0);
    }

    #[test]
    fn intent_output_round_trip() {
        let output = IntentOutput {
            session_id: Uuid::new_v4(),
            task: "Rewrite the function".to_string(),
            intent: "rewrite".to_string(),
            constraints: vec!["use Rust".to_string()],
            missing_context: vec!["which function".to_string()],
            restructured_speech: "Please rewrite the selected function using Rust".to_string(),
            final_markdown: "## Task\nRewrite the function".to_string(),
            artifacts: vec![ArtifactRef {
                event_id: Uuid::new_v4(),
                label: "context-1".to_string(),
                inline_summary: "Selected code in VSCode".to_string(),
            }],
            references: vec![],
        };

        let json = serde_json::to_string(&output).unwrap();
        let deserialized: IntentOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.task, "Rewrite the function");
        assert_eq!(deserialized.artifacts.len(), 1);
        assert_eq!(deserialized.artifacts[0].label, "context-1");
    }
}
