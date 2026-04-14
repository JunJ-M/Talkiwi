import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useEditorStore } from "../../stores/editorStore";
import { Sidebar, type SidebarView } from "./Sidebar";
import { ReviewPanel } from "./ReviewPanel";
import { MarkdownPanel } from "./MarkdownPanel";
import { Settings } from "../Settings/Settings";
import { HistoryDetail } from "../History/HistoryDetail";
import type { SessionDetail } from "../../types";

export function Shell() {
  const [view, setView] = useState<SidebarView>("recent");
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(
    null,
  );
  const initFromSession = useEditorStore((s) => s.initFromSession);

  useEffect(() => {
    const unlistenSessionComplete = listen<SessionDetail>(
      "talkiwi://session-complete",
      (event) => {
        initFromSession(event.payload);
        setView("recent");
        setSelectedSessionId(null);
      },
    );
    const unlistenOpenSettings = listen("talkiwi://open-settings", () => {
      setSelectedSessionId(null);
      setView("settings");
    });
    const unlistenOpenHistory = listen("talkiwi://open-history", () => {
      setSelectedSessionId(null);
      setView("recent");
    });

    return () => {
      unlistenSessionComplete.then((fn) => fn());
      unlistenOpenSettings.then((fn) => fn());
      unlistenOpenHistory.then((fn) => fn());
    };
  }, [initFromSession]);

  function handleSelectSession(id: string) {
    setSelectedSessionId(id);
    setView("recent");
  }

  function handleNavigate(next: SidebarView) {
    setView(next);
    setSelectedSessionId(null);
  }

  function renderCenter() {
    if (view === "settings") {
      return (
        <div className="shell-scroll">
          <Settings />
        </div>
      );
    }
    if (view === "library" || view === "plugins") {
      return (
        <div className="shell-placeholder">
          <div className="shell-placeholder-title">
            {view === "library" ? "Library" : "Plugins"}
          </div>
          <p className="shell-placeholder-body">
            {view === "library"
              ? "Curated prompts, snippets, and reusable intent templates will live here."
              : "Connect capture plugins, intent providers, and custom actions."}
          </p>
        </div>
      );
    }
    if (selectedSessionId) {
      return (
        <div className="shell-scroll">
          <HistoryDetail
            sessionId={selectedSessionId}
            onBack={() => setSelectedSessionId(null)}
          />
        </div>
      );
    }
    return <ReviewPanel />;
  }

  const showRightPane = view === "recent" && !selectedSessionId;

  return (
    <main className="shell" data-view={view}>
      <Sidebar
        activeView={view}
        onNavigate={handleNavigate}
        selectedSessionId={selectedSessionId}
        onSelectSession={handleSelectSession}
      />
      <section className="shell-center" aria-label="Detailed review">
        {renderCenter()}
      </section>
      {showRightPane && (
        <section className="shell-right" aria-label="Markdown editor">
          <MarkdownPanel />
        </section>
      )}
    </main>
  );
}
