import type { AppApi } from "./api.ts";
import type { LibraryState } from "./library-controller.ts";
import type { InstalledApp, LaunchConfig, Settings, UpdateConfig } from "./types";

type RunOperation = (operation: string, action: () => Promise<void>, changesLibrary?: boolean) => Promise<boolean>;

export function createLibraryActions(
  backend: AppApi,
  getState: () => LibraryState,
  publishState: (patch: Partial<LibraryState>) => void,
  runOperation: RunOperation,
) {
  const runAppOperation = (id: string, operation: string, action: () => Promise<void>, changesLibrary = false) => {
    if (getState().updatingAppIds.includes(id)) return Promise.resolve(false);
    return runOperation(operation, action, changesLibrary);
  };

  const publishApps = (apps: InstalledApp[]) => {
    publishState({ apps, updateReport: null });
  };

  const offerMove = async (apps: InstalledApp[]) => {
    const settings = getState().settings ?? await backend.settingsGet();
    const managedDirectory = settings.managed_dir.replace(/\/+$/, "");
    const externalIds = apps.filter((app) => {
      const directory = app.file_path.slice(0, app.file_path.lastIndexOf("/"));
      return directory !== managedDirectory;
    }).map((app) => app.id);
    publishState({ settings, importPromptIds: [...new Set([...getState().importPromptIds, ...externalIds])] });
  };
  const launchAndRefresh = async (id: string, makeExecutable = false) => {
    try {
      const prepared = await backend.libraryPrepareLaunch(id, makeExecutable);
      publishState({ apps: getState().apps.map((app) => app.id === id ? prepared : app) });
      if (!prepared.executable) {
        publishState({ executablePromptId: id, error: null });
        return;
      }
      const launched = await backend.libraryLaunch(id);
      publishState({ apps: getState().apps.map((app) => app.id === id ? launched : app), executablePromptId: null });
    } catch (error) {
      try {
        const apps = await backend.libraryList();
        publishApps(apps);
        if (apps.some((app) => app.id === id && (!app.launch_config || app.reviewed_digest === null))) {
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

  const importPath = async (path: string) => {
    const imported = await backend.libraryImport(path, false);
    publishApps(await backend.libraryList());
    await offerMove([imported]);
  };

  const scan = async () => {
    const previousIds = new Set(getState().apps.map((app) => app.id));
    const report = await backend.libraryScan();
    publishApps(await backend.libraryList());
    publishState({ scanWarnings: report.warnings });
    await offerMove(report.apps.filter((app) => !previousIds.has(app.id)));
  };

  return {
    keepImportLocation: () => publishState({ importPromptIds: getState().importPromptIds.slice(1), error: null }),
    moveImport: (id: string) => runAppOperation(id, "Move AppImage", async () => {
      const app = getState().apps.find((entry) => entry.id === id);
      if (!app) throw new Error("App not found");
      await backend.libraryImport(app.file_path, true);
      publishApps(await backend.libraryList());
      publishState({ importPromptIds: getState().importPromptIds.filter((appId) => appId !== id) });
    }, true),
    dismissExecutablePrompt: () => publishState({ executablePromptId: null, error: null }),
    confirmExecutable: () => {
      const id = getState().executablePromptId;
      if (!id) return Promise.resolve(false);
      return runAppOperation(id, "Make executable and launch", () => launchAndRefresh(id, true));
    },
    setIcon: (id: string, source: string) => runAppOperation(id, "Save icon", async () => {
      const updated = await backend.librarySetIcon(id, source);
      publishState({ apps: getState().apps.map((app) => app.id === id ? updated : app) });
    }),
    configureUpdates: (id: string, config: UpdateConfig | null) => runAppOperation(id, "Save update source", async () => {
      const configured = await backend.updatesConfigure(id, config);
      publishApps(getState().apps.map((app) => app.id === id ? configured : app));
    }, true),
    checkAppUpdates: (id: string) => {
      if (getState().isCheckingUpdates) return Promise.resolve(false);
      return runAppOperation(id, "Check app updates", async () => {
        publishState({ appUpdateChecks: { ...getState().appUpdateChecks, [id]: undefined } });
        const previousReport = getState().updateReport;
        if (previousReport) {
          publishState({ updateReport: { ...previousReport,
            updates: previousReport.updates.filter((update) => update.app_id !== id),
            unknown_versions: previousReport.unknown_versions.filter((appId) => appId !== id),
          } });
        }
        const result = await backend.updatesCheck(id);
        if (result.status === "error") throw new Error(result.error ?? "Update check failed");
        publishState({ appUpdateChecks: { ...getState().appUpdateChecks, [id]: result } });
        const report = getState().updateReport;
        publishState({ updateReport: {
          updates: [...(report?.updates ?? []).filter((update) => update.app_id !== id), ...(result.update ? [result.update] : [])],
          errors: (report?.errors ?? []).filter((error) => error.app_id !== id),
          unknown_versions: [...(report?.unknown_versions ?? []).filter((appId) => appId !== id), ...(result.status === "unknown-current" ? [id] : [])],
          checked_count: report?.checked_count ?? (result.status === "unsupported" ? 0 : 1),
          unsupported_count: report?.unsupported_count ?? (result.status === "unsupported" ? 1 : 0),
        } });
      });
    },
    scan: () => runOperation("Scan folders", scan, true),
    requestReview: (id: string) => publishState({ reviewIds: [id, ...getState().reviewIds.filter((reviewId) => reviewId !== id)], launchAfterReview: false, error: null }),
    dismissReview: () => publishState({ reviewIds: getState().reviewIds.slice(1), launchAfterReview: false, error: null }),
    configure: (id: string, config: LaunchConfig, launch: boolean) => runAppOperation(id, "Save permissions", async () => {
      const configured = await backend.libraryConfigure(id, config);
      publishState({ apps: getState().apps.map((app) => app.id === id ? configured : app) });
      if (launch) await launchAndRefresh(id);
      publishState({ reviewIds: getState().reviewIds.filter((reviewId) => reviewId !== id), launchAfterReview: false });
    }),
    launch: (id: string) => {
      if (getState().updatingAppIds.includes(id)) return Promise.resolve(false);
      const app = getState().apps.find((entry) => entry.id === id);
      if (!app?.launch_config || app.reviewed_digest === null) {
        publishState({ reviewIds: [id, ...getState().reviewIds.filter((reviewId) => reviewId !== id)], launchAfterReview: true, error: null });
        return Promise.resolve(false);
      }
      return runAppOperation(id, "Launch app", async () => {
        await launchAndRefresh(id);
      });
    },
    addUrl: (url: string) => runOperation("Download AppImage", async () => {
      await backend.libraryAddUrl(url);
      publishApps(await backend.libraryList());
    }, true),
    setMenuEntry: (id: string, enabled: boolean) => runAppOperation(id, enabled ? "Add to app menu" : "Remove from app menu", async () => {
      const updated = await backend.librarySetMenuEntry(id, enabled);
      publishState({ apps: getState().apps.map((app) => app.id === id ? updated : app) });
    }),
    importAppImage: () => runOperation("Add AppImage", async () => {
      const selected = await backend.pickAppImage();
      if (!selected) return;

      await importPath(selected);
    }, true),
    importRequestedAppImage: async () => {
      const requested = await backend.openRequestTake();
      if (!requested) return false;

      return runOperation("Add AppImage", () => importPath(requested), true);
    },
    saveFolders: (settings: Settings) => runOperation("Save folders", async () => {
      const saved = await backend.settingsSet(settings);
      publishState({ settings: saved });
      await scan();
    }, true),
    remove: (id: string, deleteFile: boolean) => runAppOperation(id, "Remove app", async () => {
      await backend.libraryRemove(id, deleteFile);
      publishState({ apps: getState().apps.filter((app) => app.id !== id), reviewIds: getState().reviewIds.filter((reviewId) => reviewId !== id), importPromptIds: getState().importPromptIds.filter((appId) => appId !== id), updateReport: null });
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
