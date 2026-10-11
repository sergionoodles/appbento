import type { InstalledApp, LaunchConfig, SandboxStatus, ScanReport, Settings, UpdateCheckResult, UpdateConfig, UpdateReport } from "./types";
import { createPreviewApi } from "./preview.ts";

export interface Commands {
  library_set_icon: { args: { id: string; source: string }; result: InstalledApp };
  library_icon: { args: { id: string }; result: string | null };
  sandbox_status: { args: undefined; result: SandboxStatus };
  library_configure: { args: { id: string; config: LaunchConfig }; result: InstalledApp };
  library_list: { args: undefined; result: InstalledApp[] };
  library_scan: { args: { extraDirs: string[] }; result: ScanReport };
  library_integrate: { args: { sourcePath: string }; result: InstalledApp };
  library_launch: { args: { id: string }; result: InstalledApp };
  library_remove: { args: { id: string; deleteFile: boolean }; result: null };
  settings_get: { args: undefined; result: Settings };
  settings_set: { args: { scanDirs: string[] | null; managedDir: string | null }; result: Settings };
  updates_configure: { args: { appId: string; config: UpdateConfig | null }; result: InstalledApp };
  updates_install: { args: { appId: string }; result: InstalledApp };
  updates_check: { args: { appId: string }; result: UpdateCheckResult };
  updates_check_all: { args: undefined; result: UpdateReport };
}

export type CommandTransport = <Command extends keyof Commands>(
  command: Command,
  args: Commands[Command]["args"],
) => Promise<Commands[Command]["result"]>;

export interface AppApi {
  readonly isTauri: boolean;
  librarySetIcon(id: string, source: string): Promise<InstalledApp>;
  libraryIcon(id: string): Promise<string | null>;
  pickIcon(): Promise<string | null>;
  sandboxStatus(): Promise<SandboxStatus>;
  libraryConfigure(id: string, config: LaunchConfig): Promise<InstalledApp>;
  libraryList(): Promise<InstalledApp[]>;
  libraryScan(): Promise<ScanReport>;
  libraryIntegrate(sourcePath: string): Promise<InstalledApp>;
  libraryLaunch(id: string): Promise<InstalledApp>;
  libraryRemove(id: string, deleteFile: boolean): Promise<void>;
  settingsGet(): Promise<Settings>;
  settingsSet(patch: Partial<Settings>): Promise<Settings>;
  updatesConfigure(appId: string, config: UpdateConfig | null): Promise<InstalledApp>;
  updatesInstall(appId: string): Promise<InstalledApp>;
  updatesCheck(appId: string): Promise<UpdateCheckResult>;
  updatesCheckAll(): Promise<UpdateReport>;
  pickAppImage(): Promise<string | null>;
  pickFolder(): Promise<string | null>;
  openExternal(url: string): Promise<void>;
}

export function validateExternalUrl(value: string): string {
  const url = new URL(value);
  if (url.protocol !== "https:" || url.username || url.password) {
    throw new Error("Only HTTPS links without credentials can be opened.");
  }
  return url.href;
}

export function createNativeApi(transport: CommandTransport): AppApi {
  return {
    isTauri: true,
    librarySetIcon: (id, source) => transport("library_set_icon", { id, source }),
    libraryIcon: (id) => transport("library_icon", { id }),
    async pickIcon() {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ multiple: false, filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "gif", "webp", "ico", "svg"] }] });
      return typeof selected === "string" ? selected : null;
    },
    sandboxStatus: () => transport("sandbox_status", undefined),
    libraryConfigure: (id, config) => transport("library_configure", { id, config }),
    libraryList: () => transport("library_list", undefined),
    libraryScan: () => transport("library_scan", { extraDirs: [] }),
    libraryIntegrate: (sourcePath) => transport("library_integrate", { sourcePath }),
    libraryLaunch: (id) => transport("library_launch", { id }),
    libraryRemove: async (id, deleteFile) => {
      await transport("library_remove", { id, deleteFile });
    },
    settingsGet: () => transport("settings_get", undefined),
    settingsSet: (patch) => transport("settings_set", {
      scanDirs: patch.scan_dirs ?? null,
      managedDir: patch.managed_dir ?? null,
    }),
    updatesConfigure: (appId, config) => transport("updates_configure", { appId, config }),
    updatesInstall: (appId) => transport("updates_install", { appId }),
    updatesCheck: (appId) => transport("updates_check", { appId }),
    updatesCheckAll: () => transport("updates_check_all", undefined),
    async pickAppImage() {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ multiple: false, filters: [{ name: "AppImage", extensions: ["AppImage", "appimage"] }] });
      return typeof selected === "string" ? selected : null;
    },
    async pickFolder() {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({ multiple: false, directory: true });
      return typeof selected === "string" ? selected : null;
    },
    async openExternal(value) {
      const url = validateExternalUrl(value);
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(url);
    },
  };
}

const invoke: CommandTransport = async (command, args) => {
  const { invoke: nativeInvoke } = await import("@tauri-apps/api/core");
  return nativeInvoke(command, args);
};

const nativeApi = createNativeApi(invoke);
const previewApi = createPreviewApi();

export const api: AppApi = new Proxy(nativeApi, {
  get(_target, property: keyof AppApi) {
    const current = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window ? nativeApi : previewApi;
    return current[property];
  },
});
