import { useCallback, useEffect, useRef, useState } from "react";
import { useToastStore } from "../../stores/toastStore";

interface CopyButtonProps {
  text: string;
}

export function CopyButton({ text }: CopyButtonProps) {
  const [copied, setCopied] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout>>();
  const addToast = useToastStore((s) => s.addToast);

  useEffect(() => {
    return () => clearTimeout(timerRef.current);
  }, []);

  const handleCopy = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      clearTimeout(timerRef.current);
      timerRef.current = setTimeout(() => setCopied(false), 2000);
    } catch {
      addToast({ message: "复制失败", type: "error", duration: 3000 });
    }
  }, [text, addToast]);

  return (
    <button className="copy-btn" onClick={handleCopy}>
      {copied ? "Copied" : "Copy"}
    </button>
  );
}
