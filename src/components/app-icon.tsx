import { createContext, useContext, useEffect, useState } from "react";
import { api, type AppApi } from "@/lib/api";
import { cn } from "@/lib/utils";
import type { CatalogEntry, InstalledApp } from "@/lib/types";

export const IconBackendContext = createContext<AppApi>(api);

function initials(name: string) {
  return name.split(/[\s-]+/).map((word) => word[0]).join("").slice(0, 2).toUpperCase();
}

export function accentOf(entry?: CatalogEntry) {
  return entry?.branding.accent ?? "var(--accent)";
}

export function AppIcon({ name, entry, app, className }: { name: string; entry?: CatalogEntry; app?: InstalledApp; className?: string }) {
  const backend = useContext(IconBackendContext);
  const [loaded, setLoaded] = useState<{ path: string; url: string } | null>(null);
  const [failedSource, setFailedSource] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const id = app?.id;
  const path = app?.icon_path;

  useEffect(() => {
    if (!id || !path) return;
    let isCurrent = true;
    backend.libraryIcon(id).then((url) => {
      if (!isCurrent) return;
      setLoaded(url ? { path, url } : null);
      setLoadError(url ? null : "Saved icon is unavailable.");
    }).catch((error: unknown) => {
      if (isCurrent) setLoadError(error instanceof Error ? error.message : String(error));
    });
    return () => { isCurrent = false; };
  }, [backend, id, path]);

  const source = path ? (loaded?.path === path ? loaded.url : null) : entry?.branding.logo;
  const image = source && source !== failedSource ? source : null;
  const accent = accentOf(entry);
  return (
    <div
      aria-hidden="true"
      title={path ? loadError ?? undefined : undefined}
      className={cn("grid place-items-center shrink-0 rounded-lg font-bold tracking-tight", className)}
      style={{ containerType: "size", background: `color-mix(in oklab, ${accent} 22%, var(--panel))`, boxShadow: `inset 0 0 0 1px color-mix(in oklab, ${accent} 40%, transparent)` }}
    >
      {image
        ? <img src={image} onError={() => setFailedSource(image)} alt="" className="size-3/5 object-contain" loading="lazy" />
        : <span style={{ color: accent, fontSize: "38cqmin" }} className="leading-none">{initials(name)}</span>}
    </div>
  );
}
