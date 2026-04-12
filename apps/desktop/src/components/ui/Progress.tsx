interface ProgressProps {
  value: number; // 0-100
  className?: string;
}

export function Progress({ value, className = "" }: ProgressProps) {
  const clamped = Math.max(0, Math.min(100, value));

  return (
    <div className={`progress ${className}`} role="progressbar" aria-valuenow={clamped} aria-valuemin={0} aria-valuemax={100}>
      <div className="progress-fill" style={{ width: `${clamped}%` }} />
    </div>
  );
}
