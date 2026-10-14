import * as React from "react";
import { Check } from "lucide-react";
import { cn } from "@/lib/utils";

export function Checkbox({ className, ...props }: Omit<React.ComponentProps<"input">, "type">) {
  return (
    <span className={cn("relative inline-flex size-4 shrink-0 has-[:disabled]:opacity-45", className)}>
      <input
        type="checkbox"
        className="peer m-0 size-4 appearance-none rounded-[4px] border border-[var(--muted)] bg-[var(--panel2)] transition-colors checked:border-[var(--accent)] checked:bg-[var(--accent)] disabled:cursor-not-allowed focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)]"
        {...props}
      />
      <Check
        aria-hidden="true"
        strokeWidth={3}
        className="pointer-events-none absolute inset-0 size-4 p-0.5 text-[var(--on-accent)] opacity-0 peer-checked:opacity-100"
      />
    </span>
  );
}
