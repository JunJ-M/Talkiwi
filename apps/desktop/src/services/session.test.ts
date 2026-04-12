import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { sessionStart, sessionStop, sessionState } from "./session";

vi.mock("@tauri-apps/api/core");
const mockInvoke = vi.mocked(invoke);

describe("session service", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("sessionStart calls invoke with session_start", async () => {
    mockInvoke.mockResolvedValue("test-uuid");
    const result = await sessionStart();
    expect(mockInvoke).toHaveBeenCalledWith("session_start");
    expect(result).toBe("test-uuid");
  });

  it("sessionStop calls invoke with session_stop", async () => {
    const output = { session_id: "123", task: "test", intent: "", constraints: [], missing_context: [], restructured_speech: "", final_markdown: "", artifacts: [], references: [] };
    mockInvoke.mockResolvedValue(output);
    const result = await sessionStop();
    expect(mockInvoke).toHaveBeenCalledWith("session_stop");
    expect(result.session_id).toBe("123");
  });

  it("sessionState calls invoke with session_state", async () => {
    mockInvoke.mockResolvedValue("idle");
    const result = await sessionState();
    expect(mockInvoke).toHaveBeenCalledWith("session_state");
    expect(result).toBe("idle");
  });
});
