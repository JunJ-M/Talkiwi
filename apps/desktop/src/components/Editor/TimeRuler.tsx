interface TimeRulerProps {
  durationMs: number;
}

export function TimeRuler({ durationMs }: TimeRulerProps) {
  const totalSec = Math.ceil(durationMs / 1000);
  const interval = getInterval(totalSec);
  const ticks: number[] = [];

  for (let s = 0; s <= totalSec; s += interval) {
    ticks.push(s);
  }

  return (
    <div className="time-ruler">
      {ticks.map((s) => {
        const left = totalSec > 0 ? (s / totalSec) * 100 : 0;
        return (
          <div
            key={s}
            className="time-ruler-tick"
            style={{ left: `${left}%` }}
          >
            <div className="time-ruler-line" />
            <span className="time-ruler-label">{formatSeconds(s)}</span>
          </div>
        );
      })}
    </div>
  );
}

function getInterval(totalSec: number): number {
  if (totalSec <= 10) return 1;
  if (totalSec <= 30) return 5;
  if (totalSec <= 120) return 10;
  return 30;
}

function formatSeconds(s: number): string {
  const m = Math.floor(s / 60);
  const sec = s % 60;
  if (m === 0) return `${sec}s`;
  return `${m}:${String(sec).padStart(2, "0")}`;
}
