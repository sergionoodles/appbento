import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

/** Label above a control, with an optional hint underneath. */
export function Field({ label, htmlFor, hint, children, className }: {
  label: string;
  htmlFor: string;
  hint?: ReactNode;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("flex flex-col gap-1.5 min-w-0", className)}>
      <label htmlFor={htmlFor} className="text-[13px] font-medium">{label}</label>
      {children}
      {hint ? <p className="text-[12.5px] leading-snug text-[var(--muted)]">{hint}</p> : null}
    </div>
  );
}

/** A titled, bordered list whose rows are separated by hairlines. */
export function FieldGroup({ title, description, action, children, footer }: {
  title: string;
  description?: string;
  action?: ReactNode;
  children: ReactNode;
  footer?: ReactNode;
}) {
  return (
    <section aria-label={title} className="flex flex-col gap-2.5">
      <div className="flex items-end justify-between gap-4">
        <div>
          <h4 className="text-[14px] font-semibold">{title}</h4>
          {description ? <p className="text-[12.5px] leading-snug text-[var(--muted)] mt-0.5">{description}</p> : null}
        </div>
        {action}
      </div>
      <div className="rounded-lg border border-[var(--border)] bg-[var(--panel2)]/40 divide-y divide-[var(--border)]">
        {children}
        {footer ? <div className="p-3">{footer}</div> : null}
      </div>
    </section>
  );
}

export function Notice({ tone = "info", icon, children, action }: {
  tone?: "info" | "warn" | "ok";
  icon?: ReactNode;
  children: ReactNode;
  action?: ReactNode;
}) {
  const toneClass = {
    info: "border-[var(--border)] text-[var(--muted)]",
    warn: "border-[color-mix(in_oklab,var(--warn)_45%,transparent)] bg-[color-mix(in_oklab,var(--warn)_8%,transparent)] text-[var(--warn)]",
    ok: "border-[color-mix(in_oklab,var(--ok)_35%,transparent)] bg-[color-mix(in_oklab,var(--ok)_7%,transparent)] text-[var(--ok)]",
  }[tone];
  return (
    <div className={cn("flex items-start gap-3 rounded-lg border px-4 py-3 text-[13px] leading-snug", toneClass)}>
      {icon ? <span className="shrink-0 mt-px">{icon}</span> : null}
      <div className="flex-1 min-w-0">{children}</div>
      {action ? <div className="shrink-0 -my-1">{action}</div> : null}
    </div>
  );
}
