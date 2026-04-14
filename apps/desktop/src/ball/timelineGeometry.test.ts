import { describe, expect, it } from "vitest";
import {
  WINDOW_MS,
  getTimelinePointPosition,
  getTimelineRangePosition,
  getTimelineWindow,
} from "./timelineGeometry";

describe("timelineGeometry", () => {
  it("anchors the first window at record start", () => {
    const window = getTimelineWindow(5_000);

    expect(window.startMs).toBe(0);
    expect(window.endMs).toBe(WINDOW_MS);
    expect(window.playheadPercent).toBeCloseTo(16.6667, 4);
  });

  it("slides the window once recording exceeds 30 seconds", () => {
    const window = getTimelineWindow(40_000);

    expect(window.startMs).toBe(10_000);
    expect(window.endMs).toBe(40_000);
    expect(window.playheadPercent).toBe(100);
  });

  it("maps points and ranges into the shared timeline rail", () => {
    const window = getTimelineWindow(12_000);
    const action = getTimelinePointPosition(4_500, window);
    const speak = getTimelineRangePosition(6_000, 9_000, window);

    expect(action?.leftPercent).toBeCloseTo(15, 4);
    expect(speak?.leftPercent).toBeCloseTo(20, 4);
    expect(speak?.widthPercent).toBeCloseTo(10, 4);
  });

  it("clips speech ranges to the visible window", () => {
    const window = getTimelineWindow(40_000);
    const clipped = getTimelineRangePosition(8_000, 14_000, window);
    const hidden = getTimelineRangePosition(1_000, 5_000, window);

    expect(clipped?.leftPercent).toBe(0);
    expect(clipped?.widthPercent).toBeCloseTo(13.3333, 4);
    expect(hidden).toBeNull();
  });
});
