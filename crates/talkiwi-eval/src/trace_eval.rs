use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use talkiwi_core::telemetry::{CaptureStatus, TraceTelemetry};

use crate::metrics::{ratio, TraceCaseMetrics, TraceSuiteMetrics};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEvalFixture {
    pub id: String,
    pub description: String,
    pub telemetry: TraceTelemetry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceGolden {
    pub expected_event_count: usize,
    pub expected_segment_count: usize,
    pub degraded: bool,
    #[serde(default)]
    pub max_alignment_anomalies: usize,
}

#[derive(Debug, Clone)]
pub struct TraceCaseResult {
    pub fixture: TraceEvalFixture,
    pub golden: TraceGolden,
    pub metrics: TraceCaseMetrics,
}

#[derive(Debug, Clone)]
pub struct TraceSuiteResult {
    pub cases: Vec<TraceCaseResult>,
    pub metrics: TraceSuiteMetrics,
}

pub fn default_fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/trace")
}

pub fn default_golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("golden/trace")
}

pub fn run_suite(
    fixtures_dir: impl AsRef<Path>,
    golden_dir: impl AsRef<Path>,
) -> anyhow::Result<TraceSuiteResult> {
    let fixtures_dir = fixtures_dir.as_ref();
    let golden_dir = golden_dir.as_ref();
    let mut case_files = discover_case_files(fixtures_dir)?;
    case_files.sort();

    let mut cases = Vec::new();
    for fixture_path in case_files {
        let fixture = load_fixture(&fixture_path)?;
        let golden = load_golden(golden_path_for(golden_dir, &fixture.id))?;
        let metrics = evaluate_case(&fixture.telemetry, &golden);
        cases.push(TraceCaseResult {
            fixture,
            golden,
            metrics,
        });
    }

    let total_cases = cases.len();
    let event_matches = cases
        .iter()
        .filter(|case| case.metrics.event_count_match)
        .count();
    let segment_matches = cases
        .iter()
        .filter(|case| case.metrics.segment_count_match)
        .count();
    let degraded_matches = cases
        .iter()
        .filter(|case| case.metrics.degraded_match)
        .count();
    let alignment_matches = cases
        .iter()
        .filter(|case| case.metrics.alignment_match)
        .count();

    Ok(TraceSuiteResult {
        cases,
        metrics: TraceSuiteMetrics {
            total_cases,
            event_count_accuracy: ratio(event_matches, total_cases),
            segment_count_accuracy: ratio(segment_matches, total_cases),
            degraded_accuracy: ratio(degraded_matches, total_cases),
            alignment_accuracy: ratio(alignment_matches, total_cases),
        },
    })
}

pub fn load_fixture(path: impl AsRef<Path>) -> anyhow::Result<TraceEvalFixture> {
    let path = path.as_ref();
    let json = fs::read_to_string(path)
        .with_context(|| format!("failed to read trace fixture {}", path.display()))?;
    serde_json::from_str(&json)
        .with_context(|| format!("failed to parse trace fixture {}", path.display()))
}

pub fn load_golden(path: impl AsRef<Path>) -> anyhow::Result<TraceGolden> {
    let path = path.as_ref();
    let json = fs::read_to_string(path)
        .with_context(|| format!("failed to read trace golden {}", path.display()))?;
    serde_json::from_str(&json)
        .with_context(|| format!("failed to parse trace golden {}", path.display()))
}

pub fn evaluate_case(telemetry: &TraceTelemetry, golden: &TraceGolden) -> TraceCaseMetrics {
    TraceCaseMetrics {
        event_count_match: telemetry.event_count == golden.expected_event_count,
        segment_count_match: telemetry.segment_count == golden.expected_segment_count,
        degraded_match: is_degraded(telemetry) == golden.degraded,
        alignment_match: telemetry.alignment_anomalies <= golden.max_alignment_anomalies,
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

fn is_degraded(telemetry: &TraceTelemetry) -> bool {
    telemetry.capture_health.iter().any(|entry| {
        matches!(
            entry.status,
            CaptureStatus::PermissionDenied | CaptureStatus::Stale | CaptureStatus::Error
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_trace_suite_matches_golden() {
        let result = run_suite(default_fixture_dir(), default_golden_dir()).unwrap();

        assert_eq!(result.metrics.total_cases, 1);
        assert_eq!(result.metrics.event_count_accuracy, 1.0);
        assert_eq!(result.metrics.segment_count_accuracy, 1.0);
        assert_eq!(result.metrics.degraded_accuracy, 1.0);
        assert_eq!(result.metrics.alignment_accuracy, 1.0);
    }
}
