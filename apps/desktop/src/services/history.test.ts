import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { historyList, historyDetail } from "./history";

vi.mock("@tauri-apps/api/core");
const mockInvoke = vi.mocked(invoke);

describe("history service", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("historyList calls invoke with limit and offset", async () => {
    mockInvoke.mockResolvedValue([]);
    const result = await historyList(10, 0);
    expect(mockInvoke).toHaveBeenCalledWith("history_list", { limit: 10, offset: 0 });
    expect(result).toEqual([]);
  });

  it("historyList passes undefined for optional params", async () => {
    mockInvoke.mockResolvedValue([]);
    await historyList();
    expect(mockInvoke).toHaveBeenCalledWith("history_list", { limit: undefined, offset: undefined });
  });

  it("historyDetail calls invoke with session id", async () => {
    const detail = { session: {}, output: {}, segments: [], events: [] };
    mockInvoke.mockResolvedValue(detail);
    const result = await historyDetail("abc-123");
    expect(mockInvoke).toHaveBeenCalledWith("history_detail", { id: "abc-123" });
    expect(result).toEqual(detail);
  });
});
