import assert from "node:assert/strict";
import { test } from "vitest";
import { DEFAULT_THEME, readThemePreference, resolveTheme, writeThemePreference } from "../src/lib/theme-preferences.ts";

test("invalid persisted theme is replaced with the default", () => {
  assert.equal(resolveTheme("unknown-theme"), DEFAULT_THEME);
  assert.equal(resolveTheme("paper"), "paper");
});

test("blocked storage keeps theme usable and exposes the limitation", () => {
  const blocked = (): Storage => { throw new Error("Storage blocked"); };
  assert.deepEqual(readThemePreference(blocked), { theme: DEFAULT_THEME, error: "Theme preferences cannot be saved in this session." });
  assert.equal(writeThemePreference("paper", blocked), "Theme preferences cannot be saved in this session.");
});
