import { useCallback, useEffect, useRef, useState } from "react";
import { useSessionStore } from "../../stores/sessionStore";
import { useSessionEvents } from "../../hooks/useSessionEvents";
import { RecordTimer } from "./RecordTimer";

export function RecordButton() {
  const state = useSessionStore((s) => s.state);
  const elapsedMs = useSessionStore((s) => s.elapsedMs);
  const setElapsedMs = useSessionStore((s) => s.setElapsedMs);
  const { start, stop, resetSession } = useSessionEvents();
  const timerRef = useRef<ReturnType<typeof setInterval>>();
  const startTimeRef = useRef<number>(0);
  const [busy, setBusy] = useState(false);

  const isRecording = state === "recording";
  const isProcessing = state === "processing";
  const isReady = state === "ready";

  useEffect(() => {
    if (isRecording) {
      startTimeRef.current = Date.now();
      timerRef.current = setInterval(() => {
        setElapsedMs(Date.now() - startTimeRef.current);
      }, 200);
    } else {
      clearInterval(timerRef.current);
    }
    return () => clearInterval(timerRef.current);
  }, [isRecording, setElapsedMs]);

  const handleClick = useCallback(async () => {
    if (busy) return;
    setBusy(true);
    try {
      if (state === "idle") {
        await start();
      } else if (isRecording) {
        await stop();
      } else if (isReady) {
        resetSession();
      }
    } finally {
      setBusy(false);
    }
  }, [busy, state, isRecording, isReady, start, stop, resetSession]);

  const label = isRecording
    ? "Stop recording"
    : isProcessing
      ? "Processing..."
      : isReady
        ? "New session"
        : "Start recording";

  return (
    <div className="record-area">
      <button
        className="record-btn"
        data-recording={isRecording}
        onClick={handleClick}
        disabled={isProcessing || busy}
        aria-label={label}
        title={label}
      >
        <span className="record-btn-inner" />
      </button>
      {isRecording && <RecordTimer elapsedMs={elapsedMs} />}
      {isProcessing && (
        <span className="record-timer" style={{ opacity: 0.6 }}>
          Processing...
        </span>
      )}
    </div>
  );
}
