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
  "window.focus": "W",
  "click.mouse": "M",
};

function formatOffset(ms: number): string {
  const seconds = Math.floor(ms / 1000);
  const minutes = Math.floor(seconds / 60);
  const secs = seconds % 60;
  return `${minutes}:${String(secs).padStart(2, "0")}`;
}

function getDetail(event: ActionEvent): string {
  const p = event.payload as Record<string, unknown>;
  switch (event.action_type) {
    case "selection.text":
      return (p.text as string) ?? "";
    case "screenshot":
      return (p.image_path as string) ?? "";
    case "clipboard.change":
      return (p.text as string) ?? (p.file_path as string) ?? "Clipboard changed";
    case "page.current":
      return (p.url as string) ?? (p.title as string) ?? "";
    case "click.link":
      return (p.to_url as string) ?? "";
    case "file.attach":
      return (p.file_name as string) ?? "";
    case "window.focus":
      return `${(p.window_title as string) ?? ""} ${(p.app_name as string) ?? ""}`.trim();
    case "click.mouse":
      return `${(p.button as string) ?? "click"} @ ${Math.round((p.x as number) ?? 0)},${Math.round((p.y as number) ?? 0)}`;
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
