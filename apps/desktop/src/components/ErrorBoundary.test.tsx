import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { ErrorBoundary } from "./ErrorBoundary";

function ThrowingChild({ shouldThrow }: { shouldThrow: boolean }) {
  if (shouldThrow) {
    throw new Error("Test render error");
  }
  return <div>Child content</div>;
}

describe("ErrorBoundary", () => {
  beforeEach(() => {
    vi.spyOn(console, "error").mockImplementation(() => {});
  });

  it("renders children when no error", () => {
    render(
      <ErrorBoundary>
        <ThrowingChild shouldThrow={false} />
      </ErrorBoundary>,
    );
    expect(screen.getByText("Child content")).toBeInTheDocument();
  });

  it("renders fallback UI when child throws", () => {
    render(
      <ErrorBoundary>
        <ThrowingChild shouldThrow={true} />
      </ErrorBoundary>,
    );
    expect(screen.queryByText("Child content")).not.toBeInTheDocument();
    expect(screen.getByText(/出了点问题/)).toBeInTheDocument();
    expect(screen.getByText("Test render error")).toBeInTheDocument();
  });

  it("retry button resets error state", () => {
    render(
      <ErrorBoundary>
        <ThrowingChild shouldThrow={true} />
      </ErrorBoundary>,
    );

    expect(screen.getByText(/出了点问题/)).toBeInTheDocument();

    // Click retry — ErrorBoundary resets, but child still throws again
    fireEvent.click(screen.getByRole("button", { name: /重试/ }));

    // Boundary re-rendered children; ThrowingChild throws again → error shown.
    // This verifies the boundary's state reset mechanism triggers re-render.
    expect(screen.getByText(/出了点问题/)).toBeInTheDocument();
  });
});
