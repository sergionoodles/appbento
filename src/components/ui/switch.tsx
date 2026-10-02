import * as React from "react";
import { cn } from "@/lib/utils";

/** A checkbox drawn as a toggle; it keeps the checkbox role and native keyboard handling. */
export function Switch({ className, ...props }: Omit<React.ComponentProps<"input">, "type">) {
  return (
    <span className={cn("relative inline-flex shrink-0", className)}>
      <input type="checkbox" className="peer absolute inset-0 m-0 opacity-0 disabled:cursor-not-allowed" {...props} />
      <span
        aria-hidden="true"
        className="pointer-events-none h-5 w-9 rounded-full bg-[var(--border)] transition-colors peer-checked:bg-[var(--accent)] peer-disabled:opacity-45 peer-focus-visible:outline-2 peer-focus-visible:outline-offset-2 peer-focus-visible:outline-[var(--accent)] after:absolute after:left-0.5 after:top-0.5 after:size-4 after:rounded-full after:bg-[var(--fg)] after:shadow-sm after:transition-transform peer-checked:after:translate-x-4 peer-checked:after:bg-[var(--on-accent)]"
      />
    </span>
  );
}
