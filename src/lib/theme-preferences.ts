import { THEMES } from "../data/themes.ts";

const THEME_KEY = "appbento-theme";
export const DEFAULT_THEME = "midnight";
const STORAGE_UNAVAILABLE = "Theme preferences cannot be saved in this session.";

export function resolveTheme(value: string | null): string {
  return THEMES.some((theme) => theme.id === value) ? value ?? DEFAULT_THEME : DEFAULT_THEME;
}

export function readThemePreference(getStorage = () => window.localStorage) {
  try {
    return { theme: resolveTheme(getStorage().getItem(THEME_KEY)), error: null };
  } catch {
    return { theme: DEFAULT_THEME, error: STORAGE_UNAVAILABLE };
  }
}

export function writeThemePreference(theme: string, getStorage = () => window.localStorage): string | null {
  try {
    getStorage().setItem(THEME_KEY, resolveTheme(theme));
    return null;
  } catch {
    return STORAGE_UNAVAILABLE;
  }
}
