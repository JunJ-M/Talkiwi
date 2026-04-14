import { render } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { WidgetSnapshot } from "../types";

const mockUseBallState = vi.hoisted(() => vi.fn());
const mockShowSettings = vi.hoisted(() => vi.fn(() => Promise.resolve()));

vi.mock("./useBallState", () => ({
  useBallState: mockUseBallState,
}));

vi.mock("../services/window", () => ({
  showSettings: mockShowSettings,
}));

import { BallApp } from "./BallApp";

const snapshot: WidgetSnapshot = {
  session_state: "ready",
  elapsed_ms: 12_000,
  mic: null,
  audio_bins: [],
  speech_bins: [],
  action_pins: [
    {
      id: "action-1",
      t: 4_500,
      type: "click.mouse",
      count: 1,
    },
  ],
  transcript: {
    partial_text: null,
    final_segments: [
      {
        text: "first segment",
        start_ms: 0,
        end_ms: 3_000,
        confidence: 0.96,
        is_final: true,
      },
      {
        text: "second segment",
        start_ms: 6_000,
        end_ms: 9_000,
        confidence: 0.94,
        is_final: true,
      },
    ],
  },
  health: {
    capture_status: [],
    degraded: false,
  },
};

const recordingSnapshot: WidgetSnapshot = {
  ...snapshot,
  session_state: "recording",
};

describe("BallApp timeline alignment", () => {
  beforeEach(() => {
    mockUseBallState.mockReturnValue({
      state: "ready",
      snapshot,
      toggle: vi.fn(),
      canToggle: true,
      requestState: "idle",
      error: null,
      clearError: vi.fn(),
    });
  });

  it("renders the playhead inside the shared timeline rail while recording", () => {
    mockUseBallState.mockReturnValue({
      state: "recording",
      snapshot: recordingSnapshot,
      toggle: vi.fn(),
      canToggle: true,
      requestState: "idle",
      error: null,
      clearError: vi.fn(),
    });

    const { container } = render(<BallApp />);

    const rail = container.querySelector(".widget-timeline-rail");
    const playhead = container.querySelector(".widget-playhead");

    expect(rail).toBeTruthy();
    expect(playhead).toBeTruthy();
    expect(playhead?.parentElement).toBe(rail);
  });

  it("removes the playhead after recording ends", () => {
    const { container } = render(<BallApp />);

    expect(container.querySelector(".widget-playhead")).toBeNull();
  });

  it("positions speak segments and action pins from the same record-start window", () => {
    const { container } = render(<BallApp />);

    const segments = container.querySelectorAll(".widget-speak-segment");
    const action = container.querySelector(".widget-action-icon");
    const ghostSpectrum = container.querySelector(".widget-live-spectrum--ghost");
    const ghostBars = container.querySelectorAll(
      ".widget-live-spectrum--ghost .widget-live-bar",
    );

    expect(segments).toHaveLength(2);
    expect(action).toBeTruthy();
    expect(ghostSpectrum).toBeTruthy();
    expect(ghostBars).toHaveLength(120);

    expect(parseFloat((segments[0] as HTMLElement).style.left)).toBeCloseTo(0, 4);
    expect(parseFloat((segments[0] as HTMLElement).style.width)).toBeCloseTo(10, 4);
    expect(parseFloat((segments[1] as HTMLElement).style.left)).toBeCloseTo(20, 4);
    expect(parseFloat((segments[1] as HTMLElement).style.width)).toBeCloseTo(10, 4);
    expect(parseFloat((action as HTMLElement).style.left)).toBeCloseTo(15, 4);
  });
});
