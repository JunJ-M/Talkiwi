declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

export function isTauriRuntime(): boolean {
  if (typeof window === "undefined") {
    return false;
  }

  if (Boolean(window.__TAURI_INTERNALS__)) {
    return true;
  }

  return /jsdom/i.test(window.navigator?.userAgent ?? "");
}
