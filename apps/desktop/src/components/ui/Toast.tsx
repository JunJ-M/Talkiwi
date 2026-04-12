import { useEffect } from "react";
import { useToastStore, type Toast as ToastData } from "../../stores/toastStore";

const DEFAULT_DURATION = 5000;

function ToastItem({ toast }: { toast: ToastData }) {
  const removeToast = useToastStore((s) => s.removeToast);
  const duration = toast.duration ?? DEFAULT_DURATION;

  useEffect(() => {
    const timer = setTimeout(() => removeToast(toast.id), duration);
    return () => clearTimeout(timer);
  }, [toast.id, duration, removeToast]);

  return (
    <div
      className={`toast toast-${toast.type}`}
    >
      <span className="toast-message">{toast.message}</span>
      <button
        className="toast-close"
        onClick={() => removeToast(toast.id)}
        aria-label="关闭"
      >
        &times;
      </button>
    </div>
  );
}

export function ToastContainer() {
  const toasts = useToastStore((s) => s.toasts);

  if (toasts.length === 0) return null;

  return (
    <div className="toast-container" aria-live="polite">
      {toasts.map((toast) => (
        <ToastItem key={toast.id} toast={toast} />
      ))}
    </div>
  );
}
