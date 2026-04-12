import { invoke } from "@tauri-apps/api/core";
import type { ActionEvent } from "../types";

export async function captureScreenshot(): Promise<ActionEvent> {
  return invoke<ActionEvent>("capture_screenshot");
}
