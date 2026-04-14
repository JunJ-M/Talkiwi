import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../../services/telemetry", () => ({
  telemetryQualityOverview: vi.fn(),
}));

import { telemetryQualityOverview } from "../../services/telemetry";
import { QualityPanel } from "./QualityPanel";

describe("QualityPanel", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders overview metrics", async () => {
    vi.mocked(telemetryQualityOverview).mockResolvedValue({
      intent_sessions: 2,
      trace_sessions: 2,
      avg_provider_latency_ms: 650,
      avg_output_confidence: 0.82,
      fallback_rate: 0.1,
      degraded_trace_rate: 0.25,
      latest_intent: {
        session_id: "s1",
        timestamp: 1,
        provider_latency_ms: 640,
        provider_success: true,
        retry_count: 1,
        fallback_used: false,
        schema_valid: true,
        repair_attempted: true,
        output_confidence: 0.8,
        reference_count: 2,
        low_confidence_refs: 0,
        intent_category: "rewrite",
      },
      latest_trace: {
        session_id: "s1",
        duration_ms: 10_000,
        segment_count: 3,
        event_count: 5,
        capture_health: [],
        event_density: 0.5,
        alignment_anomalies: 0,
      },
    });

    render(<QualityPanel />);

    await waitFor(() => {
      expect(screen.getByText("平均 provider latency")).toBeInTheDocument();
    });
    expect(screen.getByText("650 ms")).toBeInTheDocument();
    expect(screen.getByText("82%")).toBeInTheDocument();
    expect(screen.getByText("rewrite")).toBeInTheDocument();
    expect(screen.getByText("5 events")).toBeInTheDocument();
  });

  it("renders empty state without telemetry", async () => {
    vi.mocked(telemetryQualityOverview).mockResolvedValue({
      intent_sessions: 0,
      trace_sessions: 0,
      avg_provider_latency_ms: 0,
      avg_output_confidence: 0,
      fallback_rate: 0,
      degraded_trace_rate: 0,
      latest_intent: null,
      latest_trace: null,
    });

    render(<QualityPanel />);

    await waitFor(() => {
      expect(screen.getAllByText("No Data")).toHaveLength(2);
    });
  });
});
