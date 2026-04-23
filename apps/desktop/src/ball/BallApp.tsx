import { useCallback } from "react";
import { showSettings } from "../services/window";
import { widgetTraceDeleteEvent } from "../services/trace";
import type {
  SpeakSegment,
  WidgetActionPin,
} from "../types";
import {
  BIN_COUNT,
  getTimelinePointPosition,
  getTimelineRangePosition,
  getTimelineWindow,
  type TimelineWindow,
} from "./timelineGeometry";
import { TraceToolbar } from "./TraceToolbar";
import { useBallState } from "./useBallState";
import { useTracePermissions } from "./useTracePermissions";
import "./ball.css";

type BallState = "idle" | "recording" | "processing" | "ready";
type ToggleRequestState = "idle" | "starting" | "stopping";

const ACTION_TYPE_ICON: Record<string, string> = {
  "selection.text": "content_paste",
  screenshot: "camera",
  "clipboard.change": "content_copy",
  "page.current": "language",
  "click.link": "link",
  "file.attach": "attach_file",
  "window.focus": "open_in_new",
  "click.mouse": "mouse",
  "manual.note": "push_pin",
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
  "manual.note": "primary",
};

// Visually distinct small heights used for the SPEAK mini-waveform.
// Values are pixel heights for each bar (kept deterministic per segment).
const BAR_HEIGHTS = [12, 16, 20, 24, 28, 32];

function formatElapsed(ms: number): { min: string; sec: string } {
  const total = Math.floor(ms / 1000);
  return {
    min: String(Math.floor(total / 60)).padStart(2, "0"),
    sec: String(total % 60).padStart(2, "0"),
  };
}

function truncate(text: string, max: number): string {
  return text.length <= max ? text : `${text.slice(0, max)}…`;
}

// Deterministic seed derived from the segment text so the same segment
// always renders with the same waveform shape (no flicker on re-render).
function hashSeed(text: string): number {
  let hash = 0;
  for (let i = 0; i < text.length; i++) {
    hash = ((hash << 5) - hash + text.charCodeAt(i)) | 0;
  }
  return Math.abs(hash) || 1;
}

// Small LCG to produce a reproducible sequence from a seed.
function makeBars(seed: number, count: number): number[] {
  const bars: number[] = [];
  let v = seed;
  for (let i = 0; i < count; i++) {
    v = (v * 1664525 + 1013904223) | 0;
    bars.push(BAR_HEIGHTS[Math.abs(v) % BAR_HEIGHTS.length]);
  }
  return bars;
}

function hasSignalBins(bins: number[]): boolean {
  return bins.some((value) => value > 0);
}

