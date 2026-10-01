import { useEffect } from "react";
import { Compass, MoreHorizontal, Play, Plus, ShieldCheck, SlidersHorizontal, Image, Trash2 } from "lucide-react";
import type { CatalogEntry, InstalledApp, UpdateInfo } from "@/lib/types";
import { recentApps } from "@/lib/library-controller";
import { formatAge } from "@/lib/relative-time";
import { cn } from "@/lib/utils";
import { AppIcon, accentOf } from "./app-icon";
import { accessLabel, updateLabel, versionLabel } from "./app-status";
import { Menu, MenuItem } from "./ui/menu";

export const tileSelector = (id: string) => `[data-tile-id="${CSS.escape(id)}"]`;

function sizeClass(isMostUsed: boolean, isSecondMostUsed: boolean) {
  if (isMostUsed) return "span-hero";
  return isSecondMostUsed ? "span-wide" : "";
}

function AppTile({ app, entry, update, index, isMostUsed, isSecondMostUsed, onOpen, onLaunch, onPermissions, onIcon, onRemove, disabled }: {
  app: InstalledApp;
  entry?: CatalogEntry;
  update?: UpdateInfo;
  index: number;
  isMostUsed: boolean;
  isSecondMostUsed: boolean;
  onOpen: (id: string) => void;
  onLaunch: (id: string) => void;
  onPermissions: (id: string) => void;
  onIcon: (id: string) => void;
  onRemove: (id: string) => void;
  disabled: boolean;
}) {
  const isHero = isMostUsed;
  const isWide = isSecondMostUsed;
  const fileAge = formatAge(app.file_created_at);
  const baseStatus = app.launch_config ? `${accessLabel(app)} · ${versionLabel(app)}` : accessLabel(app);
  const status = [updateLabel(update), baseStatus, fileAge].filter(Boolean).join(" · ");
  return (
    <article className={cn("tile", sizeClass(isHero, isWide))} data-tile-id={app.id} style={{ "--tile-accent": accentOf(entry), "--i": index } as React.CSSProperties}>
      <button type="button" className="tile-hit" aria-label={`Open ${app.name}`} onClick={() => onOpen(app.id)} />
      <div className={cn("pointer-events-none relative h-full flex flex-col justify-between", isHero ? "p-7" : "p-5")}>
        <div className="flex items-start justify-between">
          <AppIcon app={app} name={app.name} entry={entry} className={isHero ? "size-24" : "size-16"} />
          {isHero && app.launch_count > 0 ? <span className="rounded-full bg-[var(--panel2)] px-3 py-1 text-[12px] text-[var(--muted)]">Most used</span> : null}
          <div className="pointer-events-auto tile-action">
            <Menu label={`More options for ${app.name}`} icon={<MoreHorizontal size={17} />} disabled={disabled}>
              <MenuItem icon={<SlidersHorizontal size={15} />} onSelect={() => onPermissions(app.id)}>Permissions</MenuItem>
              <MenuItem icon={<Image size={15} />} onSelect={() => onIcon(app.id)}>Change icon</MenuItem>
              <MenuItem icon={<Trash2 size={15} />} onSelect={() => onRemove(app.id)} isDanger label={`Remove ${app.name}`}>Remove</MenuItem>
            </Menu>
          </div>
        </div>
        <div className="flex items-end justify-between gap-3">
          <div className="min-w-0">
            <h3 className={cn("font-bold tracking-[-0.035em] leading-none truncate", isHero ? "text-[44px]" : "text-[22px]")}>{app.name}</h3>
            {(isHero || isWide) && entry ? <p className="mt-2 text-[var(--muted)] max-w-[34ch] line-clamp-2">{entry.app.summary}</p> : null}
            <p className={cn("mt-1.5 text-[13px] truncate", update ? "text-[var(--warn)]" : "text-[var(--muted)]")}>{status}</p>
          </div>
          <button type="button" aria-label={`Launch ${app.name}`} disabled={disabled} onClick={() => onLaunch(app.id)}
            className="tile-play tile-action pointer-events-auto grid place-items-center size-11 shrink-0 rounded-md cursor-pointer disabled:opacity-40"
            style={{ background: "var(--tile-accent)", color: "var(--on-accent)" }}>
            <Play size={18} fill="currentColor" />
          </button>
        </div>
      </div>
    </article>
  );
}

