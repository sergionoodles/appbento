import { useState } from "react";
import { cn } from "@/lib/utils";
import type { InstalledApp } from "@/lib/types";

import { useAppAppearance } from "./use-app-appearance";

function initials(name: string) {
  return name.split(/[\s-]+/).map((word) => word[0]).join("").slice(0, 2).toUpperCase();
}

export function AppIcon({ name, app, className }: { name: string; app?: InstalledApp; className?: string }) {
  const appearance = useAppAppearance(name, app);
  const [failedSource, setFailedSource] = useState<string | null>(null);
  const image = appearance.image !== failedSource ? appearance.image : null;
  const accent = appearance.palette.accent;
  return (
    <div
      aria-hidden="true"
      title={appearance.error ?? undefined}
      className={cn("grid place-items-center shrink-0 rounded-lg font-bold tracking-tight", className)}
      style={{ containerType: "size", background: `color-mix(in oklab, ${accent} 9%, var(--panel2))`, boxShadow: `inset 0 0 0 1px color-mix(in oklab, ${accent} 10%, var(--hairline))` }}
    >
      {image
        ? <img src={image} onError={() => setFailedSource(image)} alt="" draggable={false} className="size-3/5 object-contain" loading="lazy" />
        : <span style={{ color: accent, fontSize: "38cqmin" }} className="leading-none">{initials(name)}</span>}
    </div>
  );
}
