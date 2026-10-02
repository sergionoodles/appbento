import { useRef } from "react";
import { ArrowDown, ArrowUp, ChevronRight, MoreHorizontal, Pencil, Trash2 } from "lucide-react";
import type { AppGroup } from "@/lib/app-layout";
import { cn } from "@/lib/utils";
import { Menu, MenuItem, MenuSeparator } from "./ui/menu";

export interface GroupActions {
  onToggle: (groupId: string) => void;
  onRename: (groupId: string, name: string) => void;
  onShift: (groupId: string, offset: number) => void;
  onDelete: (groupId: string) => void;
  onEdit: (groupId: string | null) => void;
}

function GroupNameInput({ group, onDone }: { group: AppGroup; onDone: (name: string | null) => void }) {
  const isDone = useRef(false);
  const finish = (name: string | null) => {
    if (isDone.current) return;
    isDone.current = true;
    onDone(name);
  };

  return (
    <input aria-label="Group name" defaultValue={group.name} autoFocus onFocus={(event) => event.currentTarget.select()}
      className="h-8 min-w-0 w-64 rounded-md border border-[var(--accent)] bg-[var(--panel2)] px-2.5 text-[15px] font-semibold outline-none"
      onBlur={(event) => finish(event.currentTarget.value)}
      onKeyDown={(event) => {
        if (event.key === "Enter") finish(event.currentTarget.value);
        if (event.key !== "Escape") return;
        event.stopPropagation();
        finish(null);
      }} />
  );
}

export function SectionTitle({ title, count, level = 2 }: { title: string; count: number; level?: 1 | 2 }) {
  const Heading = level === 1 ? "h1" : "h2";
  return (
    <>
      <Heading className={cn("font-semibold truncate", level === 1 ? "text-[18px]" : "text-[15px]")}>{title}</Heading>
      <span className="text-[12px] text-[var(--muted)] tabular-nums">{count}</span>
    </>
  );
}

export function GroupHeader({ group, count, position, isEditing, isPage, actions, disabled }: {
  group: AppGroup;
  count: number;
  position: { isFirst: boolean; isLast: boolean };
  isEditing: boolean;
  /** The group fills the page, so its name is the page heading and it cannot collapse. */
  isPage: boolean;
  actions: GroupActions;
  disabled: boolean;
}) {
  const finishEditing = (name: string | null) => {
    if (name !== null) actions.onRename(group.id, name);
    actions.onEdit(null);
  };

  return (
    <div data-drop-section={group.id} className="group/header flex items-center gap-2 h-9 mb-2.5">
      {isPage ? null : (
        <button type="button" aria-expanded={!group.isCollapsed} aria-label={`${group.isCollapsed ? "Expand" : "Collapse"} ${group.name}`}
          onClick={() => actions.onToggle(group.id)}
          className="grid place-items-center size-6 -ml-1 rounded text-[var(--muted)] hover:text-[var(--fg)] hover:bg-[var(--panel2)]">
          <ChevronRight size={15} className={cn("transition-transform", !group.isCollapsed && "rotate-90")} />
        </button>
      )}
      {isEditing
        ? <GroupNameInput group={group} onDone={finishEditing} />
        : <div className="flex items-baseline gap-2 min-w-0" onDoubleClick={() => actions.onEdit(group.id)}><SectionTitle title={group.name} count={count} level={isPage ? 1 : 2} /></div>}
      <div className="flex-1 h-px bg-[var(--hairline)] ml-2" />
      <Menu label={`Options for ${group.name} group`} icon={<MoreHorizontal size={16} />} disabled={disabled}>
        <MenuItem icon={<Pencil size={15} />} onSelect={() => actions.onEdit(group.id)}>Rename</MenuItem>
        <MenuItem icon={<ArrowUp size={15} />} disabled={position.isFirst} onSelect={() => actions.onShift(group.id, -1)}>Move group up</MenuItem>
        <MenuItem icon={<ArrowDown size={15} />} disabled={position.isLast} onSelect={() => actions.onShift(group.id, 1)}>Move group down</MenuItem>
        <MenuSeparator />
        <MenuItem icon={<Trash2 size={15} />} isDanger onSelect={() => actions.onDelete(group.id)} label={`Delete ${group.name} group`}>Delete group</MenuItem>
      </Menu>
    </div>
  );
}
