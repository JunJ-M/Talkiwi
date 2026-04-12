import { invoke } from "@tauri-apps/api/core";
import type { ModelStatusResponse } from "../types";

export async function modelStatus(): Promise<ModelStatusResponse> {
  return invoke<ModelStatusResponse>("model_status");
}
