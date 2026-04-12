import { usePermissions } from "../../hooks/usePermissions";
import { Button } from "../ui/Button";

interface OnboardingProps {
  onComplete: () => void;
}

export function Onboarding({ onComplete }: OnboardingProps) {
  const { report, loading, request } = usePermissions();

  if (loading || !report) {
    return (
      <div className="onboarding">
        <h2>Setting up Talkiwi...</h2>
      </div>
    );
  }

  const allGranted = report.entries.every((e) => e.granted);

  return (
    <div className="onboarding">
      <h2>Welcome to Talkiwi</h2>
      <p style={{ color: "oklch(55% 0 0)", fontSize: "var(--text-sm)" }}>
        Grant permissions to get started
      </p>

      <div className="onboarding-steps">
        {report.entries.map((entry) => (
          <div
            key={entry.module}
            className="onboarding-step"
            data-granted={entry.granted}
          >
            <span className="onboarding-step-icon">
              {entry.granted ? "OK" : "--"}
            </span>
            <div className="onboarding-step-info">
              <div className="onboarding-step-title">{entry.module}</div>
              <div className="onboarding-step-desc">{entry.description}</div>
            </div>
            {!entry.granted && (
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

      <Button variant="primary" size="lg" onClick={onComplete}>
        {allGranted ? "Get Started" : "Skip for now"}
      </Button>
    </div>
  );
}
