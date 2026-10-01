import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { THEMES } from "@/data/themes";
import { readThemePreference, resolveTheme, writeThemePreference } from "@/lib/theme-preferences";

interface ThemeContext {
  theme: string;
  setTheme: (theme: string) => void;
  storageError: string | null;
}

const ThemeContext = createContext<ThemeContext | null>(null);

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [preference, setPreference] = useState(readThemePreference);

  useEffect(() => {
    document.documentElement.dataset.theme = preference.theme;
    document.documentElement.classList.toggle("dark", preference.theme !== "paper");
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

export function ThemeSelect() {
  const context = useContext(ThemeContext);
  if (!context) throw new Error("ThemeSelect requires a ThemeProvider.");

  return (
    <>
      <select value={context.theme} onChange={(event) => context.setTheme(event.target.value)} aria-label="Color scheme"
        className="bg-[var(--panel2)] border border-[var(--border)] rounded-md px-2 py-1 text-[12px]">
        {THEMES.map((theme) => <option key={theme.id} value={theme.id}>{theme.label}</option>)}
      </select>
      {context.storageError ? <span role="status" className="text-[12px] text-[var(--warn)]">{context.storageError}</span> : null}
    </>
  );
}
