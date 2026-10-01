import { useEffect, useState } from "react";
import { CircleCheck, FolderOpen, Folder, Loader2, ShieldCheck, ShieldOff, Sparkles, TriangleAlert, X } from "lucide-react";
import type { AppApi } from "@/lib/api";
import type { InstalledApp, LaunchConfig, SandboxPath, SandboxPermissions, SandboxStatus } from "@/lib/types";
import { restrictedPermissions, validateSandboxPaths } from "@/lib/sandbox";
import { Button } from "../../ui/button";
import { Input } from "../../ui/card";
import { FieldGroup, Notice } from "../../ui/field";
import { Select } from "../../ui/select";
import { Switch } from "../../ui/switch";

type PathAccess = SandboxPath["access"];

type ToggleKey = Exclude<keyof SandboxPermissions, "paths">;

const PERMISSIONS: { key: ToggleKey; label: string; description: string }[] = [
  { key: "network", label: "Internet and network", description: "Connect to the internet and local network services." },
  { key: "display", label: "Display", description: "Open windows. X11 access can also expose other windows and keyboard input." },
  { key: "audio", label: "Audio", description: "Play sound and record from microphones through the audio service." },
  { key: "gpu", label: "Graphics acceleration", description: "Access graphics devices and their drivers." },
  { key: "persistent_data", label: "Keep private app data", description: "Keep an isolated home folder between launches. This does not grant access to your personal home folder." },
];

const LAUNCH_MODES: { mode: LaunchConfig["mode"]; label: string; description: string; icon: typeof ShieldCheck }[] = [
  { mode: "bubblewrap", label: "Restrict with bubblewrap", description: "Runs in a sandbox with only the access you allow below.", icon: ShieldCheck },
  { mode: "direct", label: "Run without restrictions", description: "Runs like any other program on your account.", icon: ShieldOff },
];

function errorMessage(failure: unknown) {
  return failure instanceof Error ? failure.message : String(failure);
}

export function useSandboxStatus(backend: AppApi) {
  const [status, setStatus] = useState<SandboxStatus | null>(null);
  const [isChecking, setIsChecking] = useState(true);

  useEffect(() => {
    let isActive = true;
    backend.sandboxStatus()
      .then((result) => { if (isActive) setStatus(result); })
      .catch((failure: unknown) => { if (isActive) setStatus({ available: false, version: null, error: errorMessage(failure) }); })
      .finally(() => { if (isActive) setIsChecking(false); });
    return () => { isActive = false; };
  }, [backend]);

  const recheck = async () => {
    setIsChecking(true);
    try {
      setStatus(await backend.sandboxStatus());
    } catch (failure) {
      setStatus({ available: false, version: null, error: errorMessage(failure) });
    } finally {
      setIsChecking(false);
    }
  };

  const isReady = !isChecking && !!status?.available && !status.error;
  return { status, isChecking, isReady, recheck };
}

