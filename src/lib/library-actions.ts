import type { AppApi } from "./api.ts";
import type { LibraryState } from "./library-controller.ts";
import type { Settings } from "./types";

type RunOperation = (operation: string, action: () => Promise<void>, changesLibrary?: boolean) => Promise<boolean>;

export function createLibraryActions(
  backend: AppApi,
  getState: () => LibraryState,
  publishState: (patch: Partial<LibraryState>) => void,
  runOperation: RunOperation,
) {
  const scan = async () => {
    const report = await backend.libraryScan();
    publishState({ apps: await backend.libraryList(), updateReport: null, scanWarnings: report.warnings });
  };

  return {
    scan: () => runOperation("Scan folders", scan, true),
    launch: (id: string) => runOperation("Launch app", async () => {
      const launched = await backend.libraryLaunch(id);
      publishState({ apps: getState().apps.map((app) => app.id === id ? launched : app) });
    }),
    integrate: () => runOperation("Add AppImage", async () => {
      const selected = await backend.pickAppImage();
      if (!selected) return;

      await backend.libraryIntegrate(selected);
      publishState({ apps: await backend.libraryList(), updateReport: null });
    }, true),
    saveFolders: (settings: Settings) => runOperation("Save folders", async () => {
      const saved = await backend.settingsSet(settings);
      publishState({ settings: saved });
      await scan();
    }, true),
    remove: (id: string, deleteFile: boolean) => runOperation("Remove app", async () => {
      await backend.libraryRemove(id, deleteFile);
      publishState({ apps: getState().apps.filter((app) => app.id !== id), updateReport: null });
    }, true),
    async openExternal(url: string) {
      try {
        await backend.openExternal(url);
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        publishState({ error: `Open link: ${message}` });
      }
    },
  };
}
