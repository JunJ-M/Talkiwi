import { useRef, useEffect, useMemo, useState } from "react";
import WaveSurfer from "wavesurfer.js";
import RegionsPlugin from "wavesurfer.js/dist/plugins/regions.js";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { SpeakSegment } from "../../types";

interface WaveformProps {
  audioPath: string | null;
  segments: SpeakSegment[];
  durationMs: number;
  onRemoveSegment: (index: number) => void;
}

export function Waveform({
  audioPath,
  segments,
  durationMs,
  onRemoveSegment,
}: WaveformProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const wavesurferRef = useRef<WaveSurfer | null>(null);
  const [loadError, setLoadError] = useState(false);

  // Convert file path to Tauri asset URL
  const audioUrl = useMemo(
    () => (audioPath ? convertFileSrc(audioPath) : null),
    [audioPath],
  );

  useEffect(() => {
    if (!containerRef.current || !audioUrl) return;

    setLoadError(false);
    const regions = RegionsPlugin.create();

    const ws = WaveSurfer.create({
      container: containerRef.current,
      waveColor: "oklch(68% 0.15 140 / 0.5)",
      progressColor: "oklch(68% 0.21 140)",
      cursorColor: "oklch(50% 0.1 140)",
      height: 64,
      barWidth: 2,
      barGap: 1,
      barRadius: 2,
      plugins: [regions],
    });

    ws.load(audioUrl);

    ws.on("ready", () => {
      // Add segment regions
      segments.forEach((seg) => {
        regions.addRegion({
          start: seg.start_ms / 1000,
          end: seg.end_ms / 1000,
          content: seg.text.slice(0, 30),
          color: "oklch(68% 0.15 140 / 0.15)",
          drag: false,
          resize: false,
        });
      });
    });

    ws.on("error", (err) => {
      console.error("WaveSurfer load error:", err, "URL:", audioUrl);
      setLoadError(true);
    });

    wavesurferRef.current = ws;

    return () => {
      ws.destroy();
      wavesurferRef.current = null;
    };
  }, [audioUrl, segments]);

  if (!audioPath || loadError) {
    // Fallback: show segment blocks without real waveform
    return (
      <div className="waveform">
        <div className="waveform-label">🎙️ 语音轨</div>
        <div className="waveform-fallback">
          {segments.map((seg, i) => {
            const left =
              durationMs > 0 ? (seg.start_ms / durationMs) * 100 : 0;
            const width =
              durationMs > 0
                ? ((seg.end_ms - seg.start_ms) / durationMs) * 100
                : 10;
            return (
              <div
                key={i}
                className="waveform-segment-block"
                style={{ left: `${left}%`, width: `${Math.max(width, 2)}%` }}
                title={seg.text}
              >
                <span className="waveform-segment-text">
                  {seg.text.slice(0, 20)}
                </span>
                <button
                  className="waveform-segment-remove"
                  onClick={() => onRemoveSegment(i)}
                  aria-label="Remove segment"
                >
                  ×
                </button>
              </div>
            );
          })}
          {segments.length === 0 && (
            <div className="waveform-empty">无语音数据</div>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="waveform">
      <div className="waveform-label">🎙️ 语音轨</div>
      <div ref={containerRef} className="waveform-container" />
      <div className="waveform-segments">
        {segments.map((seg, i) => {
          const left =
            durationMs > 0 ? (seg.start_ms / durationMs) * 100 : 0;
          return (
            <div
              key={i}
              className="waveform-segment-label"
              style={{ left: `${left}%` }}
              title={seg.text}
            >
              <span>{seg.text.slice(0, 25)}</span>
              <button
                className="waveform-segment-remove"
                onClick={() => onRemoveSegment(i)}
                aria-label="Remove segment"
              >
                ×
              </button>
            </div>
          );
        })}
      </div>
    </div>
  );
}
