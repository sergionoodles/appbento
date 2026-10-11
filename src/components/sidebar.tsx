import type { ReactNode } from "react";
import { Folder, LayoutGrid, Plus, Sparkles } from "lucide-react";
import type { AppGroup } from "@/lib/app-layout";
import { Wordmark } from "./logo";

export type SidebarPage = { name: "all" } | { name: "group"; id: string } | { name: "updates" } | { name: "other" };

function NavItem({ icon, label, ariaLabel, count, isCurrent, isDropActive, dropSection, onClick }: {
  icon: ReactNode;
  label: string;
  ariaLabel?: string;
  count?: ReactNode;
  isCurrent: boolean;
  isDropActive?: boolean;
  dropSection?: string;
  onClick: () => void;
}) {
  return (
    <button type="button" className="nav-item w-full" aria-label={ariaLabel} aria-current={isCurrent ? "page" : undefined}
      data-drop-section={dropSection} data-drop-active={isDropActive || undefined} onClick={onClick}>
      <span className="shrink-0 opacity-80">{icon}</span>
      <span className="flex-1 truncate">{label}</span>
      {count}
    </button>
  );
}

function SidebarHeading({ children }: { children: ReactNode }) {
  return <h2 className="px-2.5 pt-1 pb-1.5 text-[11px] font-semibold uppercase tracking-[0.06em] text-[var(--muted)]">{children}</h2>;
}

function Count({ value, isHighlighted = false }: { value: number; isHighlighted?: boolean }) {
  if (isHighlighted) return <span className="grid place-items-center min-w-5 h-5 px-1 rounded-md bg-[var(--accent)] text-[var(--on-accent)] text-[11px] font-bold">{value}</span>;
  return <span className="text-[12px] tabular-nums text-[var(--muted)]">{value}</span>;
}

export function Sidebar({ page, appCount, groups, updateCount, dropSectionId, onHome, onGroup, onNewGroup, onUpdates }: {
  page: SidebarPage;
  appCount: number;
  groups: AppGroup[];
  updateCount: number;
  /** The group a dragged tile would land in, highlighted when the pointer is over its entry. */
  dropSectionId: string | null;
  onHome: () => void;
  onGroup: (id: string) => void;
  onNewGroup: () => void;
  onUpdates: () => void;
}) {
  return (
    <aside className="w-[220px] shrink-0 flex flex-col bg-[var(--chrome)] border-r border-[var(--hairline)]">
      <div className="h-[52px] shrink-0 flex items-center px-4"><Wordmark onHome={onHome} /></div>
      <nav aria-label="Library" className="flex-1 min-h-0 overflow-y-auto px-2.5 pb-3 flex flex-col gap-5">
        <div className="flex flex-col gap-0.5">
          <NavItem icon={<LayoutGrid size={16} />} label="All apps" count={<Count value={appCount} />} isCurrent={page.name === "all"} onClick={onHome} />
        </div>
        <div className="flex flex-col gap-0.5">
          <SidebarHeading>Groups</SidebarHeading>
          {groups.map((group) => (
            <NavItem key={group.id} icon={<Folder size={16} />} label={group.name} count={<Count value={group.appIds.length} />}
              isCurrent={page.name === "group" && page.id === group.id} dropSection={group.id} isDropActive={dropSectionId === group.id} onClick={() => onGroup(group.id)} />
          ))}
          <NavItem icon={<Plus size={16} />} label="New group" isCurrent={false} onClick={onNewGroup} />
        </div>
        <div className="flex flex-col gap-0.5">
          <SidebarHeading>Maintenance</SidebarHeading>
          <NavItem icon={<Sparkles size={16} />} label="Updates" ariaLabel="Updates" isCurrent={page.name === "updates"}
            count={updateCount > 0 ? <Count value={updateCount} isHighlighted /> : null} onClick={onUpdates} />
        </div>
      </nav>
    </aside>
  );
}
