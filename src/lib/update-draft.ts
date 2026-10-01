import type { UpdateConfig } from "./types";

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

export const DEFAULT_ASSET_REGEX = ".*\\.AppImage$";
export const DEFAULT_TAG_REGEX = "^v?(?P<version>.*)$";

export function draftFromConfig(config: UpdateConfig | null | undefined): UpdateDraft {
  const release = config && config.strategy !== "direct-url" ? config : null;
  return {
    strategy: config?.strategy ?? "disabled",
    url: config?.strategy === "direct-url" ? config.url : "",
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
  if (draft.strategy === "direct-url") return { strategy: draft.strategy, url: draft.url.trim() };
  return {
    strategy: draft.strategy,
    repo: draft.repo.trim(),
    asset_regex: draft.assetRegex.trim(),
    architecture: draft.architecture.trim(),
    tag_regex: draft.tagRegex.trim(),
    prerelease: draft.prerelease,
    channel: draft.channel.trim(),
  };
}

export function isDraftComplete(draft: UpdateDraft) {
  if (draft.strategy === "disabled") return true;
  if (draft.strategy === "direct-url") return !!draft.url.trim();
  return !!draft.repo.trim() && !!draft.assetRegex.trim();
}

/** Compares the configs a draft would save, so edits to fields the strategy ignores don't count. */
export function isDraftChanged(draft: UpdateDraft, saved: UpdateConfig | null | undefined) {
  return JSON.stringify(configFromDraft(draft)) !== JSON.stringify(configFromDraft(draftFromConfig(saved)));
}
