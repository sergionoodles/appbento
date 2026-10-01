import { ExternalLink, Globe, HardDrive, Monitor, MoreHorizontal, Play, RefreshCw, Settings, Image, Trash2, TriangleAlert, Volume2, Cpu, Database, type LucideIcon } from "lucide-react";
import type { CatalogEntry, InstalledApp, SandboxPermissions, UpdateInfo } from "@/lib/types";
import { cn } from "@/lib/utils";
import { AppIcon, accentOf } from "./app-icon";
import { accessLabel, updateLabel, versionLabel } from "./app-status";
import { formatAge } from "@/lib/relative-time";
import { StageHeader } from "./stage";
import { Button } from "./ui/button";
import { Menu, MenuItem } from "./ui/menu";

export const STAGE_SELECTOR = "[data-stage]";

const PERMISSION_ICONS: { key: Exclude<keyof SandboxPermissions, "paths">; label: string; icon: LucideIcon }[] = [
  { key: "network", label: "Network", icon: Globe },
  { key: "display", label: "Display", icon: Monitor },
  { key: "audio", label: "Audio", icon: Volume2 },
  { key: "gpu", label: "Graphics", icon: Cpu },
  { key: "persistent_data", label: "Own data", icon: Database },
];

function InfoTile({ title, children, className, index }: { title: string; children: React.ReactNode; className?: string; index: number }) {
  return (
    <section aria-label={title} className={cn("rounded-[10px] border border-[var(--border)] bg-[var(--panel)] p-5 flex flex-col gap-3", className)} style={{ "--i": index } as React.CSSProperties}>
      <h2 className="text-[14px] text-[var(--muted)]">{title}</h2>
      {children}
    </section>
  );
}

function AccessTile({ app }: { app: InstalledApp }) {
  const config = app.launch_config;
  if (!config) return <p className="text-[var(--warn)]">Review permissions before the first launch.</p>;
  if (config.mode === "direct") {
    return <p className="flex items-start gap-2 text-[var(--warn)]"><TriangleAlert size={18} className="shrink-0 mt-0.5" />Runs with your normal account access, including files, network and devices.</p>;
  }
  const folderCount = config.permissions.paths.length;
  return (
    <>
      <ul className="flex flex-wrap gap-2">
        {PERMISSION_ICONS.map(({ key, label, icon: Icon }) => (
          <li key={key} className={cn("flex items-center gap-1.5 rounded-md px-3 h-8 text-[13px]", config.permissions[key] ? "bg-[var(--tile-accent)] text-[var(--on-accent)]" : "bg-[var(--panel2)] text-[var(--muted)] line-through decoration-[var(--border)]")}>
            <Icon size={14} />{label}
          </li>
        ))}
      </ul>
      <p className="text-[13px] text-[var(--muted)]">{folderCount ? `${folderCount} shared ${folderCount === 1 ? "folder" : "folders"}` : "No personal folders shared"}</p>
    </>
  );
}

