import { useCallback, useEffect, useState } from "react";
import { historyDetail } from "../../services/history";
import type { SessionDetail } from "../../types";
import { Badge } from "../ui/Badge";
import { Spinner } from "../ui/Spinner";
import { Button } from "../ui/Button";
import { SpeakTrack } from "../Timeline/SpeakTrack";
import { ActionTrack } from "../Timeline/ActionTrack";
import { CopyButton } from "../Output/CopyButton";

interface HistoryDetailProps {
  sessionId: string;
  onBack: () => void;
}

export function HistoryDetail({ sessionId, onBack }: HistoryDetailProps) {
  const [detail, setDetail] = useState<SessionDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await historyDetail(sessionId);
      setDetail(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, [sessionId]);

  useEffect(() => {
    load();
  }, [load]);

  if (loading) {
    return (
      <div className="empty-state">
        <Spinner label="加载会话详情" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="empty-state">
        <span>加载失败: {error}</span>
        <Button variant="secondary" size="sm" onClick={load} aria-label="重试">
          重试
        </Button>
      </div>
    );
  }

  if (!detail) {
    return (
      <div className="empty-state">
        <span>Session not found</span>
      </div>
    );
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "var(--space-md)" }}>
      <div style={{ display: "flex", alignItems: "center", gap: "var(--space-sm)" }}>
        <button className="btn btn-ghost btn-sm" onClick={onBack}>
          Back
        </button>
        <h3 style={{ flex: 1 }}>{detail.output.task || "Session"}</h3>
        <CopyButton text={detail.output.final_markdown} />
      </div>

      {detail.output.intent && (
        <div>
          <span className="timeline-label">Intent</span>
          <p style={{ fontSize: "var(--text-sm)" }}>{detail.output.intent}</p>
        </div>
      )}

      {detail.output.constraints.length > 0 && (
        <div className="output-meta">
          {detail.output.constraints.map((c, i) => (
            <Badge key={i}>{c}</Badge>
          ))}
        </div>
      )}

      <div className="timeline-section">
        <span className="timeline-label">Speech ({detail.segments.length} segments)</span>
        <SpeakTrack segments={detail.segments} />
      </div>

      <div className="timeline-section">
        <span className="timeline-label">Actions ({detail.events.length} events)</span>
        <ActionTrack events={detail.events} />
      </div>

      <div>
        <span className="timeline-label">Final Output</span>
        <div className="output-content">{detail.output.final_markdown}</div>
      </div>
    </div>
  );
}
