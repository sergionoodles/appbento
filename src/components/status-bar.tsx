import { Loader2, TriangleAlert } from "lucide-react";

function plural(count: number, noun: string) {
  return `${count} ${noun}${count === 1 ? "" : "s"}`;
}

export function StatusBar({ status, appCount, groupCount, warning }: {
  status: string | null;
  appCount: number;
  groupCount: number;
  warning: string | null;
}) {
  return (
    <footer className="h-7 shrink-0 flex items-center gap-4 px-3 border-t border-[var(--hairline)] bg-[var(--chrome)] text-[12px] text-[var(--muted)]">
      <div aria-live="polite" className="flex-1 min-w-0">
        {status ? <p role="status" className="flex items-center gap-1.5 truncate text-[var(--fg)]"><Loader2 size={13} className="animate-spin shrink-0" />{status}…</p> : null}
      </div>
      {warning ? <span className="flex items-center gap-1.5 text-[var(--warn)] truncate"><TriangleAlert size={13} className="shrink-0" />{warning}</span> : null}
      <span className="tabular-nums shrink-0">{plural(appCount, "app")} · {plural(groupCount, "group")}</span>
    </footer>
  );
}
