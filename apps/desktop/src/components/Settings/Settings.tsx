import { useEffect } from "react";
import { useSettingsStore } from "../../stores/settingsStore";
import { ProviderSettings } from "./ProviderSettings";
import { PermissionSettings } from "./PermissionSettings";

export function Settings() {
  const load = useSettingsStore((s) => s.load);

  useEffect(() => {
    load();
  }, [load]);

  return (
    <div className="settings">
      <PermissionSettings />
      <ProviderSettings />
    </div>
  );
}
