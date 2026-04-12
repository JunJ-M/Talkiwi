import { useCallback } from "react";
import { SiriWave } from "./SiriWave";
import { IdleBall } from "./IdleBall";
import { useBallState } from "./useBallState";
import "./ball.css";

export function BallApp() {
  const { state, toggle } = useBallState();

  const handleClick = useCallback(() => {
    toggle();
  }, [toggle]);

  return (
    <div className="ball-container" onClick={handleClick}>
      {state === "recording" ? (
        <SiriWave mode="recording" size={40} />
      ) : (
        <IdleBall size={40} processing={state === "processing"} />
      )}
    </div>
  );
}
