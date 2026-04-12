import type { ActionEvent, SpeakSegment } from "../../types";
import { Waveform } from "./Waveform";
import { ActionLane } from "./ActionLane";
import { TimeRuler } from "./TimeRuler";

interface TimelineEditorProps {
  audioPath: string | null;
  segments: SpeakSegment[];
  events: ActionEvent[];
  onRemoveSegment: (index: number) => void;
  onRemoveEvent: (id: string) => void;
}

export function TimelineEditor({
  audioPath,
  segments,
  events,
  onRemoveSegment,
  onRemoveEvent,
}: TimelineEditorProps) {
  // Compute total duration from all data
  const maxSegmentMs = segments.reduce(
    (max, s) => Math.max(max, s.end_ms),
    0,
  );
  const maxEventMs = events.reduce(
    (max, e) => Math.max(max, e.session_offset_ms),
    0,
  );
  const durationMs = Math.max(maxSegmentMs, maxEventMs, 1000);

  return (
    <div className="timeline-editor">
      <Waveform
        audioPath={audioPath}
        segments={segments}
        durationMs={durationMs}
        onRemoveSegment={onRemoveSegment}
      />
      <ActionLane
        events={events}
        durationMs={durationMs}
        onRemoveEvent={onRemoveEvent}
      />
      <TimeRuler durationMs={durationMs} />
    </div>
  );
}
