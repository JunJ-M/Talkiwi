import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../services/config", () => ({
  configGet: vi.fn(),
  configUpdate: vi.fn(),
  configUpdateMany: vi.fn(),
}));

import { configGet, configUpdateMany } from "../services/config";
import { useSettingsStore } from "./settingsStore";

const mockConfig = {
  audio: {
    input_device_id: null,
    input_device_name: null,
  },
  asr: {
    active_provider: "whisper-local",
    whisper_model_path: null,
    whisper_model_size: "tiny",
    language: "zh",
    beam_size: 5,
    condition_on_previous_text: true,
    initial_prompt: "中文 prompt",
    vad_enabled: true,
    vad_threshold: 0.02,
    vad_silence_timeout_ms: 800,
    vad_min_speech_duration_ms: 300,
    max_segment_ms: 15000,
    input_gain_db: 8,
    cloud_api_key: null,
  },
  intent: {
    active_provider: "ollama",
    ollama_url: "http://localhost:11434",
    ollama_model: "qwen2.5:1.5b",
    cloud_api_key: null,
  },
  capture: {
    selection_enabled: true,
    screenshot_enabled: true,
    clipboard_enabled: true,
    page_enabled: true,
    link_enabled: true,
    file_enabled: true,
    selection_poll_interval_ms: 200,
    clipboard_poll_interval_ms: 500,
    selection_min_chars: 3,
  },
  ui: {
    panel_width: 360,
    panel_side: "right",
  },
  storage: {
    output_dir: "~/Talkiwi/sessions",
    db_path: "~/Talkiwi/data/talkiwi.db",
  },
};

describe("settingsStore", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useSettingsStore.setState({
      config: null,
      loading: false,
      error: null,
    });
  });

  it("loads config into store", async () => {
    vi.mocked(configGet).mockResolvedValue(mockConfig);

    await useSettingsStore.getState().load();

    expect(configGet).toHaveBeenCalledTimes(1);
    expect(useSettingsStore.getState().config).toEqual(mockConfig);
    expect(useSettingsStore.getState().error).toBeNull();
  });

  it("updateMany persists and refreshes config", async () => {
    const updated = {
      ...mockConfig,
      asr: {
        ...mockConfig.asr,
        beam_size: 7,
      },
    };

    vi.mocked(configUpdateMany).mockResolvedValue(undefined);
    vi.mocked(configGet).mockResolvedValue(updated);

    await useSettingsStore.getState().updateMany([
      { path: "asr.beam_size", value: 7 },
    ]);

    expect(configUpdateMany).toHaveBeenCalledWith([
      { path: "asr.beam_size", value: 7 },
    ]);
    expect(configGet).toHaveBeenCalledTimes(1);
    expect(useSettingsStore.getState().config).toEqual(updated);
  });
});
