interface PanelHeaderProps {
  onSettingsClick: () => void;
}

export function PanelHeader({ onSettingsClick }: PanelHeaderProps) {
  return (
    <header className="panel-header">
      <h1>Talkiwi</h1>
      <button
        className="btn btn-ghost btn-sm"
        onClick={onSettingsClick}
        aria-label="Settings"
      >
        Settings
      </button>
    </header>
  );
}
