import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { THEMES } from "@/data/themes";
import { readThemePreference, resolveTheme, writeThemePreference } from "@/lib/theme-preferences";

interface ThemeContext {
  theme: string;
  setTheme: (theme: string) => void;
  storageError: string | null;
}

const LIGHT_THEMES = ["paper", "daylight"];

const ThemeContext = createContext<ThemeContext | null>(null);

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [preference, setPreference] = useState(readThemePreference);

  useEffect(() => {
    document.documentElement.dataset.theme = preference.theme;
    document.documentElement.classList.toggle("dark", !LIGHT_THEMES.includes(preference.theme));
  }, [preference.theme]);

  return (
    <ThemeContext.Provider value={{
      theme: preference.theme,
      storageError: preference.error,
      setTheme: (value) => {
        const theme = resolveTheme(value);
        setPreference({ theme, error: writeThemePreference(theme) });
      },
    }}>{children}</ThemeContext.Provider>
  );
}

export function ThemeSwatches() {
  const context = useContext(ThemeContext);
  if (!context) throw new Error("ThemeSwatches requires a ThemeProvider.");

  return (
    <div role="radiogroup" aria-label="Color scheme" className="flex flex-wrap gap-2 px-3 py-2">
      {THEMES.map((theme) => (
        <button key={theme.id} type="button" role="radio" aria-checked={context.theme === theme.id} aria-label={theme.label} title={theme.label}
          onClick={() => context.setTheme(theme.id)} data-theme={theme.id}
          className="size-7 rounded-md border-2 cursor-pointer aria-checked:border-[var(--accent)] border-[var(--border)] transition-transform hover:scale-110"
          style={{ background: "linear-gradient(135deg, var(--bg) 50%, var(--accent) 50%)" }} />
      ))}
      {context.storageError ? <span role="status" className="basis-full text-[12px] text-[var(--warn)]">{context.storageError}</span> : null}
    </div>
  );
}
