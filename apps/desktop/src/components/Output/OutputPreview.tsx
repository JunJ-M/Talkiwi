import { useOutputStore } from "../../stores/outputStore";
import { useTimelineStore } from "../../stores/timelineStore";
import { Badge } from "../ui/Badge";
import { CopyButton } from "./CopyButton";
import { ReferenceAnnotation } from "./ReferenceAnnotation";

export function OutputPreview() {
  const output = useOutputStore((s) => s.output);
  const events = useTimelineStore((s) => s.events);

  if (!output) {
    return (
      <div className="empty-state">
        <span className="empty-state-icon">...</span>
        <span>No output yet. Record a session first.</span>
      </div>
    );
  }

  return (
    <div className="output-view">
      <div className="output-header">
        <h3>Output</h3>
        <CopyButton text={output.final_markdown} />
      </div>

      {output.task && (
        <div>
          <span className="timeline-label">Task</span>
          <p className="output-content">{output.task}</p>
        </div>
      )}

      {output.intent && (
        <div>
          <span className="timeline-label">Intent</span>
          <p className="output-content">{output.intent}</p>
        </div>
      )}

      {output.constraints.length > 0 && (
        <div className="output-meta">
          {output.constraints.map((c, i) => (
            <Badge key={i} variant="default">
              {c}
            </Badge>
          ))}
        </div>
      )}

      {output.references.length > 0 && (
        <div>
          <span className="timeline-label">References</span>
          <div className="output-meta">
            {output.references.map((ref, i) => (
              <ReferenceAnnotation key={i} reference={ref} events={events} />
            ))}
          </div>
        </div>
      )}

      <div>
        <span className="timeline-label">Final Output</span>
        <div className="output-content">{output.final_markdown}</div>
      </div>
    </div>
  );
}
