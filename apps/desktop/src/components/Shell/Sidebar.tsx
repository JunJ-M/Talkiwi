import { useCallback, useEffect, useMemo, useState } from "react";
import { historyList } from "../../services/history";
import type { SessionSummary } from "../../types";

export type SidebarView = "recent" | "library" | "plugins" | "settings";

interface SidebarProps {
  activeView: SidebarView;
  onNavigate: (view: SidebarView) => void;
  selectedSessionId: string | null;
  onSelectSession: (id: string) => void;
}

interface NavItem {
  id: SidebarView;
  label: string;
  icon: JSX.Element;
}

const NAV_ITEMS: NavItem[] = [
  {
    id: "recent",
    label: "Recent Sessions",
    icon: (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
        <circle cx="12" cy="12" r="9" />
        <path d="M12 7v5l3 2" strokeLinecap="round" />
      </svg>
    ),
  },
  {
    id: "library",
    label: "Library",
    icon: (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
        <path d="M4 6h16M4 12h16M4 18h10" strokeLinecap="round" />
      </svg>
    ),
  },
  {
    id: "plugins",
    label: "Plugins",
    icon: (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
        <path
          d="M10 3v4H6a2 2 0 0 0-2 2v4h4a2 2 0 1 1 0 4H4v4a2 2 0 0 0 2 2h4v-4a2 2 0 1 1 4 0v4h4a2 2 0 0 0 2-2v-4h-4a2 2 0 1 1 0-4h4V9a2 2 0 0 0-2-2h-4V3z"
          strokeLinejoin="round"
        />
      </svg>
    ),
  },
  {
    id: "settings",
    label: "Settings",
    icon: (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
        <circle cx="12" cy="12" r="3" />
        <path d="M19.4 15a1.7 1.7 0 0 0 .34 1.87l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.87-.34 1.7 1.7 0 0 0-1.03 1.56V21a2 2 0 1 1-4 0v-.09a1.7 1.7 0 0 0-1.11-1.56 1.7 1.7 0 0 0-1.87.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.7 1.7 0 0 0 4.57 15a1.7 1.7 0 0 0-1.56-1.03H3a2 2 0 1 1 0-4h.09A1.7 1.7 0 0 0 4.65 8.9a1.7 1.7 0 0 0-.34-1.87l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.7 1.7 0 0 0 1.87.34H9a1.7 1.7 0 0 0 1.03-1.56V3a2 2 0 1 1 4 0v.09a1.7 1.7 0 0 0 1.03 1.56 1.7 1.7 0 0 0 1.87-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.7 1.7 0 0 0-.34 1.87V9c.22.53.72.9 1.3 1H21a2 2 0 1 1 0 4h-.09a1.7 1.7 0 0 0-1.56 1.03z"
          strokeLinejoin="round"
        />
      </svg>
    ),
  },
];

function formatRelativeDate(timestamp: number): string {
  const date = new Date(timestamp);
  return date.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

function formatDurationCompact(ms: number): string {
  const seconds = Math.floor(ms / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  return `${hours}h`;
}

export function Sidebar({
  activeView,
  onNavigate,
  selectedSessionId,
  onSelectSession,
}: SidebarProps) {
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [loading, setLoading] = useState(true);

  const loadSessions = useCallback(async () => {
    setLoading(true);
    try {
      const result = await historyList(25, 0);
      setSessions(result);
    } catch {
      setSessions([]);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadSessions();
  }, [loadSessions]);

  const recentSessions = useMemo(() => sessions.slice(0, 10), [sessions]);

  return (
    <aside className="sidebar" aria-label="Primary">
      <header className="sidebar-header">
        <div className="sidebar-brand">
          <h1 className="sidebar-brand-title">Talkiwi</h1>
          <p className="sidebar-brand-tagline">The Digital Glass Lab</p>
        </div>
        <div className="sidebar-header-actions">
          <button
            type="button"
            className="sidebar-icon-btn"
            aria-label="Open settings"
            onClick={() => onNavigate("settings")}
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
              <circle cx="12" cy="12" r="3" />
              <path d="M19.4 15a1.7 1.7 0 0 0 .34 1.87l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.87-.34 1.7 1.7 0 0 0-1.03 1.56V21a2 2 0 1 1-4 0v-.09a1.7 1.7 0 0 0-1.11-1.56 1.7 1.7 0 0 0-1.87.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.7 1.7 0 0 0 4.57 15a1.7 1.7 0 0 0-1.56-1.03H3a2 2 0 1 1 0-4h.09A1.7 1.7 0 0 0 4.65 8.9a1.7 1.7 0 0 0-.34-1.87l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.7 1.7 0 0 0 1.87.34H9a1.7 1.7 0 0 0 1.03-1.56V3a2 2 0 1 1 4 0v.09a1.7 1.7 0 0 0 1.03 1.56 1.7 1.7 0 0 0 1.87-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.7 1.7 0 0 0-.34 1.87V9c.22.53.72.9 1.3 1H21a2 2 0 1 1 0 4h-.09a1.7 1.7 0 0 0-1.56 1.03z" strokeLinejoin="round" />
            </svg>
          </button>
          <div className="sidebar-avatar" aria-hidden>
            <span>T</span>
          </div>
        </div>
      </header>

      <nav className="sidebar-nav" aria-label="Main navigation">
        {NAV_ITEMS.map((item) => {
          const isActive = activeView === item.id;
          return (
            <button
              key={item.id}
              type="button"
              className="sidebar-nav-item"
              aria-current={isActive ? "page" : undefined}
              onClick={() => onNavigate(item.id)}
            >
              <span className="sidebar-nav-icon" aria-hidden>
                {item.icon}
              </span>
              <span className="sidebar-nav-label">{item.label}</span>
            </button>
          );
        })}
      </nav>

      <div className="sidebar-divider" role="presentation" />

      <div className="sidebar-session-list" aria-label="Recent sessions">
        {loading && (
          <div className="sidebar-session-empty">Loading sessions...</div>
        )}
        {!loading && recentSessions.length === 0 && (
          <div className="sidebar-session-empty">
            No sessions yet. Record your first one.
          </div>
        )}
        {recentSessions.map((session) => {
          const isActive = selectedSessionId === session.id;
          return (
            <button
              key={session.id}
              type="button"
              className="sidebar-session-item"
              aria-current={isActive ? "true" : undefined}
              onClick={() => onSelectSession(session.id)}
            >
              <span className="sidebar-session-title">
                {session.preview || "Untitled session"}
              </span>
              <span className="sidebar-session-meta">
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" className="sidebar-session-meta-icon">
                  <rect x="3" y="4" width="18" height="17" rx="2" />
                  <path d="M8 2v4M16 2v4M3 10h18" strokeLinecap="round" />
                </svg>
                {formatRelativeDate(session.started_at)}
                <span className="sidebar-session-duration">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" className="sidebar-session-meta-icon">
                    <circle cx="12" cy="12" r="9" />
                    <path d="M12 7v5l3 2" strokeLinecap="round" />
                  </svg>
                  {formatDurationCompact(session.duration_ms)}
                </span>
              </span>
            </button>
          );
        })}
      </div>
    </aside>
  );
}
