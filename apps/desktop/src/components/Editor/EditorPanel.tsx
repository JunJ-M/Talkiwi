import { useCallback } from "react";
import { useEditorStore } from "../../stores/editorStore";
import { sessionRegenerate } from "../../services/session";
import { TimelineEditor } from "./TimelineEditor";
import { CopyButton } from "../Output/CopyButton";

export function EditorPanel() {
  const {
    sessionId,
    audioPath,
    editedSegments,
    editedEvents,
    output,
    isRegenerating,
    removeSegment,
    removeEvent,
    setOutput,
    setRegenerating,
  } = useEditorStore();

  const handleRegenerate = useCallback(async () => {
    if (!sessionId) return;
    setRegenerating(true);
    try {
      const newOutput = await sessionRegenerate(
        sessionId,
        editedSegments,
        editedEvents,
      );
      setOutput(newOutput);
    } catch (e) {
      console.error("Regenerate failed:", e);
      setRegenerating(false);
    }
  }, [sessionId, editedSegments, editedEvents, setOutput, setRegenerating]);

  if (!sessionId) {
    return (
      <div className="editor-panel-empty">
        <p>等待录制完成...</p>
      </div>
    );
  }

  return (
    <div className="editor-panel">
      <div className="editor-panel-header">
        <h2>时间轴编辑器</h2>
        <div className="editor-panel-actions">
          <button
            className="btn btn-primary btn-sm"
            onClick={handleRegenerate}
            disabled={isRegenerating}
          >
            {isRegenerating ? "生成中..." : "重新生成"}
          </button>
        </div>
      </div>

      <div className="editor-panel-timeline">
        <TimelineEditor
          audioPath={audioPath}
          segments={editedSegments}
          events={editedEvents}
          onRemoveSegment={removeSegment}
          onRemoveEvent={removeEvent}
        />
      </div>

      {output && (
        <div className="editor-panel-output">
          <div className="editor-panel-output-header">
            <h3>意图输出</h3>
            <CopyButton text={output.final_markdown} />
          </div>
          <div className="editor-panel-output-meta">
            <span className="editor-output-badge">{output.intent}</span>
            {output.constraints.map((c, i) => (
              <span key={i} className="editor-output-constraint">
                {c}
              </span>
            ))}
          </div>
          <div className="editor-panel-output-task">
            <strong>任务:</strong> {output.task}
          </div>
          <div className="editor-panel-output-markdown">
            <pre>{output.final_markdown}</pre>
          </div>
        </div>
      )}
    </div>
  );
}
