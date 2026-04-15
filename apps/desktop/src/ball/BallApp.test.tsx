import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { WidgetSnapshot } from "../types";

const mockUseBallState = vi.hoisted(() => vi.fn());
const mockShowSettings = vi.hoisted(() => vi.fn(() => Promise.resolve()));
const mockUseTracePermissions = vi.hoisted(() => vi.fn());
const mockWidgetTraceDeleteEvent = vi.hoisted(() =>
  vi.fn(() => Promise.resolve(true)),
);

vi.mock("./useBallState", () => ({
  useBallState: mockUseBallState,
}));

vi.mock("./useTracePermissions", () => ({
  useTracePermissions: mockUseTracePermissions,
}));

vi.mock("../services/window", () => ({
  showSettings: mockShowSettings,
}));

vi.mock("../services/trace", () => ({
  captureManualNote: vi.fn(),
  capturePageContext: vi.fn(),
  captureScreenshotRegion: vi.fn(),
  captureSelectionText: vi.fn(),
  widgetTraceDeleteEvent: mockWidgetTraceDeleteEvent,
}));

vi.mock("../services/permissions", () => ({
  permissionsCheck: vi.fn(() => Promise.resolve({ entries: [] })),
  permissionsRequest: vi.fn(() => Promise.resolve(true)),
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
      source: "passive",
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

function buildBallStateMock(overrides: Record<string, unknown> = {}) {
  return {
    state: "ready",
    snapshot,
    toggle: vi.fn(),
    canToggle: true,
    requestState: "idle",
    error: null,
    clearError: vi.fn(),
    setError: vi.fn(),
    ...overrides,
  };
}

describe("BallApp timeline alignment", () => {
  beforeEach(() => {
    mockUseBallState.mockReturnValue(buildBallStateMock());
    mockUseTracePermissions.mockReturnValue({
      accessibility: true,
      screen_recording: true,
      microphone: true,
    });
  });

  it("keeps Timeline Analysis mounted in the idle empty state", () => {
    mockUseBallState.mockReturnValue(
      buildBallStateMock({ state: "idle", snapshot: null }),
    );

    const { container } = render(<BallApp />);

    expect(screen.getByText("Timeline Analysis")).toBeInTheDocument();
    expect(screen.getByText("EMPTY")).toBeInTheDocument();
    expect(
      screen.getByText("Press Record to start filling this timeline."),
    ).toBeInTheDocument();
    expect(container.querySelector(".widget-playhead")).toBeNull();
  });

  it("renders the playhead inside the shared timeline rail while recording", () => {
    mockUseBallState.mockReturnValue(
      buildBallStateMock({
        state: "recording",
        snapshot: recordingSnapshot,
      }),
    );

    const { container } = render(<BallApp />);

    const rail = container.querySelector(".widget-timeline-rail");
    const playhead = container.querySelector(".widget-playhead");

    expect(rail).toBeTruthy();
    expect(playhead).toBeTruthy();
    expect(playhead?.parentElement).toBe(rail);
  });

  it("shows recording UI once backend state is recording even if start request is still settling", () => {
    mockUseBallState.mockReturnValue(
      buildBallStateMock({
        state: "recording",
        snapshot: recordingSnapshot,
        canToggle: false,
        requestState: "starting",
      }),
    );

    const { container } = render(<BallApp />);

    expect(screen.getByText("Recording")).toBeInTheDocument();
    expect(screen.queryByText("Starting…")).not.toBeInTheDocument();
    expect(
      container.querySelector(".widget-record-btn--processing"),
    ).toBeNull();
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

  it("removes Live Compilation and keeps the timeline empty while start is pending", () => {
    mockUseBallState.mockReturnValue(
      buildBallStateMock({
        canToggle: false,
        requestState: "starting",
      }),
    );

    const { container } = render(<BallApp />);

    expect(screen.queryByText("Live Compilation")).not.toBeInTheDocument();
    expect(screen.getByText("ARMING")).toBeInTheDocument();
    expect(
      screen.getByText("Timeline stays at 00:00 until recording is live."),
    ).toBeInTheDocument();
    expect(container.querySelector(".widget-playhead")).toBeNull();
    expect(container.querySelector(".widget-action-icon")).toBeNull();
    expect(container.querySelector(".widget-speak-segment")).toBeNull();
    expect(container.querySelector(".widget-record-timer")?.textContent).toBe(
      "00:00",
    );
  });

  it("shows the TraceToolbar idle hint when not recording", () => {
    render(<BallApp />);
    expect(screen.getByText("Trace Toolbar")).toBeInTheDocument();
    expect(screen.getByText("IDLE")).toBeInTheDocument();
    expect(
      screen.getByText("开始录制后,用工具栏采集关键上下文。"),
    ).toBeInTheDocument();
  });

  it("shows the TraceToolbar buttons and user-pin highlight while recording", () => {
    const pinsSnapshot: WidgetSnapshot = {
      ...recordingSnapshot,
      action_pins: [
        {
          id: "toolbar-screenshot",
          t: 4_500,
          type: "screenshot",
          count: 1,
          source: "toolbar",
        },
        {
          id: "manual-note",
          t: 6_000,
          type: "manual.note",
          source: "manual",
        },
        {
          id: "passive-click",
          t: 2_000,
          type: "click.mouse",
          count: 2,
          source: "passive",
        },
      ],
    };

    mockUseBallState.mockReturnValue(
      buildBallStateMock({
        state: "recording",
        snapshot: pinsSnapshot,
      }),
    );

    const { container } = render(<BallApp />);

    // All 4 toolbar buttons render.
    expect(screen.getByTestId("toolbar-btn-screenshot")).toBeInTheDocument();
    expect(screen.getByTestId("toolbar-btn-selection")).toBeInTheDocument();
    expect(screen.getByTestId("toolbar-btn-page")).toBeInTheDocument();
    expect(screen.getByTestId("toolbar-btn-note")).toBeInTheDocument();
    expect(screen.getByText("READY")).toBeInTheDocument();

    // User-sourced pins carry the highlight class; passive pins don't.
    const userPins = container.querySelectorAll(".widget-action-icon--user");
    const manualPins = container.querySelectorAll(".widget-action-icon--manual");
    expect(userPins.length).toBe(2); // toolbar + manual both get --user
    expect(manualPins.length).toBe(1); // only manual gets --manual

    // Passive pin exists but carries neither class.
    const passivePins = container.querySelectorAll(
      '[data-pin-source="passive"]',
    );
    expect(passivePins.length).toBe(1);
    expect(passivePins[0].classList.contains("widget-action-icon--user")).toBe(
      false,
    );
  });

  it("disables toolbar buttons when the matching permission is denied", () => {
    mockUseTracePermissions.mockReturnValue({
      accessibility: false,
      screen_recording: false,
      microphone: true,
    });
    mockUseBallState.mockReturnValue(
      buildBallStateMock({
        state: "recording",
        snapshot: recordingSnapshot,
      }),
    );

    render(<BallApp />);

    const screenshot = screen.getByTestId("toolbar-btn-screenshot");
    const selection = screen.getByTestId("toolbar-btn-selection");
    const page = screen.getByTestId("toolbar-btn-page");
    const note = screen.getByTestId("toolbar-btn-note");

    expect(screenshot.className).toContain("widget-toolbar-btn--disabled");
    expect(selection.className).toContain("widget-toolbar-btn--disabled");
    expect(page.className).toContain("widget-toolbar-btn--disabled");
    // "note" has requires=null so it is always available
    expect(note.className).not.toContain("widget-toolbar-btn--disabled");
  });

  it("does not render the legacy Copy Transcript footer", () => {
    const { container } = render(<BallApp />);
    expect(container.querySelector(".widget-footer")).toBeNull();
    expect(screen.queryByText("Copy Transcript")).not.toBeInTheDocument();
  });
});
