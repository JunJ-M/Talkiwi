import type { ActionEvent } from "../../types";
import { ActionTag } from "./ActionTag";

interface ActionLaneProps {
  events: ActionEvent[];
  durationMs: number;
  onRemoveEvent: (id: string) => void;
}

export function ActionLane({
  events,
  durationMs,
  onRemoveEvent,
}: ActionLaneProps) {
  const safeMax = Math.max(durationMs, 1);

  return (
    <div className="action-lane">
      <div className="action-lane-label">⚡ 动作轨</div>
      <div className="action-lane-track">
        {events.map((event) => {
          const left = (event.session_offset_ms / safeMax) * 100;
          return (
            <div
              key={event.id}
              className="action-lane-item"
              style={{ left: `${Math.min(left, 95)}%` }}
            >
              <ActionTag event={event} onRemove={onRemoveEvent} />
            </div>
          );
        })}
        {events.length === 0 && (
          <div className="action-lane-empty">无动作事件</div>
        )}
      </div>
    </div>
  );
}
