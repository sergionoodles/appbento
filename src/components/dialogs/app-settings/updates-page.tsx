import type { UpdateInfo } from "@/lib/types";
import { updateLabel } from "../../app-status";
import { Button } from "../../ui/button";
import { CirclePause } from "lucide-react";
import type { UpdateDraft, UpdateDraftStrategy } from "@/lib/update-draft";
import { extractGithubRepository } from "@/lib/update-draft";
import { Input } from "../../ui/card";
import { Field, Notice } from "../../ui/field";
import { Select, type SelectOption } from "../../ui/select";
import { Switch } from "../../ui/switch";

const STRATEGIES: SelectOption<UpdateDraftStrategy>[] = [
  { value: "disabled", label: "Disabled" },
  { value: "github-release", label: "GitHub releases" },
  { value: "gitlab-release", label: "GitLab releases" },
  { value: "direct-url", label: "Direct HTTPS download" },
];

const ARCHITECTURES: SelectOption<string>[] = [
  { value: "x86_64", label: "x86_64" },
  { value: "aarch64", label: "aarch64" },
];

const CHANNELS: SelectOption<string>[] = [
  { value: "stable", label: "Stable" },
  { value: "prerelease", label: "Prerelease" },
];

const MONO_INPUT = "font-mono text-[13px]";

export function UpdatesPage({ draft, onChange, isSaving, hasUpdateSource, hasUnsavedSource, update, isCheckingUpdates, onCheckUpdate, onInstallUpdate }: {
  draft: UpdateDraft;
  hasUpdateSource: boolean;
  hasUnsavedSource: boolean;
  update?: UpdateInfo;
  isCheckingUpdates: boolean;
  onCheckUpdate: () => void;
  onInstallUpdate: () => void;
  onChange: (draft: UpdateDraft) => void;
  isSaving: boolean;
}) {
  const set = <K extends keyof UpdateDraft>(key: K, value: UpdateDraft[K]) => onChange({ ...draft, [key]: value });
  const isRelease = draft.strategy === "github-release" || draft.strategy === "gitlab-release";

  const areActionsDisabled = isSaving || isCheckingUpdates || hasUnsavedSource;

  return (
    <div className="flex flex-col gap-6">
      <section aria-label="App updates" className="flex flex-col gap-3">
        <p className="text-[13px] text-[var(--muted)]">{updateLabel(update) ?? (hasUpdateSource ? "Check your saved source for a newer release." : "Updates are disabled for this app.")}</p>
        <div className="flex flex-wrap gap-2">
          {hasUpdateSource ? <Button variant="outline" size="sm" disabled={areActionsDisabled} onClick={onCheckUpdate}>{isCheckingUpdates ? "Checking for updates…" : "Check for updates"}</Button> : null}
          {hasUpdateSource && update ? <Button variant="accent" size="sm" disabled={areActionsDisabled} onClick={onInstallUpdate}>{update.status === "available" ? "Install update" : "Install latest"}</Button> : null}
        </div>
        {hasUnsavedSource ? <p className="text-[13px] text-[var(--muted)]">Save your source changes before checking or installing updates.</p> : null}
      </section>
      <fieldset disabled={isSaving} className="grid grid-cols-2 gap-x-4 gap-y-5">
        <legend className="sr-only">Update source</legend>
        <Field label="Source" htmlFor="update-strategy" className="col-span-2">
          <Select id="update-strategy" value={draft.strategy} options={STRATEGIES} onChange={(strategy) => set("strategy", strategy)} />
        </Field>

        {draft.strategy === "disabled" ? <div className="col-span-2">
          <Notice icon={<CirclePause size={16} />}>This app is skipped when checking for updates.</Notice>
        </div> : null}

        {draft.strategy === "direct-url" ? <Field label="Download URL" htmlFor="update-url" className="col-span-2"
          hint="A stable link to the AppImage file. Each check downloads it and compares its content with your installed copy.">
          <Input id="update-url" type="url" required placeholder="https://example.org/app.AppImage" value={draft.url} onChange={(event) => set("url", event.target.value)} />
        </Field> : null}

        {isRelease ? <>
          <Field label="Repository" htmlFor="update-repo" className="col-span-2">
            <Input id="update-repo" required placeholder={draft.strategy === "github-release" ? "owner/repository" : "https://gitlab.com/group/project"} value={draft.repo}
              onChange={(event) => set("repo", event.target.value)}
              onBlur={(event) => {
                if (draft.strategy === "github-release") set("repo", extractGithubRepository(event.target.value));
              }}
              onPaste={(event) => {
                if (draft.strategy !== "github-release") return;
                const pasted = event.clipboardData.getData("text");
                const repository = extractGithubRepository(pasted);
                if (repository === pasted) return;
                event.preventDefault();
                set("repo", repository);
              }} />
          </Field>
          <Field label="Filename matcher" htmlFor="update-assets" hint={<>Regular expression matching one AppImage, e.g. <code>.*x86_64.*\.AppImage$</code></>}>
            <Input id="update-assets" required className={MONO_INPUT} value={draft.assetRegex} onChange={(event) => set("assetRegex", event.target.value)} />
          </Field>
          <Field label="Architecture" htmlFor="update-architecture">
            <Select id="update-architecture" value={draft.architecture} options={ARCHITECTURES} onChange={(architecture) => set("architecture", architecture)} />
          </Field>
          <Field label="Version tag matcher" htmlFor="update-tags" hint={<>Use a named capture, e.g. <code>^v?(?P&lt;version&gt;.*)$</code></>}>
            <Input id="update-tags" className={MONO_INPUT} value={draft.tagRegex} onChange={(event) => set("tagRegex", event.target.value)} />
          </Field>
          <Field label="Channel" htmlFor="update-channel">
            <Select id="update-channel" value={draft.channel} options={CHANNELS} onChange={(channel) => set("channel", channel)} />
          </Field>
          <label className="col-span-2 flex items-center gap-4 rounded-lg border border-[var(--border)] px-4 py-3">
            <span className="flex-1">
              <span className="block text-[14px]">Include prereleases</span>
              <span id="update-prerelease-hint" className="block text-[12.5px] text-[var(--muted)]">Offer beta and release candidate builds too.</span>
            </span>
            <Switch aria-label="Include prereleases" aria-describedby="update-prerelease-hint" checked={draft.prerelease} onChange={(event) => set("prerelease", event.target.checked)} />
          </label>
        </> : null}
      </fieldset>
    </div>
  );
}
