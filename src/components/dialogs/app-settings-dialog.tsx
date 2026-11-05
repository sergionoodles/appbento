import { useState } from "react";
import { RefreshCw, Terminal, Settings2, ShieldCheck, PanelsTopLeft, X, type LucideIcon } from "lucide-react";
import type { AppApi } from "@/lib/api";
import type { InstalledApp, LaunchConfig, UpdateConfig, UpdateCheckResult, UpdateInfo } from "@/lib/types";
import { validateSandboxPaths } from "@/lib/sandbox";
import { configFromDraft, draftFromConfig, isDraftChanged, isDraftComplete } from "@/lib/update-draft";
import { launchDraftFromConfig, parseLaunchDraft } from "@/lib/launch-draft";
import { LaunchPage } from "./app-settings/launch-page";
import { cn } from "@/lib/utils";
import { AppIcon } from "../app-icon";
import { versionLabel } from "../app-status";
import { Button } from "../ui/button";
import { Dialog } from "./dialog";
import { GeneralPage } from "./app-settings/general-page";
import { IntegrationPage } from "./app-settings/integration-page";
import { PermissionsPage, initialLaunchConfig, useLaunchConfig, useSandboxStatus } from "./app-settings/permissions-page";
import { UpdatesPage } from "./app-settings/updates-page";
import { useAppReview } from "./app-settings/use-app-review";
import { SaveStatus } from "./app-settings/save-status";

type Page = "general" | "permissions" | "launch" | "updates" | "integration";

/** Where the dialog opens; "icon" is the General page with the icon editor already open. */
export type SettingsTarget = Page | "icon";

const PAGES: { page: Page; label: string; icon: LucideIcon; description: string }[] = [
  { page: "general", label: "General", icon: Settings2, description: "How this app appears in your library." },
  { page: "permissions", label: "Permissions", icon: ShieldCheck, description: "What this app can access when it runs." },
  { page: "launch", label: "Launch", icon: Terminal, description: "Arguments and environment variables used every time this app launches." },
  { page: "integration", label: "Integration", icon: PanelsTopLeft, description: "Choose whether this app appears in your system app menu." },
  { page: "updates", label: "Updates", icon: RefreshCw, description: "Where AppBento looks for new versions. Installing an update keeps these settings." },
];

function saveButtonLabel(isCommitting: boolean, operation: string | null, launchAfterSave: boolean) {
  if (!isCommitting) return launchAfterSave ? "Save and launch" : "Save changes";
  if (operation === "Verifying app file and starting app" || operation === "Preparing app launch") return "Starting app…";
  return "Saving…";
}

