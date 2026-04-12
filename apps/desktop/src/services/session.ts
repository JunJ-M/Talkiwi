import { invoke } from "@tauri-apps/api/core";
import type {
  ActionEvent,
  IntentOutput,
  SessionState,
  SpeakSegment,
} from "../types";

export async function sessionStart(): Promise<string> {
  return invoke<string>("session_start");
}

export async function sessionStop(): Promise<IntentOutput> {
  return invoke<IntentOutput>("session_stop");
}

export async function sessionState(): Promise<SessionState> {
  return invoke<SessionState>("session_state");
}

export async function sessionRegenerate(
  sessionId: string,
  segments: SpeakSegment[],
  events: ActionEvent[],
): Promise<IntentOutput> {
  return invoke<IntentOutput>("session_regenerate", {
    sessionId,
    segments,
    events,
  });
}
