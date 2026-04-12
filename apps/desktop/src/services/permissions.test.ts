import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { permissionsCheck, permissionsRequest } from "./permissions";

vi.mock("@tauri-apps/api/core");
const mockInvoke = vi.mocked(invoke);

describe("permissions service", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("permissionsCheck calls invoke and returns report", async () => {
    const report = {
      entries: [
        { module: "microphone", granted: true, description: "mic" },
      ],
    };
    mockInvoke.mockResolvedValue(report);
    const result = await permissionsCheck();
    expect(mockInvoke).toHaveBeenCalledWith("permissions_check");
    expect(result.entries).toHaveLength(1);
  });

  it("permissionsRequest calls invoke with module name", async () => {
    mockInvoke.mockResolvedValue(false);
    const result = await permissionsRequest("microphone");
    expect(mockInvoke).toHaveBeenCalledWith("permissions_request", { module: "microphone" });
    expect(result).toBe(false);
  });
});
