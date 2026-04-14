import { useState, useEffect, useCallback, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { LogicalSize } from "@tauri-apps/api/dpi";
import {
  audioGetSelectedInput,
  audioListInputs,
  audioSetSelectedInput,
} from "../services/audio";
import { isTauriRuntime } from "../services/runtime";
import {
  sessionStart,
  sessionState as sessionGetState,
  sessionStop,
} from "../services/session";
import { showEditor } from "../services/window";
import type {
  AudioInputInfo,
  SessionState,
  WidgetSnapshot,
} from "../types";

type BallState = "idle" | "recording" | "processing" | "ready";
type ToggleRequestState = "idle" | "starting" | "stopping";
type MockPreviewMode = BallState | "restricted";

const PANEL_WIDTH = 384;
const IDLE_HEIGHT = 360;
const RECORDING_HEIGHT = 780;
const PROCESSING_HEIGHT = 720;
const READY_HEIGHT = 820;

const MOCK_INPUTS: AudioInputInfo[] = [
  {
    id: "macbook-pro-mic",
    name: "MacBook Pro 麦克风",
    is_default: true,
    sample_rates: [44_100, 48_000],
    channels: [1],
  },
  {
    id: "opal-c1",
    name: "Opal C1",
    is_default: false,
    sample_rates: [48_000],
    channels: [1],
  },
];

function normalizeState(payload: SessionState): BallState {
  if (typeof payload === "string") {
    if (
      payload === "idle" ||
      payload === "recording" ||
      payload === "processing" ||
      payload === "ready"
    ) {
      return payload;
    }
  }
  return "idle";
}

function currentPanelHeight(
  state: BallState,
  requestState: ToggleRequestState,
): number {
  if (requestState !== "idle" || state === "processing") {
    return PROCESSING_HEIGHT;
  }

  if (state === "recording") {
    return RECORDING_HEIGHT;
  }

  if (state === "ready") {
    return READY_HEIGHT;
  }

  return IDLE_HEIGHT;
}

async function syncBallWindow(
  state: BallState,
  requestState: ToggleRequestState,
) {
  if (!isTauriRuntime()) {
    return;
  }

  const window = getCurrentWebviewWindow();
  const size = new LogicalSize(
    PANEL_WIDTH,
    currentPanelHeight(state, requestState),
  );

  await window.setSize(size);
}

function shouldUseBrowserPreviewMock(): boolean {
  return !isTauriRuntime();
}

function currentMockMode(): MockPreviewMode {
  if (typeof window === "undefined") {
    return "recording";
  }

  const value = new URLSearchParams(window.location.search).get("mock");
  if (
    value === "idle" ||
    value === "recording" ||
    value === "processing" ||
    value === "ready" ||
    value === "restricted"
  ) {
    return value;
  }
  return "recording";
}

function mockStateFromMode(mode: MockPreviewMode): BallState {
  if (mode === "restricted") {
    return "recording";
  }
  return mode;
}

function buildMockBars(active: boolean): number[] {
  return Array.from({ length: 120 }, (_, index) => {
    if (!active) {
      return 0;
    }

    const amplitude = Math.abs(Math.sin((index + 4) / 4.8));
    const variation = Math.abs(Math.cos((index + 11) / 6.6));
    return 0.08 + amplitude * 0.34 + variation * 0.22;
  });
}

function buildMockSpeechBins(active: boolean): number[] {
  return Array.from({ length: 120 }, (_, index) => {
    if (!active) {
      return 0;
    }

    return index % 10 >= 2 && index % 10 <= 6 ? 1 : 0;
  });
}

function buildMockSnapshot(mode: MockPreviewMode): WidgetSnapshot {
  const sessionState = mockStateFromMode(mode);
  const isActive = sessionState === "recording";
  const hasPreview = sessionState !== "idle";
  const elapsedMs = sessionState === "idle" ? 0 : 31_000;

  return {
    session_state: sessionState,
    elapsed_ms: elapsedMs,
    mic: MOCK_INPUTS[0],
    audio_bins: buildMockBars(isActive || sessionState === "ready"),
    speech_bins: buildMockSpeechBins(isActive),
    action_pins: hasPreview
      ? [
          {
            id: "selection-1",
            t: elapsedMs - 9_100,
            type: "selection.text",
            count: 1,
          },
          {
            id: "focus-1",
            t: elapsedMs - 5_200,
            type: "window.focus",
            count: 1,
          },
          {
            id: "click-1",
            t: elapsedMs - 1_800,
            type: "click.mouse",
            count: 3,
          },
        ]
      : [],
    transcript: {
      partial_text:
        sessionState === "recording"
          ? "我先把 widget 重做到接近 Cap，再把录音和动作的反馈做可信。"
          : null,
      final_segments:
        sessionState === "idle"
          ? []
          : [
              {
                text: "我先把 widget 重做到接近 Cap，再把录音和动作的反馈做可信。",
                start_ms: 1_100,
                end_ms: 9_400,
                confidence: 0.96,
                is_final: true,
              },
              {
                text: "现在音频条、动作点和最新转写会在录制面板里一起给反馈。",
                start_ms: 10_200,
                end_ms: 21_600,
                confidence: 0.95,
                is_final: true,
              },
            ],
    },
    health: {
      capture_status:
        mode === "restricted"
          ? [
              {
                capture_id: "builtin.click",
                status: "permission_denied",
                event_count: 0,
                last_event_offset_ms: null,
              },
              {
                capture_id: "builtin.focus",
                status: "active",
                event_count: 4,
                last_event_offset_ms: elapsedMs - 1_200,
              },
            ]
          : [
              {
                capture_id: "builtin.selection",
                status: "active",
                event_count: 4,
                last_event_offset_ms: elapsedMs - 4_600,
              },
              {
                capture_id: "builtin.focus",
                status: "active",
                event_count: 3,
                last_event_offset_ms: elapsedMs - 1_900,
              },
              {
                capture_id: "builtin.click",
                status: "active",
                event_count: 6,
                last_event_offset_ms: elapsedMs - 700,
              },
            ],
      degraded: mode === "restricted",
    },
  };
}

export function useBallState() {
  const browserPreviewMock = shouldUseBrowserPreviewMock();
  const mockModeRef = useRef<MockPreviewMode>(currentMockMode());
  const [state, setState] = useState<BallState>("idle");
  const [snapshot, setSnapshot] = useState<WidgetSnapshot | null>(null);
  const [inputs, setInputs] = useState<AudioInputInfo[]>([]);
  const [selectedMic, setSelectedMic] = useState<string | null>(null);
  const [requestState, setRequestState] = useState<ToggleRequestState>("idle");
  const stateRef = useRef<BallState>("idle");
  const requestStateRef = useRef<ToggleRequestState>("idle");

  const updateState = useCallback((next: BallState) => {
    stateRef.current = next;
    setState(next);
  }, []);

  const updateRequestState = useCallback((next: ToggleRequestState) => {
    requestStateRef.current = next;
    setRequestState(next);
  }, []);

  const refreshInputs = useCallback(async () => {
    const [nextInputs, nextSelected] = await Promise.all([
      audioListInputs(),
      audioGetSelectedInput(),
    ]);
    setInputs(nextInputs);
    setSelectedMic(nextSelected);
  }, []);

  const syncStateFromBackend = useCallback(async () => {
    const nextState = await sessionGetState();
    updateState(normalizeState(nextState));
  }, [updateState]);

  const expanded = true;

  useEffect(() => {
    if (!browserPreviewMock) {
      return;
    }

    const mode = mockModeRef.current;
    setInputs(MOCK_INPUTS);
    setSelectedMic(MOCK_INPUTS[0].id);
    updateState(mockStateFromMode(mode));
    setSnapshot(buildMockSnapshot(mode));
  }, [browserPreviewMock, updateState]);

  useEffect(() => {
    if (browserPreviewMock) {
      return;
    }

    Promise.allSettled([refreshInputs(), syncStateFromBackend()]).then((results) => {
      const [inputsResult, stateResult] = results;
      if (inputsResult.status === "rejected") {
        console.error("Failed to load microphone inputs:", inputsResult.reason);
      }
      if (stateResult.status === "rejected") {
        console.error("Failed to sync session state:", stateResult.reason);
      }
    });
  }, [browserPreviewMock, refreshInputs, syncStateFromBackend]);

  useEffect(() => {
    if (browserPreviewMock) {
      return;
    }

    const unlistenState = listen<SessionState>(
      "talkiwi://session-state",
      (event) => {
        updateState(normalizeState(event.payload));
      },
    );

    const unlistenSnapshot = listen<WidgetSnapshot>(
      "talkiwi://widget-snapshot",
      (event) => {
        setSnapshot(event.payload);
        updateState(normalizeState(event.payload.session_state));
      },
    );

    const unlistenOutput = listen("talkiwi://output-ready", () => {
      updateState("ready");
      showEditor();
    });

    return () => {
      unlistenState.then((fn) => fn());
      unlistenSnapshot.then((fn) => fn());
      unlistenOutput.then((fn) => fn());
    };
  }, [browserPreviewMock, updateState]);

  useEffect(() => {
    syncBallWindow(state, requestState).catch(
      (error) => {
        console.error("Failed to resize ball window:", error);
      },
    );
  }, [requestState, state]);

  const toggle = useCallback(async () => {
    if (requestStateRef.current !== "idle") {
      return;
    }

    if (browserPreviewMock) {
      const current = stateRef.current;
      if (current === "idle" || current === "ready") {
        mockModeRef.current = "recording";
        updateState("recording");
        setSnapshot(buildMockSnapshot("recording"));
        return;
      }

      if (current === "recording") {
        mockModeRef.current = "ready";
        updateState("ready");
        setSnapshot(buildMockSnapshot("ready"));
      }
      return;
    }

    const current = stateRef.current;

    if (current === "idle" || current === "ready") {
      try {
        updateRequestState("starting");
        updateState("recording");
        await sessionStart();
        await syncStateFromBackend();
      } catch (e) {
        console.error("Failed to start session:", e);
        try {
          await syncStateFromBackend();
        } catch (syncError) {
          console.error("Failed to resync session state after start error:", syncError);
          updateState("idle");
        }
      } finally {
        updateRequestState("idle");
      }
    } else if (current === "recording") {
      try {
        updateRequestState("stopping");
        updateState("processing");
        await sessionStop();
        await syncStateFromBackend();
      } catch (e) {
        console.error("Failed to stop session:", e);
        try {
          await syncStateFromBackend();
        } catch (syncError) {
          console.error("Failed to resync session state after stop error:", syncError);
          updateState("idle");
        }
      } finally {
        updateRequestState("idle");
      }
    }
  }, [
    browserPreviewMock,
    syncStateFromBackend,
    updateRequestState,
    updateState,
  ]);

  const selectMic = useCallback(
    async (idOrName: string) => {
      if (browserPreviewMock) {
        setSelectedMic(idOrName);
        return;
      }

      await audioSetSelectedInput(idOrName);
      setSelectedMic(idOrName);
      await refreshInputs();
    },
    [browserPreviewMock, refreshInputs],
  );

  return {
    state,
    snapshot,
    inputs,
    selectedMic,
    toggle,
    selectMic,
    expanded,
    canToggle: requestState === "idle" && state !== "processing",
    requestState,
  };
}
