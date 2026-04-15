import { useCallback, useEffect, useRef, useState } from "react";
import { permissionsCheck } from "../services/permissions";
import { isTauriRuntime } from "../services/runtime";
import type { TracePermissionMatrix } from "../types";

const DEFAULT_MATRIX: TracePermissionMatrix = {
  accessibility: false,
  screen_recording: false,
  microphone: false,
};

const BROWSER_PREVIEW_MATRIX: TracePermissionMatrix = {
  accessibility: true,
  screen_recording: true,
  microphone: true,
};

// Poll interval when the widget is idle. We deliberately avoid probing
// during active recording on macOS because the screen-recording probe
// enumerates monitors — cheap in theory but it costs a few milliseconds
// per tick, and the result can't change mid-session anyway.
const POLL_INTERVAL_MS = 5000;

/**
 * Live read of the Trace Toolbar permission matrix.
 *
 * The hook reads `permissions_check` once on mount, then polls every
 * `POLL_INTERVAL_MS` when `pauseWhileRecording` is false. When running
 * outside a Tauri runtime (the browser mock / Vitest), it returns a
 * permissive matrix so the toolbar is interactive in storybook-like
 * previews without pretending to enforce anything.
 */
export function useTracePermissions(pauseWhileRecording = false) {
  const [matrix, setMatrix] = useState<TracePermissionMatrix>(DEFAULT_MATRIX);
  const cancelledRef = useRef(false);

  const tick = useCallback(async () => {
    try {
      const report = await permissionsCheck();
      if (cancelledRef.current) return;
      const next: TracePermissionMatrix = { ...DEFAULT_MATRIX };
      for (const entry of report.entries) {
        if (entry.module in next) {
          next[entry.module as keyof TracePermissionMatrix] = entry.granted;
        }
      }
      setMatrix(next);
    } catch {
      // Leave the previous matrix in place — a transient probe failure
      // shouldn't disable the toolbar mid-session.
    }
  }, []);

  useEffect(() => {
    cancelledRef.current = false;

    if (!isTauriRuntime()) {
      setMatrix(BROWSER_PREVIEW_MATRIX);
      return () => {
        cancelledRef.current = true;
      };
    }

    void tick();

    if (pauseWhileRecording) {
      return () => {
        cancelledRef.current = true;
      };
    }

    const handle = window.setInterval(() => void tick(), POLL_INTERVAL_MS);
    return () => {
      cancelledRef.current = true;
      window.clearInterval(handle);
    };
  }, [pauseWhileRecording, tick]);

  return matrix;
}
