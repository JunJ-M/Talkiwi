import { create } from "zustand";

export type ToastType = "info" | "success" | "error";

export interface Toast {
  id: string;
  message: string;
  type: ToastType;
  duration?: number;
}

interface ToastInput {
  message: string;
  type: ToastType;
  duration?: number;
}

interface ToastStore {
  toasts: Toast[];
  addToast: (input: ToastInput) => void;
  removeToast: (id: string) => void;
  clear: () => void;
}

export const useToastStore = create<ToastStore>((set) => ({
  toasts: [],

  addToast: (input) => {
    const id = crypto.randomUUID();
    const toast: Toast = { id, ...input };
    set((state) => ({ toasts: [...state.toasts, toast] }));
  },

  removeToast: (id) =>
    set((state) => ({ toasts: state.toasts.filter((t) => t.id !== id) })),

  clear: () => set({ toasts: [] }),
}));
