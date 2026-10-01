import { ExternalLink, Github, MoreHorizontal, Check } from "lucide-react";
import type { CatalogEntry, InstalledApp } from "@/lib/types";
import { AppIcon, accentOf } from "./app-icon";
import { StageHeader } from "./stage";
import { Menu, MenuItem } from "./ui/menu";

export function CatalogView({ catalog, apps, isPreview, onBack, onOpen }: {
  catalog: CatalogEntry[];
  apps: InstalledApp[];
  isPreview: boolean;
  onBack: () => void;
  onOpen: (url: string) => void;
}) {
  return (
    <div>
      <StageHeader title="Discover" onBack={onBack} />
      <div className="bento intro" style={{ gridAutoRows: "200px" }}>
        {catalog.map((entry, index) => {
          const isInstalled = apps.some((app) => app.catalog_id === entry.app.id);
          return (
            <article key={entry.app.id} className="tile" style={{ "--tile-accent": accentOf(entry), "--i": index } as React.CSSProperties}>
              <div className="h-full p-5 flex flex-col justify-between">
                <div className="flex items-start justify-between">
                  <AppIcon name={entry.app.name} entry={entry} className="size-12" />
                  <Menu label={`More options for ${entry.app.name}`} icon={<MoreHorizontal size={17} />}>
                    <MenuItem icon={<ExternalLink size={15} />} onSelect={() => onOpen(entry.app.website)} label={`Open ${entry.app.name} website`}>Website</MenuItem>
                    {entry.app.repo ? <MenuItem icon={<Github size={15} />} onSelect={() => onOpen(entry.app.repo ?? entry.app.website)} label={`Open ${entry.app.name} repository`}>Repository</MenuItem> : null}
                  </Menu>
                </div>
                <div>
                  <h3 className="text-[20px] font-bold tracking-[-0.03em] flex items-center gap-2">
                    {entry.app.name}
                    {isInstalled ? <span className="grid place-items-center size-5 rounded-md bg-[var(--tile-accent)] text-[var(--on-accent)]" role="img" aria-label="Installed"><Check size={12} strokeWidth={3} /></span> : null}
                  </h3>
                  <p className="text-[13px] text-[var(--muted)] line-clamp-2 mt-1">{entry.app.summary}</p>
                </div>
              </div>
            </article>
          );
        })}
      </div>
      {!catalog.length ? <p className="text-[var(--muted)]">{isPreview ? "The sample preview has no catalog. Open the desktop app to browse bundled entries." : "No catalog entries match your search, or the catalog could not be loaded."}</p> : null}
      <p className="mt-6 text-[13px] text-[var(--muted)]">Entries link to each app's own site. Download the AppImage there, then add it from the + menu.</p>
    </div>
  );
}