export function PermissionsPage({ app, backend, config, onChange, sandbox, isReview, isSaving, onError }: {
  app: InstalledApp;
  backend: AppApi;
  config: LaunchConfig;
  onChange: (config: LaunchConfig) => void;
  sandbox: ReturnType<typeof useSandboxStatus>;
  isReview: boolean;
  isSaving: boolean;
  onError: (error: string | null) => void;
}) {
  const [hasUsedDefaults, setHasUsedDefaults] = useState(false);
  const setPermissions = (permissions: SandboxPermissions) => onChange({ ...config, permissions });
  const defaults = app.permission_defaults;

  return (
    <div className="flex flex-col gap-7">
      {isReview ? <Notice icon={<ShieldCheck size={16} />}>Review how this AppImage can access your computer before its first launch. You can change this later.</Notice> : null}
      {!backend.isTauri ? <Notice tone="warn" icon={<TriangleAlert size={16} />}>Sample preview: permissions are simulated. No sandbox or app is launched here.</Notice> : null}

      <fieldset disabled={isSaving} className="grid grid-cols-2 gap-3">
        <legend className="sr-only">Launch mode</legend>
        {LAUNCH_MODES.map(({ mode, label, description, icon: Icon }) => (
          <label key={mode} className="group relative flex cursor-pointer flex-col gap-3 rounded-lg border border-[var(--border)] p-4 transition-colors hover:border-[var(--muted)] has-[:checked]:border-[var(--accent)] has-[:checked]:bg-[color-mix(in_oklab,var(--accent)_7%,transparent)] has-[:focus-visible]:outline-2 has-[:focus-visible]:outline-offset-2 has-[:focus-visible]:outline-[var(--accent)]">
            <input type="radio" name="launch-mode" className="sr-only" aria-label={label} aria-describedby={`launch-mode-${mode}`}
              checked={config.mode === mode} onChange={() => onChange({ ...config, mode })} />
            <span className="flex items-center justify-between">
              <span className="grid size-9 place-items-center rounded-md bg-[var(--panel2)] text-[var(--muted)] group-has-[:checked]:bg-[var(--accent)] group-has-[:checked]:text-[var(--on-accent)]"><Icon size={18} /></span>
              <span aria-hidden="true" className="size-4 rounded-full border-2 border-[var(--border)] group-has-[:checked]:border-[5px] group-has-[:checked]:border-[var(--accent)]" />
            </span>
            <span>
              <span className="block text-[14px] font-semibold">{label}</span>
              <span id={`launch-mode-${mode}`} className="block text-[12.5px] leading-snug text-[var(--muted)] mt-0.5">{description}</span>
            </span>
          </label>
        ))}
      </fieldset>

      {config.mode === "direct"
        ? <Notice tone="warn" icon={<TriangleAlert size={16} />}>This app will run with your normal account access, including your files, network, and devices.</Notice>
        : <>
          <BubblewrapStatus status={sandbox.status} isChecking={sandbox.isChecking} isSaving={isSaving} onRecheck={() => void sandbox.recheck()} />
          {defaults ? <Notice icon={<Sparkles size={16} />} action={
            <Button size="sm" variant="outline" disabled={isSaving} onClick={() => {
              onChange({ mode: "bubblewrap", permissions: structuredClone(defaults) });
              setHasUsedDefaults(true);
              onError(null);
            }}>Use import defaults</Button>
          }>{hasUsedDefaults ? "Defaults applied. Review every permission and folder below before saving." : "Permissions were suggested when this app was imported. They are not applied until you choose them."}</Notice> : null}
          <PermissionToggles permissions={config.permissions} isSaving={isSaving} onChange={setPermissions} />
          <SharedFolders permissions={config.permissions} backend={backend} isSaving={isSaving} onChange={setPermissions} onError={onError} />
        </>}
    </div>
  );
}

export function initialLaunchConfig(app: InstalledApp): LaunchConfig {
  return structuredClone(app.launch_config ?? { mode: "direct", permissions: restrictedPermissions() });
}

function BubblewrapStatus({ status, isChecking, isSaving, onRecheck }: {
  status: SandboxStatus | null;
  isChecking: boolean;
  isSaving: boolean;
  onRecheck: () => void;
}) {
  const recheck = <Button size="sm" variant="ghost" aria-label="Recheck bubblewrap" disabled={isChecking || isSaving} onClick={onRecheck}>Recheck</Button>;
  if (isChecking) {
    return <Notice icon={<Loader2 size={16} className="animate-spin" />} action={recheck}><p role="status">Checking bubblewrap…</p></Notice>;
  }
  if (status?.available && !status.error) {
    return <Notice tone="ok" icon={<CircleCheck size={16} />} action={recheck}>Bubblewrap is available{status.version ? ` (${status.version})` : ""}.</Notice>;
  }
  return (
    <Notice tone="warn" icon={<TriangleAlert size={16} />} action={recheck}>
      <p>{status?.version ? "Bubblewrap is installed but cannot create a sandbox" : "Bubblewrap is unavailable"}{status?.error ? `: ${status.error}` : "."}</p>
      {status?.version
        ? <p className="mt-1.5 text-[var(--muted)]">Check your system's user namespace restrictions, then recheck.</p>
        : <div className="mt-2 text-[var(--muted)]">
          <p>On Arch Linux, install it in a terminal:</p>
          <code className="block my-2 select-all rounded-md bg-[var(--bg)] px-3 py-2 text-[var(--fg)]">sudo pacman -S bubblewrap</code>
          <p>For other distributions, install the bubblewrap package with your package manager.</p>
        </div>}
    </Notice>
  );
}

