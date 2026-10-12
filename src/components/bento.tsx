import { useEffect } from "react";
import { PackageOpen } from "lucide-react";
import { UNGROUPED_ID, moveApp, type AppGroup, type AppLayout } from "@/lib/app-layout";
import type { TileDrag } from "@/lib/use-tile-drag";
import type { InstalledApp, UpdateInfo } from "@/lib/types";
import { cn } from "@/lib/utils";
import { AppTile, TileGhost, type TileActions } from "./app-tile";
import { GroupHeader, SectionTitle, type GroupActions } from "./group-header";


const UNGROUPED_NAME = "Ungrouped";
const ENTRANCE_DURATION_MS = 900;

interface Section {
  id: string;
  name: string;
  group: AppGroup | null;
  apps: InstalledApp[];
}

export function BentoSkeleton() {
  return (
    <div className="bento" role="status" aria-label="Loading local library">
      {Array.from({ length: 6 }, (_, index) => (
        <div key={index} className="rounded-[10px] bg-[var(--panel)] skeleton" style={{ animationDelay: `${index * 120}ms` }} />
      ))}
    </div>
  );
}

function buildSections(layout: AppLayout, apps: InstalledApp[]): Section[] {
  const byId = new Map(apps.map((app) => [app.id, app]));
  const resolve = (ids: string[]) => ids.flatMap((id) => byId.get(id) ?? []);
  return [
    { id: UNGROUPED_ID, name: UNGROUPED_NAME, group: null, apps: resolve(layout.ungrouped) },
    ...layout.groups.map((group) => ({ id: group.id, name: group.name, group, apps: resolve(group.appIds) })),
  ];
}

function EmptyGroup({ sectionId }: { sectionId: string }) {
  return (
    <div data-drop-section={sectionId} className="section-drop h-[132px] grid place-items-center rounded-[10px] border border-dashed border-[var(--border)] text-[13px] text-[var(--muted)]">
      Drag apps here.
    </div>
  );
}

export function Bento({ layout, apps, updates, activeGroupId, isFiltering, drag, editingGroupId, isAnimated, onEntered, tileActions, groupActions, disabled }: {
  layout: AppLayout;
  /** Apps to show, already filtered by the search query. */
  apps: InstalledApp[];
  updates: UpdateInfo[];
  activeGroupId: string | null;
  isFiltering: boolean;
  drag: TileDrag | null;
  editingGroupId: string | null;
  isAnimated: boolean;
  onEntered: () => void;
  tileActions: TileActions;
  groupActions: GroupActions;
  disabled: boolean;
}) {
  useEffect(() => {
    if (!isAnimated) return;
    const timer = window.setTimeout(onEntered, ENTRANCE_DURATION_MS);
    return () => window.clearTimeout(timer);
  }, [isAnimated, onEntered]);

  const shownLayout = drag ? moveApp(layout, drag.appId, drag.target.sectionId, drag.target.index) : layout;
  const sections = buildSections(shownLayout, apps);
  const hasGroups = layout.groups.length > 0;
  const visibleSections = sections.filter((section) => {
    if (activeGroupId) return section.id === activeGroupId;
    if (!section.group) return section.apps.length > 0 || (hasGroups && drag !== null);
    return !isFiltering || section.apps.length > 0;
  });
  const updatesById = new Map(updates.map((update) => [update.app_id, update]));
  const draggedApp = drag ? apps.find((app) => app.id === drag.appId) : undefined;

  return (
    <div className="flex flex-col gap-7">
      {activeGroupId ? null : <div className="flex items-baseline gap-2 -mb-3"><SectionTitle title="All apps" count={apps.length} level={1} /></div>}
      {visibleSections.map((section) => {
        const groupIndex = layout.groups.findIndex((group) => group.id === section.id);
        const isCollapsed = section.group?.isCollapsed && !activeGroupId;
        return (
          <section key={section.id} data-section-id={section.id} aria-label={section.name}>
            {section.group
              ? <GroupHeader group={section.group} count={section.apps.length} position={{ isFirst: groupIndex === 0, isLast: groupIndex === layout.groups.length - 1 }}
                  isEditing={editingGroupId === section.id} isPage={activeGroupId !== null} actions={groupActions} disabled={disabled} />
              : hasGroups ? <div data-drop-section={UNGROUPED_ID} className="flex items-baseline gap-2 h-9 mb-2.5"><SectionTitle title={UNGROUPED_NAME} count={section.apps.length} /></div> : null}
            {isCollapsed ? null : section.apps.length ? (
              <div className={cn("bento", isAnimated && "intro")}>
                {section.apps.map((app, index) => (
                  <AppTile key={app.id} app={app} index={index} update={updatesById.get(app.id)}
                    actions={tileActions} isPlaceholder={drag?.appId === app.id} disabled={disabled} />
                ))}
              </div>
            ) : <EmptyGroup sectionId={section.id} />}
          </section>
        );
      })}
      {!apps.length && (isFiltering || !hasGroups) ? (
        <p className="flex items-center gap-2 text-[var(--muted)]"><PackageOpen size={16} />{isFiltering ? "No apps match your search." : "No apps yet. Add an AppImage or scan your folders from the + menu."}</p>
      ) : null}
      {drag && draggedApp ? (
        <TileGhost app={draggedApp} update={updatesById.get(draggedApp.id)}
          isShrunk={drag.isOverZone} style={{ left: drag.x - drag.offsetX, top: drag.y - drag.offsetY, width: drag.width, height: drag.height, transformOrigin: `${drag.offsetX}px ${drag.offsetY}px` }} />
      ) : null}
    </div>
  );
}
