import type { ActionEvent, Reference } from "../../types";

interface ReferenceAnnotationProps {
  reference: Reference;
  events: ActionEvent[];
}

export function ReferenceAnnotation({
  reference,
  events,
}: ReferenceAnnotationProps) {
  const resolvedEvent = reference.resolved_event_id
    ? events.find((e) => e.id === reference.resolved_event_id)
    : events[reference.resolved_event_idx] ?? null;

  return (
    <span
      className="badge badge-accent"
      title={
        resolvedEvent
          ? `Refers to: ${resolvedEvent.action_type} at ${resolvedEvent.session_offset_ms}ms`
          : `Reference: ${reference.strategy}`
      }
    >
      {reference.spoken_text}
    </span>
  );
}
