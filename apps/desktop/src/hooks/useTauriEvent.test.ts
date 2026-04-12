import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook } from "@testing-library/react";
import { listen } from "@tauri-apps/api/event";
import { useTauriEvent } from "./useTauriEvent";

vi.mock("@tauri-apps/api/event");
const mockListen = vi.mocked(listen);

describe("useTauriEvent", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("subscribes to the specified event on mount", () => {
    const unlisten = vi.fn();
    mockListen.mockResolvedValue(unlisten);

    const handler = vi.fn();
    renderHook(() => useTauriEvent("talkiwi://test", handler));

    expect(mockListen).toHaveBeenCalledWith("talkiwi://test", expect.any(Function));
  });

  it("calls unlisten on unmount", async () => {
    const unlisten = vi.fn();
    mockListen.mockResolvedValue(unlisten);

    const handler = vi.fn();
    const { unmount } = renderHook(() => useTauriEvent("talkiwi://test", handler));

    // Wait for the listen promise to resolve
    await vi.waitFor(() => expect(unlisten).not.toHaveBeenCalled());
    unmount();
    expect(unlisten).toHaveBeenCalled();
  });
});
