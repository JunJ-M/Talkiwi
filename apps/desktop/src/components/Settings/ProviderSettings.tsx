import { useEffect, useMemo, useState } from "react";
import { useSettingsStore } from "../../stores/settingsStore";
import { modelStatus } from "../../services/model";
import type { AppConfig, ModelStatusResponse } from "../../types";
import { Button } from "../ui/Button";
import { Badge } from "../ui/Badge";

type DraftState = {
  active_provider: string;
  whisper_model_size: string;
  language: string;
  beam_size: number;
  condition_on_previous_text: boolean;
  initial_prompt: string;
  vad_enabled: boolean;
  vad_threshold: number;
  vad_silence_timeout_ms: number;
  vad_min_speech_duration_ms: number;
  max_segment_ms: number;
  input_gain_db: number;
  cloud_api_key: string;
  ollama_url: string;
  ollama_model: string;
};

function toDraft(config: AppConfig): DraftState {
  return {
    active_provider: config.asr.active_provider,
    whisper_model_size: config.asr.whisper_model_size ?? "small",
    language: config.asr.language ?? "",
    beam_size: config.asr.beam_size,
    condition_on_previous_text: config.asr.condition_on_previous_text,
    initial_prompt: config.asr.initial_prompt ?? "",
    vad_enabled: config.asr.vad_enabled,
    vad_threshold: config.asr.vad_threshold,
    vad_silence_timeout_ms: config.asr.vad_silence_timeout_ms,
    vad_min_speech_duration_ms: config.asr.vad_min_speech_duration_ms,
    max_segment_ms: config.asr.max_segment_ms,
    input_gain_db: config.asr.input_gain_db,
    cloud_api_key: config.asr.cloud_api_key ?? "",
    ollama_url: config.intent.ollama_url,
    ollama_model: config.intent.ollama_model,
  };
}

function applyChinesePreset(draft: DraftState): DraftState {
  return {
    ...draft,
    language: "zh",
    beam_size: 5,
    condition_on_previous_text: true,
    initial_prompt:
      "以下是普通话中文口述，可能包含英文术语、代码符号、变量名、产品名和缩写。请保持原文转写，不要翻译，不要补充解释。",
    vad_enabled: true,
    vad_threshold: 0.02,
    vad_silence_timeout_ms: 800,
    vad_min_speech_duration_ms: 300,
    max_segment_ms: 15000,
    input_gain_db: 8,
  };
}

