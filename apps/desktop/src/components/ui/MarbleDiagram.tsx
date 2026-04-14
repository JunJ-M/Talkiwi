import { useState, useRef, useEffect, useCallback } from "react";
import type { ActionEvent } from "../../types";
import { MarblePopover } from "./MarblePopover";

interface MarbleDiagramProps {
  events: ActionEvent[];
  durationMs: number;
  selectedId?: string | null;
  onSelectEvent?: (id: string) => void;
  onRemoveEvent?: (id: string) => void;
}

const MARBLE_COLORS: Record<string, string> = {
  "selection.text": "oklch(65% 0.18 250)",
  screenshot: "oklch(60% 0.2 300)",
  "clipboard.change": "oklch(65% 0.18 145)",
  "page.current": "oklch(68% 0.18 60)",
  "click.link": "oklch(68% 0.21 190)",
  "file.attach": "oklch(55% 0.05 250)",
  "window.focus": "oklch(72% 0.09 95)",
  "click.mouse": "oklch(74% 0.18 35)",
};

function formatOffset(ms: number): string {
  const seconds = Math.floor(ms / 1000);
  const minutes = Math.floor(seconds / 60);
  const secs = seconds % 60;
  return `${minutes}:${String(secs).padStart(2, "0")}`;
}

/** Extract a short detail string from an action event's payload. */
function getDetail(event: ActionEvent): string {
  const p = event.payload as Record<string, unknown>;
  switch (event.action_type) {
    case "selection.text":
      return (p.text as string) ?? "";
    case "screenshot":
      return (p.image_path as string) ?? "";
    case "clipboard.change":
      return (p.text as string) ?? (p.file_path as string) ?? "";
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

export function MarbleDiagram({
  events,
  durationMs,
  selectedId,
  onSelectEvent,
  onRemoveEvent,
}: MarbleDiagramProps) {
  const [internalSelectedId, setInternalSelectedId] = useState<string | null>(
    null,
  );
  const containerRef = useRef<HTMLDivElement>(null);
  const activeId = selectedId ?? internalSelectedId;
  const safeMax = Math.max(durationMs, 1);

  const handleSelect = useCallback(
    (id: string) => {
      const next = activeId === id ? null : id;
      setInternalSelectedId(next);
      if (next && onSelectEvent) onSelectEvent(next);
    },
    [activeId, onSelectEvent],
  );

  const handleClose = useCallback(() => {
    setInternalSelectedId(null);
  }, []);

  // Close popover on Escape
  useEffect(() => {
    if (!activeId) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") handleClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [activeId, handleClose]);

  if (events.length === 0) {
    return (
      <div className="marble-diagram marble-diagram--empty">
        <div className="marble-line" />
        <span className="marble-empty-label">No actions captured</span>
      </div>
    );
  }

  const selectedEvent = activeId
    ? events.find((e) => e.id === activeId)
    : undefined;

  return (
    <div className="marble-diagram" ref={containerRef}>
      <div className="marble-line" />
      {events.map((event) => {
        const left = (event.session_offset_ms / safeMax) * 100;
        const color =
          MARBLE_COLORS[event.action_type] ?? "oklch(60% 0.05 0)";
        const isActive = event.id === activeId;
        const typeLabel = event.action_type.split(".").pop() ?? "";
        const initial = typeLabel.charAt(0).toUpperCase();

        return (
          <button
            key={event.id}
            className={`marble-circle ${isActive ? "marble-circle--active" : ""}`}
            style={
              {
                left: `${Math.min(left, 97)}%`,
                "--marble-color": color,
              } as React.CSSProperties
            }
            onClick={() => handleSelect(event.id)}
            title={`${event.action_type} @ ${formatOffset(event.session_offset_ms)}`}
            aria-label={`${event.action_type} at ${formatOffset(event.session_offset_ms)}`}
            aria-pressed={isActive}
          >
            {initial}
          </button>
        );
      })}
      {selectedEvent && (
        <MarblePopover
          event={selectedEvent}
          left={(selectedEvent.session_offset_ms / safeMax) * 100}
          onClose={handleClose}
          onRemove={onRemoveEvent}
          getDetail={getDetail}
          formatOffset={formatOffset}
        />
      )}
    </div>
  );
}
