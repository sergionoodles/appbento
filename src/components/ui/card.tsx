import * as React from "react";
import { cn } from "@/lib/utils";

export function Card({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("tui-panel p-4", className)} {...props} />;
}

export function Badge({ className, ...props }: React.HTMLAttributes<HTMLSpanElement>) {
  return (
    <span
      className={cn(
        "inline-flex items-center rounded border border-[var(--border)] px-1.5 py-0.5 text-[11px] tracking-wider uppercase text-[var(--muted)]",
        className
      )}
      {...props}
    />
  );
}

export function Input({ className, ...props }: React.InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      className={cn(
        "bg-[var(--panel2)] border border-[var(--border)] rounded-md px-3 py-1.5 text-[13px] text-[var(--fg)] outline-none focus:border-[var(--accent)] w-full",
        className
      )}
      {...props}
    />
  );
}