export function AppDetail({ app, entry, update, isLaunching, disabled, onBack, onLaunch, onSettings, onIcon, onRemove, onOpenUrl, onUpdateSource, onCheckUpdate, onInstallUpdate, isCheckingUpdates }: {
  app: InstalledApp;
  entry?: CatalogEntry;
  update?: UpdateInfo;
  isLaunching: boolean;
  disabled: boolean;
  isCheckingUpdates: boolean;
  onUpdateSource: (id: string) => void;
  onCheckUpdate: (id: string) => void;
  onInstallUpdate: (id: string) => void;
  onBack: () => void;
  onLaunch: (id: string) => void;
  onSettings: (id: string) => void;
  onIcon: (id: string) => void;
  onRemove: (id: string) => void;
  onOpenUrl: (url: string) => void;
}) {
  const updateText = updateLabel(update);
  const fileAge = formatAge(app.file_created_at);
  return (
    <div style={{ "--tile-accent": accentOf(entry) } as React.CSSProperties}>
      <StageHeader title={app.name} onBack={onBack}>
        <Button variant="soft" size="icon" aria-label={`Settings for ${app.name}`} disabled={disabled} onClick={() => onSettings(app.id)}><Settings size={16} /></Button>
        <Menu label={`More options for ${app.name}`} variant="soft" icon={<MoreHorizontal size={17} />} disabled={disabled}>
          {entry ? <MenuItem icon={<ExternalLink size={15} />} onSelect={() => onOpenUrl(entry.app.website)}>Website</MenuItem> : null}
          <MenuItem icon={<RefreshCw size={15} />} onSelect={() => onUpdateSource(app.id)}>Update source</MenuItem>
          <MenuItem icon={<Image size={15} />} onSelect={() => onIcon(app.id)}>Change icon</MenuItem>
          <MenuItem icon={<Trash2 size={15} />} onSelect={() => onRemove(app.id)} isDanger label={`Remove ${app.name}`}>Remove</MenuItem>
        </Menu>
      </StageHeader>

      <div className="grid gap-3.5 md:grid-cols-3 md:grid-rows-[auto_auto]">
        <section data-stage aria-label={`${app.name} launcher`} className="tile md:col-span-2 md:row-span-2 min-h-[380px] p-9 flex flex-col justify-between">
          <div className="settle flex flex-col gap-6">
            <AppIcon app={app} name={app.name} entry={entry} className="size-36" />
            <div>
              <p className="text-[var(--muted)] max-w-[40ch] text-[17px] leading-snug">{entry?.app.summary ?? "A portable app, ready to launch."}</p>
              <p className="mt-3 text-[14px] text-[var(--muted)]">{versionLabel(app)} · {accessLabel(app)}</p>
            </div>
          </div>
          <Button variant="accent" size="lg" disabled={disabled} onClick={() => onLaunch(app.id)}
            className={cn("self-start mt-8 h-14 px-9 text-[18px] bg-[var(--tile-accent)]", isLaunching && "launching")}>
            <Play size={20} fill="currentColor" />{isLaunching ? "Launching…" : "Launch"}
          </Button>
        </section>

        <div className="settle contents">
          <InfoTile title="Access" index={1}><AccessTile app={app} /></InfoTile>
          <InfoTile title="Version" index={2}>
            <p className="text-[26px] font-bold tracking-[-0.03em] leading-none">{updateText ?? versionLabel(app)}</p>
            {update ? <Button variant="outline" size="sm" className="self-start" disabled={disabled || isCheckingUpdates} onClick={() => onInstallUpdate(app.id)}>{update.status === "available" ? "Install update" : "Install latest"}</Button>
              : <p className="text-[13px] text-[var(--muted)]">{app.update_config ? "Check your saved source for a newer release." : "Updates are disabled for this app."}</p>}
            <div className="flex flex-wrap gap-2">
              {app.update_config ? <Button variant="outline" size="sm" disabled={disabled || isCheckingUpdates} onClick={() => onCheckUpdate(app.id)}>Check for updates</Button> : null}
              <Button variant="ghost" size="sm" disabled={disabled} onClick={() => onUpdateSource(app.id)}>Configure updates</Button>
            </div>
          </InfoTile>
        </div>

        <InfoTile title="File" index={3} className="md:col-span-3">
          <div className="flex flex-wrap items-center gap-x-8 gap-y-2 text-[14px]">
            <span className="flex items-center gap-2 min-w-0"><HardDrive size={15} className="text-[var(--muted)] shrink-0" /><span className="truncate">{app.file_path}</span></span>
            {fileAge ? <span className="text-[var(--muted)]" title={new Date(app.file_created_at ?? "").toLocaleString()}>Downloaded {fileAge}</span> : null}
            <span className="text-[var(--muted)]">{(app.size_bytes / 1e6).toFixed(0)} MB</span>
            <span className="text-[var(--muted)]">{app.launch_count === 1 ? "Launched once" : `Launched ${app.launch_count} times`}</span>
            {!app.integrated ? <span className="text-[var(--muted)]">Portable, not in the app menu</span> : null}
          </div>
        </InfoTile>
      </div>
    </div>
  );
}
