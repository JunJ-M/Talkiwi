import type { ActionEvent } from "../../types";

const ACTION_ICONS: Record<string, string> = {
  "selection.text": "📋",
  screenshot: "📸",
  "clipboard.change": "📎",
  "page.current": "🌐",
  "click.link": "🔗",
  "file.attach": "📄",
};

interface ActionTagProps {
  event: ActionEvent;
  onRemove: (id: string) => void;
}

export function ActionTag({ event, onRemove }: ActionTagProps) {
  const icon = ACTION_ICONS[event.action_type] ?? "⚡";
  const label = event.action_type.split(".").pop() ?? event.action_type;

  return (
    <div className="action-tag" title={getDetail(event)}>
      <span className="action-tag-icon">{icon}</span>
      <span className="action-tag-label">{label}</span>
      <button
        className="action-tag-remove"
        onClick={(e) => {
          e.stopPropagation();
          onRemove(event.id);
        }}
        aria-label={`Remove ${label}`}
      >
        ×
      </button>
    </div>
  );
}

function getDetail(event: ActionEvent): string {
  const p = event.payload;
  if ("text" in p && typeof p.text === "string") return p.text.slice(0, 100);
  if ("url" in p && typeof p.url === "string") return p.url;
  if ("title" in p && typeof p.title === "string") return p.title;
  if ("file_name" in p && typeof p.file_name === "string") return p.file_name;
  if ("image_path" in p && typeof p.image_path === "string")
    return p.image_path;
  return event.action_type;
}
