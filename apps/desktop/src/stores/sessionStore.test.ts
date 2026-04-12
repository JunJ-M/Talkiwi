import { describe, it, expect, beforeEach } from "vitest";
import { useSessionStore } from "./sessionStore";

describe("sessionStore", () => {
  beforeEach(() => {
    useSessionStore.getState().reset();
  });

  it("starts with idle state", () => {
    const { state, sessionId, elapsedMs } = useSessionStore.getState();
    expect(state).toBe("idle");
    expect(sessionId).toBeNull();
    expect(elapsedMs).toBe(0);
  });

  it("updates session state", () => {
    useSessionStore.getState().setState("recording");
    expect(useSessionStore.getState().state).toBe("recording");
  });

  it("sets session id", () => {
    useSessionStore.getState().setSessionId("test-123");
    expect(useSessionStore.getState().sessionId).toBe("test-123");
  });

  it("resets to initial state", () => {
    useSessionStore.getState().setState("recording");
    useSessionStore.getState().setSessionId("test-123");
    useSessionStore.getState().setElapsedMs(5000);

    useSessionStore.getState().reset();

    const { state, sessionId, elapsedMs } = useSessionStore.getState();
    expect(state).toBe("idle");
    expect(sessionId).toBeNull();
    expect(elapsedMs).toBe(0);
  });
});
