import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { telemetryQualityOverview } from "../../services/telemetry";
import type { QualityOverview } from "../../types";
import { Badge } from "../ui/Badge";
import { Button } from "../ui/Button";

function formatPercent(value: number): string {
  return `${Math.round(value * 100)}%`;
}

function formatLatency(ms: number): string {
  return `${Math.round(ms)} ms`;
}

export function QualityPanel() {
  const [overview, setOverview] = useState<QualityOverview | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let mounted = true;

    const load = async () => {
      setLoading(true);
      try {
        const next = await telemetryQualityOverview();
        if (mounted) {
          setOverview(next);
          setError(null);
        }
      } catch (err) {
        if (mounted) {
          setError(err instanceof Error ? err.message : String(err));
        }
      } finally {
        if (mounted) {
          setLoading(false);
        }
      }
    };

    load();
    const unlisten = listen("talkiwi://session-complete", () => {
      void load();
    });

    return () => {
      mounted = false;
      unlisten.then((fn) => fn());
    };
  }, []);

  return (
    <div className="settings-section">
      <div className="settings-header-row">
        <span className="settings-label">Quality</span>
        <Button
          variant="secondary"
          size="sm"
          onClick={async () => {
            setLoading(true);
            try {
              const next = await telemetryQualityOverview();
              setOverview(next);
              setError(null);
            } catch (err) {
              setError(err instanceof Error ? err.message : String(err));
            } finally {
              setLoading(false);
            }
          }}
        >
          {loading ? "刷新中..." : "刷新"}
        </Button>
      </div>

      {error && <div className="settings-error">质量指标读取失败：{error}</div>}

      {!overview ? (
        <div className="settings-note">尚无质量数据。</div>
      ) : (
        <div className="settings-quality-grid">
          <div className="settings-quality-card">
            <span className="settings-row-label">平均 provider latency</span>
            <strong>{formatLatency(overview.avg_provider_latency_ms)}</strong>
          </div>
          <div className="settings-quality-card">
            <span className="settings-row-label">平均 output confidence</span>
            <strong>{formatPercent(overview.avg_output_confidence)}</strong>
          </div>
          <div className="settings-quality-card">
            <span className="settings-row-label">fallback rate</span>
            <strong>{formatPercent(overview.fallback_rate)}</strong>
          </div>
          <div className="settings-quality-card">
            <span className="settings-row-label">degraded trace rate</span>
            <strong>{formatPercent(overview.degraded_trace_rate)}</strong>
          </div>

          <div className="settings-status-card">
            <div className="settings-status-row">
              <span className="settings-row-label">Latest Intent</span>
              {overview.latest_intent ? (
                <Badge
                  variant={
                    overview.latest_intent.fallback_used ? "warning" : "success"
                  }
                >
                  {overview.latest_intent.intent_category}
                </Badge>
              ) : (
                <Badge variant="default">No Data</Badge>
              )}
            </div>
            {overview.latest_intent && (
              <>
                <div className="settings-status-line">
                  latency: {formatLatency(overview.latest_intent.provider_latency_ms)}
                </div>
                <div className="settings-status-line">
                  confidence: {formatPercent(overview.latest_intent.output_confidence)}
                </div>
                <div className="settings-status-line">
                  retry: {overview.latest_intent.retry_count} / refs:{" "}
                  {overview.latest_intent.reference_count}
                </div>
              </>
            )}
          </div>

          <div className="settings-status-card">
            <div className="settings-status-row">
              <span className="settings-row-label">Latest Trace</span>
              {overview.latest_trace ? (
                <Badge
                  variant={
                    overview.latest_trace.capture_health.some((entry) =>
                      ["permission_denied", "stale", "error"].includes(entry.status),
                    )
                      ? "warning"
                      : "success"
                  }
                >
                  {overview.latest_trace.event_count} events
                </Badge>
              ) : (
                <Badge variant="default">No Data</Badge>
              )}
            </div>
            {overview.latest_trace && (
              <>
                <div className="settings-status-line">
                  duration: {Math.round(overview.latest_trace.duration_ms / 1000)}s
                </div>
                <div className="settings-status-line">
                  density: {overview.latest_trace.event_density.toFixed(2)} evt/s
                </div>
                <div className="settings-status-line">
                  anomalies: {overview.latest_trace.alignment_anomalies}
                </div>
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
