import { useRef, useEffect } from "react";
import type { ActionEvent } from "../../types";
import { ActionEventCard } from "./ActionEventCard";

interface ActionTrackProps {
  events: ActionEvent[];
}

export function ActionTrack({ events }: ActionTrackProps) {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (containerRef.current) {
      containerRef.current.scrollTop = containerRef.current.scrollHeight;
    }
  }, [events.length]);

  if (events.length === 0) {
    return (
      <div className="empty-state">
        <span className="empty-state-icon">...</span>
        <span>No actions captured yet</span>
      </div>
    );
  }

  return (
    <div className="action-track" ref={containerRef}>
      {events.map((event) => (
        <ActionEventCard key={event.id} event={event} />
      ))}
    </div>
  );
}
