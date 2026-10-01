import { describe, expect, it } from "vitest";
import { formatAge } from "../src/lib/relative-time";

const NOW = new Date("2026-10-01T12:00:00Z");

describe("formatAge", () => {
  it.each([
    ["2026-10-01T08:00:00Z", "today"],
    ["2026-09-30T08:00:00Z", "yesterday"],
    ["2026-09-10T12:00:00Z", "3 weeks ago"],
    ["2026-06-01T12:00:00Z", "4 months ago"],
    ["2024-10-01T12:00:00Z", "2 years ago"],
  ])("formats %s as %s", (iso, expected) => {
    expect(formatAge(iso, NOW)).toBe(expected);
  });

  it("returns null for missing or invalid dates", () => {
    expect(formatAge(null, NOW)).toBeNull();
    expect(formatAge("not a date", NOW)).toBeNull();
  });
});
