import { useCallback, useEffect, useState } from "react";
import { permissionsCheck, permissionsRequest } from "../services/permissions";
import type { PermissionReport } from "../types";

export function usePermissions() {
  const [report, setReport] = useState<PermissionReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await permissionsCheck();
      setReport(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const request = useCallback(
    async (module: string) => {
      try {
        await permissionsRequest(module);
      } catch {
        // Request may fail (e.g., system settings unavailable) — still refresh
      }
      await refresh();
    },
    [refresh],
  );

  return { report, loading, error, refresh, request };
}
