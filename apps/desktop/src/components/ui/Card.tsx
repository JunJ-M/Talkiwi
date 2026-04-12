import type { ReactNode } from "react";

interface CardProps {
  header?: ReactNode;
  children: ReactNode;
  className?: string;
}

export function Card({ header, children, className = "" }: CardProps) {
  return (
    <div className={`card ${className}`}>
      {header && <div className="card-header">{header}</div>}
      <div className="card-body">{children}</div>
    </div>
  );
}
