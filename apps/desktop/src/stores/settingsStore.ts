import { create } from "zustand";
import { configGet, configUpdate, configUpdateMany } from "../services/config";
import type { AppConfig } from "../types";

interface SettingsStore {
  config: AppConfig | null;
  loading: boolean;
  error: string | null;
  load: () => Promise<void>;
  update: (path: string, value: unknown) => Promise<void>;
  updateMany: (updates: Array<{ path: string; value: unknown }>) => Promise<void>;
}

export const useSettingsStore = create<SettingsStore>((set) => ({
  config: null,
  loading: false,
  error: null,

  load: async () => {
    set({ loading: true, error: null });
    try {
      const config = await configGet();
      set({ config, error: null });
    } catch (err) {
      set({ error: err instanceof Error ? err.message : String(err) });
    } finally {
      set({ loading: false });
    }
  },

  update: async (path, value) => {
    set({ loading: true, error: null });
    try {
      await configUpdate(path, value);
      const config = await configGet();
      set({ config, error: null });
    } catch (err) {
      set({ error: err instanceof Error ? err.message : String(err) });
    } finally {
      set({ loading: false });
    }
  },

  updateMany: async (updates) => {
    set({ loading: true, error: null });
    try {
      await configUpdateMany(updates);
      const config = await configGet();
      set({ config, error: null });
    } catch (err) {
      set({ error: err instanceof Error ? err.message : String(err) });
    } finally {
      set({ loading: false });
    }
  },
}));
