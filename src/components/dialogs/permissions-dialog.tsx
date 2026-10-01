import { useEffect, useState } from "react";
import type { AppApi } from "@/lib/api";
import type { CatalogEntry, InstalledApp, LaunchConfig, SandboxPermissions, SandboxStatus } from "@/lib/types";
import { restrictedPermissions, validateSandboxPaths } from "@/lib/sandbox";
import { Dialog } from "./dialog";
import { Button } from "../ui/button";

const CHOICE_CLASS = "flex items-center gap-3 rounded-lg border border-[var(--border)] bg-[var(--panel2)] px-4 py-3 cursor-pointer has-[:checked]:border-[var(--accent)]";

const PERMISSIONS: { key: Exclude<keyof SandboxPermissions, "paths">; label: string; description: string }[] = [
  { key: "network", label: "Internet and network", description: "Connect to the internet and local network services." },
  { key: "display", label: "Display", description: "Open windows. X11 access can also expose other windows and keyboard input." },
  { key: "audio", label: "Audio", description: "Play sound and record from microphones through the audio service." },
  { key: "gpu", label: "Graphics acceleration", description: "Access graphics devices and their drivers." },
  { key: "persistent_data", label: "Keep private app data", description: "Keep an isolated home folder between launches. This does not grant access to your personal home folder." },
];

export function PermissionsDialog({ app, catalog, backend, launchAfterSave, onClose, onSave, isSaving, error, returnFocusElement }: {
  app: InstalledApp;
  catalog?: CatalogEntry;
  backend: AppApi;
  launchAfterSave: boolean;
  onClose: () => void;
  onSave: (config: LaunchConfig, launch: boolean) => Promise<boolean>;
  isSaving: boolean;
  error: string | null;
  returnFocusElement?: HTMLElement | null;
}) {
  const [config, setConfig] = useState<LaunchConfig>(() => structuredClone(app.launch_config ?? { mode: "direct", permissions: restrictedPermissions() }));
  const [status, setStatus] = useState<SandboxStatus | null>(null);
  const [isChecking, setIsChecking] = useState(true);
  const [localError, setLocalError] = useState<string | null>(null);
  const [hasUsedSuggestion, setHasUsedSuggestion] = useState(false);
  const checkStatus = async () => {
    setIsChecking(true);
    try {
      setStatus(await backend.sandboxStatus());
    } catch (failure) {
      setStatus({ available: false, version: null, error: failure instanceof Error ? failure.message : String(failure) });
    } finally {
      setIsChecking(false);
    }
  };
  useEffect(() => {
    let isActive = true;
    void backend.sandboxStatus().then((result) => {
      if (isActive) setStatus(result);
    }).catch((failure: unknown) => {
      if (isActive) setStatus({ available: false, version: null, error: failure instanceof Error ? failure.message : String(failure) });
    }).finally(() => { if (isActive) setIsChecking(false); });
    return () => { isActive = false; };
  }, [backend]);

  const canSave = !isSaving && (config.mode === "direct" || (!isChecking && status?.available && !status.error));
  const save = async () => {
    const validation = config.mode === "bubblewrap" ? validateSandboxPaths(config.permissions) : null;
    setLocalError(validation);
    if (validation || !canSave) return;
    await onSave(config, launchAfterSave);
  };

  return <Dialog title={`Launch permissions for ${app.name}`} onClose={onClose} canClose={!isSaving} returnFocusElement={returnFocusElement}>
    <p className="text-[14px] text-[var(--muted)]">{app.launch_config ? "Change how this AppImage can access your computer." : "Review how this AppImage can access your computer before its first launch."} You can change these permissions later.</p>
    {!backend.isTauri ? <p className="text-[13px] text-[var(--warn)]">Sample preview: permissions are simulated. No sandbox or app is launched here.</p> : null}
    <fieldset disabled={isSaving} className="flex flex-col gap-2 text-[14px]">
      <legend className="font-medium mb-2">Launch mode</legend>
      <label className={CHOICE_CLASS}><input type="radio" name="launch-mode" className="accent-[var(--accent)]" checked={config.mode === "bubblewrap"} onChange={() => setConfig({ ...config, mode: "bubblewrap" })} />Restrict with bubblewrap</label>
      <label className={CHOICE_CLASS}><input type="radio" name="launch-mode" className="accent-[var(--accent)]" checked={config.mode === "direct"} onChange={() => setConfig({ ...config, mode: "direct" })} />Run without restrictions</label>
      {config.mode === "direct" ? <p className="text-[var(--warn)]">This app will run with your normal account access, including your files, network, and devices.</p> : null}
    </fieldset>
    {config.mode === "bubblewrap" ? <>
      <BubblewrapStatus status={status} isChecking={isChecking} isSaving={isSaving} onRecheck={() => void checkStatus()} />
      {catalog?.sandbox ? <section className="text-[13px]">
        <Button disabled={isSaving} onClick={() => {
          setConfig({ mode: "bubblewrap", permissions: structuredClone(catalog.sandbox ?? restrictedPermissions()) });
          setHasUsedSuggestion(true);
          setLocalError(null);
        }}>Use catalog suggestion</Button>
        <p className="text-[var(--muted)] mt-2">{hasUsedSuggestion ? "Suggestion applied. Review every permission and path below before saving." : "The catalog has suggested permissions. They are not applied until you choose them."}</p>
      </section> : null}
      <PermissionFields permissions={config.permissions} isSaving={isSaving} onChange={(permissions) => setConfig({ ...config, permissions })} />
      <AllowedFolders permissions={config.permissions} backend={backend} isSaving={isSaving} onChange={(permissions) => setConfig({ ...config, permissions })} onError={setLocalError} />
    </> : null}
    {error || localError ? <p role="alert" className="text-[var(--bad)] text-[13px]">{localError ?? error}</p> : null}
    <div className="sticky bottom-0 -mx-7 -mb-7 px-7 py-4 bg-[var(--panel)] flex justify-end gap-2 mt-auto"><Button variant="ghost" disabled={isSaving} onClick={onClose}>Cancel</Button><Button variant="accent" disabled={!canSave} onClick={() => void save()}>{isSaving ? "Saving…" : launchAfterSave ? "Save and launch" : "Save permissions"}</Button></div>
  </Dialog>;
}

