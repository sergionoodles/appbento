import type { UpdateConfig, UrlUpdateConfig, UrlUpdateStrategy } from "./types";

export type UpdateDraftStrategy = UpdateConfig["strategy"] | "disabled";

/** Every field the update form can edit, kept even when the chosen strategy ignores it. */
export interface UpdateDraft {
  strategy: UpdateDraftStrategy;
  url: string;
  repo: string;
  assetRegex: string;
  architecture: string;
  tagRegex: string;
  channel: string;
  prerelease: boolean;
}

const URL_STRATEGIES: readonly string[] = ["direct-url", "zsync", "electron-feed"] satisfies UrlUpdateStrategy[];

function isUrlStrategy(strategy: UpdateDraftStrategy): strategy is UrlUpdateStrategy {
  return URL_STRATEGIES.includes(strategy);
}

function isUrlConfig(config: UpdateConfig): config is UrlUpdateConfig {
  return isUrlStrategy(config.strategy);
}

export const DEFAULT_ASSET_REGEX = ".*\\.AppImage$";
export const DEFAULT_TAG_REGEX = "^v?(?P<version>.*)$";

export function extractGithubRepository(value: string): string {
  const trimmed = value.trim();
  if (!/^https?:\/\//i.test(trimmed)) return value;

  try {
    const url = new URL(trimmed);
    if (url.hostname !== "github.com" || url.username || url.password) return value;

    const parts = url.pathname.split("/").filter(Boolean);
    const owner = parts[0];
    const repository = parts[1]?.replace(/\.git$/, "");
    if (!owner || !repository || ![owner, repository].every((part) => /^[A-Za-z0-9_.-]+$/.test(part))) return value;

    return `${owner}/${repository}`;
  } catch {
    return value;
  }
}

export function draftFromConfig(config: UpdateConfig | null | undefined): UpdateDraft {
  const release = config && !isUrlConfig(config) ? config : null;
  return {
    strategy: config?.strategy ?? "disabled",
    url: config && isUrlConfig(config) ? config.url : "",
    repo: release?.repo ?? "",
    assetRegex: release?.asset_regex ?? DEFAULT_ASSET_REGEX,
    architecture: release?.architecture ?? "x86_64",
    tagRegex: release?.tag_regex ?? DEFAULT_TAG_REGEX,
    channel: release?.channel ?? "stable",
    prerelease: release?.prerelease ?? false,
  };
}

export function configFromDraft(draft: UpdateDraft): UpdateConfig | null {
  if (draft.strategy === "disabled") return null;
  if (isUrlStrategy(draft.strategy)) return { strategy: draft.strategy, url: draft.url.trim() };
  return {
    strategy: draft.strategy,
    repo: draft.strategy === "github-release"
      ? extractGithubRepository(draft.repo).trim().replace(/^\//, "").replace(/\/$/, "")
      : draft.repo.trim(),
    asset_regex: draft.assetRegex.trim(),
    architecture: draft.architecture.trim(),
    tag_regex: draft.tagRegex.trim(),
    prerelease: draft.prerelease,
    channel: draft.channel.trim(),
  };
}

export function isDraftComplete(draft: UpdateDraft) {
  if (draft.strategy === "disabled") return true;
  if (isUrlStrategy(draft.strategy)) return !!draft.url.trim();
  return !!draft.repo.trim() && !!draft.assetRegex.trim();
}

/** Compares the configs a draft would save, so edits to fields the strategy ignores don't count. */
export function isDraftChanged(draft: UpdateDraft, saved: UpdateConfig | null | undefined) {
  return JSON.stringify(configFromDraft(draft)) !== JSON.stringify(configFromDraft(draftFromConfig(saved)));
}
