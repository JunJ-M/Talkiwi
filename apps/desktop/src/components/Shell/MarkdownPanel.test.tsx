import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { useEditorStore } from "../../stores/editorStore";
import { MarkdownPanel } from "./MarkdownPanel";

vi.mock("../../services/session", () => ({
  sessionRegenerate: vi.fn(),
}));

function resetEditorStore() {
  useEditorStore.setState({
    sessionId: null,
    audioPath: null,
    editedSegments: [],
    editedEvents: [],
    output: null,
    isRegenerating: false,
  });
}

describe("MarkdownPanel", () => {
  beforeEach(() => {
    resetEditorStore();
    vi.clearAllMocks();
  });

  it("shows empty state when markdown is unavailable", () => {
    render(<MarkdownPanel />);

    expect(screen.getByText("Markdown Editor")).toBeInTheDocument();
    expect(screen.getByText(/No markdown yet/i)).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Copy to Clipboard/i }),
    ).toBeDisabled();
  });

  it("renders structured markdown preview and model choices", () => {
    useEditorStore.setState({
      sessionId: "session-1",
      output: {
        session_id: "session-1",
        task: "Summarize onboarding",
        intent: "Turn spoken notes into markdown",
        intent_category: "summarize",
        constraints: [],
        missing_context: [],
        restructured_speech: "",
        final_markdown: `# User Onboarding Flow

1. Initial trigger detected.
2. Process \`captureActiveState\` for validation.
3. Forward link to the development squad.`,
        artifacts: [],
        references: [],
        output_confidence: 0.95,
        risk_level: "low",
      },
    });

    render(<MarkdownPanel />);

    expect(screen.getByText("Compiled Markdown")).toBeInTheDocument();
    expect(screen.getByText("User Onboarding Flow")).toBeInTheDocument();
    expect(screen.getByText("Initial trigger detected.")).toBeInTheDocument();
    expect(screen.getByText("captureActiveState")).toBeInTheDocument();
    expect(screen.getByRole("option", { name: "Cloud Claude (3.5)" })).toBeInTheDocument();
  });

  it("changes selected model from the dropdown", () => {
    useEditorStore.setState({
      sessionId: "session-1",
      output: {
        session_id: "session-1",
        task: "",
        intent: "",
        intent_category: "unknown",
        constraints: [],
        missing_context: [],
        restructured_speech: "",
        final_markdown: "# Heading",
        artifacts: [],
        references: [],
        output_confidence: 0.5,
        risk_level: "low",
      },
    });

    render(<MarkdownPanel />);

    const select = screen.getByRole("combobox");
    fireEvent.change(select, { target: { value: "cloud-claude" } });
    expect(select).toHaveValue("cloud-claude");
  });
});
