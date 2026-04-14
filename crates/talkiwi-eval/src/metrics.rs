use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct IntentCaseMetrics {
    pub intent_match: bool,
    pub task_match: bool,
    pub reference_precision: f32,
    pub reference_recall: f32,
    pub empty: bool,
    pub output_confidence_pass: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct IntentSuiteMetrics {
    pub total_cases: usize,
    pub intent_accuracy: f32,
    pub task_accuracy: f32,
    pub reference_precision: f32,
    pub reference_recall: f32,
    pub empty_rate: f32,
    pub output_confidence_pass_rate: f32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TraceCaseMetrics {
    pub event_count_match: bool,
    pub segment_count_match: bool,
    pub degraded_match: bool,
    pub alignment_match: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TraceSuiteMetrics {
    pub total_cases: usize,
    pub event_count_accuracy: f32,
    pub segment_count_accuracy: f32,
    pub degraded_accuracy: f32,
    pub alignment_accuracy: f32,
}

pub fn ratio(matches: usize, total: usize) -> f32 {
    if total == 0 {
        0.0
    } else {
        matches as f32 / total as f32
    }
}
