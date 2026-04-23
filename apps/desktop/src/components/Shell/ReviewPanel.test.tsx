import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { useEditorStore } from "../../stores/editorStore";
import { ReviewPanel } from "./ReviewPanel";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
  convertFileSrc: vi.fn((path: string) => `asset://${path}`),
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

describe("ReviewPanel", () => {
  beforeEach(() => {
    resetEditorStore();
  });

  it("shows empty state before any session is loaded", () => {
    render(<ReviewPanel />);

    expect(screen.getByText("Waiting for a session")).toBeInTheDocument();
    expect(
      screen.getByText(/Once a session completes, the detailed review/i),
    ).toBeInTheDocument();
  });

  it("renders dashboard review tracks from session data", () => {
    useEditorStore.setState({
      sessionId: "session-1",
      editedSegments: [
        {
          text: "Let's document the user onboarding flow first.",
          start_ms: 5000,
          end_ms: 9000,
          confidence: 0.92,
          is_final: true,
        },
      ],
      editedEvents: [
        {
          id: "evt-1",
          session_id: "session-1",
          timestamp: 1,
          session_offset_ms: 12000,
          duration_ms: null,
          action_type: "selection.text",
          plugin_id: "builtin",
          payload: {
            text: "captureActiveState()",
            app_name: "VSCode",
            window_title: "editor.tsx",
            char_count: 20,
          },
          semantic_hint: "Selected code",
          confidence: 1,
        },
        {
          id: "evt-2",
          session_id: "session-1",
          timestamp: 2,
          session_offset_ms: 18000,
          duration_ms: null,
          action_type: "click.link",
          plugin_id: "builtin",
          payload: {
            from_url: "https://talkiwi.dev",
            to_url: "https://talkiwi.dev/spec",
            title: "Product spec",
          },
          semantic_hint: "Opened spec",
          confidence: 1,
        },
      ],
    });

    render(<ReviewPanel />);

    expect(screen.getByText("Timeline Analysis")).toBeInTheDocument();
    expect(screen.getByText("Detailed Review")).toBeInTheDocument();
    expect(screen.getByText("Speak")).toBeInTheDocument();
    expect(screen.getByText("Action")).toBeInTheDocument();
    expect(
      screen.getByText("Let's document the user onboarding flow first."),
    ).toBeInTheDocument();
    expect(screen.getByText("Selected text")).toBeInTheDocument();
    expect(screen.getByText("captureActiveState()")).toBeInTheDocument();
    expect(screen.getByText("Product spec")).toBeInTheDocument();
  });

  it("renders every speech segment and action instead of truncating at four", () => {
    useEditorStore.setState({
      sessionId: "session-1",
      editedSegments: Array.from({ length: 5 }, (_, index) => ({
        text: `Segment ${index + 1}`,
        start_ms: index * 1000,
        end_ms: index * 1000 + 800,
        confidence: 0.9,
        is_final: true,
      })),
      editedEvents: Array.from({ length: 6 }, (_, index) => ({
        id: `evt-${index + 1}`,
        session_id: "session-1",
        timestamp: index + 1,
        session_offset_ms: index * 1000,
        duration_ms: null,
        action_type: "selection.text",
        plugin_id: "builtin",
        payload: {
          text: `Action payload ${index + 1}`,
          app_name: "VSCode",
          window_title: "editor.tsx",
          char_count: 20,
        },
        semantic_hint: null,
        confidence: 1,
      })),
    });

    render(<ReviewPanel />);

    expect(screen.getByText("Segment 5")).toBeInTheDocument();
    expect(screen.getByText("Action payload 6")).toBeInTheDocument();
  });

  it("shows an audio fallback when the session has a recording but no transcript", () => {
    useEditorStore.setState({
      sessionId: "session-1",
      audioPath: "/tmp/talkiwi/sessions/session-1/audio.wav",
      editedSegments: [],
      editedEvents: [],
    });

    render(<ReviewPanel />);

    expect(screen.getByText(/Captured audio: audio\.wav/i)).toBeInTheDocument();
    expect(
      screen.queryByText("No transcribed speech yet for this session."),
    ).not.toBeInTheDocument();
  });
});
