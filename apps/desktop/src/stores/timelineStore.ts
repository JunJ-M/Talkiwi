import { create } from "zustand";
import type { ActionEvent, SpeakSegment } from "../types";

interface TimelineStore {
  segments: SpeakSegment[];
  events: ActionEvent[];
  addSegment: (segment: SpeakSegment) => void;
  addEvent: (event: ActionEvent) => void;
  clear: () => void;
}

export const useTimelineStore = create<TimelineStore>((set) => ({
  segments: [],
  events: [],
  addSegment: (segment) =>
    set((state) => ({ segments: [...state.segments, segment] })),
  addEvent: (event) =>
    set((state) => ({ events: [...state.events, event] })),
  clear: () => set({ segments: [], events: [] }),
}));
