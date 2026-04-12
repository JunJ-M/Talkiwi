import { useCallback, useEffect, useState } from "react";
import { historyList } from "../../services/history";
import type { SessionSummary } from "../../types";
import { Spinner } from "../ui/Spinner";
import { Button } from "../ui/Button";

interface HistoryListProps {
  onSelect: (id: string) => void;
}

function formatDate(timestamp: number): string {
  return new Date(timestamp).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function formatDuration(ms: number): string {
  const seconds = Math.floor(ms / 1000);
  const minutes = Math.floor(seconds / 60);
  if (minutes < 1) return `${seconds}s`;
  return `${minutes}m ${seconds % 60}s`;
}

export function HistoryList({ onSelect }: HistoryListProps) {
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await historyList(50, 0);
      setSessions(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  if (loading) {
    return (
      <div className="empty-state">
        <Spinner label="加载历史记录" />
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

  if (sessions.length === 0) {
    return (
      <div className="empty-state">
        <span className="empty-state-icon">...</span>
        <span>No sessions yet. Record your first session!</span>
      </div>
    );
  }

  return (
    <div className="history-list">
      {sessions.map((session) => (
        <div
          key={session.id}
          className="history-item"
          onClick={() => onSelect(session.id)}
          role="button"
          tabIndex={0}
          onKeyDown={(e) => {
            if (e.key === "Enter") onSelect(session.id);
          }}
        >
          <div className="history-item-info">
            <div className="history-item-preview">
              {session.preview || "Untitled session"}
            </div>
            <div className="history-item-meta">
              {formatDate(session.started_at)} &middot;{" "}
              {formatDuration(session.duration_ms)}
            </div>
          </div>
          <div className="history-item-stats">
            <span>{session.speak_segment_count} seg</span>
            <span>{session.action_event_count} act</span>
          </div>
        </div>
      ))}
    </div>
  );
}