function BubblewrapStatus({ status, isChecking, isSaving, onRecheck }: {
  status: SandboxStatus | null;
  isChecking: boolean;
  isSaving: boolean;
  onRecheck: () => void;
}) {
  return <section aria-label="Bubblewrap availability" className="rounded-lg bg-[var(--panel2)] p-4 text-[14px]">
    {isChecking ? <p role="status">Checking bubblewrap…</p> : status?.available && !status.error ? <p>Bubblewrap available{status.version ? ` (${status.version})` : ""}.</p> : <>
      <p className="text-[var(--warn)]">{status?.version ? "Bubblewrap is installed but cannot create a sandbox" : "Bubblewrap is unavailable"}{status?.error ? `: ${status.error}` : "."}</p>
      {!status?.version ? <><p className="mt-2">On Arch Linux, install it in a terminal:</p><code className="block my-2 select-all rounded-md bg-[var(--bg)] px-3 py-2">sudo pacman -S bubblewrap</code>
        <p className="text-[var(--muted)]">For other Linux distributions, find the bubblewrap package and installation command for your distribution.</p></> : <p className="mt-2 text-[var(--muted)]">Check your system's user namespace restrictions, then recheck.</p>}
    </>}
    <Button size="sm" className="mt-2" disabled={isChecking || isSaving} onClick={onRecheck}>Recheck bubblewrap</Button>
  </section>;
}

function PermissionFields({ permissions, isSaving, onChange }: {
  permissions: SandboxPermissions;
  isSaving: boolean;
  onChange: (permissions: SandboxPermissions) => void;
}) {
  return <fieldset disabled={isSaving} className="flex flex-col gap-2">
    <legend className="font-medium text-[14px] mb-2">Permissions</legend>
    {PERMISSIONS.map(({ key, label, description }) => <label key={key} className={`${CHOICE_CLASS} items-start text-[14px]`}>
      <input type="checkbox" className="mt-1 accent-[var(--accent)]" checked={permissions[key]} onChange={(event) => onChange({ ...permissions, [key]: event.target.checked })} />
      <span>{label}<span className="block text-[13px] text-[var(--muted)]">{description}</span></span>
    </label>)}
  </fieldset>;
}

function AllowedFolders({ permissions, backend, isSaving, onChange, onError }: {
  permissions: SandboxPermissions;
  backend: AppApi;
  isSaving: boolean;
  onChange: (permissions: SandboxPermissions) => void;
  onError: (error: string | null) => void;
}) {
  const [path, setPath] = useState("");
  const [access, setAccess] = useState<"read-only" | "read-write">("read-only");
  const addPath = (selected: string) => {
    const updated = { ...permissions, paths: [...permissions.paths, { path: selected.trim(), access }] };
    const validation = validateSandboxPaths(updated);
    onError(validation);
    if (validation) return;
    onChange(updated);
    setPath("");
  };
  const pickPath = async () => {
    try {
      const selected = await backend.pickFolder();
      if (selected) addPath(selected);
    } catch (failure) {
      onError(failure instanceof Error ? failure.message : String(failure));
    }
  };
  return <section aria-label="Allowed paths" className="text-[14px] flex flex-col gap-2">
    <h3 className="font-medium">Folders</h3>
    <p className="text-[13px] text-[var(--muted)]">Your personal files are blocked unless you add a folder. Read-write access lets the app change or delete its contents.</p>
    {!permissions.paths.length ? <p>No personal paths allowed.</p> : null}
    {permissions.paths.map((grant, index) => <div key={grant.path} className="flex flex-wrap gap-2 items-center">
      <span className="break-all flex-1">{grant.path}</span>
      <select aria-label={`Access for ${grant.path}`} disabled={isSaving} value={grant.access} onChange={(event) => {
        const selected = event.target.value === "read-write" ? "read-write" : "read-only";
        onChange({ ...permissions, paths: permissions.paths.map((entry, entryIndex) => entryIndex === index ? { ...entry, access: selected } : entry) });
      }}><option value="read-only">Read only</option><option value="read-write">Read and write</option></select>
      <Button size="sm" disabled={isSaving} aria-label={`Remove path ${grant.path}`} onClick={() => onChange({ ...permissions, paths: permissions.paths.filter((_, entryIndex) => entryIndex !== index) })}>Remove</Button>
    </div>)}
    <label>Absolute path<input className="block w-full mt-1 h-10 rounded-md bg-[var(--panel2)] border border-[var(--border)] px-4 outline-none focus:border-[var(--accent)]" value={path} disabled={isSaving} placeholder="/home/you/Documents" onChange={(event) => setPath(event.target.value)} /></label>
    <label>New path access<select className="ml-2" value={access} disabled={isSaving} onChange={(event) => setAccess(event.target.value === "read-write" ? "read-write" : "read-only")}><option value="read-only">Read only</option><option value="read-write">Read and write</option></select></label>
    <div className="flex gap-2"><Button disabled={isSaving || !path.trim()} onClick={() => addPath(path)}>Add path</Button>{backend.isTauri ? <Button disabled={isSaving} onClick={() => void pickPath()}>Choose folder</Button> : null}</div>
  </section>;
}
