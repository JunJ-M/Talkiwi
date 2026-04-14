import { useRef, useEffect } from "react";
import type { ActionEvent } from "../../types";

interface MarblePopoverProps {
  event: ActionEvent;
  left: number;
  onClose: () => void;
  onRemove?: (id: string) => void;
  getDetail: (event: ActionEvent) => string;
  formatOffset: (ms: number) => string;
}

export function MarblePopover({
  event,
  left,
  onClose,
  onRemove,
  getDetail,
  formatOffset,
}: MarblePopoverProps) {
  const popoverRef = useRef<HTMLDivElement>(null);

  // Close on click outside
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (
        popoverRef.current &&
        !popoverRef.current.contains(e.target as Node)
      ) {
        onClose();
      }
    };
    // Delay to avoid catching the marble click that opened us
    const id = setTimeout(() => {
      document.addEventListener("mousedown", handler);
    }, 0);
    return () => {
      clearTimeout(id);
      document.removeEventListener("mousedown", handler);
    };
  }, [onClose]);

  const typeLabel = event.action_type.split(".").pop() ?? event.action_type;
  const detail = getDetail(event);
  // Position popover: clamp so it doesn't overflow
  const clampedLeft = Math.max(10, Math.min(left, 70));

  return (
    <div
      ref={popoverRef}
      className="marble-popover"
      style={{ left: `${clampedLeft}%` }}
    >
      <div className="marble-popover-header">
        <span className="marble-popover-type">{typeLabel}</span>
        <span className="marble-popover-time">
          {formatOffset(event.session_offset_ms)}
        </span>
      </div>
      {detail && (
        <div className="marble-popover-detail" title={detail}>
          {detail}
        </div>
      )}
      {event.semantic_hint && (
        <div className="marble-popover-hint">{event.semantic_hint}</div>
      )}
      {onRemove && (
        <button
          className="marble-popover-remove"
          onClick={() => onRemove(event.id)}
          aria-label="Remove action"
        >
          Remove
        </button>
      )}
    </div>
  );
}
