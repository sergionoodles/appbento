import { ArrowUpCircle, ExternalLink, Play, Trash2 } from "lucide-react";
import type { CatalogEntry, InstalledApp, UpdateInfo } from "@/lib/types";
import { Badge, Card } from "./ui/card";
import { Button } from "./ui/button";
import { cn } from "@/lib/utils";
import { recentApps } from "@/lib/library-controller";

export function catalogById(catalog: CatalogEntry[], id?: string | null) {
  return catalog.find((c) => c.app.id === id);
}

function initials(name: string) {
  return name.split(/[\s-]+/).map((w) => w[0]).join("").slice(0, 2).toUpperCase();
}

export function AppTile({
  app,
  catalog,
  update,
  large,
  onLaunch,
  disabled,
}: {
  app: InstalledApp;
  catalog: CatalogEntry[];
  update?: UpdateInfo;
  large?: boolean;
  onLaunch: (id: string) => void;
  disabled?: boolean;
}) {
  const meta = catalogById(catalog, app.catalog_id);
  const accent = meta?.branding.accent ?? "var(--accent)";
  return (
    <Card
      data-corners={meta?.app.id ?? "app"}
      className={cn("flex flex-col gap-2 overflow-hidden", large && "md:col-span-2 md:row-span-2 min-h-[220px]")}
    >
      {meta?.branding.cover && large ? (
        <div className="h-20 -m-4 mb-0 overflow-hidden border-b border-[var(--border)]">
          <img src={meta.branding.cover} alt="" className="w-full h-full object-cover opacity-70" loading="lazy" />
        </div>
      ) : null}
      <div className="flex items-start gap-3">
        <div
          className={cn("grid place-items-center rounded-md font-bold shrink-0", large ? "w-14 h-14 text-xl" : "w-10 h-10 text-sm")}
          style={{ background: `color-mix(in srgb, ${accent} 25%, transparent)`, border: `1px solid ${accent}` }}
        >
          {meta?.branding.logo ? (
            <img src={meta.branding.logo} alt={app.name} className="w-2/3 h-2/3 object-contain" loading="lazy" />
          ) : (
            <span style={{ color: accent }}>{initials(app.name)}</span>
          )}
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className="font-bold truncate">{app.name}</span>
            {update ? <Badge className="border-[var(--warn)] text-[var(--warn)]">{update.status === "available" ? "update" : "version unknown"}</Badge> : null}
            {!app.integrated ? <Badge>portable</Badge> : null}
          </div>
          <div className="text-[12px] text-[var(--muted)] truncate">
            <span className="tui-prompt">$ </span>./{app.file_name}
          </div>
          {meta ? <div className="text-[12px] text-[var(--muted)] truncate">{meta.app.summary}</div> : null}
        </div>
      </div>
      <div className="mt-auto flex items-center gap-2 text-[12px] text-[var(--muted)]">
        <span>v{app.version_current ?? "?"} → {update?.latest ?? "?"}</span>
        <span className="ml-auto flex gap-1">
          <Button size="sm" variant="default" disabled={disabled} onClick={() => onLaunch(app.id)}>
            <Play size={13} /> run
          </Button>
        </span>
      </div>
    </Card>
  );
}

export function BentoGrid({
  apps,
  catalog,
  updates,
  onLaunch,
  disabled,
}: {
  apps: InstalledApp[];
  catalog: CatalogEntry[];
  updates: UpdateInfo[];
  onLaunch: (id: string) => void;
  disabled?: boolean;
}) {
  const recent = recentApps(apps);
  const byId = new Map(updates.map((u) => [u.app_id, u]));
  if (recent.length === 0) {
    return (
      <Card data-corners="empty">
        <p className="text-[var(--muted)]">No matching AppImages. Use <b>scan</b> or <b>add .AppImage</b>.</p>
      </Card>
    );
  }
  return (
    <div className="grid grid-cols-1 md:grid-cols-4 gap-3">
      {recent.map((app, i) => (
        <AppTile key={app.id} app={app} catalog={catalog} update={byId.get(app.id)} large={i < 2} onLaunch={onLaunch} disabled={disabled} />
      ))}
    </div>
  );
}

export function LibraryRow({
  app,
  onLaunch,
  onRemove,
  update,
  disabled,
}: {
  app: InstalledApp;
  onLaunch: (id: string) => void;
  disabled?: boolean;
  onRemove: (id: string) => void;
  update?: UpdateInfo;
}) {
  return (
    <div className="flex items-center gap-3 px-3 py-2 border-b border-[var(--border)] last:border-0 text-[13px]">
      <span className="font-bold w-40 truncate">{app.name}</span>
      <span className="text-[var(--muted)] flex-1 truncate hidden md:block">{app.file_path}</span>
      <span className="text-[var(--muted)]">{(app.size_bytes / 1e6).toFixed(0)}M</span>
      <span className="text-[var(--muted)]">×{app.launch_count}</span>
      {update ? <span title={update.status === "available" ? "Update available" : "Current version unknown"}><ArrowUpCircle size={15} aria-label={update.status === "available" ? "Update available" : "Current version unknown"} className="text-[var(--warn)]" /></span> : null}
      <Button size="sm" disabled={disabled} aria-label={`Launch ${app.name}`} onClick={() => onLaunch(app.id)}><Play size={13} /></Button>
      <Button size="sm" variant="ghost" disabled={disabled} aria-label={`Remove ${app.name}`} onClick={() => onRemove(app.id)}><Trash2 size={13} /></Button>
    </div>
  );
}

export function CatalogCard({ entry, installed, onOpen }: { entry: CatalogEntry; installed: boolean; onOpen: (url: string) => void }) {
  return (
    <Card data-corners={entry.app.verified ? "verified" : "community"} className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <img src={entry.branding.logo} alt="" className="w-8 h-8 object-contain rounded" loading="lazy" />
        <div className="font-bold truncate">{entry.app.name}</div>
        {installed ? <Badge className="ml-auto">installed</Badge> : <Badge className="ml-auto">{entry.update.strategy}</Badge>}
      </div>
      <p className="text-[12px] text-[var(--muted)] line-clamp-2">{entry.app.summary}</p>
      <div className="flex gap-1 flex-wrap">
        {entry.app.categories.map((c) => <Badge key={c}>{c}</Badge>)}
      </div>
      <div className="mt-auto flex gap-2 text-[12px]">
        <Button size="sm" variant="ghost" onClick={() => onOpen(entry.app.website)} aria-label={`Open ${entry.app.name} website`}>
          <ExternalLink size={12} /> site
        </Button>
        {entry.app.repo ? <Button size="sm" variant="ghost" onClick={() => onOpen(entry.app.repo ?? entry.app.website)} aria-label={`Open ${entry.app.name} repository`}>repo</Button> : null}
        <span className="ml-auto text-[var(--muted)]">{entry.update.github?.repo}</span>
      </div>
    </Card>
  );
}
