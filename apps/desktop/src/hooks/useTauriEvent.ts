import { useEffect } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export function useTauriEvent<T>(
  event: string,
  handler: (payload: T) => void,
): void {
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let cancelled = false;

    listen<T>(event, (e) => handler(e.payload))
      .then((fn) => {
        if (cancelled) {
          fn();
        } else {
          unlisten = fn;
        }
      })
      .catch((err) => {
        if (!cancelled) {
          console.error(`Failed to listen for ${event}:`, err);
        }
      });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [event, handler]);
}
