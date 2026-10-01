import * as React from "react";
import { ChevronDown } from "lucide-react";
import { cn } from "@/lib/utils";

/** Native select with the platform chrome removed so it follows the active theme. */
export function Select({ className, children, ...props }: React.ComponentProps<"select">) {
  return (
    <span className={cn("relative inline-flex w-full", className)}>
      <select
        className="peer appearance-none w-full h-10 rounded-md border border-[var(--border)] bg-[var(--panel2)] pl-3.5 pr-9 text-[14px] text-[var(--fg)] cursor-pointer outline-none transition-colors hover:border-[var(--muted)] focus:border-[var(--accent)] disabled:opacity-45 disabled:cursor-not-allowed"
        {...props}
      >
        {children}
      </select>
      <ChevronDown aria-hidden="true" size={16} className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-[var(--muted)] peer-focus:text-[var(--accent)]" />
    </span>
  );
}
