import { useRef, useEffect } from "react";

interface IdleBallProps {
  size: number;
  processing: boolean;
}

export function IdleBall({ size, processing }: IdleBallProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const rafRef = useRef<number>(0);
  const angleRef = useRef(0);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const dpr = window.devicePixelRatio || 1;
    canvas.width = size * dpr;
    canvas.height = size * dpr;
    const ctx = canvas.getContext("2d")!;
    ctx.scale(dpr, dpr);

    const cx = size / 2;
    const cy = size / 2;
    const borderWidth = 2;
    const borderRadius = (size - borderWidth) / 2;
    const dotRadius = 5;
    const red = "#ff3b30";

    function draw() {
      ctx.clearRect(0, 0, size, size);

      // Black background circle
      ctx.beginPath();
      ctx.arc(cx, cy, size / 2 - 1, 0, Math.PI * 2);
      ctx.fillStyle = "rgba(0, 0, 0, 0.88)";
      ctx.fill();

      if (processing) {
        // Spinning arc
        angleRef.current += 0.08;
        const start = angleRef.current;
        const sweep = Math.PI * 0.75; // 3/8 of circle

        ctx.beginPath();
        ctx.arc(cx, cy, borderRadius, start, start + sweep);
        ctx.strokeStyle = red;
        ctx.lineWidth = borderWidth;
        ctx.lineCap = "round";
        ctx.stroke();
      } else {
        // Static full red border
        ctx.beginPath();
        ctx.arc(cx, cy, borderRadius, 0, Math.PI * 2);
        ctx.strokeStyle = red;
        ctx.lineWidth = borderWidth;
        ctx.stroke();
      }

      // Center red dot
      ctx.beginPath();
      ctx.arc(cx, cy, dotRadius, 0, Math.PI * 2);
      ctx.fillStyle = red;
      ctx.fill();

      rafRef.current = requestAnimationFrame(draw);
    }

    draw();

    return () => {
      cancelAnimationFrame(rafRef.current);
    };
  }, [size, processing]);

  return (
    <canvas
      ref={canvasRef}
      style={{ width: size, height: size, borderRadius: "50%" }}
    />
  );
}
