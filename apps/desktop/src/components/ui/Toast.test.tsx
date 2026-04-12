import { render, screen, act } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { ToastContainer } from "./Toast";
import { useToastStore } from "../../stores/toastStore";

describe("ToastContainer", () => {
  beforeEach(() => {
    useToastStore.getState().clear();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("renders toast messages", () => {
    useToastStore.getState().addToast({ message: "操作成功", type: "success" });
    render(<ToastContainer />);
    expect(screen.getByText("操作成功")).toBeInTheDocument();
  });

  it("renders error toast with error styling", () => {
    useToastStore.getState().addToast({
      message: "连接失败",
      type: "error",
    });
    render(<ToastContainer />);
    const el = screen.getByText("连接失败").closest(".toast");
    expect(el).toHaveClass("toast-error");
  });

  it("auto-dismisses after duration", () => {
    useToastStore.getState().addToast({
      message: "临时消息",
      type: "info",
      duration: 3000,
    });
    render(<ToastContainer />);
    expect(screen.getByText("临时消息")).toBeInTheDocument();

    act(() => {
      vi.advanceTimersByTime(3100);
    });

    expect(screen.queryByText("临时消息")).not.toBeInTheDocument();
  });

  it("shows multiple toasts", () => {
    const store = useToastStore.getState();
    store.addToast({ message: "消息一", type: "info" });
    store.addToast({ message: "消息二", type: "error" });
    render(<ToastContainer />);
    expect(screen.getByText("消息一")).toBeInTheDocument();
    expect(screen.getByText("消息二")).toBeInTheDocument();
  });
});
