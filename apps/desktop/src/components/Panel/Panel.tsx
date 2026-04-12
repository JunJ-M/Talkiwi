import type { ReactNode } from "react";
import { PanelHeader } from "./PanelHeader";

type View = "record" | "history" | "settings";

interface PanelProps {
  activeView: View;
  onViewChange: (view: View) => void;
  children: ReactNode;
}

export function Panel({ activeView, onViewChange, children }: PanelProps) {
  return (
    <main className="panel">
      <PanelHeader onSettingsClick={() => onViewChange("settings")} />
      <nav className="nav-tabs" aria-label="Main navigation">
        <button
          className="nav-tab"
          aria-selected={activeView === "record"}
          onClick={() => onViewChange("record")}
        >
          Record
        </button>
        <button
          className="nav-tab"
          aria-selected={activeView === "history"}
          onClick={() => onViewChange("history")}
        >
          History
        </button>
        <button
          className="nav-tab"
          aria-selected={activeView === "settings"}
          onClick={() => onViewChange("settings")}
        >
          Settings
        </button>
      </nav>
      <section className="panel-body">{children}</section>
    </main>
  );
}

export type { View };
