use crate::metrics::{IntentSuiteMetrics, TraceSuiteMetrics};

pub fn render_intent_report(metrics: &IntentSuiteMetrics) -> String {
    format!(
        "Intent Suite\ncases: {}\nintent_accuracy: {:.2}\ntask_accuracy: {:.2}\nreference_precision: {:.2}\nreference_recall: {:.2}\nrelation_accuracy: {:.2}\nempty_rate: {:.2}\noutput_confidence_pass_rate: {:.2}",
        metrics.total_cases,
        metrics.intent_accuracy,
        metrics.task_accuracy,
        metrics.reference_precision,
        metrics.reference_recall,
        metrics.relation_accuracy,
        metrics.empty_rate,
        metrics.output_confidence_pass_rate
    )
}

pub fn render_trace_report(metrics: &TraceSuiteMetrics) -> String {
    format!(
        "Trace Suite\ncases: {}\nevent_count_accuracy: {:.2}\nsegment_count_accuracy: {:.2}\ndegraded_accuracy: {:.2}\nalignment_accuracy: {:.2}",
        metrics.total_cases,
        metrics.event_count_accuracy,
        metrics.segment_count_accuracy,
        metrics.degraded_accuracy,
        metrics.alignment_accuracy
    )
}
