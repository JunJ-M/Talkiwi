type SpinnerSize = "sm" | "md" | "lg";

interface SpinnerProps {
  size?: SpinnerSize;
  label?: string;
}

export function Spinner({ size = "md", label }: SpinnerProps) {
  return (
    <span
      className={`spinner spinner-${size}`}
      role="status"
      aria-label={label ?? "加载中"}
    >
      <span className="spinner-circle" />
      {label && <span className="sr-only">{label}</span>}
    </span>
  );
}
