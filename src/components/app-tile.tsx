import { useAppAppearance } from "./use-app-appearance";
import { MoreHorizontal, FileText, Play, Settings, Trash2, PanelsTopLeft } from "lucide-react";
import type { InstalledApp, UpdateInfo } from "@/lib/types";
import { formatAge } from "@/lib/relative-time";
import { cn } from "@/lib/utils";
import { AppIcon } from "./app-icon";
import { accessLabel, updateLabel, versionLabel } from "./app-status";
import { Menu, MenuItem, MenuSeparator } from "./ui/menu";

export interface TileActions {
  onLaunch: (id: string) => void;
  onViewLogs: (id: string) => void;
  onSettings: (id: string) => void;
  onRemove: (id: string) => void;
  onSetMenuEntry: (id: string, enabled: boolean) => void;
  onDragStart: (event: React.PointerEvent<HTMLElement>, id: string) => void;
}

const ACCENT_PLAY_STYLE = { background: "var(--tile-accent)", color: "var(--on-accent)" };
const NEUTRAL_PLAY_STYLE = { background: "color-mix(in oklab, var(--fg) 18%, var(--panel2))", color: "var(--fg)" };

function statusText(app: InstalledApp, update?: UpdateInfo) {
  const fileAge = formatAge(app.file_created_at);
  return [updateLabel(update), accessLabel(app), fileAge].filter(Boolean).join(" · ");
}

function TileFace({ app, update, menu, launch }: {
  app: InstalledApp;
  update?: UpdateInfo;
  menu?: React.ReactNode;
  launch?: React.ReactNode;
}) {
  return (
    <div className="pointer-events-none relative h-full flex flex-col justify-between p-3">
      <div className="flex items-center gap-2.5">
        <AppIcon app={app} name={app.name} className="size-12" imageClassName="size-[85%]" />
        <div className="min-w-0 flex-1">
          <h3 className="text-[15px] font-semibold leading-tight truncate">{app.name}</h3>
          <p className="mt-0.5 text-[12px] text-[var(--muted)] truncate">{versionLabel(app)}</p>
        </div>
      </div>
      <div className="flex items-center gap-1.5">
        <p className={cn("min-w-0 flex-1 text-[12px] truncate", update ? "text-[var(--warn)]" : "text-[var(--muted)]")}>{statusText(app, update)}</p>
        {menu}
        {launch}
      </div>
    </div>
  );
}

/** The floating copy of a tile that follows the pointer while it is dragged. */
export function TileGhost({ app, update, isShrunk, style }: {
  app: InstalledApp;
  update?: UpdateInfo;
  isShrunk: boolean;
  style: React.CSSProperties;
}) {
  const appearance = useAppAppearance(app.name, app);
  return (
    <div aria-hidden="true" className={cn("tile tile-ghost", isShrunk && "tile-ghost-shrunk")} style={{ ...appearance.style, "--i": 0, ...style } as React.CSSProperties}>
      <TileFace app={app} update={update} />
    </div>
  );
}

/** Right-clicking a tile opens the same menu as its "more" button, as in a desktop file manager. */
function openTileMenu(event: React.MouseEvent<HTMLElement>) {
  const trigger = event.currentTarget.querySelector<HTMLButtonElement>('[aria-haspopup="menu"]:not(:disabled)');
  if (!trigger) return;
  event.preventDefault();
  if (trigger.getAttribute("aria-expanded") !== "true") trigger.click();
}

export function AppTile({ app, update, index, actions, isPlaceholder, disabled }: {
  app: InstalledApp;
  update?: UpdateInfo;
  index: number;
  actions: TileActions;
  isPlaceholder: boolean;
  disabled: boolean;
}) {
  const appearance = useAppAppearance(app.name, app);
  const menu = (
    <div className="tile-menu tile-action pointer-events-auto shrink-0">
      <Menu label={`More options for ${app.name}`} icon={<MoreHorizontal size={16} />} disabled={disabled}>
        <MenuItem icon={<Settings size={15} />} onSelect={() => actions.onSettings(app.id)}>Settings</MenuItem>
        <MenuItem icon={<FileText size={15} />} onSelect={() => actions.onViewLogs(app.id)}>View logs</MenuItem>
        <MenuItem icon={<PanelsTopLeft size={15} />} onSelect={() => actions.onSetMenuEntry(app.id, !app.desktop_id)}>
          {app.desktop_id ? "Remove from app menu" : "Add to app menu"}
        </MenuItem>
        <MenuSeparator />
        <MenuItem icon={<Trash2 size={15} />} onSelect={() => actions.onRemove(app.id)} isDanger label={`Remove ${app.name}`}>Remove</MenuItem>
      </Menu>
    </div>
  );
  const launch = (
    <button type="button" aria-label={`Launch ${app.name}`} disabled={disabled} onClick={() => actions.onLaunch(app.id)}
      className="tile-play tile-action pointer-events-auto grid place-items-center size-9 shrink-0 rounded-md"
      style={appearance.palette.isVeryDark ? NEUTRAL_PLAY_STYLE : ACCENT_PLAY_STYLE}>
      <Play size={16} fill="currentColor" />
    </button>
  );

  return (
    <article aria-label={app.name} className={cn("tile", isPlaceholder && "tile-placeholder")} data-tile-id={app.id} style={{ ...appearance.style, "--i": index } as React.CSSProperties}
      onPointerDown={(event) => actions.onDragStart(event, app.id)} onContextMenu={openTileMenu}>
      <TileFace app={app} update={update} menu={menu} launch={launch} />
    </article>
  );
}
