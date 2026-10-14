import type { AppApi } from "./api.ts";
import { createLibraryActions } from "./library-actions.ts";
import type { InstalledApp, ScanWarning, Settings, UpdateReport } from "./types";

export interface LibraryState {
  reviewIds: string[];
  launchAfterReview: boolean;
  apps: InstalledApp[];
  settings: Settings | null;
  updateReport: UpdateReport | null;
  scanWarnings: ScanWarning[];
  isLoading: boolean;
  isCheckingUpdates: boolean;
  operation: string | null;
  updatingAppIds: string[];
  error: string | null;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function createLibraryController(backend: AppApi) {
  let state: LibraryState = {
    reviewIds: [], launchAfterReview: false,
    apps: [], settings: null, updateReport: null, scanWarnings: [],
    isLoading: true, isCheckingUpdates: false, operation: null, updatingAppIds: [], error: null,
  };
  let hasInitialized = false;
  let revision = 0;
  let needsUpdateCheck = false;
  const subscribers = new Set<() => void>();
  const publish = (patch: Partial<LibraryState>) => {
    state = { ...state, ...patch };
    subscribers.forEach((subscriber) => subscriber());
  };

  const loadLocal = async () => {
    const results = await Promise.allSettled([backend.libraryList(), backend.settingsGet()]);
    const [apps, settings] = results;
    const errors = results.filter((result) => result.status === "rejected").map((result) => errorMessage(result.reason));
    publish({
      apps: apps.status === "fulfilled" ? apps.value : state.apps,
      settings: settings.status === "fulfilled" ? settings.value : state.settings,
      isLoading: false,
      ...(errors.length ? { error: errors.join("; ") } : {}),
    });
  };

  const checkUpdates = async () => {
    if (state.isCheckingUpdates || state.operation || state.updatingAppIds.length) return;
    needsUpdateCheck = false;
    const requestedRevision = revision;
    publish({ isCheckingUpdates: true });
    try {
      const updateReport = await backend.updatesCheckAll();
      if (revision === requestedRevision) publish({ updateReport });
      else needsUpdateCheck = true;
    } catch (error) {
      if (revision === requestedRevision) {
        publish({ updateReport: null, error: `Update check failed: ${errorMessage(error)}` });
      }
    } finally {
      publish({ isCheckingUpdates: false });
      if (needsUpdateCheck) void checkUpdates();
    }
  };

  const runOperation = async (operation: string, action: () => Promise<void>, changesLibrary = false): Promise<boolean> => {
    if (state.operation || state.isLoading) return false;
    if (changesLibrary) {
      revision += 1;
      needsUpdateCheck = true;
    }
    publish({ operation, error: null, ...(changesLibrary ? { updateReport: null } : {}) });
    try {
      await action();
      return true;
    } catch (error) {
      publish({ error: `${operation}: ${errorMessage(error)}` });
      return false;
    } finally {
      publish({ operation: null });
      if (needsUpdateCheck && !state.isCheckingUpdates) void checkUpdates();
    }
  };

  const installUpdate = async (id: string): Promise<boolean> => {
    if (state.isLoading || state.operation || state.updatingAppIds.includes(id)) return false;
    revision += 1;
    needsUpdateCheck = true;
    publish({ updatingAppIds: [...state.updatingAppIds, id], error: null });
    try {
      const updated = await backend.updatesInstall(id);
      const report = state.updateReport;
      revision += 1;
      publish({
        apps: state.apps.map((app) => app.id === id ? updated : app),
        updateReport: report ? { ...report,
          updates: report.updates.filter((update) => update.app_id !== id),
          errors: report.errors.filter((error) => error.app_id !== id),
          unknown_versions: report.unknown_versions.filter((appId) => appId !== id),
        } : null,
      });
      return true;
    } catch (error) {
      publish({ error: `Install update: ${errorMessage(error)}` });
      return false;
    } finally {
      publish({ updatingAppIds: state.updatingAppIds.filter((appId) => appId !== id) });
      if (needsUpdateCheck) void checkUpdates();
    }
  };

  return {
    getState: () => state,
    subscribe(listener: () => void) {
      subscribers.add(listener);
      return () => { subscribers.delete(listener); };
    },
    async initialize() {
      if (hasInitialized) return;
      hasInitialized = true;
      void checkUpdates();
      await loadLocal();
    },
    reload: () => runOperation("Reload library", loadLocal),
    checkUpdates,
    dismissError: () => publish({ error: null }),
    dismissScanWarnings: () => publish({ scanWarnings: [] }),
    ...createLibraryActions(backend, () => state, publish, runOperation),
    installUpdate,
  };
}

export type LibraryController = ReturnType<typeof createLibraryController>;
