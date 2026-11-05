import { expect, it } from "vitest";
import { configFromDraft, draftFromConfig, extractGithubRepository, isDraftChanged, isDraftComplete } from "../src/lib/update-draft";
import type { UpdateConfig } from "../src/lib/types";

const release: UpdateConfig = {
  strategy: "github-release", repo: "owner/app", asset_regex: ".*\\.AppImage$",
  architecture: "x86_64", tag_regex: "^v?(?P<version>.*)$", prerelease: false, channel: "stable",
};

it("round-trips a saved config without reporting a change", () => {
  expect(configFromDraft(draftFromConfig(release))).toEqual(release);
  expect(isDraftChanged(draftFromConfig(release), release)).toBe(false);
});

it.each(["direct-url", "zsync", "electron-feed"] as const)("round-trips a saved %s config through the URL field", (strategy) => {
  const saved: UpdateConfig = { strategy, url: "https://example.org/feed" };
  const draft = draftFromConfig(saved);
  expect(draft).toMatchObject({ strategy, url: "https://example.org/feed" });
  expect(configFromDraft({ ...draft, url: "  https://example.org/feed  " })).toEqual(saved);
  expect(isDraftChanged(draft, saved)).toBe(false);
  expect(isDraftChanged({ ...draft, url: "https://example.org/other" }, saved)).toBe(true);
  expect(isDraftChanged({ ...draft, repo: "owner/unused" }, saved)).toBe(false);
});

it("ignores edits to fields the chosen strategy does not use", () => {
  const draft = { ...draftFromConfig(null), repo: "owner/unused" };
  expect(isDraftChanged(draft, null)).toBe(false);
  expect(isDraftChanged({ ...draft, strategy: "github-release" }, null)).toBe(true);
});

it("requires a repository for releases and a URL for direct downloads", () => {
  expect(isDraftComplete({ ...draftFromConfig(null), strategy: "github-release" })).toBe(false);
  for (const strategy of ["direct-url", "zsync", "electron-feed"] as const) {
    expect(isDraftComplete({ ...draftFromConfig(null), strategy, url: "  " })).toBe(false);
    expect(isDraftComplete({ ...draftFromConfig(null), strategy, url: "https://example.org/a" })).toBe(true);
  }
  expect(isDraftComplete(draftFromConfig(release))).toBe(true);
});

it("extracts a GitHub repository URL when saving a release draft", () => {
  const draft = { ...draftFromConfig(release), repo: "https://github.com/stablyai/orca/releases" };
  expect(configFromDraft(draft)).toMatchObject({ repo: "stablyai/orca" });
  expect(configFromDraft({ ...draft, repo: "/stablyai/orca" })).toMatchObject({ repo: "stablyai/orca" });
});

it("preserves GitLab repository URLs", () => {
  const draft = { ...draftFromConfig(release), strategy: "gitlab-release" as const, repo: "https://gitlab.com/group/app" };
  expect(configFromDraft(draft)).toMatchObject({ repo: draft.repo });
});

it("extracts repository roots, clone URLs, and release links with queries", () => {
  for (const value of [
    "https://github.com/stablyai/orca",
    "https://github.com/stablyai/orca.git/",
    " https://github.com/stablyai/orca/releases/tag/v1.0?tab=readme#assets ",
  ]) {
    expect(extractGithubRepository(value)).toBe("stablyai/orca");
  }
});

it("leaves incomplete, invalid, and non-GitHub input for validation", () => {
  for (const value of [
    "owner/repo", "https://github.com/owner", "https://github.com/owner/%20",
    "https://github.com.evil.example/owner/repo", "https://user:password@github.com/owner/repo",
    "https://gitlab.com/owner/repo", "https://[invalid",
  ]) {
    expect(extractGithubRepository(value)).toBe(value);
  }
});
