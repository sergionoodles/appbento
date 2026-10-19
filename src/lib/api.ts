import type { ConfigurationPreferences, ConfigurationPreview, ConfigurationRestoreRequest } from "./configuration";
import type { InstalledApp, LaunchConfig, LaunchLog, SandboxStatus, ScanReport, Settings, UpdateCheckResult, UpdateConfig, UpdateReport } from "./types";
import { createPreviewApi } from "./preview.ts";
import type { Channel } from "@tauri-apps/api/core";
import type { SelfUpdateBackend, SelfUpdateCheck, SelfUpdateProgress } from "./self-update";

export interface SelfLauncherStatus {
  installed: boolean;
  dismissed: boolean;
}

export interface Commands {
  self_launcher_status: { args: undefined; result: SelfLauncherStatus };
  self_launcher_install: { args: undefined; result: SelfLauncherStatus };
  self_launcher_dismiss: { args: undefined; result: SelfLauncherStatus };
  self_update_check: { args: undefined; result: SelfUpdateCheck };
  self_update_install: { args: { onProgress: Channel<SelfUpdateProgress> }; result: null };
  configuration_export: { args: { path: string; preferences: ConfigurationPreferences }; result: null };
  configuration_read: { args: { path: string }; result: ConfigurationPreview };
  configuration_restore: { args: { request: ConfigurationRestoreRequest }; result: ConfigurationPreferences };
  configuration_preferences: { args: { completed: boolean }; result: ConfigurationPreferences | null };
  library_set_icon: { args: { id: string; source: string }; result: InstalledApp };
  library_icon: { args: { id: string }; result: string | null };
  sandbox_status: { args: undefined; result: SandboxStatus };
  library_configure: { args: { id: string; config: LaunchConfig }; result: InstalledApp };
  library_list: { args: undefined; result: InstalledApp[] };
  library_scan: { args: { extraDirs: string[] }; result: ScanReport };
  library_add_url: { args: { url: string }; result: InstalledApp };
  library_set_menu_entry: { args: { id: string; enabled: boolean }; result: InstalledApp };
  library_import: { args: { sourcePath: string; moveToManaged: boolean }; result: InstalledApp };
  library_launch_log: { args: { id: string }; result: LaunchLog | null };
  library_prepare_launch: { args: { id: string; makeExecutable: boolean }; result: InstalledApp };
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

export interface AppApi extends SelfUpdateBackend {
  readonly isTauri: boolean;
  selfLauncherStatus(): Promise<SelfLauncherStatus>;
  selfLauncherInstall(): Promise<SelfLauncherStatus>;
  selfLauncherDismiss(): Promise<SelfLauncherStatus>;
  configurationExport(preferences: ConfigurationPreferences): Promise<boolean>;
  configurationRead(): Promise<ConfigurationPreview | null>;
  configurationRestore(request: ConfigurationRestoreRequest): Promise<ConfigurationPreferences>;
  configurationPreferences(completed: boolean): Promise<ConfigurationPreferences | null>;
  librarySetIcon(id: string, source: string): Promise<InstalledApp>;
  libraryIcon(id: string): Promise<string | null>;
  pickIcon(): Promise<string | null>;
  sandboxStatus(): Promise<SandboxStatus>;
  libraryConfigure(id: string, config: LaunchConfig): Promise<InstalledApp>;
  libraryList(): Promise<InstalledApp[]>;
  libraryScan(): Promise<ScanReport>;
  libraryAddUrl(url: string): Promise<InstalledApp>;
  librarySetMenuEntry(id: string, enabled: boolean): Promise<InstalledApp>;
  libraryImport(sourcePath: string, moveToManaged: boolean): Promise<InstalledApp>;
  libraryLaunchLog(id: string): Promise<LaunchLog | null>;
  libraryPrepareLaunch(id: string, makeExecutable: boolean): Promise<InstalledApp>;
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
    selfLauncherStatus: () => transport("self_launcher_status", undefined),
    selfLauncherInstall: () => transport("self_launcher_install", undefined),
    selfLauncherDismiss: () => transport("self_launcher_dismiss", undefined),
    selfUpdateCheck: () => transport("self_update_check", undefined),
    async selfUpdateInstall(onProgress) {
      const { Channel } = await import("@tauri-apps/api/core");
      const channel = new Channel<SelfUpdateProgress>();
      channel.onmessage = onProgress;
      await transport("self_update_install", { onProgress: channel });
    },
    async configurationExport(preferences) {
      const { save } = await import("@tauri-apps/plugin-dialog");
      const path = await save({ defaultPath: "appbento-settings.json", filters: [{ name: "AppBento configuration", extensions: ["json"] }] });
      if (!path) return false;
      await transport("configuration_export", { path, preferences });
      return true;
    },
    async configurationRead() {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const path = await open({ multiple: false, filters: [{ name: "AppBento configuration", extensions: ["json"] }] });
      return typeof path === "string" ? transport("configuration_read", { path }) : null;
    },
    configurationRestore: (request) => transport("configuration_restore", { request }),
    configurationPreferences: (completed) => transport("configuration_preferences", { completed }),
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
    libraryAddUrl: (url) => transport("library_add_url", { url }),
    librarySetMenuEntry: (id, enabled) => transport("library_set_menu_entry", { id, enabled }),
    libraryImport: (sourcePath, moveToManaged) => transport("library_import", { sourcePath, moveToManaged }),
    libraryLaunchLog: (id) => transport("library_launch_log", { id }),
    libraryPrepareLaunch: (id, makeExecutable) => transport("library_prepare_launch", { id, makeExecutable }),
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
