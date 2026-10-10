import type { ButtonHTMLAttributes, ReactNode } from "react";

type Variant = "primary" | "secondary" | "ghost" | "danger";

const VARIANTS: Record<Variant, string> = {
  primary:
    "bg-[var(--accent)] text-[var(--on-accent)] shadow-[0_8px_20px_-12px_var(--accent)] hover:bg-[var(--accent-strong)] disabled:opacity-50",
  secondary:
    "bg-[var(--surface)] text-[var(--text)] ring-1 ring-[var(--line)] hover:ring-[var(--accent)] disabled:opacity-50",
  ghost: "text-[var(--muted)] hover:text-[var(--text)] hover:bg-[var(--accent-tint)] disabled:opacity-50",
  danger: "bg-[var(--danger)] text-white hover:brightness-110 disabled:opacity-50",
};

export function Button({
  variant = "secondary",
  className = "",
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant }) {
  return (
    <button
      {...rest}
      className={`inline-flex items-center justify-center gap-2 rounded-xl px-4 py-2 text-sm font-semibold transition focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)] disabled:cursor-not-allowed ${VARIANTS[variant]} ${className}`}
    />
  );
}

export function Card({ children, className = "" }: { children: ReactNode; className?: string }) {
  return (
    <section className={`rounded-2xl border border-[var(--line)] bg-[var(--surface)] p-5 shadow-[var(--shadow)] ${className}`}>
      {children}
    </section>
  );
}

export function ProgressBar({ value, label }: { value: number; label?: string }) {
  const pct = Math.max(0, Math.min(100, value));
  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-valuenow={Math.round(pct)}
      aria-valuemin={0}
      aria-valuemax={100}
      className="h-2 w-full overflow-hidden rounded-full bg-[var(--line)]"
    >
      <div className="h-full rounded-full bg-[var(--accent)] transition-[width] duration-300" style={{ width: `${pct}%` }} />
    </div>
  );
}

/** Medidor de nível: 0 a 1 (RMS). Barras acendem com o volume, na cor de destaque. */
export function LevelMeter({ level, bars = 24 }: { level: number; bars?: number }) {
  const active = Math.round(Math.min(1, level * 4) * bars);
  return (
    <div className="flex h-8 items-end gap-[3px]" aria-label={`Nível de áudio ${Math.round(level * 100)}%`}>
      {Array.from({ length: bars }, (_, i) => (
        <span
          key={i}
          className="w-1.5 rounded-sm transition-colors"
          style={{
            height: `${30 + ((i * 7) % 70)}%`,
            background: i < active ? "var(--accent)" : "var(--line)",
          }}
        />
      ))}
    </div>
  );
}

export function Badge({ tone = "neutral", children }: { tone?: "neutral" | "ok" | "warn" | "error" | "info"; children: ReactNode }) {
  const tones: Record<string, string> = {
    neutral: "bg-[var(--panel)] text-[var(--muted)]",
    ok: "bg-[var(--ok-tint)] text-[var(--ok)]",
    warn: "bg-[var(--warn-tint)] text-[var(--warn-ink)]",
    error: "bg-[var(--danger)]/15 text-[var(--danger)]",
    info: "bg-[var(--accent-tint)] text-[var(--accent-strong)]",
  };
  return (
    <span className={`inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-semibold ${tones[tone]}`}>{children}</span>
  );
}

export function EmptyState({ title, text, action }: { title: string; text: string; action?: ReactNode }) {
  return (
    <div className="flex flex-col items-center gap-2 rounded-2xl border border-dashed border-[var(--line)] bg-[var(--surface)] px-6 py-12 text-center">
      <p className="font-display text-lg font-semibold" style={{ fontFamily: "var(--display)" }}>
        {title}
      </p>
      <p className="max-w-md text-sm text-[var(--muted)]">{text}</p>
      {action && <div className="mt-3">{action}</div>}
    </div>
  );
}

export function Notice({ tone = "info", children }: { tone?: "info" | "warn" | "error" | "ok"; children: ReactNode }) {
  const tones: Record<string, string> = {
    info: "bg-[var(--accent-tint)] text-[var(--text)]",
    warn: "bg-[var(--warn-tint)] text-[var(--warn-ink)]",
    error: "bg-[var(--danger)]/10 text-[var(--danger)]",
    ok: "bg-[var(--ok-tint)] text-[var(--ok)]",
  };
  return (
    <div role={tone === "error" || tone === "warn" ? "alert" : "status"} className={`rounded-xl px-4 py-3 text-sm ${tones[tone]}`}>
      {children}
    </div>
  );
}
