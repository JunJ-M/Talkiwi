import { invoke } from "@tauri-apps/api/core";
import type { AudioInputInfo } from "../types";

export async function audioListInputs(): Promise<AudioInputInfo[]> {
  return invoke<AudioInputInfo[]>("audio_list_inputs");
}

export async function audioGetSelectedInput(): Promise<string | null> {
  return invoke<string | null>("audio_get_selected_input");
}

export async function audioSetSelectedInput(idOrName: string): Promise<void> {
  return invoke("audio_set_selected_input", { idOrName });
}
