import { useCallback, useState } from "react";
import { showSettings } from "../services/window";
import type {
  SpeakSegment,
  WidgetActionPin,
} from "../types";
import { useBallState } from "./useBallState";
import "./ball.css";

type BallState = "idle" | "recording" | "processing" | "ready";
type ToggleRequestState = "idle" | "starting" | "stopping";

const ACTION_WINDOW_MS = 12_000;

const ACTION_TYPE_ICON: Record<string, string> = {
  "selection.text": "content_paste",
  screenshot: "camera",
  "clipboard.change": "content_copy",
  "page.current": "language",
  "click.link": "link",
  "file.attach": "attach_file",
  "window.focus": "open_in_new",
  "click.mouse": "mouse",
};

const ACTION_TYPE_VARIANT: Record<string, string> = {
  "selection.text": "tertiary",
  "clipboard.change": "tertiary",
  "click.mouse": "tertiary",
  screenshot: "secondary",
  "window.focus": "secondary",
  "page.current": "primary",
  "click.link": "primary",
  "file.attach": "primary",
};

function formatElapsed(ms: number): { min: string; sec: string } {
  const total = Math.floor(ms / 1000);
  return {
    min: String(Math.floor(total / 60)).padStart(2, "0"),
    sec: String(total % 60).padStart(2, "0"),
  };
}

function formatTimeNow(): string {
  const d = new Date();
  const h = d.getHours();
  const m = String(d.getMinutes()).padStart(2, "0");
  return `${h % 12 || 12}:${m} ${h >= 12 ? "PM" : "AM"}`;
}

function truncate(text: string, max: number): string {
  return text.length <= max ? text : `${text.slice(0, max)}…`;
}

function recentPins(
  pins: WidgetActionPin[],
  elapsedMs: number,
): WidgetActionPin[] {
  const start = Math.max(0, elapsedMs - ACTION_WINDOW_MS);
  return pins.filter((p) => p.t >= start);
}

function buildRecordClass(
  state: BallState,
  requestState: ToggleRequestState,
): string {
  const base = "widget-record-btn";
  if (requestState !== "idle" || state === "processing") {
    return `${base} widget-record-btn--processing`;
  }
  if (state === "recording") {
    return base;
  }
  return `${base} widget-record-btn--idle`;
}

function recordLabel(
  state: BallState,
  requestState: ToggleRequestState,
): string {
  if (requestState === "starting") return "Starting…";
  if (requestState === "stopping") return "Finishing…";
  if (state === "recording") return "Recording";
  if (state === "processing") return "Processing";
  return "Record";
}

/* ---------- Sub-components ---------- */

function SpeakTrack({ segments }: { segments: SpeakSegment[] }) {
  const visible = segments.slice(-3);

  if (visible.length === 0) {
    return (
      <div className="widget-track-content">
        <div className="widget-track-gap" />
      </div>
    );
  }

  return (
    <div className="widget-track-content">
      {visible.map((seg, i) => (
        <span key={`${seg.start_ms}-${i}`} className="widget-speak-block">
          {truncate(seg.text, 18)}
        </span>
      ))}
      {visible.length < 3 && <div className="widget-track-gap" />}
    </div>
  );
}

function ActionTrack({ pins }: { pins: WidgetActionPin[] }) {
  const visible = pins.slice(-6);

  if (visible.length === 0) {
    return (
      <div className="widget-track-content">
        <div className="widget-action-dot" />
        <div className="widget-track-gap" />
      </div>
    );
  }

  return (
    <div className="widget-track-content">
      <div className="widget-action-dot" />
      {visible.map((pin) => {
        const icon = ACTION_TYPE_ICON[pin.type] ?? "circle";
        const variant = ACTION_TYPE_VARIANT[pin.type] ?? "primary";
        return (
          <div
            key={pin.id}
            className={`widget-action-icon widget-action-icon--${variant}`}
            title={`${pin.type}${pin.count ? ` ×${pin.count}` : ""}`}
          >
            <span className="material-symbols-outlined msi--sm">{icon}</span>
          </div>
        );
      })}
    </div>
  );
}

