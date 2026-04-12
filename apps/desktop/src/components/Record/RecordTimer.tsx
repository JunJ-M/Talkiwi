interface RecordTimerProps {
  elapsedMs: number;
}

function formatTime(ms: number): string {
  const totalSeconds = Math.floor(ms / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

export function RecordTimer({ elapsedMs }: RecordTimerProps) {
  return <span className="record-timer">{formatTime(elapsedMs)}</span>;
}
