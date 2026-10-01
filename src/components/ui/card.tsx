import * as React from "react";
import { cn } from "@/lib/utils";

export function Card({ className, ...props }: React.HTMLAttributes<HTMLDivElement>) {
  return <div className={cn("rounded-[10px] border border-[var(--border)] bg-[var(--panel)] p-5", className)} {...props} />;
}

export function Badge({ className, ...props }: React.HTMLAttributes<HTMLSpanElement>) {
  return (
    <span
      className={cn("inline-flex items-center gap-1 rounded-md bg-[var(--panel2)] px-2.5 py-0.5 text-[12px] text-[var(--muted)]", className)}
      {...props}
    />
  );
}

export function Input({ className, ...props }: React.ComponentProps<"input">) {
  return (
    <input
      className={cn(
        "bg-[var(--panel2)] border border-[var(--border)] rounded-md px-4 h-10 text-[14px] text-[var(--fg)] placeholder:text-[var(--muted)] outline-none focus:border-[var(--accent)] w-full",
        className
      )}
      {...props}
    />
  );
}
