import { invoke } from "@tauri-apps/api/core";
import type { AppConfig } from "../types";

export async function configGet(): Promise<AppConfig> {
  return invoke<AppConfig>("config_get");
}

export async function configUpdate(
  path: string,
  value: unknown,
): Promise<void> {
  return invoke("config_update", { path, value });
}

export async function configUpdateMany(
  updates: Array<{ path: string; value: unknown }>,
): Promise<void> {
  return invoke("config_update_many", { updates });
}
