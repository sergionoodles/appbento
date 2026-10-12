import type { AppLayout } from "./app-layout";
import { readLayout } from "./app-layout";
import { readThemePreference } from "./theme-preferences";
import type { InstalledApp, Settings } from "./types";

export interface ConfigurationPreferences {
  theme: string;
  layout: AppLayout;
  display_names: Record<string, string>;
}

export interface ConfigurationBackup {
  format: "appbento-configuration";
  version: 1;
  home_directory: string;
  settings: Settings;
  preferences: ConfigurationPreferences;
  apps: { app: InstalledApp; icon: number[] | null }[];
}

export interface ConfigurationPreview {
  backup: ConfigurationBackup;
  settings: Settings;
  app_paths: Record<string, string>;
  permission_paths: Record<string, string>;
}

export type ConfigurationRestoreRequest = ConfigurationPreview;

const PREFERENCE_KEYS = ["appbento-theme", "appbento-layout", "appbento-display-names"] as const;

export function readConfigurationPreferences(): ConfigurationPreferences {
  const theme = readThemePreference();
  const layout = readLayout();
  if (theme.error || layout.error) throw new Error(theme.error ?? layout.error ?? "Cannot read preferences");
  const names: unknown = JSON.parse(localStorage.getItem(PREFERENCE_KEYS[2]) ?? "{}");
  if (typeof names !== "object" || names === null || Array.isArray(names)
    || Object.values(names).some((name) => typeof name !== "string")) {
    throw new Error("Saved app display names are unreadable.");
  }
  return { theme: theme.theme, layout: layout.layout, display_names: names as Record<string, string> };
}

/** Roll back local preferences if storage is unavailable during restoration. */
export function restoreConfigurationPreferences(preferences: ConfigurationPreferences): void {
  const previous = PREFERENCE_KEYS.map((key) => localStorage.getItem(key));
  const next = [preferences.theme, JSON.stringify(preferences.layout), JSON.stringify(preferences.display_names)];
  try {
    PREFERENCE_KEYS.forEach((key, index) => localStorage.setItem(key, next[index]));
  } catch (error) {
    PREFERENCE_KEYS.forEach((key, index) => {
      const value = previous[index];
      if (value === null) localStorage.removeItem(key);
      else localStorage.setItem(key, value);
    });
    throw error;
  }
}
