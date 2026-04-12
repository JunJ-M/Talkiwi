import { create } from "zustand";
import type {
  ActionEvent,
  IntentOutput,
  SessionDetail,
  SpeakSegment,
} from "../types";

interface EditorStore {
  sessionId: string | null;
  audioPath: string | null;
  editedSegments: SpeakSegment[];
  editedEvents: ActionEvent[];
  output: IntentOutput | null;
  isRegenerating: boolean;

  initFromSession: (detail: SessionDetail) => void;
  initFromRecording: (
    sessionId: string,
    segments: SpeakSegment[],
    events: ActionEvent[],
    output: IntentOutput,
    audioPath: string | null,
  ) => void;
  removeSegment: (index: number) => void;
  removeEvent: (eventId: string) => void;
  addEvent: (event: ActionEvent) => void;
  setOutput: (output: IntentOutput) => void;
  setRegenerating: (value: boolean) => void;
  clear: () => void;
}

export const useEditorStore = create<EditorStore>((set) => ({
  sessionId: null,
  audioPath: null,
  editedSegments: [],
  editedEvents: [],
  output: null,
  isRegenerating: false,

  initFromSession: (detail) =>
    set({
      sessionId: detail.session.id,
      audioPath: detail.audio_path,
      editedSegments: detail.segments,
      editedEvents: detail.events,
      output: detail.output,
      isRegenerating: false,
    }),

  initFromRecording: (sessionId, segments, events, output, audioPath) =>
    set({
      sessionId,
      audioPath,
      editedSegments: segments,
      editedEvents: events,
      output,
      isRegenerating: false,
    }),

  removeSegment: (index) =>
    set((state) => ({
      editedSegments: state.editedSegments.filter((_, i) => i !== index),
    })),

  removeEvent: (eventId) =>
    set((state) => ({
      editedEvents: state.editedEvents.filter((e) => e.id !== eventId),
    })),

  addEvent: (event) =>
    set((state) => ({
      editedEvents: [...state.editedEvents, event].sort(
        (a, b) => a.session_offset_ms - b.session_offset_ms,
      ),
    })),

  setOutput: (output) => set({ output, isRegenerating: false }),

  setRegenerating: (value) => set({ isRegenerating: value }),

  clear: () =>
    set({
      sessionId: null,
      audioPath: null,
      editedSegments: [],
      editedEvents: [],
      output: null,
      isRegenerating: false,
    }),
}));
