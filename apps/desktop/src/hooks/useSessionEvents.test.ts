import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useSessionEvents } from "./useSessionEvents";
import { useSessionStore } from "../stores/sessionStore";
import { useTimelineStore } from "../stores/timelineStore";
import { useOutputStore } from "../stores/outputStore";
import { useToastStore } from "../stores/toastStore";
import type { IntentOutput, SpeakSegment, ActionEvent } from "../types";

const mockInvoke = vi.mocked(invoke);
const mockListen = vi.mocked(listen);

// Track registered listen callbacks by event name
type EventCallback = (event: { event: string; id: number; payload: unknown }) => void;
const eventCallbacks = new Map<string, EventCallback>();

function resetStores() {
  useSessionStore.getState().reset();
  useTimelineStore.getState().clear();
  useOutputStore.getState().clear();
  useToastStore.getState().clear();
}

const mockOutput: IntentOutput = {
  session_id: "session-123",
  task: "Test task",
  intent: "analyze",
  constraints: [],
  missing_context: [],
  restructured_speech: "Test speech",
  final_markdown: "## Test",
  artifacts: [],
  references: [],
};

describe("useSessionEvents", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    eventCallbacks.clear();
    resetStores();

    // Capture listen callbacks for simulating Tauri events
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    mockListen.mockImplementation(async (event: any, cb: any) => {
      eventCallbacks.set(event as string, cb as EventCallback);
      return () => {
        eventCallbacks.delete(event as string);
      };
    });
  });

  it("start sets recording state and session ID", async () => {
    mockInvoke.mockResolvedValue("session-123");

    const { result } = renderHook(() => useSessionEvents());

    await act(async () => {
      await result.current.start();
    });

    expect(useSessionStore.getState().state).toBe("recording");
    expect(useSessionStore.getState().sessionId).toBe("session-123");
  });

  it("start clears timeline and output before recording", async () => {
    // Pre-populate stores
    useTimelineStore.getState().addSegment({
      text: "old", start_ms: 0, end_ms: 100, confidence: 1, is_final: true,
    });
    useOutputStore.getState().setOutput(mockOutput);

    mockInvoke.mockResolvedValue("session-456");

    const { result } = renderHook(() => useSessionEvents());

    await act(async () => {
      await result.current.start();
    });

    expect(useTimelineStore.getState().segments).toHaveLength(0);
    expect(useOutputStore.getState().output).toBeNull();
  });

  it("start error sets error state and shows toast", async () => {
    mockInvoke.mockRejectedValue(new Error("Microphone not available"));

    const { result } = renderHook(() => useSessionEvents());

    await act(async () => {
      await result.current.start();
    });

    const state = useSessionStore.getState().state;
    expect(state).toEqual({ error: "Microphone not available" });
    expect(useToastStore.getState().toasts).toHaveLength(1);
    expect(useToastStore.getState().toasts[0].type).toBe("error");
  });

  it("speak-segment event adds to timeline store", async () => {
    mockInvoke.mockResolvedValue("session-789");

    renderHook(() => useSessionEvents());

    // Wait for listen registration
    await vi.waitFor(() => {
      expect(eventCallbacks.has("talkiwi://speak-segment")).toBe(true);
    });

    const segment: SpeakSegment = {
      text: "Hello world",
      start_ms: 0,
      end_ms: 1000,
      confidence: 0.95,
      is_final: true,
    };

    act(() => {
      eventCallbacks.get("talkiwi://speak-segment")!({ event: "talkiwi://speak-segment", id: 1, payload: segment });
    });

    expect(useTimelineStore.getState().segments).toHaveLength(1);
    expect(useTimelineStore.getState().segments[0].text).toBe("Hello world");
  });

  it("action-event adds to timeline store", async () => {
    renderHook(() => useSessionEvents());

    await vi.waitFor(() => {
      expect(eventCallbacks.has("talkiwi://action-event")).toBe(true);
    });

    const event: ActionEvent = {
      id: "evt-1",
      session_id: "session-1",
      timestamp: Date.now(),
      session_offset_ms: 500,
      duration_ms: null,
      action_type: "screenshot",
      plugin_id: "builtin",
      payload: {
        image_path: "/tmp/test.png",
        width: 1920,
        height: 1080,
        ocr_text: null,
      },
      semantic_hint: null,
      confidence: 1.0,
    };

    act(() => {
      eventCallbacks.get("talkiwi://action-event")!({ event: "talkiwi://action-event", id: 2, payload: event });
    });

    expect(useTimelineStore.getState().events).toHaveLength(1);
    expect(useTimelineStore.getState().events[0].action_type).toBe("screenshot");
  });

  it("output-ready event sets output store", async () => {
    renderHook(() => useSessionEvents());

    await vi.waitFor(() => {
      expect(eventCallbacks.has("talkiwi://output-ready")).toBe(true);
    });

    act(() => {
      eventCallbacks.get("talkiwi://output-ready")!({ event: "talkiwi://output-ready", id: 3, payload: mockOutput });
    });

    expect(useOutputStore.getState().output).toEqual(mockOutput);
  });

  it("stop transitions through processing to ready", async () => {
    // Start first
    mockInvoke.mockResolvedValueOnce("session-123");
    const { result } = renderHook(() => useSessionEvents());

    await act(async () => {
      await result.current.start();
    });

    // Stop
    mockInvoke.mockResolvedValueOnce(mockOutput);

    await act(async () => {
      await result.current.stop();
    });

    expect(useSessionStore.getState().state).toBe("ready");
    expect(useOutputStore.getState().output).toEqual(mockOutput);
  });

  it("stop error sets error state and shows toast", async () => {
    mockInvoke.mockResolvedValueOnce("session-123");
    const { result } = renderHook(() => useSessionEvents());

    await act(async () => {
      await result.current.start();
    });

    mockInvoke.mockRejectedValueOnce(new Error("Ollama offline"));

    await act(async () => {
      await result.current.stop();
    });

    const state = useSessionStore.getState().state;
    expect(state).toEqual({ error: "Ollama offline" });
    expect(useToastStore.getState().toasts.some((t) => t.type === "error")).toBe(true);
  });

  it("resetSession clears all stores", async () => {
    // Populate stores
    useSessionStore.getState().setState("recording");
    useSessionStore.getState().setSessionId("session-1");
    useTimelineStore.getState().addSegment({
      text: "test", start_ms: 0, end_ms: 100, confidence: 1, is_final: true,
    });
    useOutputStore.getState().setOutput(mockOutput);

    const { result } = renderHook(() => useSessionEvents());

    act(() => {
      result.current.resetSession();
    });

    expect(useSessionStore.getState().state).toBe("idle");
    expect(useSessionStore.getState().sessionId).toBeNull();
    expect(useTimelineStore.getState().segments).toHaveLength(0);
    expect(useOutputStore.getState().output).toBeNull();
  });
});
