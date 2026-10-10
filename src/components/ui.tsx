import type { ButtonHTMLAttributes, ReactNode } from "react";

type Variant = "primary" | "secondary" | "ghost" | "danger";

const VARIANTS: Record<Variant, string> = {
  primary: "bg-[var(--accent)] text-white hover:brightness-110 disabled:opacity-50",
  secondary:
    "bg-[var(--surface)] text-[var(--text)] ring-1 ring-black/10 hover:bg-black/5 dark:ring-white/10 dark:hover:bg-white/5 disabled:opacity-50",
  ghost: "text-[var(--muted)] hover:text-[var(--text)] hover:bg-black/5 dark:hover:bg-white/5 disabled:opacity-50",
  danger: "bg-red-600 text-white hover:bg-red-700 disabled:opacity-50",
};

export function Button({
  variant = "secondary",
  className = "",
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant }) {
  return (
    <button
      {...rest}
      className={`inline-flex items-center justify-center gap-2 rounded-lg px-4 py-2 text-sm font-medium transition focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)] disabled:cursor-not-allowed ${VARIANTS[variant]} ${className}`}
    />
  );
}

export function Card({ children, className = "" }: { children: ReactNode; className?: string }) {
  return <section className={`rounded-2xl bg-[var(--surface)] p-5 shadow-sm ring-1 ring-black/5 dark:ring-white/5 ${className}`}>{children}</section>;
}

export function ProgressBar({ value, label }: { value: number; label?: string }) {
  const pct = Math.max(0, Math.min(100, value));
  return (
    <div role="progressbar" aria-label={label} aria-valuenow={Math.round(pct)} aria-valuemin={0} aria-valuemax={100} className="h-2 w-full overflow-hidden rounded-full bg-black/10 dark:bg-white/10">
      <div className="h-full rounded-full bg-[var(--accent)] transition-[width] duration-300" style={{ width: `${pct}%` }} />
    </div>
  );
}

/** Medidor de nível: 0 a 1 (RMS). Barras crescem com o volume. */
export function LevelMeter({ level, bars = 24 }: { level: number; bars?: number }) {
  const active = Math.round(Math.min(1, level * 4) * bars);
  return (
    <div className="flex h-8 items-end gap-[3px]" aria-label={`Nível de áudio ${Math.round(level * 100)}%`}>
      {Array.from({ length: bars }, (_, i) => (
        <span
          key={i}
          className={`w-1.5 rounded-sm transition-colors ${i < active ? "bg-emerald-500" : "bg-black/10 dark:bg-white/10"}`}
          style={{ height: `${30 + ((i * 7) % 70)}%` }}
        />
      ))}
    </div>
  );
}

export function Badge({ tone = "neutral", children }: { tone?: "neutral" | "ok" | "warn" | "error" | "info"; children: ReactNode }) {
  const tones = {
    neutral: "bg-black/5 text-[var(--muted)] dark:bg-white/10",
    ok: "bg-emerald-500/15 text-emerald-700 dark:text-emerald-300",
    warn: "bg-amber-500/15 text-amber-700 dark:text-amber-300",
    error: "bg-red-500/15 text-red-700 dark:text-red-300",
    info: "bg-blue-500/15 text-blue-700 dark:text-blue-300",
  };
  return <span className={`inline-flex rounded-full px-2.5 py-0.5 text-xs font-medium ${tones[tone]}`}>{children}</span>;
}

export function EmptyState({ title, text, action }: { title: string; text: string; action?: ReactNode }) {
  return (
    <div className="flex flex-col items-center gap-2 rounded-2xl border border-dashed border-black/10 px-6 py-10 text-center dark:border-white/10">
      <p className="font-medium">{title}</p>
      <p className="max-w-md text-sm text-[var(--muted)]">{text}</p>
      {action && <div className="mt-2">{action}</div>}
    </div>
  );
}

export function Notice({ tone = "info", children }: { tone?: "info" | "warn" | "error" | "ok"; children: ReactNode }) {
  const tones = {
    info: "bg-blue-500/10 text-blue-800 dark:text-blue-200",
    warn: "bg-amber-500/10 text-amber-800 dark:text-amber-200",
    error: "bg-red-500/10 text-red-800 dark:text-red-200",
    ok: "bg-emerald-500/10 text-emerald-800 dark:text-emerald-200",
  };
  return <div role={tone === "error" || tone === "warn" ? "alert" : "status"} className={`rounded-xl px-4 py-3 text-sm ${tones[tone]}`}>{children}</div>;
}