function formatBytes(bytes: number): string {
  if (bytes <= 0) return "0 B";
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

export function ProviderSettings() {
  const config = useSettingsStore((s) => s.config);
  const loading = useSettingsStore((s) => s.loading);
  const error = useSettingsStore((s) => s.error);
  const updateMany = useSettingsStore((s) => s.updateMany);
  const [draft, setDraft] = useState<DraftState | null>(null);
  const [model, setModel] = useState<ModelStatusResponse | null>(null);
  const [modelError, setModelError] = useState<string | null>(null);

  useEffect(() => {
    if (config) {
      setDraft(toDraft(config));
    }
  }, [config]);

  useEffect(() => {
    if (!config) return;
    let cancelled = false;

    modelStatus()
      .then((status) => {
        if (!cancelled) {
          setModel(status);
          setModelError(null);
        }
      })
      .catch((err) => {
        if (!cancelled) {
          setModel(null);
          setModelError(err instanceof Error ? err.message : String(err));
        }
      });

    return () => {
      cancelled = true;
    };
  }, [config]);

  const isDirty = useMemo(() => {
    if (!config || !draft) return false;
    return JSON.stringify(toDraft(config)) !== JSON.stringify(draft);
  }, [config, draft]);

  if (!config || !draft) {
    return <div className="empty-state"><span>Loading config...</span></div>;
  }

  const isCloud = draft.active_provider === "openai-whisper";

  const save = async () => {
    await updateMany([
      { path: "asr.active_provider", value: draft.active_provider },
      { path: "asr.whisper_model_size", value: draft.whisper_model_size },
      { path: "asr.language", value: draft.language.trim() || null },
      { path: "asr.beam_size", value: Math.max(1, draft.beam_size) },
      {
        path: "asr.cloud_api_key",
        value: draft.cloud_api_key.trim() || null,
      },
      {
        path: "asr.condition_on_previous_text",
        value: draft.condition_on_previous_text,
      },
      {
        path: "asr.initial_prompt",
        value: draft.initial_prompt.trim() || null,
      },
      { path: "asr.vad_enabled", value: draft.vad_enabled },
      {
        path: "asr.vad_threshold",
        value: Math.max(0, draft.vad_threshold),
      },
      {
        path: "asr.vad_silence_timeout_ms",
        value: Math.max(100, draft.vad_silence_timeout_ms),
      },
      {
        path: "asr.vad_min_speech_duration_ms",
        value: Math.max(100, draft.vad_min_speech_duration_ms),
      },
      {
        path: "asr.max_segment_ms",
        value: Math.max(1000, draft.max_segment_ms),
      },
      {
        path: "asr.input_gain_db",
        value: Math.min(24, Math.max(-12, draft.input_gain_db)),
      },
      { path: "intent.ollama_url", value: draft.ollama_url.trim() },
      { path: "intent.ollama_model", value: draft.ollama_model.trim() },
    ]);
  };

  return (
    <div className="settings-section">
      <div className="settings-header-row">
        <span className="settings-label">ASR & Intent</span>
        <div className="settings-header-actions">
          <Button
            variant="secondary"
            size="sm"
            onClick={() => setDraft(applyChinesePreset(draft))}
          >
            应用中文推荐配置
          </Button>
          <Button
            variant="primary"
            size="sm"
            onClick={save}
            disabled={!isDirty || loading}
          >
            {loading ? "保存中..." : "保存配置"}
          </Button>
        </div>
      </div>

      <div className="settings-note">
        社区成熟实现通常会显式指定 `language`、启用 VAD、使用 `beam_size=5`
        左右，并给出技术领域 prompt；对低电平麦克风，还会提供输入增益或预处理选项。这里的配置会在下一次录制直接生效。
      </div>

      <div className="settings-subsection">
        <div className="settings-subtitle">Whisper 运行参数</div>

        <div className="settings-grid">
          <label className="settings-field">
            <span className="settings-row-label">ASR Provider</span>
            <select
              className="settings-input"
              value={draft.active_provider}
              onChange={(e) =>
                setDraft((prev) =>
                  prev ? { ...prev, active_provider: e.target.value } : prev,
                )
              }
            >
              <option value="whisper-local">Whisper Local (whisper.cpp)</option>
              <option value="openai-whisper">OpenAI Whisper API</option>
            </select>
          </label>

          {isCloud && (
            <label className="settings-field">
              <span className="settings-row-label">API Key</span>
              <input
                className="settings-input"
                type="password"
                value={draft.cloud_api_key}
                onChange={(e) =>
                  setDraft((prev) =>
                    prev ? { ...prev, cloud_api_key: e.target.value } : prev,
                  )
                }
                placeholder="sk-..."
              />
            </label>
          )}

          {!isCloud && (
          <label className="settings-field">
            <span className="settings-row-label">模型尺寸</span>
            <select
              className="settings-input"
              value={draft.whisper_model_size}
              onChange={(e) =>
                setDraft((prev) =>
                  prev ? { ...prev, whisper_model_size: e.target.value } : prev,
                )
              }
            >
              <option value="tiny">tiny</option>
              <option value="base">base</option>
              <option value="small">small</option>
              <option value="medium">medium</option>
              <option value="large-v3">large-v3</option>
            </select>
          </label>
          )}

          <label className="settings-field">
            <span className="settings-row-label">语言提示</span>
            <input
              className="settings-input"
              value={draft.language}
              onChange={(e) =>
                setDraft((prev) =>
                  prev ? { ...prev, language: e.target.value } : prev,
                )
              }
              placeholder="留空为 auto，例如 zh / en / ja"
            />
          </label>

          {!isCloud && (
          <label className="settings-field">
            <span className="settings-row-label">Beam Size</span>
            <input
              className="settings-input"
              type="number"
              min={1}
              max={10}
              value={draft.beam_size}
              onChange={(e) =>
                setDraft((prev) =>
                  prev
                    ? { ...prev, beam_size: Number(e.target.value) || 1 }
                    : prev,
                )
              }
            />
          </label>
          )}

          {!isCloud && (
          <label className="settings-field settings-field-checkbox">
            <span className="settings-row-label">保留前文上下文</span>
            <input
              type="checkbox"
              checked={draft.condition_on_previous_text}
              onChange={(e) =>
                setDraft((prev) =>
                  prev
                    ? {
                        ...prev,
                        condition_on_previous_text: e.target.checked,
                      }
                    : prev,
                )
              }
            />
          </label>
          )}

          <label className="settings-field settings-field-checkbox">
            <span className="settings-row-label">启用 VAD 分段</span>
            <input
              type="checkbox"
              checked={draft.vad_enabled}
              onChange={(e) =>
                setDraft((prev) =>
                  prev ? { ...prev, vad_enabled: e.target.checked } : prev,
                )
              }
            />
          </label>

          <label className="settings-field">
            <span className="settings-row-label">VAD 阈值</span>
            <input
              className="settings-input"
              type="number"
              min={0}
              step="0.001"
              value={draft.vad_threshold}
              onChange={(e) =>
                setDraft((prev) =>
                  prev
                    ? { ...prev, vad_threshold: Number(e.target.value) || 0 }
                    : prev,
                )
              }
            />
          </label>

          <label className="settings-field">
            <span className="settings-row-label">静音结束阈值（ms）</span>
            <input
              className="settings-input"
              type="number"
              min={100}
              step="100"
              value={draft.vad_silence_timeout_ms}
              onChange={(e) =>
                setDraft((prev) =>
                  prev
                    ? {
                        ...prev,
                        vad_silence_timeout_ms: Number(e.target.value) || 100,
                      }
                    : prev,
                )
              }
            />
          </label>

          <label className="settings-field">
            <span className="settings-row-label">最短语音长度（ms）</span>
            <input
              className="settings-input"
              type="number"
              min={100}
              step="100"
              value={draft.vad_min_speech_duration_ms}
              onChange={(e) =>
                setDraft((prev) =>
                  prev
                    ? {
                        ...prev,
                        vad_min_speech_duration_ms:
                          Number(e.target.value) || 100,
                      }
                    : prev,
                )
              }
            />
          </label>

          <label className="settings-field">
            <span className="settings-row-label">最大分段时长（ms）</span>
            <input
              className="settings-input"
              type="number"
              min={1000}
              step="1000"
              value={draft.max_segment_ms}
              onChange={(e) =>
                setDraft((prev) =>
                  prev
                    ? {
                        ...prev,
                        max_segment_ms: Number(e.target.value) || 1000,
                      }
                    : prev,
                )
              }
            />
          </label>

          <label className="settings-field">
            <span className="settings-row-label">输入增益（dB）</span>
            <input
              className="settings-input"
              type="number"
              min={-12}
              max={24}
              step="1"
              value={draft.input_gain_db}
              onChange={(e) =>
                setDraft((prev) =>
                  prev
                    ? {
                        ...prev,
                        input_gain_db: Number(e.target.value) || 0,
                      }
                    : prev,
                )
              }
            />
          </label>
        </div>

        <label className="settings-field">
          <span className="settings-row-label">初始提示词</span>
          <textarea
            className="settings-input settings-textarea"
            rows={4}
            value={draft.initial_prompt}
            onChange={(e) =>
              setDraft((prev) =>
                prev ? { ...prev, initial_prompt: e.target.value } : prev,
              )
            }
            placeholder="用于提升中文口语、术语和代码符号的转写稳定性"
          />
        </label>

        {!isCloud && (
        <div className="settings-status-card">
          <div className="settings-status-row">
            <span className="settings-row-label">模型状态</span>
            {model ? (
              <Badge variant={model.exists ? "success" : "warning"}>
                {model.exists ? "已找到模型" : "模型缺失"}
              </Badge>
            ) : (
              <Badge variant="default">未加载</Badge>
            )}
          </div>
          {model && (
            <>
              <div className="settings-status-line">
                当前文件：{model.path}
              </div>
              <div className="settings-status-line">
                文件大小：{formatBytes(model.file_size_bytes)}，推荐体积：
                {model.expected_size_display}
              </div>
            </>
          )}
          {modelError && (
            <div className="settings-status-line">模型状态读取失败：{modelError}</div>
          )}
        </div>
        )}
      </div>

      <div className="settings-subsection">
        <div className="settings-subtitle">Intent Provider</div>
        <div className="settings-grid">
          <label className="settings-field">
            <span className="settings-row-label">Intent Provider</span>
            <input
              className="settings-input"
              value={config.intent.active_provider}
              disabled
            />
          </label>

          <label className="settings-field">
            <span className="settings-row-label">Ollama URL</span>
            <input
              className="settings-input"
              value={draft.ollama_url}
              onChange={(e) =>
                setDraft((prev) =>
                  prev ? { ...prev, ollama_url: e.target.value } : prev,
                )
              }
            />
          </label>

          <label className="settings-field">
            <span className="settings-row-label">Ollama Model</span>
            <input
              className="settings-input"
              value={draft.ollama_model}
              onChange={(e) =>
                setDraft((prev) =>
                  prev ? { ...prev, ollama_model: e.target.value } : prev,
                )
              }
            />
          </label>
        </div>
      </div>

      {error && <div className="settings-error">保存失败：{error}</div>}
    </div>
  );
}
