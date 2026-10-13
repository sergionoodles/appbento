import type { AppApi } from "./api.ts";
import { restrictedPermissions } from "./sandbox";
import type { InstalledApp, Settings, UpdateCheckResult, UpdateConfig, UpdateInfo, UpdateReport } from "./types";

const PREVIEW_DATE = "2026-01-01T12:00:00Z";

export function createPreviewApp(name: string, version: string | null): InstalledApp {
  const fileName = `${name}${version ? `-${version}` : ""}.AppImage`;
  const path = `/preview/Applications/${fileName}`;
  return {
    launch_config: null, update_config: null,
    id: path, name, file_path: path, source_path: null, file_name: fileName, size_bytes: 180_000_000,
    executable: true, integrated: true,
    version_current: version, icon_path: null, desktop_id: null,
    launch_count: 0, last_launched: null, added_at: PREVIEW_DATE, file_created_at: PREVIEW_DATE,
  };
}

export function createPreviewApi(): AppApi {
  let apps: InstalledApp[] = [createPreviewApp("Obsidian", "1.5.3"), createPreviewApp("Joplin", "2.13.0")].map((app) => ({ ...app, launch_config: { mode: "direct" as const, permissions: restrictedPermissions() } }));
  apps = apps.map((app, index) => ({ ...app, update_config: index === 0 ? {
    strategy: "github-release" as const, repo: "obsidianmd/obsidian-releases", asset_regex: ".*AppImage$",
    architecture: "x86_64", tag_regex: "^v?(?P<version>.*)$", prerelease: false, channel: "stable",
  } : null }));
  const installedDirectUrls = new Map<string, string>();
  let settings: Settings = { scan_dirs: [], managed_dir: "/preview/Applications" };
  const findApp = (id: string) => {
    const app = apps.find((entry) => entry.id === id);
    if (!app) throw new Error("App not found in preview.");
    return app;
  };

  const checkApp = (app: InstalledApp): UpdateCheckResult => {
    if (!app.update_config) return { app_id: app.id, status: "unsupported", update: null, error: null };
    const isDirect = app.update_config.strategy === "direct-url";
    const latest = isDirect ? "Updated file" : "1.6.7";
    if ((app.update_config.strategy === "direct-url" && installedDirectUrls.get(app.id) === app.update_config.url) || app.version_current === latest) return { app_id: app.id, status: "up-to-date", update: null, error: null };
    const update: UpdateInfo = {
      app_id: app.id, current: app.version_current, latest,
      download_url: app.update_config.strategy === "direct-url" ? app.update_config.url : "https://example.org/preview.AppImage",
      asset_name: isDirect ? `${app.name}.AppImage` : `${app.name}-${latest}.AppImage`, published_at: PREVIEW_DATE, size_bytes: 185_000_000,
      status: isDirect || app.version_current ? "available" : "unknown-current",
    };
    return { app_id: app.id, status: update.status, update, error: null };
  };
  const configureUpdates = (id: string, config: UpdateConfig | null) => {
    const configured = { ...findApp(id), update_config: structuredClone(config) };
    apps = apps.map((app) => app.id === id ? configured : app);
    return structuredClone(configured);
  };

  return {
    isTauri: false,
    async selfUpdateCheck() { return { currentVersion: "0.5.0", update: null, unavailableReason: "Updating AppBento requires the desktop AppImage." }; },
    async selfUpdateInstall() { throw new Error("Updating AppBento requires the desktop AppImage."); },
    async configurationExport() { throw new Error("Exporting settings requires the desktop app."); },
    async configurationRead() { throw new Error("Importing settings requires the desktop app."); },
    async configurationRestore() { throw new Error("Restoring settings requires the desktop app."); },
    async configurationPreferences() { return null; },
    async librarySetIcon() { throw new Error("Saving icons requires the desktop app."); },
    async libraryIcon() { return null; },
    async pickIcon() { return null; },
    async sandboxStatus() { return { available: true, version: "sample", error: null }; },
    async libraryConfigure(id, config) {
      const configured = { ...findApp(id), launch_config: structuredClone(config) };
      apps = apps.map((app) => app.id === id ? configured : app);
      return structuredClone(configured);
    },
    async libraryList() { return structuredClone(apps); },
    async libraryScan() { return { apps: structuredClone(apps), warnings: [] }; },
    async libraryAddUrl() { throw new Error("Downloading AppImages requires the desktop app."); },
    async libraryIntegrate() { throw new Error("File integration requires the desktop app."); },
    async libraryLaunchLog(id) {
      const app = findApp(id);
      return app.last_launched ? { started_at: app.last_launched, text: `[AppBento preview] Launched ${app.name}` } : null;
    },
    async libraryLaunch(id) {
      const app = findApp(id);
      if (!app.launch_config) throw new Error("Review launch permissions first.");
      const launched = { ...app, launch_count: app.launch_count + 1, last_launched: new Date().toISOString() };
      apps = apps.map((entry) => entry.id === id ? launched : entry);
      return structuredClone(launched);
    },
    async libraryRemove(id) {
      findApp(id);
      apps = apps.filter((app) => app.id !== id);
    },
    async settingsGet() { return structuredClone(settings); },
    async settingsSet(patch) {
      settings = { ...settings, ...patch };
      return structuredClone(settings);
    },
    async updatesConfigure(id, config) { return configureUpdates(id, config); },
    async updatesCheck(id) { return checkApp(findApp(id)); },
    async updatesInstall(id) {
      const app = findApp(id);
      const update = checkApp(app).update;
      if (!update) throw new Error(app.update_config ? "No update available." : "Configure an update source first.");
      if (app.update_config?.strategy === "direct-url") installedDirectUrls.set(id, app.update_config.url);
      const updated = { ...app, version_current: app.update_config?.strategy === "direct-url" ? app.version_current : update.latest, file_name: app.integrated ? app.file_name : update.asset_name,
        file_path: app.integrated ? app.file_path : `${settings.managed_dir}/${update.asset_name}`, source_path: null, size_bytes: update.size_bytes ?? app.size_bytes };
      apps = apps.map((entry) => entry.id === id ? updated : entry);
      return structuredClone(updated);
    },
    async updatesCheckAll(): Promise<UpdateReport> {
      const results = apps.map(checkApp);
      return {
        updates: results.flatMap((result) => result.update ? [result.update] : []),
        errors: [], unknown_versions: results.filter((result) => result.status === "unknown-current").map((result) => result.app_id),
        checked_count: results.filter((result) => result.status !== "unsupported").length,
        unsupported_count: results.filter((result) => result.status === "unsupported").length,
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
