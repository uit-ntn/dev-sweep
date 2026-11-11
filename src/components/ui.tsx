import type { ButtonHTMLAttributes, ReactNode } from "react";
import type { Risk } from "../types";

type Variant = "primary" | "secondary" | "ghost" | "danger";

const variants: Record<Variant, string> = {
  primary: "bg-accent text-accent-fg hover:opacity-90",
  secondary: "bg-surface-2 text-text border border-border hover:bg-border/60",
  ghost: "text-muted hover:text-text hover:bg-surface-2",
  danger: "bg-caution text-white hover:opacity-90",
};

export function Button({
  variant = "secondary",
  className = "",
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant }) {
  return (
    <button
      {...props}
      className={`inline-flex items-center justify-center gap-2 rounded-md px-3 py-1.5 text-sm font-medium transition disabled:cursor-not-allowed disabled:opacity-50 ${variants[variant]} ${className}`}
    />
  );
}

export function Card({ children, className = "" }: { children: ReactNode; className?: string }) {
  return <section className={`rounded-lg border border-border bg-surface p-4 ${className}`}>{children}</section>;
}

export function Stat({ label, value, hint }: { label: string; value: ReactNode; hint?: ReactNode }) {
  return (
    <Card>
      <div className="text-xs font-medium uppercase tracking-wide text-muted">{label}</div>
      <div className="mt-1 text-2xl font-semibold tabular-nums">{value}</div>
      {hint && <div className="mt-1 text-xs text-muted">{hint}</div>}
    </Card>
  );
}

const riskStyles: Record<Risk, string> = {
  low: "bg-low-soft text-low",
  review: "bg-review-soft text-review",
  caution: "bg-caution-soft text-caution",
};

export const riskLabel: Record<Risk, string> = {
  low: "Low risk",
  review: "Review",
  caution: "Caution",
};

export function RiskBadge({ risk }: { risk: Risk }) {
  return (
    <span className={`inline-flex whitespace-nowrap rounded px-1.5 py-0.5 text-xs font-medium ${riskStyles[risk]}`}>
      {riskLabel[risk]}
    </span>
  );
}

export function Notice({ tone = "info", children }: { tone?: "info" | "warn" | "error"; children: ReactNode }) {
  const styles = {
    info: "border-border bg-surface-2 text-text",
    warn: "border-review/40 bg-review-soft text-text",
    error: "border-caution/40 bg-caution-soft text-text",
  }[tone];
  return (
    <div role={tone === "error" ? "alert" : undefined} className={`rounded-md border px-3 py-2 text-sm ${styles}`}>
      {children}
    </div>
  );
}

export function PageHeader({ title, subtitle, actions }: { title: string; subtitle?: string; actions?: ReactNode }) {
  return (
    <header className="mb-5 flex flex-wrap items-end justify-between gap-3">
      <div>
        <h1 className="text-xl font-semibold">{title}</h1>
        {subtitle && <p className="mt-0.5 text-sm text-muted">{subtitle}</p>}
      </div>
      {actions && <div className="flex flex-wrap gap-2">{actions}</div>}
    </header>
  );
}

export function Empty({ children }: { children: ReactNode }) {
  return <div className="py-10 text-center text-sm text-muted">{children}</div>;
}
