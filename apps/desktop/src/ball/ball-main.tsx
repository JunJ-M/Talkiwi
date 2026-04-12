import React from "react";
import ReactDOM from "react-dom/client";
import { BallApp } from "./BallApp";

ReactDOM.createRoot(document.getElementById("ball-root")!).render(
  <React.StrictMode>
    <BallApp />
  </React.StrictMode>,
);
