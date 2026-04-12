import { render, screen, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { HistoryDetail } from "./HistoryDetail";
import type { SessionDetail } from "../../types";

const mockInvoke = vi.mocked(invoke);

const mockDetail: SessionDetail = {
  session: {
    id: "session-1",
    state: "ready",
    started_at: 1712880000000,
    ended_at: 1712880030000,
    duration_ms: 30000,
  },
  output: {
    session_id: "session-1",
    task: "Fix the login bug",
    intent: "debug",
    constraints: ["TypeScript"],
    missing_context: [],
    restructured_speech: "I need to fix this login bug",
    final_markdown: "## Task\nFix the login bug",
    artifacts: [],
    references: [],
  },
  segments: [
    { text: "I need to fix this bug", start_ms: 0, end_ms: 2000, confidence: 0.95, is_final: true },
  ],
  events: [],
  audio_path: null,
};

describe("HistoryDetail", () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  it("shows spinner while loading", () => {
    mockInvoke.mockImplementation(() => new Promise(() => {}));
    render(<HistoryDetail sessionId="session-1" onBack={() => {}} />);
    expect(screen.getByRole("status")).toBeInTheDocument();
  });

  it("renders session detail after loading", async () => {
    mockInvoke.mockResolvedValue(mockDetail);
    render(<HistoryDetail sessionId="session-1" onBack={() => {}} />);

    await waitFor(() => {
      expect(screen.getByText("Fix the login bug")).toBeInTheDocument();
    });
  });

  it("shows error state with retry on failure", async () => {
    mockInvoke.mockRejectedValue(new Error("Session not found"));
    render(<HistoryDetail sessionId="session-1" onBack={() => {}} />);

    await waitFor(() => {
      expect(screen.getByText(/加载失败/)).toBeInTheDocument();
    });
    expect(screen.getByRole("button", { name: /重试/ })).toBeInTheDocument();
  });
});
