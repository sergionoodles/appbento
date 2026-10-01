import { expect, it } from "vitest";
import { configFromDraft, draftFromConfig, isDraftChanged, isDraftComplete } from "../src/lib/update-draft";
import type { UpdateConfig } from "../src/lib/types";

const release: UpdateConfig = {
  strategy: "github-release", repo: "owner/app", asset_regex: ".*\\.AppImage$",
  architecture: "x86_64", tag_regex: "^v?(?P<version>.*)$", prerelease: false, channel: "stable",
};

it("round-trips a saved config without reporting a change", () => {
  expect(configFromDraft(draftFromConfig(release))).toEqual(release);
  expect(isDraftChanged(draftFromConfig(release), release)).toBe(false);
});

it("ignores edits to fields the chosen strategy does not use", () => {
  const draft = { ...draftFromConfig(null), repo: "owner/unused" };
  expect(isDraftChanged(draft, null)).toBe(false);
  expect(isDraftChanged({ ...draft, strategy: "github-release" }, null)).toBe(true);
});

it("requires a repository for releases and a URL for direct downloads", () => {
  expect(isDraftComplete({ ...draftFromConfig(null), strategy: "github-release" })).toBe(false);
  expect(isDraftComplete({ ...draftFromConfig(null), strategy: "direct-url", url: "  " })).toBe(false);
  expect(isDraftComplete(draftFromConfig(release))).toBe(true);
});
