import type { InstalledApp, UpdateInfo } from "@/lib/types";

export function accessLabel(app: InstalledApp) {
  if (!app.launch_config) return "Needs setup";
  return app.launch_config.mode === "bubblewrap" ? "Bubblewrap" : "Runs as is";
}

export function updateLabel(update?: UpdateInfo) {
  if (!update) return null;
  return update.status === "available" ? `Update to ${update.latest}` : "Version unknown";
}

export function versionLabel(app: InstalledApp) {
  return app.version_current ? `v${app.version_current}` : "Version unknown";
}
