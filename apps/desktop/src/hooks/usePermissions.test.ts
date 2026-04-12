import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, act, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { usePermissions } from "./usePermissions";
import type { PermissionReport } from "../types";

const mockInvoke = vi.mocked(invoke);

const mockReport: PermissionReport = {
  entries: [
    { module: "microphone", granted: true, description: "Audio recording" },
    { module: "accessibility", granted: false, description: "Screen reading" },
    { module: "screen_recording", granted: false, description: "Screen capture" },
  ],
};

describe("usePermissions", () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  it("loads permission report on mount", async () => {
    mockInvoke.mockResolvedValue(mockReport);

    const { result } = renderHook(() => usePermissions());

    expect(result.current.loading).toBe(true);

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.report).toEqual(mockReport);
  });

  it("request calls permissions_request and refreshes", async () => {
    mockInvoke
      .mockResolvedValueOnce(mockReport) // initial check
      .mockResolvedValueOnce(true) // permission request
      .mockResolvedValueOnce({ // refresh after request
        entries: [
          { module: "microphone", granted: true, description: "Audio recording" },
          { module: "accessibility", granted: true, description: "Screen reading" },
          { module: "screen_recording", granted: false, description: "Screen capture" },
        ],
      });

    const { result } = renderHook(() => usePermissions());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    await act(async () => {
      await result.current.request("accessibility");
    });

    expect(mockInvoke).toHaveBeenCalledWith("permissions_request", { module: "accessibility" });
    // After request, report is refreshed
    const accessEntry = result.current.report?.entries.find(
      (e) => e.module === "accessibility",
    );
    expect(accessEntry?.granted).toBe(true);
  });

  it("handles check failure gracefully", async () => {
    mockInvoke.mockRejectedValue(new Error("Permission API unavailable"));

    const { result } = renderHook(() => usePermissions());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    // Report is null on failure
    expect(result.current.report).toBeNull();
  });
});