function buildRecordClass(
  state: BallState,
  requestState: ToggleRequestState,
): string {
  const base = "widget-record-btn";
  const isVisualStarting =
    requestState === "starting" && state !== "recording";
  if (isVisualStarting || requestState === "stopping" || state === "processing") {
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
  if (requestState === "starting" && state !== "recording") return "Starting…";
  if (requestState === "stopping") return "Finishing…";
  if (state === "recording") return "Recording";
  if (state === "processing") return "Processing";
  return "Record";
}

/* ---------- Sub-components ---------- */

interface SpeakWaveformSegmentProps {
  segment: SpeakSegment;
  index: number;
  window: TimelineWindow;
}

function SpeakWaveformSegment({
  segment,
  index,
  window,
}: SpeakWaveformSegmentProps) {
  const position = getTimelineRangePosition(
    segment.start_ms,
    segment.end_ms,
    window,
  );

  if (!position) {
    return null;
  }

  const seed = hashSeed(segment.text || `${segment.start_ms}`);
  const durationMs = Math.max(250, segment.end_ms - segment.start_ms);
  const barCount = Math.min(20, Math.max(6, Math.round(durationMs / 500)));
  const bars = makeBars(seed, barCount);
  const variant = index % 2 === 0 ? "primary" : "secondary";

  return (
    <div
      className={`widget-speak-segment widget-speak-segment--${variant}`}
      style={{
        left: `${position.leftPercent}%`,
        width: `${position.widthPercent}%`,
      }}
      title={`${segment.text.trim() || "…"} @ ${segment.start_ms}–${segment.end_ms} ms`}
    >
      <div className="widget-speak-segment-bars">
        {bars.map((h, i) => {
          const tone =
            i % 3 === 0
              ? "widget-speak-bar widget-speak-bar-strong"
              : i % 3 === 1
                ? "widget-speak-bar"
                : "widget-speak-bar widget-speak-bar-dim";
          return <div key={i} className={tone} style={{ height: `${h}px` }} />;
        })}
      </div>
      <div className="widget-speak-segment-label">
        {truncate(segment.text.trim() || "…", 18)}
      </div>
    </div>
  );
}

// Live audio spectrum — renders ALL `BIN_COUNT` backend bins as a
// fixed-width timeline. Each bin is placed left-to-right across the
// track: bin[0] corresponds to the earliest slice of the visible
// window (time = windowStart) and bin[BIN_COUNT-1] corresponds to the
// newest slice (time = windowStart + WINDOW_MS). The playhead element
// computed separately in BallApp is rendered at the absolute position
// of `elapsedMs` inside the same coordinate system so that the bars
// and the playhead share one timeline — previously the playhead was
// pinned to a static 33% offset while bars slid in flex layout, which
// made the two tracks appear to disagree about where "now" is.
interface LiveAudioSpectrumProps {
  bins: number[];
  speechBins?: number[];
  isActive: boolean;
  className?: string;
}

function LiveAudioSpectrum({
  bins,
  speechBins = [],
  isActive,
  className,
}: LiveAudioSpectrumProps) {
  return (
    <div
      className={`widget-live-spectrum ${isActive ? "widget-live-spectrum--active" : ""} ${className ?? ""}`.trim()}
      aria-label="Live audio spectrum"
    >
      {Array.from({ length: BIN_COUNT }, (_, i) => {
        const v = bins[i] ?? 0;
        // Normalize into a comfortable visual range: a peak close to
        // 1.0 ≈ 32px, a silent frame ≈ 3px.
        const clamped = Math.max(0, Math.min(1, v));
        const height = Math.max(3, Math.round(3 + clamped * 29));
        const isSpeech = (speechBins[i] ?? 0) > 0;
        return (
          <div
            key={i}
            className={`widget-live-bar ${isSpeech ? "widget-live-bar--speech" : ""}`}
            style={{ height: `${height}px` }}
          />
        );
      })}
    </div>
  );
}

interface SpeakTrackProps {
  segments: SpeakSegment[];
  audioBins: number[];
  speechBins: number[];
  isRecording: boolean;
  showFrozenAudio: boolean;
  window: TimelineWindow;
}

function SpeakTrack({
  segments,
  audioBins,
  speechBins,
  isRecording,
  showFrozenAudio,
  window,
}: SpeakTrackProps) {
  // While recording, always show the live spectrum so the user sees
  // audio feedback immediately — even before the first ASR segment
  // finalizes. Finalized segments are rendered as mini-waveforms on
  // top / alongside after recording ends.
  if (isRecording) {
    return (
      <div className="widget-track-content">
        <LiveAudioSpectrum
          bins={audioBins}
          speechBins={speechBins}
          isActive={true}
        />
      </div>
    );
  }

  const visible = segments.filter(
    (segment) =>
      segment.end_ms > window.startMs && segment.start_ms < window.endMs,
  );
  const hasAudioSignal = hasSignalBins(audioBins);
  const shouldShowFrozenSpectrum = showFrozenAudio || hasAudioSignal;

  if (visible.length === 0) {
    if (shouldShowFrozenSpectrum) {
      return (
        <div className="widget-track-content">
          <LiveAudioSpectrum
            bins={audioBins}
            speechBins={speechBins}
            isActive={true}
          />
        </div>
      );
    }

    return (
      <div className="widget-track-content widget-speak-track">
        <div className="widget-track-baseline" />
      </div>
    );
  }

  return (
    <div className="widget-track-content widget-speak-track">
      <div className="widget-track-baseline" />
      <div className="widget-speak-spectrum-underlay">
        <LiveAudioSpectrum
          bins={audioBins}
          speechBins={speechBins}
          isActive={false}
          className="widget-live-spectrum--ghost"
        />
      </div>
      <div className="widget-speak-segments">
        {visible.map((seg, i) => (
          <SpeakWaveformSegment
            key={`${seg.start_ms}-${seg.end_ms}-${i}`}
            segment={seg}
            index={i}
            window={window}
          />
        ))}
      </div>
    </div>
  );
}

// Action track — positions each pin by its absolute session-time offset
// within the same [windowStart, windowStart + WINDOW_MS] rolling window
// the SPEAK track uses, so icons align on the timeline with the audio
// waveform and the playhead. The previous flex-gap layout just stacked
// the last six icons on the left regardless of when they actually
// happened, which made it impossible to see any correlation between an
// action and the speech around it.
interface ActionTrackProps {
  pins: WidgetActionPin[];
  window: TimelineWindow;
  onDeletePin?: (pinId: string) => void;
}

function ActionTrack({ pins, window, onDeletePin }: ActionTrackProps) {
  const visible = pins
    .map((pin) => ({
      pin,
      position: getTimelinePointPosition(pin.t, window),
    }))
    .filter(
      (
        entry,
      ): entry is {
        pin: WidgetActionPin;
        position: NonNullable<ReturnType<typeof getTimelinePointPosition>>;
      } => entry.position !== null,
    );

  return (
    <div className="widget-track-content widget-action-track">
      <div className="widget-track-baseline" />
      {visible.map(({ pin, position }) => {
        const icon = ACTION_TYPE_ICON[pin.type] ?? "circle";
        const variant = ACTION_TYPE_VARIANT[pin.type] ?? "primary";
        const source = pin.source ?? "passive";
        const isUserSourced = source === "toolbar" || source === "manual";
        const titleHint = isUserSourced ? " · 右键删除" : "";
        return (
          <div
            key={pin.id}
            className={[
              "widget-action-icon",
              `widget-action-icon--${variant}`,
              isUserSourced && "widget-action-icon--user",
              source === "manual" && "widget-action-icon--manual",
            ]
              .filter(Boolean)
              .join(" ")}
            title={`${pin.type}${pin.count ? ` ×${pin.count}` : ""}${titleHint}`}
            style={{ left: `${position.leftPercent}%` }}
            onContextMenu={(e) => {
              if (!isUserSourced || !onDeletePin) {
                return;
              }
              e.preventDefault();
              onDeletePin(pin.id);
            }}
            data-pin-source={source}
          >
            <span className="material-symbols-outlined msi--sm">{icon}</span>
          </div>
        );
      })}
    </div>
  );
}

function timelineBadgeLabel(
  state: BallState,
  requestState: ToggleRequestState,
  hasContent: boolean,
): string {
  if (requestState === "starting" && state !== "recording") {
    return "ARMING";
  }
  if (state === "recording") {
    return "AUTO-SYNC ACTIVE";
  }
  if (state === "processing") {
    return "PROCESSING";
  }
  if (hasContent) {
    return "CAPTURED";
  }
  return "EMPTY";
}

function timelineHint(
  state: BallState,
  requestState: ToggleRequestState,
  hasContent: boolean,
): string | null {
  if (requestState === "starting" && state !== "recording") {
    return "Timeline stays at 00:00 until recording is live.";
  }
  if (!hasContent) {
    return "Press Record to start filling this timeline.";
  }
  return null;
}

/* ---------- Main Widget ---------- */

export function BallApp() {
  const {
    state,
    snapshot,
    toggle,
    canToggle,
    requestState,
    error,
    clearError,
    setError,
  } = useBallState();

  const isRecording = state === "recording";
  const permissions = useTracePermissions(isRecording);

  const isAwaitingRecording =
    requestState === "starting" && state !== "recording";
  const visibleSnapshot = isAwaitingRecording ? null : snapshot;
  const elapsedMs = visibleSnapshot?.elapsed_ms ?? 0;
  const timer = formatElapsed(elapsedMs);

  // Shared rolling-window geometry for SPEAK, ACTION and the playhead.
  // The playhead starts at t=0 when recording begins and only slides to
  // the right edge once the 30 s window is fully occupied.
  const timelineWindow = getTimelineWindow(elapsedMs);

  const finalSegments = visibleSnapshot?.transcript.final_segments ?? [];
  const actionPins = visibleSnapshot?.action_pins ?? [];
  const audioBins = visibleSnapshot?.audio_bins ?? [];
  const speechBins = visibleSnapshot?.speech_bins ?? [];
  const hasAudioSignal = hasSignalBins(audioBins);
  const hasFrozenAudioTrack =
    !isRecording && elapsedMs > 0 && audioBins.length > 0;
  const hasTimelineContent =
    isRecording ||
    hasFrozenAudioTrack ||
    hasAudioSignal ||
    finalSegments.length > 0 ||
    actionPins.length > 0;
  const timelineSectionHint = timelineHint(
    state,
    requestState,
    hasTimelineContent,
  );

  const handleDeletePin = useCallback(async (pinId: string) => {
    try {
      await widgetTraceDeleteEvent(pinId);
    } catch (err) {
      console.error("Failed to delete pin:", err);
    }
  }, []);

  const handleToolbarError = useCallback(
    (message: string | null) => {
      if (message === null) {
        clearError();
      } else {
        setError(message);
      }
    },
    [clearError, setError],
  );

  return (
    <div className="widget-shell">
      <aside className="widget-panel">
        {/* === Header === */}
        <div className="widget-header" data-tauri-drag-region>
          <header className="widget-top-bar" data-tauri-drag-region>
            <div className="widget-brand" data-tauri-drag-region>
              <span className="widget-brand-text" data-tauri-drag-region>
                Talkiwi
              </span>
              <span className="widget-brand-sub" data-tauri-drag-region>
                The Digital Glass Lab
              </span>
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

          {/* Error banner */}
          {error && (
            <div className="widget-error-banner" role="alert">
              <span className="material-symbols-outlined">error</span>
              <span className="widget-error-banner-text">{error}</span>
              <button
                type="button"
                className="widget-error-banner-close"
                aria-label="Dismiss error"
                onClick={clearError}
              >
                <span className="material-symbols-outlined">close</span>
              </button>
            </div>
          )}
        </div>

        {/* === Scrollable Canvas === */}
        <div className="widget-canvas">
          {/* Timeline Analysis */}
          <section className="widget-section">
            <div className="widget-section-header">
              <h3 className="widget-section-title">
                <span
                  className="material-symbols-outlined msi--sm"
                  style={{ color: "var(--tki-blue-400)" }}
                >
                  analytics
                </span>
                Timeline Analysis
              </h3>
              <span className="widget-section-badge">
                {timelineBadgeLabel(state, requestState, hasTimelineContent)}
              </span>
            </div>
            <div className="widget-timeline">
              <div className="widget-timeline-labels">
                <div className="widget-track-label">SPEAK</div>
                <div className="widget-track-label">ACTION</div>
              </div>
              <div className="widget-timeline-rail">
                {isRecording && (
                  <div
                    className="widget-playhead"
                    style={{ left: `${timelineWindow.playheadPercent}%` }}
                  />
                )}
                <div className="widget-timeline-grid" />
                <div className="widget-timeline-tracks">
                  <div className="widget-track">
                    <SpeakTrack
                      segments={finalSegments}
                      audioBins={audioBins}
                      speechBins={speechBins}
                      isRecording={isRecording}
                      showFrozenAudio={hasFrozenAudioTrack}
                      window={timelineWindow}
                    />
                  </div>
                  <div className="widget-track">
                    <ActionTrack
                      pins={actionPins}
                      window={timelineWindow}
                      onDeletePin={handleDeletePin}
                    />
                  </div>
                </div>
              </div>
            </div>
            {timelineSectionHint && (
              <p className="widget-section-caption">{timelineSectionHint}</p>
            )}
          </section>

          {/* Trace Toolbar — on-demand context capture during recording */}
          <TraceToolbar
            isRecording={isRecording}
            permissions={permissions}
            onError={handleToolbarError}
          />

          {/* Processing indicator */}
          {state === "processing" && (
            <div className="widget-processing-overlay">
              <div className="widget-processing-spinner" />
              <span>Finalizing session…</span>
            </div>
          )}
        </div>
      </aside>
    </div>
  );
}
