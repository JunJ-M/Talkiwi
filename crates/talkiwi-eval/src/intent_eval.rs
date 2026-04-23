use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use talkiwi_core::event::ActionEvent;
use talkiwi_core::output::{IntentOutput, Reference};
use talkiwi_core::session::SpeakSegment;
use talkiwi_core::telemetry::IntentTelemetry;
use talkiwi_core::traits::intent::{IntentProvider, IntentRaw};
use talkiwi_engine::IntentEngine;
use uuid::Uuid;

use crate::metrics::{ratio, IntentCaseMetrics, IntentSuiteMetrics};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentEvalFixture {
    pub id: String,
    pub description: String,
    pub segments: Vec<SpeakSegment>,
    pub events: Vec<ActionEvent>,
    pub provider_output: IntentRaw,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MatchMode {
    #[default]
    Exact,
    Semantic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StringExpectation {
    pub expected: String,
    #[serde(default)]
    pub match_mode: MatchMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferenceExpectation {
    pub spoken_text: String,
    /// Primary expected event index. For v1/Single references this is
    /// the only target; for v2 multi-target references, it is the first
    /// target and must also appear in `expected_event_indices` if that
    /// is specified.
    pub expected_event_index: usize,
    #[serde(default)]
    pub match_mode: MatchMode,

    /// v2: multi-target expectation. When set, every listed index must
    /// appear in the actual reference's `targets` list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_event_indices: Option<Vec<usize>>,
    /// v2: expected `RefRelation` as a snake_case string, e.g.
    /// `"single"`, `"composition"`, `"contrast"`, `"subtraction"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_relation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintExpectation {
    #[serde(default)]
    pub min_count: usize,
    #[serde(default = "default_max_count")]
    pub max_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentGolden {
    pub task: StringExpectation,
    pub intent: StringExpectation,
    #[serde(default)]
    pub references: Vec<ReferenceExpectation>,
    #[serde(default)]
    pub constraints: ConstraintExpectation,
    #[serde(default)]
    pub output_confidence_min: f32,
}

#[derive(Debug, Clone)]
pub struct IntentCaseResult {
    pub fixture: IntentEvalFixture,
    pub golden: IntentGolden,
    pub output: IntentOutput,
    pub telemetry: IntentTelemetry,
    pub metrics: IntentCaseMetrics,
}

#[derive(Debug, Clone)]
pub struct IntentSuiteResult {
    pub cases: Vec<IntentCaseResult>,
    pub metrics: IntentSuiteMetrics,
}

struct FixtureProvider {
    response: IntentRaw,
}

#[async_trait::async_trait]
impl IntentProvider for FixtureProvider {
    fn id(&self) -> &str {
        "fixture"
    }

    fn name(&self) -> &str {
        "Fixture Provider"
    }

    fn requires_network(&self) -> bool {
        false
    }

    async fn is_available(&self) -> bool {
        true
    }

    async fn restructure(
        &self,
        _transcript: &str,
        _events_summary: &str,
        _system_prompt: &str,
    ) -> anyhow::Result<IntentRaw> {
        Ok(self.response.clone())
    }
}

pub fn default_fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/intent")
}

pub fn default_golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("golden/intent")
}

pub async fn run_suite(
    fixtures_dir: impl AsRef<Path>,
    golden_dir: impl AsRef<Path>,
) -> anyhow::Result<IntentSuiteResult> {
    let fixtures_dir = fixtures_dir.as_ref();
    let golden_dir = golden_dir.as_ref();
    let mut case_files = discover_case_files(fixtures_dir)?;
    case_files.sort();

    let mut cases = Vec::new();
    for fixture_path in case_files {
        let fixture = load_fixture(&fixture_path)?;
        let golden_path = golden_path_for(golden_dir, &fixture.id);
        let golden = load_golden(&golden_path)?;
        let output = run_fixture(&fixture).await?;
        let metrics = evaluate_case(&output.0, &golden);

        cases.push(IntentCaseResult {
            fixture,
            golden,
            output: output.0,
            telemetry: output.1,
            metrics,
        });
    }

    let total_cases = cases.len();
    let intent_matches = cases
        .iter()
        .filter(|case| case.metrics.intent_match)
        .count();
    let task_matches = cases.iter().filter(|case| case.metrics.task_match).count();
    let empty_cases = cases.iter().filter(|case| case.metrics.empty).count();
    let confidence_passes = cases
        .iter()
        .filter(|case| case.metrics.output_confidence_pass)
        .count();
    let reference_precision = if total_cases == 0 {
        0.0
    } else {
        cases
            .iter()
            .map(|case| case.metrics.reference_precision)
            .sum::<f32>()
            / total_cases as f32
    };
    let reference_recall = if total_cases == 0 {
        0.0
    } else {
        cases
            .iter()
            .map(|case| case.metrics.reference_recall)
            .sum::<f32>()
            / total_cases as f32
    };
    let relation_accuracy = if total_cases == 0 {
        0.0
    } else {
        cases
            .iter()
            .map(|case| case.metrics.relation_accuracy)
            .sum::<f32>()
            / total_cases as f32
    };

    Ok(IntentSuiteResult {
        cases,
        metrics: IntentSuiteMetrics {
            total_cases,
            intent_accuracy: ratio(intent_matches, total_cases),
            task_accuracy: ratio(task_matches, total_cases),
            reference_precision,
            reference_recall,
            empty_rate: ratio(empty_cases, total_cases),
            output_confidence_pass_rate: ratio(confidence_passes, total_cases),
            relation_accuracy,
        },
    })
}

pub fn load_fixture(path: impl AsRef<Path>) -> anyhow::Result<IntentEvalFixture> {
    let path = path.as_ref();
    let json = fs::read_to_string(path)
        .with_context(|| format!("failed to read intent fixture {}", path.display()))?;
    serde_json::from_str(&json)
        .with_context(|| format!("failed to parse intent fixture {}", path.display()))
}

pub fn load_golden(path: impl AsRef<Path>) -> anyhow::Result<IntentGolden> {
    let path = path.as_ref();
    let json = fs::read_to_string(path)
        .with_context(|| format!("failed to read intent golden {}", path.display()))?;
    serde_json::from_str(&json)
        .with_context(|| format!("failed to parse intent golden {}", path.display()))
}

pub async fn run_fixture(
    fixture: &IntentEvalFixture,
) -> anyhow::Result<(IntentOutput, IntentTelemetry)> {
    let engine = IntentEngine::new(
        Box::new(FixtureProvider {
            response: fixture.provider_output.clone(),
        }),
        None,
    );

    engine
        .process_with_telemetry(&fixture.segments, &fixture.events, Uuid::nil())
        .await
}

pub fn evaluate_case(output: &IntentOutput, golden: &IntentGolden) -> IntentCaseMetrics {
    let intent_match = matches_string(
        &output.intent,
        &golden.intent.expected,
        golden.intent.match_mode,
    );
    let task_match = matches_string(&output.task, &golden.task.expected, golden.task.match_mode);
    let reference_precision = compute_reference_precision(&output.references, &golden.references);
    let reference_recall = compute_reference_recall(&output.references, &golden.references);
    let constraint_count = output.constraints.len();
    let constraints_ok = constraint_count >= golden.constraints.min_count
        && constraint_count <= golden.constraints.max_count;

    let relation_accuracy = compute_relation_accuracy(&output.references, &golden.references);

    IntentCaseMetrics {
        intent_match,
        task_match: task_match && constraints_ok,
        reference_precision,
        reference_recall,
        empty: output.task.trim().is_empty() || output.final_markdown.trim().is_empty(),
        output_confidence_pass: output.output_confidence >= golden.output_confidence_min,
        relation_accuracy,
    }
}

fn discover_case_files(fixtures_dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(fixtures_dir)
        .with_context(|| format!("failed to read fixture dir {}", fixtures_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            files.push(path);
        }
    }
    Ok(files)
}

fn golden_path_for(golden_dir: &Path, fixture_id: &str) -> PathBuf {
    golden_dir.join(format!("{fixture_id}.golden.json"))
}

fn compute_reference_precision(actual: &[Reference], expected: &[ReferenceExpectation]) -> f32 {
    if actual.is_empty() {
        return if expected.is_empty() { 1.0 } else { 0.0 };
    }

    let correct = count_matching_references(actual, expected);
    correct as f32 / actual.len() as f32
}

fn compute_reference_recall(actual: &[Reference], expected: &[ReferenceExpectation]) -> f32 {
    if expected.is_empty() {
        return if actual.is_empty() { 1.0 } else { 0.0 };
    }

    let correct = count_matching_references(actual, expected);
    correct as f32 / expected.len() as f32
}

fn count_matching_references(actual: &[Reference], expected: &[ReferenceExpectation]) -> usize {
    let mut used_actual = vec![false; actual.len()];
    let mut matches = 0usize;

    for expectation in expected {
        if let Some((idx, _reference)) = actual.iter().enumerate().find(|(idx, reference)| {
            !used_actual[*idx]
                && reference_matches_expectation(reference, expectation)
        }) {
            used_actual[idx] = true;
            matches += 1;
        }
    }

    matches
}

fn reference_matches_expectation(reference: &Reference, expectation: &ReferenceExpectation) -> bool {
    if !matches_string(
        &reference.spoken_text,
        &expectation.spoken_text,
        expectation.match_mode,
    ) {
        return false;
    }

    if reference.primary_event_idx() != expectation.expected_event_index {
        return false;
    }

    if let Some(expected_indices) = &expectation.expected_event_indices {
        let actual_indices: std::collections::HashSet<usize> = if reference.targets.is_empty() {
            std::iter::once(reference.primary_event_idx()).collect()
        } else {
            reference.targets.iter().map(|t| t.event_idx).collect()
        };
        for idx in expected_indices {
            if !actual_indices.contains(idx) {
                return false;
            }
        }
    }

    true
}

/// Count references whose `relation` matches the expectation's
/// `expected_relation` (case-insensitive snake_case). Only expectations
/// with `expected_relation` set participate in the ratio.
fn compute_relation_accuracy(actual: &[Reference], expected: &[ReferenceExpectation]) -> f32 {
    let with_expected: Vec<&ReferenceExpectation> = expected
        .iter()
        .filter(|e| e.expected_relation.is_some())
        .collect();
    if with_expected.is_empty() {
        return 1.0;
    }

    let mut used_actual = vec![false; actual.len()];
    let mut correct = 0usize;
    for expectation in &with_expected {
        let Some(expected_relation) = expectation.expected_relation.as_ref() else {
            continue;
        };
        if let Some((idx, _)) = actual.iter().enumerate().find(|(idx, reference)| {
            !used_actual[*idx]
                && matches_string(
                    &reference.spoken_text,
                    &expectation.spoken_text,
                    expectation.match_mode,
                )
        }) {
            used_actual[idx] = true;
            let actual_relation = match actual[idx].relation {
                talkiwi_core::output::RefRelation::Single => "single",
                talkiwi_core::output::RefRelation::Composition => "composition",
                talkiwi_core::output::RefRelation::Contrast => "contrast",
                talkiwi_core::output::RefRelation::Subtraction => "subtraction",
                talkiwi_core::output::RefRelation::Unknown => "unknown",
            };
            if actual_relation.eq_ignore_ascii_case(expected_relation) {
                correct += 1;
            }
        }
    }

    correct as f32 / with_expected.len() as f32
}

fn matches_string(actual: &str, expected: &str, mode: MatchMode) -> bool {
    let actual_normalized = normalize(actual);
    let expected_normalized = normalize(expected);

    match mode {
        MatchMode::Exact => actual_normalized == expected_normalized,
        MatchMode::Semantic => {
            actual_normalized == expected_normalized
                || actual_normalized.contains(&expected_normalized)
                || expected_normalized.contains(&actual_normalized)
        }
    }
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !ch.is_whitespace() && !ch.is_ascii_punctuation())
        .flat_map(char::to_lowercase)
        .collect()
}

fn default_max_count() -> usize {
    usize::MAX
}

impl Default for ConstraintExpectation {
    fn default() -> Self {
        Self {
            min_count: 0,
            max_count: default_max_count(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sample_intent_suite_matches_golden() {
        let result = run_suite(default_fixture_dir(), default_golden_dir())
            .await
            .unwrap();

        // Fixtures: 001 legacy v1, 002 v2 composition, 003 v2 contrast.
        // `FixtureProvider` is deterministic — every metric should land
        // at exactly 1.0, leaving room for no silent regressions.
        assert_eq!(result.metrics.total_cases, 3);
        assert_eq!(result.metrics.intent_accuracy, 1.0);
        assert_eq!(result.metrics.task_accuracy, 1.0);
        assert_eq!(result.metrics.reference_precision, 1.0);
        assert_eq!(result.metrics.reference_recall, 1.0);
        assert_eq!(result.metrics.empty_rate, 0.0);
        assert_eq!(result.metrics.output_confidence_pass_rate, 1.0);
        assert_eq!(result.metrics.relation_accuracy, 1.0);
        assert_eq!(result.cases[0].output.intent, "rewrite");
    }

    #[test]
    fn semantic_match_ignores_whitespace_and_prefix() {
        assert!(matches_string(
            "请帮我重写选中的代码函数",
            "重写选中的代码",
            MatchMode::Semantic
        ));
    }
}
