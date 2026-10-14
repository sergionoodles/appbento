import { expect, it } from "vitest";
import { createLaunchFixPrompt } from "../src/lib/launch-fix-prompt";
import { createPreviewApp } from "../src/lib/preview";
import type { LaunchLog } from "../src/lib/types";

it.each([
  { lineCount: 0, separator: "\n", hasTrailingNewline: false },
  { lineCount: 3, separator: "\n", hasTrailingNewline: false },
  { lineCount: 15, separator: "\n", hasTrailingNewline: false },
  { lineCount: 20, separator: "\n", hasTrailingNewline: false },
  { lineCount: 20, separator: "\r\n", hasTrailingNewline: true },
])("includes only the latest 15 log lines: %j", ({ lineCount, separator, hasTrailingNewline }) => {
  const lines = Array.from({ length: lineCount }, (_, index) => `Output ${index + 1}`);
  const text = lines.join(separator) + (hasTrailingNewline ? separator : "");
  const log = { started_at: "2026-10-14T10:00:00Z", text };
  const prompt = createLaunchFixPrompt(createPreviewApp("Example", "1.0"), log);
  const diagnostics = JSON.parse(prompt.split("Diagnostics (JSON):\n")[1]) as { latest_launch: LaunchLog };

  expect(diagnostics.latest_launch).toEqual({ ...log, text: lines.slice(-15).join("\n") });
  expect(log.text).toBe(text);
});