function ShortcutTile({ icon, title, hint, className, index, onClick, isDashed }: {
  icon: React.ReactNode;
  title: string;
  hint: string;
  className?: string;
  index: number;
  onClick: () => void;
  isDashed?: boolean;
}) {
  return (
    <article className={cn("tile tile-plain", isDashed && "tile-dashed", className)} style={{ "--i": index } as React.CSSProperties}>
      <button type="button" className="tile-hit" aria-label={title} onClick={onClick} />
      <div className="pointer-events-none h-full p-5 flex flex-col justify-between">
        <span className="grid place-items-center size-12 rounded-md bg-[var(--panel2)]">{icon}</span>
        <div>
          <h3 className="text-[20px] font-bold tracking-[-0.03em]">{title}</h3>
          <p className="text-[13px] text-[var(--muted)]">{hint}</p>
        </div>
      </div>
    </article>
  );
}

export function BentoSkeleton() {
  return (
    <div className="bento" role="status" aria-label="Loading local library">
      {Array.from({ length: 6 }, (_, index) => (
        <div key={index} className={cn("rounded-[10px] bg-[var(--panel)] skeleton", sizeClass(index === 0, index === 1))} style={{ animationDelay: `${index * 120}ms` }} />
      ))}
    </div>
  );
}

export function Bento({ apps, catalog, updates, isAnimated, onEntered, onOpen, onLaunch, onPermissions, onIcon, onRemove, onDiscover, onAdd, canAdd, disabled }: {
  apps: InstalledApp[];
  catalog: CatalogEntry[];
  updates: UpdateInfo[];
  isAnimated: boolean;
  onEntered: () => void;
  onOpen: (id: string) => void;
  onLaunch: (id: string) => void;
  onPermissions: (id: string) => void;
  onIcon: (id: string) => void;
  onRemove: (id: string) => void;
  onDiscover: () => void;
  onAdd: () => void;
  canAdd: boolean;
  disabled: boolean;
}) {
  useEffect(() => {
    if (!isAnimated) return;
    const timer = window.setTimeout(onEntered, 1400);
    return () => window.clearTimeout(timer);
  }, [isAnimated, onEntered]);

  const recent = recentApps(apps, apps.length);
  const ranked = [...recent].sort((first, second) => second.launch_count - first.launch_count || (second.last_launched ?? "").localeCompare(first.last_launched ?? ""));
  const mostUsedId = ranked[0]?.launch_count ? ranked[0].id : null;
  const secondMostUsedId = ranked[1]?.launch_count ? ranked[1].id : null;
  const updatesById = new Map(updates.map((update) => [update.app_id, update]));
  return (
    <>
      <h1 className="sr-only">Recent applications</h1>
      <div className={cn("bento", isAnimated && "intro")}>
        {recent.map((app, index) => (
          <AppTile key={app.id} app={app} index={index} isMostUsed={app.id === mostUsedId} isSecondMostUsed={app.id === secondMostUsedId} entry={catalog.find((entry) => entry.app.id === app.catalog_id)} update={updatesById.get(app.id)}
            onOpen={onOpen} onLaunch={onLaunch} onPermissions={onPermissions} onIcon={onIcon} onRemove={onRemove} disabled={disabled} />
        ))}
        <ShortcutTile index={recent.length} className="span-wide" icon={<Compass size={20} />} title="Discover apps" hint={`${catalog.length} ready-made entries`} onClick={onDiscover} />
        {canAdd ? <ShortcutTile index={recent.length + 1} isDashed icon={<Plus size={20} />} title="Add an AppImage" hint="Or scan your folders" onClick={onAdd} /> : null}
      </div>
      {!recent.length ? <p className="mt-6 flex items-center gap-2 text-[var(--muted)]"><ShieldCheck size={16} />No apps match. Add an AppImage or scan your folders from the + menu.</p> : null}
    </>
  );
}
