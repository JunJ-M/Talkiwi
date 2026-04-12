import { useState, useEffect } from "react";
import "./styles/global.css";
import { useEditorStore } from "./stores/editorStore";
import { Panel, type View } from "./components/Panel/Panel";
import { EditorPanel } from "./components/Editor/EditorPanel";
import { HistoryList } from "./components/History/HistoryList";
import { HistoryDetail } from "./components/History/HistoryDetail";
import { Settings } from "./components/Settings/Settings";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { ToastContainer } from "./components/ui/Toast";
import { listen } from "@tauri-apps/api/event";
import type { SessionDetail } from "./types";

function App() {
  const [view, setView] = useState<View>("record");
  const [selectedSessionId, setSelectedSessionId] = useState<string | null>(
    null,
  );
  const initFromSession = useEditorStore((s) => s.initFromSession);

  // Listen for session-complete event to initialize editor with audio path
  useEffect(() => {
    const unlistenSessionComplete = listen<SessionDetail>(
      "talkiwi://session-complete",
      (event) => {
        initFromSession(event.payload);
        setView("record");
      },
    );

    return () => {
      unlistenSessionComplete.then((fn) => fn());
    };
  }, [initFromSession]);

  function renderEditorView() {
    return <EditorPanel />;
  }

  function renderHistoryView() {
    if (selectedSessionId) {
      return (
        <HistoryDetail
          sessionId={selectedSessionId}
          onBack={() => setSelectedSessionId(null)}
        />
      );
    }
    return <HistoryList onSelect={setSelectedSessionId} />;
  }

  function renderContent() {
    switch (view) {
      case "record":
        return renderEditorView();
      case "history":
        return renderHistoryView();
      case "settings":
        return <Settings />;
    }
  }

  return (
    <ErrorBoundary>
      <Panel
        activeView={view}
        onViewChange={(v) => {
          setView(v);
          setSelectedSessionId(null);
        }}
      >
        <div className="view-content" key={view}>
          {renderContent()}
        </div>
      </Panel>
      <ToastContainer />
    </ErrorBoundary>
  );
}

export default App;
