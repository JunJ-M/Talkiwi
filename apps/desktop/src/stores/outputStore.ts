import { create } from "zustand";
import type { IntentOutput } from "../types";

interface OutputStore {
  output: IntentOutput | null;
  setOutput: (output: IntentOutput) => void;
  clear: () => void;
}

export const useOutputStore = create<OutputStore>((set) => ({
  output: null,
  setOutput: (output) => set({ output }),
  clear: () => set({ output: null }),
}));
