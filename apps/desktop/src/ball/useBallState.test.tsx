import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { IntentOutput } from "../types";

const mockListen = vi.hoisted(() => vi.fn(() => Promise.resolve(() => {})));
const mockSetSize = vi.hoisted(() => vi.fn(() => Promise.resolve()));
const mockEditorShow = vi.hoisted(() => vi.fn(() => Promise.resolve()));
const mockEditorFocus = vi.hoisted(() => vi.fn(() => Promise.resolve()));
const mockSessionStart = vi.hoisted(() => vi.fn());
const mockSessionStop = vi.hoisted(() => vi.fn());
const mockSessionGetState = vi.hoisted(() => vi.fn());
const mockAudioListInputs = vi.hoisted(() => vi.fn());
const mockAudioGetSelectedInput = vi.hoisted(() => vi.fn());
const mockAudioSetSelectedInput = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/event", () => ({
  listen: mockListen,
}));

vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({
    setSize: mockSetSize,
  }),
  WebviewWindow: {
    getByLabel: vi.fn(() =>
      Promise.resolve({
        show: mockEditorShow,
        setFocus: mockEditorFocus,
      }),
    ),
  },
}));

vi.mock("@tauri-apps/api/dpi", () => ({
  LogicalSize: class LogicalSize {
    width: number;
    height: number;

    constructor(width: number, height: number) {
      this.width = width;
      this.height = height;
    }
  },
}));

vi.mock("../services/audio", () => ({
  audioListInputs: mockAudioListInputs,
  audioGetSelectedInput: mockAudioGetSelectedInput,
  audioSetSelectedInput: mockAudioSetSelectedInput,
}));

vi.mock("../services/session", () => ({
  sessionStart: mockSessionStart,
  sessionStop: mockSessionStop,
  sessionState: mockSessionGetState,
}));

import { useBallState } from "./useBallState";

const sampleOutput: IntentOutput = {
  session_id: "session-1",
  task: "task",
  intent: "intent",
  intent_category: "unknown",
  constraints: [],
  missing_context: [],
  restructured_speech: "",
  final_markdown: "",
  artifacts: [],
  references: [],
  output_confidence: 0.8,
  risk_level: "low",
};

describe("useBallState", () => {
  beforeEach(() => {
    vi.clearAllMocks();

    mockAudioListInputs.mockResolvedValue([
      {
        id: "mic-1",
        name: "Built-in Mic",
        is_default: true,
        sample_rates: [44_100],
        channels: [1],
      },
    ]);
    mockAudioGetSelectedInput.mockResolvedValue("mic-1");
    mockAudioSetSelectedInput.mockResolvedValue(undefined);
    mockSessionStart.mockResolvedValue("session-1");
    mockSessionStop.mockResolvedValue(sampleOutput);
    mockSessionGetState.mockResolvedValue("idle");
  });

  it("syncs the initial widget state from the backend", async () => {
    mockSessionGetState.mockResolvedValue("recording");

    const { result } = renderHook(() => useBallState());

    await waitFor(() => {
      expect(result.current.state).toBe("recording");
    });

    expect(mockAudioListInputs).toHaveBeenCalledTimes(1);
    expect(mockAudioGetSelectedInput).toHaveBeenCalledTimes(1);
    expect(mockSessionGetState).toHaveBeenCalledTimes(1);
    expect(mockSetSize).toHaveBeenCalled();
  });

  it("ignores repeated toggle clicks while start is already in flight", async () => {
    mockSessionGetState
      .mockResolvedValueOnce("idle")
      .mockResolvedValueOnce("recording");

    let resolveStart: ((value: string) => void) | null = null;
    mockSessionStart.mockReturnValue(
      new Promise((resolve) => {
        resolveStart = resolve;
      }),
    );

    const { result } = renderHook(() => useBallState());

    await waitFor(() => {
      expect(result.current.state).toBe("idle");
    });

    act(() => {
      void result.current.toggle();
      void result.current.toggle();
    });

    expect(mockSessionStart).toHaveBeenCalledTimes(1);
    expect(mockSessionStop).not.toHaveBeenCalled();
    expect(result.current.requestState).toBe("starting");

    expect(resolveStart).toBeTypeOf("function");
    resolveStart!("session-1");

    await waitFor(() => {
      expect(result.current.state).toBe("recording");
      expect(result.current.requestState).toBe("idle");
    });
  });
});
