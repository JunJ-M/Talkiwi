use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IntentCategory {
    Rewrite,
    Analyze,
    Summarize,
    Generate,
    Debug,
    Query,
    Unknown,
}

impl IntentCategory {
    pub fn from_llm_output(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "rewrite" | "重写" => Self::Rewrite,
            "analyze" | "分析" => Self::Analyze,
            "summarize" | "总结" | "概述" => Self::Summarize,
            "generate" | "生成" | "创建" => Self::Generate,
            "debug" | "调试" | "修复" => Self::Debug,
            "query" | "查询" | "问答" => Self::Query,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

impl RiskLevel {
    pub fn from_confidence(confidence: f32) -> Self {
        if confidence >= 0.8 {
            Self::Low
        } else if confidence >= 0.5 {
            Self::Medium
        } else {
            Self::High
        }
    }
}

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
    #[serde(default = "default_intent_category")]
    pub intent_category: IntentCategory,
    pub constraints: Vec<String>,
    pub missing_context: Vec<String>,
    pub restructured_speech: String,
    pub final_markdown: String,
    pub artifacts: Vec<ArtifactRef>,
    pub references: Vec<Reference>,
    #[serde(default)]
    pub output_confidence: f32,
    #[serde(default = "default_risk_level")]
    pub risk_level: RiskLevel,
}

fn default_intent_category() -> IntentCategory {
    IntentCategory::Unknown
}

fn default_risk_level() -> RiskLevel {
    RiskLevel::High
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
            intent_category: IntentCategory::Rewrite,
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
            output_confidence: 0.88,
            risk_level: RiskLevel::Low,
        };

        let json = serde_json::to_string(&output).unwrap();
        let deserialized: IntentOutput = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.task, "Rewrite the function");
        assert_eq!(deserialized.artifacts.len(), 1);
        assert_eq!(deserialized.artifacts[0].label, "context-1");
        assert_eq!(deserialized.intent_category, IntentCategory::Rewrite);
        assert_eq!(deserialized.risk_level, RiskLevel::Low);
    }
}