function PermissionToggles({ permissions, isSaving, onChange }: {
  permissions: SandboxPermissions;
  isSaving: boolean;
  onChange: (permissions: SandboxPermissions) => void;
}) {
  return (
    <FieldGroup title="Access" description="Everything is blocked unless you turn it on.">
      {PERMISSIONS.map(({ key, label, description }) => (
        <label key={key} className="flex cursor-pointer items-center gap-4 px-4 py-3">
          <span className="flex-1 min-w-0">
            <span className="block text-[14px]">{label}</span>
            <span id={`permission-${key}`} className="block text-[12.5px] leading-snug text-[var(--muted)]">{description}</span>
          </span>
          <Switch aria-label={label} aria-describedby={`permission-${key}`} disabled={isSaving} checked={permissions[key]}
            onChange={(event) => onChange({ ...permissions, [key]: event.target.checked })} />
        </label>
      ))}
    </FieldGroup>
  );
}

function AccessSelect({ value, onChange, ...props }: { value: PathAccess; onChange: (access: PathAccess) => void; "aria-label": string; disabled: boolean; className?: string }) {
  return (
    <Select value={value} onChange={(event) => onChange(event.target.value === "read-write" ? "read-write" : "read-only")} {...props}>
      <option value="read-only">Read only</option>
      <option value="read-write">Read and write</option>
    </Select>
  );
}

function SharedFolders({ permissions, backend, isSaving, onChange, onError }: {
  permissions: SandboxPermissions;
  backend: AppApi;
  isSaving: boolean;
  onChange: (permissions: SandboxPermissions) => void;
  onError: (error: string | null) => void;
}) {
  const [path, setPath] = useState("");
  const [access, setAccess] = useState<PathAccess>("read-only");

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
      onError(errorMessage(failure));
    }
  };

  const addRow = (
    <form className="flex items-center gap-2" onSubmit={(event) => {
      event.preventDefault();
      if (path.trim()) addPath(path);
    }}>
      <Input aria-label="Absolute path" className="flex-1 font-mono text-[13px]" value={path} disabled={isSaving} placeholder="/home/you/Documents" onChange={(event) => setPath(event.target.value)} />
      <AccessSelect aria-label="New path access" className="w-40" value={access} disabled={isSaving} onChange={setAccess} />
      <Button type="submit" disabled={isSaving || !path.trim()}>Add path</Button>
      {backend.isTauri ? <Button size="icon" className="size-10" aria-label="Choose folder" disabled={isSaving} onClick={() => void pickPath()}><FolderOpen size={16} /></Button> : null}
    </form>
  );

  return (
    <FieldGroup title="Shared folders" description="Your personal files are blocked unless you add a folder. Read and write access lets the app change or delete its contents." footer={addRow}>
      {!permissions.paths.length ? <p className="px-4 py-3 text-[13px] text-[var(--muted)]">No personal paths allowed.</p> : null}
      {permissions.paths.map((grant, index) => (
        <div key={grant.path} className="flex items-center gap-3 px-4 py-2">
          <Folder size={16} className="shrink-0 text-[var(--muted)]" />
          <span className="flex-1 min-w-0 truncate font-mono text-[13px]" title={grant.path}>{grant.path}</span>
          <AccessSelect aria-label={`Access for ${grant.path}`} className="w-40" value={grant.access} disabled={isSaving}
            onChange={(selected) => onChange({ ...permissions, paths: permissions.paths.map((entry, entryIndex) => entryIndex === index ? { ...entry, access: selected } : entry) })} />
          <Button variant="ghost" size="icon" aria-label={`Remove path ${grant.path}`} disabled={isSaving}
            onClick={() => onChange({ ...permissions, paths: permissions.paths.filter((_, entryIndex) => entryIndex !== index) })}><X size={15} /></Button>
        </div>
      ))}
    </FieldGroup>
  );
}
