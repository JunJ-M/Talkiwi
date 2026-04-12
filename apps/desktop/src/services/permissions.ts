import { invoke } from "@tauri-apps/api/core";
import type { PermissionReport } from "../types";

export async function permissionsCheck(): Promise<PermissionReport> {
  return invoke<PermissionReport>("permissions_check");
}

export async function permissionsRequest(module: string): Promise<boolean> {
  return invoke<boolean>("permissions_request", { module });
}
