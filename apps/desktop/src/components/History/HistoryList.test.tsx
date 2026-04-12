import { render, screen, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { HistoryList } from "./HistoryList";
import type { SessionSummary } from "../../types";

const mockInvoke = vi.mocked(invoke);

const mockSessions: SessionSummary[] = [
  {
    id: "session-1",
    started_at: 1712880000000,
    duration_ms: 30000,
    speak_segment_count: 5,
    action_event_count: 3,
    preview: "Help me fix this bug",
  },
  {
    id: "session-2",
    started_at: 1712876400000,
    duration_ms: 60000,
    speak_segment_count: 10,
    action_event_count: 7,
    preview: "Review this code",
  },
];

describe("HistoryList", () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  it("shows spinner while loading", () => {
    mockInvoke.mockImplementation(() => new Promise(() => {}));
    render(<HistoryList onSelect={() => {}} />);
    expect(screen.getByRole("status")).toBeInTheDocument();
  });

  it("renders sessions after loading", async () => {
    mockInvoke.mockResolvedValue(mockSessions);
    render(<HistoryList onSelect={() => {}} />);

    await waitFor(() => {
      expect(screen.getByText("Help me fix this bug")).toBeInTheDocument();
    });
    expect(screen.getByText("Review this code")).toBeInTheDocument();
  });

  it("shows empty state when no sessions", async () => {
    mockInvoke.mockResolvedValue([]);
    render(<HistoryList onSelect={() => {}} />);

    await waitFor(() => {
      expect(screen.getByText(/No sessions yet/)).toBeInTheDocument();
    });
  });

  it("shows error state with retry on failure", async () => {
    mockInvoke.mockRejectedValue(new Error("DB connection failed"));
    render(<HistoryList onSelect={() => {}} />);

    await waitFor(() => {
      expect(screen.getByText(/加载失败/)).toBeInTheDocument();
    });
    expect(screen.getByRole("button", { name: /重试/ })).toBeInTheDocument();
  });
});
