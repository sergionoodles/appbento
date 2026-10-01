import type { AppApi } from "./api.ts";
import type { LibraryState } from "./library-controller.ts";
import type { InstalledApp, LaunchConfig, Settings } from "./types";

type RunOperation = (operation: string, action: () => Promise<void>, changesLibrary?: boolean) => Promise<boolean>;

export function createLibraryActions(
  backend: AppApi,
  getState: () => LibraryState,
  publishState: (patch: Partial<LibraryState>) => void,
  runOperation: RunOperation,
) {
  const publishApps = (apps: InstalledApp[]) => {
    publishState({ apps, updateReport: null });
  };
  const launchAndRefresh = async (id: string) => {
    try {
      const launched = await backend.libraryLaunch(id);
      publishState({ apps: getState().apps.map((app) => app.id === id ? launched : app) });
    } catch (error) {
      try {
        const apps = await backend.libraryList();
        publishApps(apps);
        if (apps.some((app) => app.id === id && !app.launch_config)) {
          publishState({ reviewIds: [id, ...getState().reviewIds.filter((reviewId) => reviewId !== id)], launchAfterReview: true });
        }
      } catch (refreshError) {
        const launchMessage = error instanceof Error ? error.message : String(error);
        const refreshMessage = refreshError instanceof Error ? refreshError.message : String(refreshError);
        throw Object.assign(
          new AggregateError([error, refreshError], `${launchMessage}; Library refresh failed: ${refreshMessage}`),
          { cause: error },
        );
      }
      throw error;
    }
  };

  const scan = async () => {
    const report = await backend.libraryScan();
    publishApps(await backend.libraryList());
    publishState({ scanWarnings: report.warnings });
  };

  return {
    setIcon: (id: string, source: string) => runOperation("Save icon", async () => {
      const updated = await backend.librarySetIcon(id, source);
      publishState({ apps: getState().apps.map((app) => app.id === id ? updated : app) });
    }),
    scan: () => runOperation("Scan folders", scan, true),
    requestReview: (id: string) => publishState({ reviewIds: [id, ...getState().reviewIds.filter((reviewId) => reviewId !== id)], launchAfterReview: false, error: null }),
    dismissReview: () => publishState({ reviewIds: getState().reviewIds.slice(1), launchAfterReview: false, error: null }),
    configure: (id: string, config: LaunchConfig, launch: boolean) => runOperation("Save permissions", async () => {
      const configured = await backend.libraryConfigure(id, config);
      publishState({ apps: getState().apps.map((app) => app.id === id ? configured : app) });
      if (launch) await launchAndRefresh(id);
      publishState({ reviewIds: getState().reviewIds.filter((reviewId) => reviewId !== id), launchAfterReview: false });
    }),
    launch: (id: string) => {
      if (!getState().apps.find((app) => app.id === id)?.launch_config) {
        publishState({ reviewIds: [id, ...getState().reviewIds.filter((reviewId) => reviewId !== id)], launchAfterReview: true, error: null });
        return Promise.resolve(false);
      }
      return runOperation("Launch app", async () => {
        await launchAndRefresh(id);
      });
    },
    integrate: () => runOperation("Add AppImage", async () => {
      const selected = await backend.pickAppImage();
      if (!selected) return;

      await backend.libraryIntegrate(selected);
      publishApps(await backend.libraryList());
    }, true),
    saveFolders: (settings: Settings) => runOperation("Save folders", async () => {
      const saved = await backend.settingsSet(settings);
      publishState({ settings: saved });
      await scan();
    }, true),
    remove: (id: string, deleteFile: boolean) => runOperation("Remove app", async () => {
      await backend.libraryRemove(id, deleteFile);
      publishState({ apps: getState().apps.filter((app) => app.id !== id), reviewIds: getState().reviewIds.filter((reviewId) => reviewId !== id), updateReport: null });
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
