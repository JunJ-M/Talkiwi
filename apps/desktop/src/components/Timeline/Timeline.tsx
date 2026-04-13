import { useEffect, useRef } from "react";
import { useTimelineStore } from "../../stores/timelineStore";
import { SpeakTrack } from "./SpeakTrack";
import { MarbleDiagram } from "../ui/MarbleDiagram";

export function Timeline() {
  const segments = useTimelineStore((s) => s.segments);
  const events = useTimelineStore((s) => s.events);
  const endRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to bottom when new segments or events arrive
  useEffect(() => {
    if (typeof endRef.current?.scrollIntoView === "function") {
      endRef.current.scrollIntoView({ behavior: "smooth", block: "end" });
    }
  }, [segments.length, events.length]);

  // Compute duration from segments and events for marble positioning
  const maxSegmentMs = segments.reduce((max, s) => Math.max(max, s.end_ms), 0);
  const maxEventMs = events.reduce(
    (max, e) => Math.max(max, e.session_offset_ms),
    0,
  );
  const durationMs = Math.max(maxSegmentMs, maxEventMs, 1000);

  return (
    <div className="timeline">
      <div className="timeline-section">
        <span className="timeline-label">Speech</span>
        <SpeakTrack segments={segments} />
      </div>
      <div className="timeline-section">
        <span className="timeline-label">Actions</span>
        <MarbleDiagram events={events} durationMs={durationMs} />
      </div>
      <div ref={endRef} />
    </div>
  );
}
