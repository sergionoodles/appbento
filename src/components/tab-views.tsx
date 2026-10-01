import type { CatalogEntry, InstalledApp, Settings, UpdateInfo, UpdateReport } from "@/lib/types";
import { BentoGrid, CatalogCard, LibraryRow } from "./views";
import { Card } from "./ui/card";
import { Button } from "./ui/button";

interface LibraryViewProps {
  apps: InstalledApp[];
  updates: UpdateInfo[];
  onLaunch: (id: string) => void;
  disabled: boolean;
}

export function BentoView({ apps, catalog, updates, onLaunch, disabled }: LibraryViewProps & { catalog: CatalogEntry[] }) {
  return (
    <div className="flex flex-col gap-3">
      <h1 className="tui-title">Recent applications</h1>
      <BentoGrid apps={apps} catalog={catalog} updates={updates} onLaunch={onLaunch} disabled={disabled} />
      <Card data-corners="catalog-spotlight">
        <div className="tui-title mb-2">catalog spotlight</div>
        {catalog.slice(0, 4).map((entry) => (
          <div key={entry.app.id} className="text-[13px] flex gap-2 py-0.5">
            <span className="tui-prompt">+</span>
            <span className="font-bold">{entry.app.name}</span>
            <span className="text-[var(--muted)] truncate">{entry.app.summary}</span>
          </div>
        ))}
        {!catalog.length ? <p className="text-[13px] text-[var(--muted)]">No catalog entries loaded.</p> : null}
      </Card>
    </div>
  );
}

export function LibraryView({ apps, settings, updates, onLaunch, onRemove, disabled }: LibraryViewProps & {
  settings: Settings | null;
  onRemove: (id: string) => void;
}) {
  const updatesById = new Map(updates.map((update) => [update.app_id, update]));
  return (
    <Card data-corners={`library // ${apps.length}`}>
      <h1 className="tui-title mb-2">AppImage library</h1>
      <div className="text-[12px] text-[var(--muted)] px-1 pb-2">
        managed: <span className="text-[var(--fg)]">{settings?.managed_dir ?? "unavailable"}</span>
        {settings?.scan_dirs.length ? <> · extra scan: {settings.scan_dirs.join(", ")}</> : null}
      </div>
      {apps.map((app) => <LibraryRow key={app.id} app={app} onLaunch={onLaunch} onRemove={onRemove} update={updatesById.get(app.id)} disabled={disabled} />)}
      {!apps.length ? <p className="text-[var(--muted)] p-2">No matching apps. Scan folders or add an AppImage.</p> : null}
    </Card>
  );
}

export function CatalogView({ catalog, apps, isPreview, onOpen }: {
  catalog: CatalogEntry[];
  apps: InstalledApp[];
  isPreview: boolean;
  onOpen: (url: string) => void;
}) {
  return (
    <div className="flex flex-col gap-3">
      <h1 className="tui-title">Catalog ({catalog.length})</h1>
      <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
        {catalog.map((entry) => <CatalogCard key={entry.app.id} entry={entry} installed={apps.some((app) => app.catalog_id === entry.app.id)} onOpen={onOpen} />)}
      </div>
      {!catalog.length ? <Card><p className="text-[13px] text-[var(--muted)]">{isPreview ? "The sample preview has no catalog. Open the desktop app to browse bundled entries." : "No catalog entries match your filter or the catalog could not be loaded."}</p></Card> : null}
    </div>
  );
}

export function UpdatesView({ report, isChecking, onCheck, onOpen }: {
  report: UpdateReport | null;
  isChecking: boolean;
  onCheck: () => void;
  onOpen: (url: string) => void;
}) {
  return (
    <Card data-corners="updates">
      <div className="flex items-center justify-between gap-3 mb-3">
        <h1 className="tui-title">Release checks</h1>
        <Button disabled={isChecking} onClick={onCheck}>{isChecking ? "checking…" : "check releases"}</Button>
      </div>
      <p className="text-[12px] text-[var(--muted)] mb-2">Downloads open in your browser. Add the downloaded AppImage manually; installed files are not updated automatically.</p>
      {!report ? <p role="status">{isChecking ? "Checking releases…" : "No current release report. Check releases to see available downloads."}</p> : (
        <>
          <p className="text-[12px] text-[var(--muted)] mb-2">{report.checked_count} checked · {report.unsupported_count} without a supported update source · {report.errors.length} failed</p>
          {report.updates.map((update) => (
            <div key={update.app_id} className="flex flex-wrap items-center gap-3 py-2 border-b border-[var(--border)] text-[13px]">
              <span className="truncate flex-1">{update.asset_name}</span>
              <span className="text-[var(--muted)]">{update.status === "unknown-current" ? "current version unknown" : `${update.current} → ${update.latest}`}</span>
              <Button size="sm" onClick={() => onOpen(update.download_url)}>{update.status === "available" ? "download update" : "download latest"}</Button>
            </div>
          ))}
          {report.errors.map((error) => <p key={error.app_id} className="text-[13px] text-[var(--bad)] mt-2">{error.app_id}: {error.message}</p>)}
          {!report.updates.length ? <p className="text-[var(--muted)] mt-2">{report.errors.length || report.unsupported_count ? "No confirmed updates. Some apps could not be checked." : "No newer releases found for checked apps."}</p> : null}
        </>
      )}
    </Card>
  );
}
