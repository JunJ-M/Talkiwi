import { invoke } from "@tauri-apps/api/core";
import type { SessionDetail, SessionSummary } from "../types";

export async function historyList(
  limit?: number,
  offset?: number,
): Promise<SessionSummary[]> {
  return invoke<SessionSummary[]>("history_list", { limit, offset });
}

export async function historyDetail(id: string): Promise<SessionDetail> {
  return invoke<SessionDetail>("history_detail", { id });
}
