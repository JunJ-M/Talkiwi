import { usePermissions } from "../../hooks/usePermissions";
import { Button } from "../ui/Button";
import { Spinner } from "../ui/Spinner";
import { Badge } from "../ui/Badge";

export function PermissionSettings() {
  const { report, loading, error, refresh, request } = usePermissions();

  if (loading && !report) {
    return (
      <div className="empty-state">
        <Spinner label="检查权限" />
      </div>
    );
  }

  if (error && !report) {
    return (
      <div className="empty-state">
        <span>权限检查失败: {error}</span>
        <Button variant="secondary" size="sm" onClick={refresh} aria-label="重试">
          重试
        </Button>
      </div>
    );
  }

  if (!report) return null;

  return (
    <div className="settings-section">
      <span className="settings-label">Permissions</span>
      {report.entries.map((entry) => (
        <div key={entry.module} className="settings-row">
          <div>
            <div className="settings-row-label">{entry.module}</div>
            <div className="settings-row-value">{entry.description}</div>
          </div>
          {entry.granted ? (
            <Badge variant="success">Granted</Badge>
          ) : (
            <Button
              variant="secondary"
              size="sm"
              onClick={() => request(entry.module)}
            >
              Grant
            </Button>
          )}
        </div>
      ))}
    </div>
  );
}
