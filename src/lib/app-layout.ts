/** User-defined arrangement of the library: an ordered ungrouped list plus ordered, named groups. */
export const UNGROUPED_ID = "ungrouped";
export const NEW_GROUP_NAME = "New group";

const LAYOUT_KEY = "appbento-layout";
const STORAGE_UNAVAILABLE = "The app arrangement cannot be saved in this session.";

export interface AppGroup {
  id: string;
  name: string;
  appIds: string[];
  isCollapsed: boolean;
}

export interface AppLayout {
  ungrouped: string[];
  groups: AppGroup[];
}

export const EMPTY_LAYOUT: AppLayout = { ungrouped: [], groups: [] };

export function sectionAppIds(layout: AppLayout, sectionId: string): string[] {
  if (sectionId === UNGROUPED_ID) return layout.ungrouped;
  return layout.groups.find((group) => group.id === sectionId)?.appIds ?? [];
}

export function sectionOf(layout: AppLayout, appId: string): string | null {
  if (layout.ungrouped.includes(appId)) return UNGROUPED_ID;
  return layout.groups.find((group) => group.appIds.includes(appId))?.id ?? null;
}

function withSection(layout: AppLayout, sectionId: string, appIds: string[]): AppLayout {
  if (sectionId === UNGROUPED_ID) return { ...layout, ungrouped: appIds };
  return { ...layout, groups: layout.groups.map((group) => group.id === sectionId ? { ...group, appIds } : group) };
}

function withoutApp(layout: AppLayout, appId: string): AppLayout {
  return {
    ungrouped: layout.ungrouped.filter((id) => id !== appId),
    groups: layout.groups.map((group) => ({ ...group, appIds: group.appIds.filter((id) => id !== appId) })),
  };
}

/**
 * Drops ids of apps that are no longer installed and appends newly found apps to the ungrouped list,
 * so every installed app appears exactly once.
 */
export function arrangeLayout(layout: AppLayout, installedIds: string[]): AppLayout {
  const installed = new Set(installedIds);
  const placed = new Set<string>();
  const keep = (ids: string[]) => ids.filter((id) => {
    if (!installed.has(id) || placed.has(id)) return false;
    placed.add(id);
    return true;
  });
  const groups = layout.groups.map((group) => ({ ...group, appIds: keep(group.appIds) }));
  const ungrouped = keep(layout.ungrouped);
  const unplaced = installedIds.filter((id) => !placed.has(id));
  return { ungrouped: [...ungrouped, ...unplaced], groups };
}

/** Moves an app into a section at `index`, counted in that section without the moved app. */
export function moveApp(layout: AppLayout, appId: string, sectionId: string, index: number): AppLayout {
  const isKnownSection = sectionId === UNGROUPED_ID || layout.groups.some((group) => group.id === sectionId);
  if (!isKnownSection) return layout;
  const remaining = withoutApp(layout, appId);
  const target = [...sectionAppIds(remaining, sectionId)];
  const clampedIndex = Math.max(0, Math.min(index, target.length));
  target.splice(clampedIndex, 0, appId);
  return withSection(remaining, sectionId, target);
}

/**
 * Insertion index for a drag that hovers `overId`: before it, or after it when `isAfter`.
 * Counted without the dragged app, matching `moveApp`.
 */
export function dropIndex(layout: AppLayout, draggedId: string, sectionId: string, overId: string, isAfter: boolean): number {
  const others = sectionAppIds(layout, sectionId).filter((id) => id !== draggedId);
  const overIndex = others.indexOf(overId);
  if (overIndex === -1) return others.length;
  return isAfter ? overIndex + 1 : overIndex;
}

export function uniqueGroupName(layout: AppLayout, baseName = NEW_GROUP_NAME): string {
  const names = new Set(layout.groups.map((group) => group.name.toLowerCase()));
  if (!names.has(baseName.toLowerCase())) return baseName;
  let suffix = 2;
  while (names.has(`${baseName} ${suffix}`.toLowerCase())) suffix += 1;
  return `${baseName} ${suffix}`;
}

export function createGroup(layout: AppLayout, id: string, name: string, appIds: string[] = []): AppLayout {
  const cleaned = appIds.reduce(withoutApp, layout);
  return { ...cleaned, groups: [...cleaned.groups, { id, name, appIds, isCollapsed: false }] };
}

export function renameGroup(layout: AppLayout, groupId: string, name: string): AppLayout {
  const trimmed = name.trim();
  if (!trimmed) return layout;
  return { ...layout, groups: layout.groups.map((group) => group.id === groupId ? { ...group, name: trimmed } : group) };
}

/** Deleting a group keeps its apps: they return to the end of the ungrouped list in their group order. */
export function deleteGroup(layout: AppLayout, groupId: string): AppLayout {
  const group = layout.groups.find((entry) => entry.id === groupId);
  if (!group) return layout;
  return {
    ungrouped: [...layout.ungrouped, ...group.appIds],
    groups: layout.groups.filter((entry) => entry.id !== groupId),
  };
}

export function toggleGroup(layout: AppLayout, groupId: string): AppLayout {
  return { ...layout, groups: layout.groups.map((group) => group.id === groupId ? { ...group, isCollapsed: !group.isCollapsed } : group) };
}

export function shiftGroup(layout: AppLayout, groupId: string, offset: number): AppLayout {
  const index = layout.groups.findIndex((group) => group.id === groupId);
  const target = index + offset;
  if (index === -1 || target < 0 || target >= layout.groups.length) return layout;
  const groups = [...layout.groups];
  const [group] = groups.splice(index, 1);
  groups.splice(target, 0, group);
  return { ...layout, groups };
}

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === "string");
}

function parseGroup(value: unknown): AppGroup | null {
  if (typeof value !== "object" || value === null) return null;
  const { id, name, appIds, isCollapsed } = value as Record<string, unknown>;
  if (typeof id !== "string" || typeof name !== "string" || !isStringArray(appIds)) return null;
  return { id, name, appIds, isCollapsed: isCollapsed === true };
}

/** Accepts only well-formed saved layouts; anything else is treated as no arrangement. */
export function parseLayout(raw: string | null): AppLayout {
  if (!raw) return EMPTY_LAYOUT;
  const value: unknown = JSON.parse(raw);
  if (typeof value !== "object" || value === null) return EMPTY_LAYOUT;
  const { ungrouped, groups } = value as Record<string, unknown>;
  return {
    ungrouped: isStringArray(ungrouped) ? ungrouped : [],
    groups: Array.isArray(groups) ? groups.map(parseGroup).filter((group) => group !== null) : [],
  };
}

export function readLayout(getStorage = () => window.localStorage): { layout: AppLayout; error: string | null } {
  try {
    return { layout: parseLayout(getStorage().getItem(LAYOUT_KEY)), error: null };
  } catch (error) {
    const reason = error instanceof SyntaxError ? "The saved app arrangement was unreadable and has been reset." : STORAGE_UNAVAILABLE;
    return { layout: EMPTY_LAYOUT, error: reason };
  }
}

export function writeLayout(layout: AppLayout, getStorage = () => window.localStorage): string | null {
  try {
    getStorage().setItem(LAYOUT_KEY, JSON.stringify(layout));
    return null;
  } catch {
    return STORAGE_UNAVAILABLE;
  }
}
