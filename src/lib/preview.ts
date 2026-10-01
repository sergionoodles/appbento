import type { AppApi } from "./api.ts";
import type { InstalledApp, Settings, UpdateReport } from "./types";

const PREVIEW_DATE = "2026-01-01T12:00:00Z";

export function createPreviewApp(name: string, version: string | null): InstalledApp {
  const fileName = `${name}${version ? `-${version}` : ""}.AppImage`;
  const path = `/preview/Applications/${fileName}`;
  return {
    id: path, name, file_path: path, source_path: null, file_name: fileName, size_bytes: 180_000_000,
    executable: true, integrated: true, catalog_id: name.toLowerCase(),
    version_current: version, icon_path: null, desktop_id: null,
    launch_count: 0, last_launched: null, added_at: PREVIEW_DATE,
  };
}

export function createPreviewApi(): AppApi {
  let apps = [createPreviewApp("Obsidian", "1.5.3"), createPreviewApp("Joplin", "2.13.0")];
  let settings: Settings = { scan_dirs: [], managed_dir: "/preview/Applications" };
  const findApp = (id: string) => {
    const app = apps.find((entry) => entry.id === id);
    if (!app) throw new Error("App not found in preview.");
    return app;
  };

  return {
    isTauri: false,
    async libraryList() { return structuredClone(apps); },
    async libraryScan() { return { apps: structuredClone(apps), warnings: [] }; },
    async libraryIntegrate() { throw new Error("File integration requires the desktop app."); },
    async libraryLaunch(id) {
      const app = findApp(id);
      const launched = { ...app, launch_count: app.launch_count + 1, last_launched: new Date().toISOString() };
      apps = apps.map((entry) => entry.id === id ? launched : entry);
      return structuredClone(launched);
    },
    async libraryRemove(id) {
      findApp(id);
      apps = apps.filter((app) => app.id !== id);
    },
    async catalogList() { return []; },
    async settingsGet() { return structuredClone(settings); },
    async settingsSet(patch) {
      settings = { ...settings, ...patch };
      return structuredClone(settings);
    },
    async updatesCheckAll(): Promise<UpdateReport> {
      const app = apps.find((entry) => entry.catalog_id === "obsidian");
      return {
        updates: app ? [{ app_id: app.id, current: app.version_current, latest: "1.6.7", download_url: "https://github.com/obsidianmd/obsidian-releases/releases", asset_name: "Obsidian-1.6.7.AppImage", published_at: PREVIEW_DATE, size_bytes: 185_000_000, status: "available" }] : [],
        errors: [], unknown_versions: [], checked_count: app ? 1 : 0, unsupported_count: apps.length - (app ? 1 : 0),
      };
    },
    async pickAppImage() { return null; },
    async pickFolder() { return null; },
    async openExternal(value) {
      const { validateExternalUrl } = await import("./api.ts");
      const url = validateExternalUrl(value);
      window.open(url, "_blank", "noopener,noreferrer");
    },
  };
}
