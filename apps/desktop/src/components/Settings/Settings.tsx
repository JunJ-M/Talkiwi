import { useEffect } from "react";
import { useSettingsStore } from "../../stores/settingsStore";
import { ProviderSettings } from "./ProviderSettings";
import { PermissionSettings } from "./PermissionSettings";
import { QualityPanel } from "./QualityPanel";

export function Settings() {
  const load = useSettingsStore((s) => s.load);

  useEffect(() => {
    load();
  }, [load]);

  return (
    <div className="settings">
      <QualityPanel />
      <PermissionSettings />
      <ProviderSettings />
    </div>
  );
}
