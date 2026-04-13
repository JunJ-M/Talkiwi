import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { useTimelineStore } from "../../stores/timelineStore";
import { Timeline } from "./Timeline";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
  emit: vi.fn(),
}));

describe("Timeline", () => {
  beforeEach(() => {
    useTimelineStore.getState().clear();
  });

  it("shows empty states when no data", () => {
    render(<Timeline />);
    expect(screen.getByText("Waiting for speech...")).toBeInTheDocument();
    expect(screen.getByText("No actions captured")).toBeInTheDocument();
  });

  it("renders speak segments", () => {
    useTimelineStore.getState().addSegment({
      text: "hello world",
      start_ms: 0,
      end_ms: 1000,
      confidence: 0.95,
      is_final: true,
    });
    render(<Timeline />);
    expect(screen.getByText(/hello world/)).toBeInTheDocument();
  });

  it("renders action events as marble circles", () => {
    useTimelineStore.getState().addEvent({
      id: "evt-1",
      session_id: "sess-1",
      timestamp: Date.now(),
      session_offset_ms: 5000,
      duration_ms: null,
      action_type: "selection.text",
      plugin_id: "builtin",
      payload: {
        text: "selected text here",
        app_name: "VSCode",
        window_title: "main.rs",
        char_count: 18,
      },
      semantic_hint: null,
      confidence: 0.9,
    });
    render(<Timeline />);
    // Marble shows initial letter "T" for "text"
    expect(screen.getByText("T")).toBeInTheDocument();
    // Marble has title with offset
    expect(
      screen.getByTitle("selection.text @ 0:05"),
    ).toBeInTheDocument();
  });

  it("renders click.link as marble circle", () => {
    useTimelineStore.getState().addEvent({
      id: "evt-2",
      session_id: "sess-1",
      timestamp: Date.now(),
      session_offset_ms: 2000,
      duration_ms: null,
      action_type: "click.link",
      plugin_id: "builtin",
      payload: {
        from_url: "https://source.example.com",
        to_url: "https://target.example.com",
        title: "Target",
      },
      semantic_hint: null,
      confidence: 1,
    });

    render(<Timeline />);
    // Marble shows initial letter "L" for "link"
    expect(screen.getByText("L")).toBeInTheDocument();
    expect(
      screen.getByTitle("click.link @ 0:02"),
    ).toBeInTheDocument();
  });

  it("renders multiple segments with final/non-final styling", () => {
    useTimelineStore.getState().addSegment({
      text: "first",
      start_ms: 0,
      end_ms: 500,
      confidence: 0.9,
      is_final: true,
    });
    useTimelineStore.getState().addSegment({
      text: "partial",
      start_ms: 500,
      end_ms: 1000,
      confidence: 0.6,
      is_final: false,
    });
    render(<Timeline />);
    const segments = document.querySelectorAll(".speak-segment");
    expect(segments).toHaveLength(2);
    expect(segments[0].getAttribute("data-final")).toBe("true");
    expect(segments[1].getAttribute("data-final")).toBe("false");
  });
});
