import { useState } from "react";
import {
  arrangeLayout, createGroup, deleteGroup, moveApp, readLayout, renameGroup, shiftApp, shiftGroup,
  toggleGroup, uniqueGroupName, writeLayout, type AppLayout,
} from "./app-layout";

/** Keeps the user's arrangement in sync with the installed apps and saves every change. */
export function useAppLayout(installedIds: string[]) {
  const [saved, setSaved] = useState(readLayout);
  const layout = arrangeLayout(saved.layout, installedIds);

  const update = (change: (current: AppLayout) => AppLayout) => {
    const next = change(layout);
    setSaved({ layout: next, error: writeLayout(next) });
  };

  const addGroup = (appIds: string[] = []) => {
    const id = crypto.randomUUID();
    update((current) => createGroup(current, id, uniqueGroupName(current), appIds));
    return id;
  };

  return {
    layout,
    error: saved.error,
    actions: {
      addGroup,
      moveApp: (appId: string, sectionId: string, index: number) => update((current) => moveApp(current, appId, sectionId, index)),
      shiftApp: (appId: string, offset: number) => update((current) => shiftApp(current, appId, offset)),
      renameGroup: (groupId: string, name: string) => update((current) => renameGroup(current, groupId, name)),
      deleteGroup: (groupId: string) => update((current) => deleteGroup(current, groupId)),
      toggleGroup: (groupId: string) => update((current) => toggleGroup(current, groupId)),
      shiftGroup: (groupId: string, offset: number) => update((current) => shiftGroup(current, groupId, offset)),
    },
  };
}

export type LayoutActions = ReturnType<typeof useAppLayout>["actions"];
