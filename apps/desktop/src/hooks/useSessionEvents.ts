import { useCallback } from "react";
import { useSessionStore } from "../stores/sessionStore";
import { useTimelineStore } from "../stores/timelineStore";
import { useOutputStore } from "../stores/outputStore";
import { useToastStore } from "../stores/toastStore";
import { useTauriEvent } from "./useTauriEvent";
import { sessionStart, sessionStop } from "../services/session";
import type {
  ActionEvent,
  IntentOutput,
  SessionState,
  SpeakSegment,
} from "../types";

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

export function useSessionEvents() {
  const setSessionState = useSessionStore((s) => s.setState);
  const setSessionId = useSessionStore((s) => s.setSessionId);
  const reset = useSessionStore((s) => s.reset);
  const addSegment = useTimelineStore((s) => s.addSegment);
  const addEvent = useTimelineStore((s) => s.addEvent);
  const clearTimeline = useTimelineStore((s) => s.clear);
  const setOutput = useOutputStore((s) => s.setOutput);
  const clearOutput = useOutputStore((s) => s.clear);
  const addToast = useToastStore((s) => s.addToast);

  // Listen for streaming events from Tauri backend
  useTauriEvent<SessionState>("talkiwi://session-state", setSessionState);
  useTauriEvent<SpeakSegment>("talkiwi://speak-segment", addSegment);
  useTauriEvent<ActionEvent>("talkiwi://action-event", addEvent);
  useTauriEvent<IntentOutput>("talkiwi://output-ready", setOutput);

  const start = useCallback(async () => {
    try {
      clearTimeline();
      clearOutput();
      const id = await sessionStart();
      setSessionId(id);
      setSessionState("recording");
    } catch (err) {
      const msg = errorMessage(err);
      setSessionState({ error: msg });
      addToast({ message: `录制启动失败: ${msg}`, type: "error" });
    }
  }, [clearTimeline, clearOutput, setSessionId, setSessionState, addToast]);

  const stop = useCallback(async () => {
    setSessionState("processing");
    try {
      const output = await sessionStop();
      setOutput(output);
      setSessionState("ready");
    } catch (err) {
      const msg = errorMessage(err);
      setSessionState({ error: msg });
      addToast({ message: `处理失败: ${msg}`, type: "error" });
    }
  }, [setSessionState, setOutput, addToast]);

  const resetSession = useCallback(() => {
    reset();
    clearTimeline();
    clearOutput();
  }, [reset, clearTimeline, clearOutput]);

  return { start, stop, resetSession };
}
