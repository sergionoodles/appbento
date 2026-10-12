export interface SelfUpdate {
  version: string;
  notes: string | null;
  downloadUrl: string;
  installDisabledReason: string | null;
}

export interface SelfUpdateCheck {
  currentVersion: string;
  update: SelfUpdate | null;
  unavailableReason: string | null;
}

export type SelfUpdateProgress =
  | { phase: "downloading"; downloaded: number; total: number | null }
  | { phase: "installing" | "restarting" };

export interface SelfUpdateBackend {
  readonly isTauri: boolean;
  selfUpdateCheck(): Promise<SelfUpdateCheck>;
  selfUpdateInstall(onProgress: (progress: SelfUpdateProgress) => void): Promise<void>;
}

export interface SelfUpdateState {
  result: SelfUpdateCheck | null;
  isChecking: boolean;
  progress: SelfUpdateProgress | null;
  error: string | null;
}

export const SELF_UPDATE_INTERVAL = 6 * 60 * 60 * 1000;

export function createSelfUpdateController(backend: SelfUpdateBackend) {
  let state: SelfUpdateState = { result: null, isChecking: false, progress: null, error: null };
  const subscribers = new Set<() => void>();
  const publish = (patch: Partial<SelfUpdateState>) => {
    state = { ...state, ...patch };
    subscribers.forEach((subscriber) => subscriber());
  };

  return {
    getState: () => state,
    subscribe(listener: () => void) {
      subscribers.add(listener);
      return () => { subscribers.delete(listener); };
    },
    async check() {
      if (!backend.isTauri || state.isChecking || state.progress) return;
      publish({ isChecking: true, error: null });
      try {
        publish({ result: await backend.selfUpdateCheck() });
      } catch (error) {
        publish({ error: error instanceof Error ? error.message : String(error) });
      } finally {
        publish({ isChecking: false });
      }
    },
    async install(isBlocked: boolean) {
      if (isBlocked || !state.result?.update || state.result.update.installDisabledReason || state.isChecking || state.progress) return;
      publish({ progress: { phase: "downloading", downloaded: 0, total: null }, error: null });
      try {
        await backend.selfUpdateInstall((progress) => publish({ progress }));
        publish({ progress: { phase: "restarting" } });
      } catch (error) {
        publish({ progress: null, error: error instanceof Error ? error.message : String(error) });
      }
    },
  };
}
