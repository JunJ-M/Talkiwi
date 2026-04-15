import { useCallback, useState } from "react";
import {
  captureManualNote,
  capturePageContext,
  captureScreenshotRegion,
  captureSelectionText,
} from "../services/trace";
import { permissionsRequest } from "../services/permissions";
import type {
  TracePermissionMatrix,
  TracePermissionModule,
} from "../types";

type ActionId = "screenshot" | "selection" | "page" | "note";

interface ToolbarAction {
  id: ActionId;
  icon: string;
  label: string;
  /** Which permission the button needs — `null` means always available. */
  requires: TracePermissionModule | null;
}

const ACTIONS: readonly ToolbarAction[] = [
  {
    id: "screenshot",
    icon: "camera",
    label: "截图",
    requires: "screen_recording",
  },
  {
    id: "selection",
    icon: "content_paste",
    label: "选中文本",
    requires: "accessibility",
  },
  {
    id: "page",
    icon: "language",
    label: "当前页面",
    requires: "accessibility",
  },
  {
    id: "note",
    icon: "push_pin",
    label: "手动标记",
    requires: null,
  },
];

const MANUAL_NOTE_MAX_CHARS = 280;

interface TraceToolbarProps {
  isRecording: boolean;
  permissions: TracePermissionMatrix;
  onError: (message: string | null) => void;
}

export function TraceToolbar({
  isRecording,
  permissions,
  onError,
}: TraceToolbarProps) {
  const [busy, setBusy] = useState<ActionId | null>(null);
  const [noteDraft, setNoteDraft] = useState<string | null>(null);

  const runAction = useCallback(
    async (action: ToolbarAction) => {
      if (busy) return;
      if (action.requires && !permissions[action.requires]) {
        // Open the system settings pane and surface a gentle hint.
        try {
          await permissionsRequest(action.requires);
        } catch {
          // non-fatal: the user still sees the error below
        }
        onError("需要先在系统偏好设置中授权");
        return;
      }

      if (action.id === "note") {
        onError(null);
        setNoteDraft("");
        return;
      }

      setBusy(action.id);
      onError(null);
      try {
        switch (action.id) {
          case "screenshot":
            await captureScreenshotRegion(null);
            break;
          case "selection":
            await captureSelectionText();
            break;
          case "page":
            await capturePageContext();
            break;
        }
      } catch (err) {
        onError(err instanceof Error ? err.message : String(err));
      } finally {
        setBusy(null);
      }
    },
    [busy, permissions, onError],
  );

  const submitNote = useCallback(async () => {
    if (noteDraft === null) return;
    const trimmed = noteDraft.trim();
    if (trimmed.length === 0) {
      setNoteDraft(null);
      return;
    }
    if (trimmed.length > MANUAL_NOTE_MAX_CHARS) {
      onError(`标记最多 ${MANUAL_NOTE_MAX_CHARS} 字`);
      return;
    }
    setBusy("note");
    onError(null);
    try {
      await captureManualNote(trimmed);
      setNoteDraft(null);
    } catch (err) {
      onError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(null);
    }
  }, [noteDraft, onError]);

  const cancelNote = useCallback(() => {
    setNoteDraft(null);
    onError(null);
  }, [onError]);

  return (
    <section className="widget-section widget-toolbar-section">
      <div className="widget-section-header">
        <h3 className="widget-section-title">
          <span
            className="material-symbols-outlined msi--sm"
            style={{ color: "var(--tki-blue-400)" }}
          >
            bookmark_add
          </span>
          Trace Toolbar
        </h3>
        <span className="widget-section-badge">
          {isRecording ? "READY" : "IDLE"}
        </span>
      </div>

      {!isRecording ? (
        <p className="widget-toolbar-hint">
          开始录制后,用工具栏采集关键上下文。
        </p>
      ) : (
        <>
          <div className="widget-toolbar-grid">
            {ACTIONS.map((action) => {
              const gated =
                action.requires !== null && !permissions[action.requires];
              const isBusy = busy === action.id;
              return (
                <button
                  key={action.id}
                  type="button"
                  className={[
                    "widget-toolbar-btn",
                    gated && "widget-toolbar-btn--disabled",
                    isBusy && "widget-toolbar-btn--busy",
                  ]
                    .filter(Boolean)
                    .join(" ")}
                  onClick={() => void runAction(action)}
                  disabled={busy !== null && !isBusy}
                  title={gated ? "需要授权" : undefined}
                  data-testid={`toolbar-btn-${action.id}`}
                >
                  <span className="material-symbols-outlined">
                    {action.icon}
                  </span>
                  <span className="widget-toolbar-btn-label">
                    {action.label}
                  </span>
                </button>
              );
            })}
          </div>

          {noteDraft !== null && (
            <div className="widget-toolbar-note">
              <textarea
                className="widget-toolbar-note-input"
                placeholder={`记一句话(最多 ${MANUAL_NOTE_MAX_CHARS} 字)`}
                maxLength={MANUAL_NOTE_MAX_CHARS}
                value={noteDraft}
                onChange={(e) => setNoteDraft(e.target.value)}
                autoFocus
                data-testid="toolbar-note-input"
              />
              <div className="widget-toolbar-note-footer">
                <span className="widget-toolbar-note-count">
                  {noteDraft.length} / {MANUAL_NOTE_MAX_CHARS}
                </span>
                <div className="widget-toolbar-note-actions">
                  <button type="button" onClick={cancelNote}>
                    取消
                  </button>
                  <button
                    type="button"
                    className="widget-toolbar-note-submit"
                    onClick={() => void submitNote()}
                  >
                    保存
                  </button>
                </div>
              </div>
            </div>
          )}
        </>
      )}
    </section>
  );
}
