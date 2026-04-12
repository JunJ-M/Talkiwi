import { create } from "zustand";
import type { SessionState } from "../types";

interface SessionStore {
  state: SessionState;
  sessionId: string | null;
  elapsedMs: number;
  setState: (state: SessionState) => void;
  setSessionId: (id: string | null) => void;
  setElapsedMs: (ms: number) => void;
  reset: () => void;
}

export const useSessionStore = create<SessionStore>((set) => ({
  state: "idle",
  sessionId: null,
  elapsedMs: 0,
  setState: (state) => set({ state }),
  setSessionId: (sessionId) => set({ sessionId }),
  setElapsedMs: (elapsedMs) => set({ elapsedMs }),
  reset: () => set({ state: "idle", sessionId: null, elapsedMs: 0 }),
}));
