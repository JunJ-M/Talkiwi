// Timeline geometry — MUST stay in sync with `widget_preview.rs`.
export const WINDOW_MS = 30_000;
export const BIN_MS = 250;
export const BIN_COUNT = WINDOW_MS / BIN_MS;

export interface TimelineWindow {
  startMs: number;
  endMs: number;
  playheadPercent: number;
}

export interface TimelinePointPosition {
  leftPercent: number;
}

export interface TimelineRangePosition {
  leftPercent: number;
  widthPercent: number;
}

function clampPercent(value: number): number {
  return Math.max(0, Math.min(100, value));
}

export function getTimelineWindow(elapsedMs: number): TimelineWindow {
  const startMs = Math.max(0, elapsedMs - WINDOW_MS);
  const endMs = startMs + WINDOW_MS;
  const playheadPercent = clampPercent(((elapsedMs - startMs) / WINDOW_MS) * 100);

  return {
    startMs,
    endMs,
    playheadPercent,
  };
}

export function getTimelinePointPosition(
  offsetMs: number,
  window: TimelineWindow,
): TimelinePointPosition | null {
  if (offsetMs < window.startMs || offsetMs > window.endMs) {
    return null;
  }

  return {
    leftPercent: clampPercent(
      ((offsetMs - window.startMs) / WINDOW_MS) * 100,
    ),
  };
}

export function getTimelineRangePosition(
  startMs: number,
  endMs: number,
  window: TimelineWindow,
): TimelineRangePosition | null {
  const clampedStartMs = Math.max(startMs, window.startMs);
  const clampedEndMs = Math.min(Math.max(endMs, startMs), window.endMs);

  if (clampedEndMs <= clampedStartMs) {
    return null;
  }

  return {
    leftPercent: clampPercent(
      ((clampedStartMs - window.startMs) / WINDOW_MS) * 100,
    ),
    widthPercent: clampPercent(
      ((clampedEndMs - clampedStartMs) / WINDOW_MS) * 100,
    ),
  };
}
