import { useRef, useEffect } from "react";
import SiriWaveLib from "siriwave";

interface SiriWaveProps {
  mode: "idle" | "recording";
  size: number;
}

export function SiriWave({ mode, size }: SiriWaveProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const instanceRef = useRef<SiriWaveLib | null>(null);

  useEffect(() => {
    if (!containerRef.current) return;

    if (instanceRef.current) {
      instanceRef.current.dispose();
      instanceRef.current = null;
    }

    const wave = new SiriWaveLib({
      container: containerRef.current,
      style: "ios9",
      width: size,
      height: size,
      speed: 0.1,
      amplitude: 4,
      autostart: true,
      cover: true,
    });

    instanceRef.current = wave;

    return () => {
      wave.dispose();
      instanceRef.current = null;
    };
  }, [size]);

  useEffect(() => {
    const wave = instanceRef.current;
    if (!wave) return;

    if (mode === "recording") {
      wave.setSpeed(0.1);
      wave.setAmplitude(4);
    } else {
      wave.setSpeed(0.05);
      wave.setAmplitude(2);
    }
  }, [mode]);

  return (
    <div
      ref={containerRef}
      className="siriwave-container"
      style={{ width: size, height: size }}
    />
  );
}
