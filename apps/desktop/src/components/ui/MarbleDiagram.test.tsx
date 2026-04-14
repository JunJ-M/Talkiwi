import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { MarbleDiagram } from "./MarbleDiagram";
import type { ActionEvent } from "../../types";

function makeEvent(
  overrides: Partial<ActionEvent> & {
    action_type: string;
    session_offset_ms: number;
  },
): ActionEvent {
  return {
    id: overrides.id ?? crypto.randomUUID(),
    session_id: "session-1",
    timestamp: Date.now(),
    duration_ms: null,
    plugin_id: "builtin",
    semantic_hint: null,
    confidence: 1,
    payload: { text: "test", app_name: "VSCode", window_title: "main.rs", char_count: 4 },
    ...overrides,
  } as ActionEvent;
}

describe("MarbleDiagram", () => {
  it("renders empty state when no events", () => {
    render(<MarbleDiagram events={[]} durationMs={10000} />);
    expect(screen.getByText("No actions captured")).toBeInTheDocument();
  });

  it("renders correct number of marbles", () => {
    const events = [
      makeEvent({ action_type: "selection.text", session_offset_ms: 1000 }),
      makeEvent({ action_type: "screenshot", session_offset_ms: 3000 }),
      makeEvent({ action_type: "clipboard.change", session_offset_ms: 5000 }),
    ];

    render(<MarbleDiagram events={events} durationMs={10000} />);

    const marbles = screen.getAllByRole("button");
    expect(marbles).toHaveLength(3);
  });

  it("shows initial letter of action type on marble", () => {
    const events = [
      makeEvent({ action_type: "selection.text", session_offset_ms: 1000 }),
      makeEvent({ action_type: "screenshot", session_offset_ms: 3000 }),
    ];

    render(<MarbleDiagram events={events} durationMs={10000} />);

    expect(screen.getByText("T")).toBeInTheDocument(); // text
    expect(screen.getByText("S")).toBeInTheDocument(); // screenshot
  });

  it("calls onSelectEvent when marble clicked", () => {
    const onSelect = vi.fn();
    const events = [
      makeEvent({
        id: "ev-1",
        action_type: "selection.text",
        session_offset_ms: 2000,
      }),
    ];

    render(
      <MarbleDiagram
        events={events}
        durationMs={10000}
        onSelectEvent={onSelect}
      />,
    );

    fireEvent.click(screen.getByRole("button"));
    expect(onSelect).toHaveBeenCalledWith("ev-1");
  });

  it("shows popover when marble is clicked", () => {
    const events = [
      makeEvent({
        id: "ev-1",
        action_type: "selection.text",
        session_offset_ms: 5000,
      }),
    ];

    render(<MarbleDiagram events={events} durationMs={10000} />);

    fireEvent.click(screen.getByRole("button"));

    expect(screen.getByText("text")).toBeInTheDocument();
    expect(screen.getByText("0:05")).toBeInTheDocument();
  });

  it("shows remove button in popover when onRemoveEvent provided", () => {
    const onRemove = vi.fn();
    const events = [
      makeEvent({
        id: "ev-1",
        action_type: "selection.text",
        session_offset_ms: 2000,
      }),
    ];

    render(
      <MarbleDiagram
        events={events}
        durationMs={10000}
        onRemoveEvent={onRemove}
      />,
    );

    fireEvent.click(screen.getByText("T"));
    fireEvent.click(screen.getByText("Remove"));
    expect(onRemove).toHaveBeenCalledWith("ev-1");
  });
});
