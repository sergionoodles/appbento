import { useContext, useEffect, useState, type CSSProperties } from "react";
import type { AppApi } from "@/lib/api";
import type { InstalledApp } from "@/lib/types";
import { extractImagePalette, paletteFromName, type AppPalette } from "@/lib/app-palette";
import { IconBackendContext } from "./icon-backend-context";

interface LoadedAppearance {
  image: string | null;
  palette: AppPalette | null;
  error: string | null;
}

type PaletteResult = { palette: AppPalette | null; error: string | null };
const paletteCaches = new WeakMap<AppApi, Map<string, Promise<PaletteResult>>>();

const appearances = new WeakMap<AppApi, Map<string, Promise<LoadedAppearance>>>();

function loadIcon(backend: AppApi, app?: InstalledApp): Promise<string | null> {
  if (app?.icon_path) return backend.libraryIcon(app.id);
  return Promise.resolve(null);
}

function loadAppearance(backend: AppApi, key: string, app?: InstalledApp) {
  const cache = appearances.get(backend) ?? new Map<string, Promise<LoadedAppearance>>();
  appearances.set(backend, cache);
  const cached = cache.get(key);
  if (cached) return cached;

  const load = async (): Promise<LoadedAppearance> => {
    const image = await loadIcon(backend, app);
    return { image, palette: null, error: app?.icon_path && !image ? "Saved icon is unavailable." : null };
  };
  const pending = load().catch((error: unknown) => ({ image: null, palette: null, error: String(error) }));
  cache.set(key, pending);
  return pending;
}

export function useAppAppearance(name: string, app?: InstalledApp) {
  const backend = useContext(IconBackendContext);
  const key = app?.icon_path ? JSON.stringify([app.id, app.icon_path]) : JSON.stringify([app?.id]);
  const [loaded, setLoaded] = useState<{ key: string; backend: AppApi; appearance: LoadedAppearance } | null>(null);

  useEffect(() => {
    let isCurrent = true;
    void loadAppearance(backend, key, app).then(async (appearance) => {
      if (!isCurrent) return;
      setLoaded({ key, backend, appearance });
      if (!appearance.image) return;

      const palettes = paletteCaches.get(backend) ?? new Map<string, Promise<PaletteResult>>();
      paletteCaches.set(backend, palettes);
      const pending = palettes.get(appearance.image) ?? extractImagePalette(appearance.image)
        .then((palette) => ({ palette, error: null }))
        .catch((error: unknown) => ({ palette: null, error: String(error) }));
      palettes.set(appearance.image, pending);
      const colors = await pending;
      if (isCurrent) setLoaded({ key, backend, appearance: { ...appearance, ...colors } });
    });
    return () => { isCurrent = false; };
  }, [backend, key, app]);

  const appearance = loaded?.key === key && loaded.backend === backend ? loaded.appearance : null;
  const palette = appearance?.palette ?? paletteFromName(name);
  return {
    image: appearance?.image ?? null,
    error: appearance?.error ?? null,
    palette,
    style: { "--tile-accent": palette.accent, "--on-accent": palette.onAccent } as CSSProperties,
  };
}
