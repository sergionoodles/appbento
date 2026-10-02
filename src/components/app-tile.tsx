import { ArrowLeft, ArrowRight, FolderInput, FolderPlus, Image, MoreHorizontal, Play, Settings, Trash2 } from "lucide-react";
import type { CatalogEntry, InstalledApp, UpdateInfo } from "@/lib/types";
import { formatAge } from "@/lib/relative-time";
import { cn } from "@/lib/utils";
import { AppIcon, accentOf } from "./app-icon";
import { accessLabel, updateLabel, versionLabel } from "./app-status";
import { Menu, MenuItem, MenuSeparator } from "./ui/menu";

export interface TileActions {
  onOpen: (id: string) => void;
  onLaunch: (id: string) => void;
  onSettings: (id: string) => void;
  onIcon: (id: string) => void;
  onRemove: (id: string) => void;
  onShift: (id: string, offset: number) => void;
  onMoveTo: (id: string, sectionId: string) => void;
  onNewGroup: (id: string) => void;
  onDragStart: (event: React.PointerEvent<HTMLElement>, id: string) => void;
}

/** Where a tile can go from its menu: the neighbouring positions and every other section. */
export interface TilePlacement {
  isFirst: boolean;
  isLast: boolean;
  destinations: { id: string; name: string }[];
}

function tileStyle(entry: CatalogEntry | undefined, index: number) {
  return { "--tile-accent": accentOf(entry), "--i": index } as React.CSSProperties;
}

function statusText(app: InstalledApp, update?: UpdateInfo) {
  const fileAge = formatAge(app.file_created_at);
  const baseStatus = app.launch_config ? `${accessLabel(app)} · ${versionLabel(app)}` : accessLabel(app);
  return [updateLabel(update), baseStatus, fileAge].filter(Boolean).join(" · ");
}

function TileFace({ app, entry, update, menu, launch }: {
  app: InstalledApp;
  entry?: CatalogEntry;
  update?: UpdateInfo;
  menu?: React.ReactNode;
  launch?: React.ReactNode;
}) {
  return (
    <div className="pointer-events-none relative h-full flex flex-col justify-between p-4">
      <div className="flex items-start justify-between">
        <AppIcon app={app} name={app.name} entry={entry} className="size-12" />
        {menu}
      </div>
      <div className="flex items-end justify-between gap-3">
        <div className="min-w-0">
          <h3 className="text-[15px] font-semibold leading-tight truncate">{app.name}</h3>
          <p className={cn("mt-0.5 text-[12px] truncate", update ? "text-[var(--warn)]" : "text-[var(--muted)]")}>{statusText(app, update)}</p>
        </div>
        {launch}
      </div>
    </div>
  );
}

/** The floating copy of a tile that follows the pointer while it is dragged. */
export function TileGhost({ app, entry, update, isShrunk, style }: {
  app: InstalledApp;
  entry?: CatalogEntry;
  update?: UpdateInfo;
  isShrunk: boolean;
  style: React.CSSProperties;
}) {
  return (
    <div aria-hidden="true" className={cn("tile tile-ghost", isShrunk && "tile-ghost-shrunk")} style={{ ...tileStyle(entry, 0), ...style }}>
      <TileFace app={app} entry={entry} update={update} />
    </div>
  );
}

function ArrangeItems({ app, placement, actions }: { app: InstalledApp; placement: TilePlacement; actions: TileActions }) {
  return (
    <>
      <MenuItem icon={<ArrowLeft size={15} />} disabled={placement.isFirst} onSelect={() => actions.onShift(app.id, -1)}>Move earlier</MenuItem>
      <MenuItem icon={<ArrowRight size={15} />} disabled={placement.isLast} onSelect={() => actions.onShift(app.id, 1)}>Move later</MenuItem>
      {placement.destinations.map((destination) => (
        <MenuItem key={destination.id} icon={<FolderInput size={15} />} onSelect={() => actions.onMoveTo(app.id, destination.id)}>Move to {destination.name}</MenuItem>
      ))}
      <MenuItem icon={<FolderPlus size={15} />} onSelect={() => actions.onNewGroup(app.id)}>Move to new group</MenuItem>
    </>
  );
}

/** Right-clicking a tile opens the same menu as its "more" button, as in a desktop file manager. */
function openTileMenu(event: React.MouseEvent<HTMLElement>) {
  const trigger = event.currentTarget.querySelector<HTMLButtonElement>('[aria-haspopup="menu"]:not(:disabled)');
  if (!trigger) return;
  event.preventDefault();
  if (trigger.getAttribute("aria-expanded") !== "true") trigger.click();
}

export function AppTile({ app, entry, update, index, placement, actions, isPlaceholder, disabled }: {
  app: InstalledApp;
  entry?: CatalogEntry;
  update?: UpdateInfo;
  index: number;
  placement: TilePlacement;
  actions: TileActions;
  isPlaceholder: boolean;
  disabled: boolean;
}) {
  const menu = (
    <div className="pointer-events-auto tile-action -mr-1.5 -mt-1.5">
      <Menu label={`More options for ${app.name}`} icon={<MoreHorizontal size={17} />} disabled={disabled}>
        <MenuItem icon={<Settings size={15} />} onSelect={() => actions.onSettings(app.id)}>Settings</MenuItem>
        <MenuItem icon={<Image size={15} />} onSelect={() => actions.onIcon(app.id)}>Change icon</MenuItem>
        <MenuSeparator />
        <ArrangeItems app={app} placement={placement} actions={actions} />
        <MenuSeparator />
        <MenuItem icon={<Trash2 size={15} />} onSelect={() => actions.onRemove(app.id)} isDanger label={`Remove ${app.name}`}>Remove</MenuItem>
      </Menu>
    </div>
  );
  const launch = (
    <button type="button" aria-label={`Launch ${app.name}`} disabled={disabled} onClick={() => actions.onLaunch(app.id)}
      className="tile-play tile-action pointer-events-auto grid place-items-center size-9 shrink-0 rounded-md disabled:opacity-40"
      style={{ background: "var(--tile-accent)", color: "var(--on-accent)" }}>
      <Play size={16} fill="currentColor" />
    </button>
  );

  return (
    <article className={cn("tile", isPlaceholder && "tile-placeholder")} data-tile-id={app.id} style={tileStyle(entry, index)}
      onPointerDown={(event) => actions.onDragStart(event, app.id)} onContextMenu={openTileMenu}>
      <button type="button" className="tile-hit" aria-label={`Open ${app.name}`} onClick={() => actions.onOpen(app.id)} />
      <TileFace app={app} entry={entry} update={update} menu={menu} launch={launch} />
    </article>
  );
}
