import { invoke } from "@tauri-apps/api/core";
import type { QualityOverview } from "../types";

export async function telemetryQualityOverview(): Promise<QualityOverview> {
  return invoke<QualityOverview>("telemetry_quality_overview");
}
