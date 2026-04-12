import type { ActionEvent } from "../../types";

interface ActionEventCardProps {
  event: ActionEvent;
}

const ACTION_ICONS: Record<string, string> = {
  "selection.text": "T",
  "screenshot": "S",
  "clipboard.change": "C",
  "page.current": "P",
  "click.link": "L",
  "file.attach": "F",
};

function formatOffset(ms: number): string {
  const seconds = Math.floor(ms / 1000);
  const minutes = Math.floor(seconds / 60);
  const secs = seconds % 60;
  return `${minutes}:${String(secs).padStart(2, "0")}`;
}

function getDetail(event: ActionEvent): string {
  switch (event.action_type) {
    case "selection.text":
      return event.payload.text;
    case "screenshot":
      return event.payload.image_path;
    case "clipboard.change":
      return event.payload.text ?? event.payload.file_path ?? "Clipboard changed";
    case "page.current":
      return event.payload.url ?? event.payload.title;
    case "click.link":
      return event.payload.to_url;
    case "file.attach":
      return event.payload.file_name;
    default:
      return event.semantic_hint ?? event.action_type;
  }
}

export function ActionEventCard({ event }: ActionEventCardProps) {
  const icon = ACTION_ICONS[event.action_type] ?? "?";
  const typeLabel = event.action_type.split(".").pop() ?? event.action_type;

  return (
    <div className="action-card">
      <span className="action-card-icon">{icon}</span>
      <div className="action-card-content">
        <span className="action-card-type">{typeLabel}</span>
        <div className="action-card-detail">{getDetail(event)}</div>
      </div>
      <span className="action-card-time">
        {formatOffset(event.session_offset_ms)}
      </span>
    </div>
  );
}