export function AppSettingsDialog({ app, backend, openTo, originalName, launchAfterSave, onClose, onSaved, onSavePermissions, onSaveUpdates, onSaveIcon, onSaveName, onSetMenuEntry, isSaving, operation = null, isUpdating = false, error, returnFocusElement, update, updateCheck, isCheckingUpdates, onCheckUpdate, onInstallUpdate }: {
  app: InstalledApp;
  update?: UpdateInfo;
  updateCheck?: UpdateCheckResult;
  isCheckingUpdates: boolean;
  onCheckUpdate: () => void;
  onInstallUpdate: () => void;
  backend: AppApi;
  openTo: SettingsTarget;
  originalName: string;
  launchAfterSave: boolean;
  onClose: () => void;
  onSaved: () => void;
  onSavePermissions: (config: LaunchConfig, launch: boolean, reviewToken: string | null) => Promise<boolean>;
  onSaveUpdates: (config: UpdateConfig | null) => Promise<boolean>;
  onSaveIcon: (source: string) => Promise<boolean>;
  onSaveName: (name: string) => void;
  onSetMenuEntry: (enabled: boolean) => Promise<boolean>;
  isSaving: boolean;
  operation?: string | null;
  isUpdating?: boolean;
  error: string | null;
  returnFocusElement?: HTMLElement | null;
}) {
  const [page, setPage] = useState<Page>(openTo === "icon" ? "general" : openTo);
  const [isEditingIcon, setIsEditingIcon] = useState(openTo === "icon");
  const [name, setName] = useState(app.name);
  const { config: launchConfig, setConfig: setLaunchConfig, isLoading: isLoadingDefaults, error: defaultsError } = useLaunchConfig(app, backend);
  const [launchDraft, setLaunchDraft] = useState(() => launchDraftFromConfig(initialLaunchConfig(app)));
  const [updateDraft, setUpdateDraft] = useState(() => draftFromConfig(app.update_config));
  const [localError, setLocalError] = useState<string | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const sandbox = useSandboxStatus(backend);
  const review = useAppReview(app, backend);

  const isReview = review.needsReview;
  const hasNameChange = name.trim() !== app.name;
  // A restored policy awaiting review needs confirmation even if its settings are unchanged.
  const hasPermissionChange = isReview || JSON.stringify(launchConfig) !== JSON.stringify(app.launch_config);
  const hasLaunchChange = JSON.stringify(launchDraft) !== JSON.stringify(launchDraftFromConfig(initialLaunchConfig(app)));
  const hasConfigChange = hasPermissionChange || hasLaunchChange;
  const hasUpdateChange = isDraftChanged(updateDraft, app.update_config);
  const changedPages: Record<Page, boolean> = { general: hasNameChange, permissions: hasPermissionChange, launch: hasLaunchChange, updates: hasUpdateChange, integration: false };
  const isPermissionsBlocked = hasConfigChange && launchConfig.mode === "bubblewrap" && !sandbox.isReady;
  const isCommitting = isSaving || isSubmitting;
  const isBusy = isCommitting || isUpdating || isLoadingDefaults;
  const canSave = !isBusy && !defaultsError && !!name.trim() && (hasNameChange || hasConfigChange || hasUpdateChange)
    && !isPermissionsBlocked && (!hasUpdateChange || isDraftComplete(updateDraft)) && !review.isVerifying && !review.error;
  const current = PAGES.find((entry) => entry.page === page) ?? PAGES[0];

  const save = async () => {
    const pathError = hasPermissionChange && launchConfig.mode === "bubblewrap" ? validateSandboxPaths(launchConfig.permissions) : null;
    setLocalError(pathError);
    if (pathError || !canSave) return;

    const config = (() => {
      try {
        return hasLaunchChange ? { ...launchConfig, ...parseLaunchDraft(launchDraft) } : launchConfig;
      } catch (failure) {
        setLocalError(failure instanceof Error ? failure.message : String(failure));
        setPage("launch");
        return null;
      }
    })();
    if (!config) return;

    setIsSubmitting(true);
    try {
      if (hasNameChange) onSaveName(name.trim());
      if (hasUpdateChange && !await onSaveUpdates(configFromDraft(updateDraft))) return;
      // Permissions go last because saving them can also launch the app.
      if (hasConfigChange && !await onSavePermissions(config, launchAfterSave, review.token)) return;
      onSaved();
    } catch (failure) {
      setLocalError(failure instanceof Error ? failure.message : String(failure));
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <Dialog title={`Settings for ${app.name}`} onClose={onClose} canClose={!isCommitting} returnFocusElement={returnFocusElement} isBare
      className="w-[min(960px,100%)] h-[min(660px,calc(100vh-24px))] max-h-none">
      <div className="grid h-full min-h-0 grid-cols-[232px_minmax(0,1fr)]">
        <aside className="flex min-h-0 flex-col gap-5 border-r border-[var(--border)] bg-[color-mix(in_oklab,var(--panel2)_55%,var(--panel))] px-3 py-5">
          <div className="flex items-center gap-3 px-2">
            <AppIcon app={app} name={app.name} className="size-10" />
            <div className="min-w-0">
              <p className="truncate text-[15px] font-semibold leading-tight">{app.name}</p>
              <p className="truncate text-[12px] text-[var(--muted)]">{versionLabel(app)}</p>
            </div>
          </div>
          <nav aria-label="Settings sections" className="flex flex-col gap-0.5">
            {PAGES.map(({ page: key, label, icon: Icon }) => (
              <button key={key} type="button" aria-current={page === key ? "page" : undefined} onClick={() => setPage(key)}
                className={cn(
                  "flex h-9 items-center gap-2.5 rounded-md px-2.5 text-left text-[14px] transition-colors",
                  page === key ? "bg-[var(--panel)] text-[var(--fg)] shadow-sm ring-1 ring-[var(--border)]" : "text-[var(--muted)] hover:bg-[var(--panel)]/60 hover:text-[var(--fg)]"
                )}>
                <Icon size={16} className={page === key ? "text-[var(--accent)]" : undefined} />
                <span className="flex-1">{label}</span>
                {changedPages[key] ? <span className="size-1.5 rounded-full bg-[var(--accent)]" title="Unsaved changes" /> : null}
              </button>
            ))}
          </nav>
        </aside>

        <div className="flex min-h-0 flex-col">
          <header className="flex items-start justify-between gap-4 px-8 pt-6 pb-5">
            <div>
              <h3 className="text-[20px] font-bold tracking-[-0.03em] leading-tight">{current.label}</h3>
              <p className="mt-1 text-[13px] text-[var(--muted)]">{current.description}</p>
            </div>
            <Button variant="ghost" size="icon" aria-label="Close settings" disabled={isCommitting} onClick={onClose} className="-mr-2"><X size={18} /></Button>
          </header>

          <div key={page} className="min-h-0 flex-1 overflow-y-auto px-8 pb-8">
            {page === "general" ? <GeneralPage app={app} backend={backend} name={name} originalName={originalName} onNameChange={setName}
              isEditingIcon={isEditingIcon} onEditIcon={setIsEditingIcon} onSaveIcon={onSaveIcon} isSaving={isBusy} /> : null}
            {page === "permissions" ? <PermissionsPage app={app} backend={backend} config={launchConfig} onChange={setLaunchConfig} sandbox={sandbox}
              isReview={isReview} isSaving={isBusy} onError={setLocalError} /> : null}
            {page === "launch" ? <LaunchPage draft={launchDraft} onChange={setLaunchDraft} isSaving={isBusy} /> : null}
            {page === "integration" ? <IntegrationPage app={app} isSaving={isBusy} onSetMenuEntry={onSetMenuEntry} /> : null}
            {page === "updates" && app.kind === "binary" ? <p className="text-[13px] text-[var(--muted)]">Executable updates are not supported yet. Replace the file yourself, then review its permissions again.</p> : null}
            {page === "updates" && app.kind === "appimage" ? <UpdatesPage draft={updateDraft} onChange={setUpdateDraft} isSaving={isBusy}
              hasUpdateSource={!!app.update_config} hasUnsavedSource={hasUpdateChange} origin={app.update_config_origin} update={update} updateCheck={updateCheck} isUpdating={isUpdating} isCheckingUpdates={isCheckingUpdates}
              onCheckUpdate={onCheckUpdate} onInstallUpdate={onInstallUpdate} /> : null}
          </div>

          <footer className="flex items-center gap-3 border-t border-[var(--border)] px-8 py-4">
            <div className="flex-1 min-w-0 text-[13px]">
              <SaveStatus isVerifying={review.isVerifying} needsReview={isReview} verificationError={review.error}
                isSaving={isCommitting} isUpdating={isUpdating} operation={operation} isLoadingDefaults={isLoadingDefaults}
                hasChanges={hasNameChange || hasConfigChange || hasUpdateChange} onRetry={review.retry} />
              {localError || defaultsError || review.error || error
                ? <p role="alert" className="mt-2 text-[var(--bad)]">{localError ?? defaultsError ?? review.error ?? error}</p>
                : null}
            </div>
            <Button variant="ghost" disabled={isCommitting} onClick={onClose}>Cancel</Button>
            <Button variant="accent" disabled={!canSave} aria-describedby="app-settings-save-status" onClick={() => void save()}>{saveButtonLabel(isCommitting, operation, launchAfterSave)}</Button>
          </footer>
        </div>
      </div>
    </Dialog>
  );
}