function CompilationView({
  partialText,
  finalSegments,
  outputMarkdown,
  state,
}: {
  partialText: string | null | undefined;
  finalSegments: SpeakSegment[];
  outputMarkdown: string | null;
  state: BallState;
}) {
  if (state === "ready" && outputMarkdown) {
    const lines = outputMarkdown
      .split("\n")
      .filter((l) => l.trim().length > 0)
      .slice(0, 8);
    const heading = lines[0]?.startsWith("#")
      ? lines[0].replace(/^#+\s*/, "")
      : null;
    const bodyLines = heading ? lines.slice(1) : lines;

    return (
      <div className="widget-compilation">
        <div className="widget-compilation-dots">
          <div className="widget-compilation-dot" />
          <div className="widget-compilation-dot" />
          <div className="widget-compilation-dot" />
        </div>
        {heading && (
          <div className="widget-compilation-comment"># {heading}</div>
        )}
        <div className="widget-compilation-body">
          {bodyLines.map((line, i) => (
            <div key={i}>
              <span className="widget-compilation-num">{i + 1}.</span> {line}
            </div>
          ))}
        </div>
      </div>
    );
  }

  // During recording — show live transcript
  const allTexts: string[] = [];
  for (const seg of finalSegments) {
    if (seg.text.trim()) allTexts.push(seg.text.trim());
  }
  if (partialText?.trim()) allTexts.push(partialText.trim());

  if (allTexts.length === 0) {
    return (
      <div className="widget-compilation">
        <div className="widget-compilation-dots">
          <div className="widget-compilation-dot" />
          <div className="widget-compilation-dot" />
          <div className="widget-compilation-dot" />
        </div>
        <div className="widget-compilation-empty">
          Waiting for speech input…
        </div>
      </div>
    );
  }

  // Split combined text into sentences for display
  const combined = allTexts.join(" ");
  const sentences = combined
    .split(/(?<=[。.!?！？\n])\s*/)
    .filter((s) => s.trim().length > 0)
    .slice(0, 6);

  return (
    <div className="widget-compilation">
      <div className="widget-compilation-dots">
        <div className="widget-compilation-dot" />
        <div className="widget-compilation-dot" />
        <div className="widget-compilation-dot" />
      </div>
      <div className="widget-compilation-comment">
        # Live Transcript
      </div>
      <div className="widget-compilation-body">
        {sentences.map((sentence, i) => (
          <div key={i}>
            <span className="widget-compilation-num">{i + 1}.</span>{" "}
            {truncate(sentence, 60)}
          </div>
        ))}
      </div>
    </div>
  );
}

/* ---------- Main Widget ---------- */

export function BallApp() {
  const {
    state,
    snapshot,
    toggle,
    canToggle,
    requestState,
  } = useBallState();

  const [copied, setCopied] = useState(false);

  const elapsedMs = snapshot?.elapsed_ms ?? 0;
  const timer = formatElapsed(elapsedMs);

  const finalSegments = snapshot?.transcript.final_segments ?? [];
  const partialText = snapshot?.transcript.partial_text;
  const actionPins = recentPins(snapshot?.action_pins ?? [], elapsedMs);

  const showTimeline =
    state === "recording" ||
    state === "processing" ||
    state === "ready" ||
    finalSegments.length > 0 ||
    actionPins.length > 0;

  const showCompilation =
    state === "recording" ||
    state === "processing" ||
    state === "ready" ||
    finalSegments.length > 0 ||
    (partialText?.trim()?.length ?? 0) > 0;

  const isReady = state === "ready";

  // Get output markdown when ready
  // In production this comes from the session detail;
  // for the widget we reconstruct from transcript
  const outputMarkdown = isReady
    ? finalSegments.map((s) => s.text).join("\n") || null
    : null;

  const handleCopy = useCallback(async () => {
    const content = isReady
      ? outputMarkdown ?? ""
      : finalSegments.map((s) => s.text).join("\n");
    if (!content) return;

    try {
      await navigator.clipboard.writeText(content);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      console.error("Failed to copy:", err);
    }
  }, [isReady, outputMarkdown, finalSegments]);

  return (
    <div className="widget-shell">
      <aside className="widget-panel">
        {/* === Header === */}
        <div className="widget-header">
          <header className="widget-top-bar">
            <div className="widget-brand">
              <span className="widget-brand-text">Talkiwi</span>
              <span className="widget-brand-sub">The Digital Glass Lab</span>
            </div>
            <div className="widget-header-actions">
              <button
                type="button"
                className="widget-icon-btn"
                aria-label="Open settings"
                onClick={() => void showSettings()}
              >
                <span className="material-symbols-outlined msi--lg">
                  settings
                </span>
              </button>
            </div>
          </header>

          {/* Record button */}
          <button
            type="button"
            className={buildRecordClass(state, requestState)}
            onClick={toggle}
            disabled={!canToggle}
          >
            <div className="widget-record-left">
              <div className="widget-record-indicator">
                {state === "recording" && requestState === "idle" && (
                  <div className="widget-record-ping" />
                )}
                <div className="widget-record-dot" />
              </div>
              <span className="widget-record-label">
                {recordLabel(state, requestState)}
              </span>
            </div>
            <div className="widget-record-timer">
              <span className="widget-record-timer-dim">{timer.min}:</span>
              <span>{timer.sec}</span>
            </div>
          </button>
        </div>

        {/* === Scrollable Canvas === */}
        <div className="widget-canvas">
          {/* Timeline Analysis */}
          {showTimeline && (
            <section className="widget-section">
              <div className="widget-section-header">
                <h3 className="widget-section-title">
                  <span className="material-symbols-outlined msi--sm" style={{ color: "var(--tki-blue-400)" }}>
                    analytics
                  </span>
                  Timeline Analysis
                </h3>
                <span className="widget-section-badge">
                  {state === "recording" ? "AUTO-SYNC ACTIVE" : "COMPLETED"}
                </span>
              </div>
              <div className="widget-timeline">
                <div className="widget-playhead" />
                <div className="widget-timeline-tracks">
                  <div className="widget-track">
                    <div className="widget-track-label">SPEAK</div>
                    <SpeakTrack segments={finalSegments} />
                  </div>
                  <div className="widget-track">
                    <div className="widget-track-label">ACTION</div>
                    <ActionTrack pins={actionPins} />
                  </div>
                </div>
                <div className="widget-timeline-grid" />
              </div>
            </section>
          )}

          {/* Live Compilation */}
          {showCompilation && (
            <section className="widget-section">
              <div className="widget-section-header">
                <h3 className="widget-section-title">
                  <span className="material-symbols-outlined msi--sm" style={{ color: "#dbb4ff" }}>
                    terminal
                  </span>
                  Live Compilation
                </h3>
              </div>
              <CompilationView
                partialText={partialText}
                finalSegments={finalSegments}
                outputMarkdown={outputMarkdown}
                state={state}
              />
            </section>
          )}

          {/* Processing indicator */}
          {state === "processing" && (
            <div className="widget-processing-overlay">
              <div className="widget-processing-spinner" />
              <span>Analyzing intent…</span>
            </div>
          )}

          {/* Idle hint */}
          {state === "idle" && !showTimeline && !showCompilation && (
            <div className="widget-idle-hint">
              <span className="material-symbols-outlined">mic</span>
              <span>Press Record to begin capturing voice and context</span>
            </div>
          )}
        </div>

        {/* === Footer === */}
        <footer className="widget-footer">
          <button
            type="button"
            className="widget-copy-btn"
            onClick={() => void handleCopy()}
            disabled={finalSegments.length === 0 && !outputMarkdown}
          >
            <span className="material-symbols-outlined">
              {copied ? "check" : "content_copy"}
            </span>
            {copied ? "Copied!" : "Copy Result"}
          </button>
          <div className="widget-save-info">
            {isReady ? (
              <>
                <div className="widget-save-status">
                  <span className="material-symbols-outlined msi--filled">
                    cloud_done
                  </span>
                  <span className="widget-save-label">Saved</span>
                </div>
                <span className="widget-save-time">{formatTimeNow()}</span>
              </>
            ) : state === "recording" ? (
              <>
                <div className="widget-save-status">
                  <span className="material-symbols-outlined msi--sm">
                    fiber_manual_record
                  </span>
                  <span className="widget-save-label">Live</span>
                </div>
                <span className="widget-save-time">{formatTimeNow()}</span>
              </>
            ) : null}
          </div>
        </footer>
      </aside>
    </div>
  );
}
