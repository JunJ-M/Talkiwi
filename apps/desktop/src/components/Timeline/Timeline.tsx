import { useEffect, useRef } from "react";
import { useTimelineStore } from "../../stores/timelineStore";
import { SpeakTrack } from "./SpeakTrack";
import { ActionTrack } from "./ActionTrack";

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

  return (
    <div className="timeline">
      <div className="timeline-section">
        <span className="timeline-label">Speech</span>
        <SpeakTrack segments={segments} />
      </div>
      <div className="timeline-section">
        <span className="timeline-label">Actions</span>
        <ActionTrack events={events} />
      </div>
      <div ref={endRef} />
    </div>
  );
}
