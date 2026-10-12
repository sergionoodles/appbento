import { useState } from "react";
import { cn } from "@/lib/utils";
import type { InstalledApp } from "@/lib/types";

import { useAppAppearance } from "./use-app-appearance";

function initials(name: string) {
  return name.split(/[\s-]+/).map((word) => word[0]).join("").slice(0, 2).toUpperCase();
}

export function AppIcon({ name, app, className, imageClassName = "size-3/5" }: {
  name: string;
  app?: InstalledApp;
  className?: string;
  /** Size of the image inside the tinted plate; smaller leaves more padding around it. */
  imageClassName?: string;
}) {
  const appearance = useAppAppearance(name, app);
  const [failedSource, setFailedSource] = useState<string | null>(null);
  const image = appearance.image !== failedSource ? appearance.image : null;
  const accent = appearance.palette.accent;
  // Lift the plate slightly so near-black logos stay visible on dark themes; light themes barely change.
  const plate = appearance.palette.isVeryDark
    ? "color-mix(in oklab, white 6%, var(--panel2))"
    : `color-mix(in oklab, ${accent} 9%, var(--panel2))`;
  const outline = appearance.palette.isVeryDark
    ? "color-mix(in oklab, white 8%, var(--border))"
    : `color-mix(in oklab, ${accent} 10%, var(--hairline))`;
  return (
    <div
      aria-hidden="true"
      title={appearance.error ?? undefined}
      className={cn("grid place-items-center shrink-0 rounded-lg font-bold tracking-tight", className)}
      style={{ containerType: "size", background: plate, boxShadow: `inset 0 0 0 1px ${outline}` }}
    >
      {image
        ? <img src={image} onError={() => setFailedSource(image)} alt="" draggable={false} className={cn("object-contain", imageClassName)} loading="lazy" />
        : <span style={{ color: accent, fontSize: "38cqmin" }} className="leading-none">{initials(name)}</span>}
    </div>
  );
}
