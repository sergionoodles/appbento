import type { AppApi } from "./api.ts";
import { createLibraryActions } from "./library-actions.ts";
import type { CatalogEntry, InstalledApp, ScanWarning, Settings, UpdateReport } from "./types";

export interface LibraryState {
  apps: InstalledApp[];
  catalog: CatalogEntry[];
  settings: Settings | null;
  updateReport: UpdateReport | null;
  scanWarnings: ScanWarning[];
  isLoading: boolean;
  isCheckingUpdates: boolean;
  operation: string | null;
  error: string | null;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function recentApps(apps: InstalledApp[], limit = 6): InstalledApp[] {
  return [...apps].sort((first, second) =>
    (second.last_launched ?? "").localeCompare(first.last_launched ?? "")
  ).slice(0, limit);
}

export function createLibraryController(backend: AppApi) {
  let state: LibraryState = {
    apps: [], catalog: [], settings: null, updateReport: null, scanWarnings: [],
    isLoading: true, isCheckingUpdates: false, operation: null, error: null,
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
    const results = await Promise.allSettled([backend.libraryList(), backend.catalogList(), backend.settingsGet()]);
    const [apps, catalog, settings] = results;
    const errors = results.filter((result) => result.status === "rejected").map((result) => errorMessage(result.reason));
    publish({
      apps: apps.status === "fulfilled" ? apps.value : state.apps,
      catalog: catalog.status === "fulfilled" ? catalog.value : state.catalog,
      settings: settings.status === "fulfilled" ? settings.value : state.settings,
      isLoading: false,
      ...(errors.length ? { error: errors.join("; ") } : {}),
    });
  };

  const checkUpdates = async () => {
    if (state.isCheckingUpdates || state.operation) return;
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
      if (needsUpdateCheck && !state.operation) void checkUpdates();
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
  };
}

export type LibraryController = ReturnType<typeof createLibraryController>;
