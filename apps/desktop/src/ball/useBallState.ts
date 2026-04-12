import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";

type BallState = "idle" | "recording" | "processing" | "ready";

export function useBallState() {
  const [state, setState] = useState<BallState>("idle");
  const stateRef = useRef<BallState>("idle");

  // Keep ref in sync
  const updateState = useCallback((next: BallState) => {
    stateRef.current = next;
    setState(next);
  }, []);

  useEffect(() => {
    const unlistenState = listen<string>("talkiwi://session-state", (event) => {
      const payload = event.payload;
      if (
        payload === "idle" ||
        payload === "recording" ||
        payload === "processing" ||
        payload === "ready"
      ) {
        updateState(payload);
      }
    });

    const unlistenOutput = listen("talkiwi://output-ready", () => {
      updateState("ready");
      showEditor();
    });

    return () => {
      unlistenState.then((fn) => fn());
      unlistenOutput.then((fn) => fn());
    };
  }, [updateState]);

  const toggle = useCallback(async () => {
    const current = stateRef.current;

    if (current === "idle" || current === "ready") {
      try {
        updateState("recording");
        await invoke("session_start");
      } catch (e) {
        console.error("Failed to start session:", e);
        updateState("idle");
      }
    } else if (current === "recording") {
      try {
        updateState("processing");
        await invoke("session_stop");
      } catch (e) {
        console.error("Failed to stop session:", e);
        updateState("idle");
      }
    }
    // Ignore clicks during "processing"
  }, [updateState]);

  return { state, toggle };
}

async function showEditor() {
  try {
    const editor = await WebviewWindow.getByLabel("editor");
    if (editor) {
      await editor.show();
      await editor.setFocus();
    }
  } catch (e) {
    console.error("Failed to show editor:", e);
  }
}
