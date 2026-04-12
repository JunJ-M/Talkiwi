import { useRef, useEffect } from "react";
import type { SpeakSegment } from "../../types";

interface SpeakTrackProps {
  segments: SpeakSegment[];
}

export function SpeakTrack({ segments }: SpeakTrackProps) {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (containerRef.current) {
      containerRef.current.scrollTop = containerRef.current.scrollHeight;
    }
  }, [segments.length]);

  if (segments.length === 0) {
    return (
      <div className="empty-state">
        <span className="empty-state-icon">...</span>
        <span>Waiting for speech...</span>
      </div>
    );
  }

  return (
    <div className="speak-track" ref={containerRef}>
      {segments.map((seg, i) => (
        <span
          key={i}
          className="speak-segment"
          data-final={seg.is_final}
        >
          {seg.text}{" "}
        </span>
      ))}
    </div>
  );
}
