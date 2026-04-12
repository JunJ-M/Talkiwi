import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { useSessionStore } from "../../stores/sessionStore";
import { RecordButton } from "./RecordButton";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
  emit: vi.fn(),
}));

describe("RecordButton", () => {
  beforeEach(() => {
    useSessionStore.getState().reset();
  });

  it("renders start recording button in idle state", () => {
    render(<RecordButton />);
    const btn = screen.getByRole("button", { name: /start recording/i });
    expect(btn).toBeInTheDocument();
    expect(btn).not.toBeDisabled();
    expect(btn.dataset.recording).toBe("false");
  });

  it("renders stop recording button when recording", () => {
    useSessionStore.getState().setState("recording");
    render(<RecordButton />);
    const btn = screen.getByRole("button", { name: /stop recording/i });
    expect(btn).toBeInTheDocument();
    expect(btn.dataset.recording).toBe("true");
  });

  it("shows timer when recording", () => {
    useSessionStore.getState().setState("recording");
    useSessionStore.getState().setElapsedMs(65000);
    render(<RecordButton />);
    expect(screen.getByText("01:05")).toBeInTheDocument();
  });

  it("shows processing state", () => {
    useSessionStore.getState().setState("processing");
    render(<RecordButton />);
    const btn = screen.getByRole("button", { name: /processing/i });
    expect(btn).toBeDisabled();
    expect(screen.getByText("Processing...")).toBeInTheDocument();
  });

  it("shows new session button when ready", () => {
    useSessionStore.getState().setState("ready");
    render(<RecordButton />);
    expect(
      screen.getByRole("button", { name: /new session/i }),
    ).toBeInTheDocument();
  });
});
