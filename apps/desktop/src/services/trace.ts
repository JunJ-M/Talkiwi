import { invoke } from "@tauri-apps/api/core";
import type { ActionEvent } from "../types";

/** Rectangle for a region screenshot, in physical pixels. */
export interface RegionRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/**
 * Capture the currently selected text in the front application.
 * Fails if accessibility permission is denied or nothing is selected.
 */
export async function captureSelectionText(): Promise<ActionEvent> {
  return invoke<ActionEvent>("capture_selection_text");
}

/**
 * Capture the active app + window title as a PageCurrent event.
 * V1 does not populate the URL field — see design doc.
 */
export async function capturePageContext(): Promise<ActionEvent> {
  return invoke<ActionEvent>("capture_page_context");
}

/**
 * Record a short manual note, capped at 280 characters.
 * The backend trims whitespace and rejects empty / oversize payloads.
 */
export async function captureManualNote(text: string): Promise<ActionEvent> {
  return invoke<ActionEvent>("capture_manual_note", { text });
}

/**
 * Capture a screenshot. When `region` is null/undefined the command
 * falls through to a full-screen grab (same path as `captureScreenshot`).
 */
export async function captureScreenshotRegion(
  region?: RegionRect | null,
): Promise<ActionEvent> {
  return invoke<ActionEvent>("capture_screenshot_region", {
    region: region ?? null,
  });
}

/**
 * Soft-delete an event from the active session's timeline. Returns true
 * if a matching event was found. The event stays on disk; it is simply
 * excluded from the prompt.
 */
export async function widgetTraceDeleteEvent(
  eventId: string,
): Promise<boolean> {
  return invoke<boolean>("widget_trace_delete_event", { eventId });
}
