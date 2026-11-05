export interface SandboxPath {
  path: string;
  access: "read-only" | "read-write";
}

export interface DbusRule {
  name: string;
  access: "talk" | "own";
}

export interface DesktopIntegration {
  portals: boolean;
  notifications: boolean;
  tray: boolean;
  media_controls: boolean;
  secrets: boolean;
  custom: DbusRule[];
}

export interface SandboxPermissions {
  network: boolean;
  display: boolean;
  audio: boolean;
  gpu: boolean;
  persistent_data: boolean;
  paths: SandboxPath[];
  desktop_integration: DesktopIntegration;
}

export interface LaunchConfig {
  arguments?: string[];
  environment?: Record<string, string>;
  mode: "direct" | "bubblewrap";
  permissions: SandboxPermissions;
}

export interface SandboxStatus {
  available: boolean;
  version: string | null;
  error: string | null;
  desktop_integration_available: boolean;
  desktop_integration_error: string | null;
}

export type ReleaseUpdateStrategy = "github-release" | "gitlab-release";

export interface ReleaseUpdateConfig {
  strategy: ReleaseUpdateStrategy;
  repo: string;
  asset_regex: string;
  architecture: string;
  tag_regex: string;
  prerelease: boolean;
  channel: string;
}

export type UrlUpdateStrategy = "direct-url" | "zsync" | "electron-feed";

/** Sources identified by one HTTPS URL: a file, a `.zsync` control file, or an electron-builder feed folder. */
export interface UrlUpdateConfig {
  strategy: UrlUpdateStrategy;
  url: string;
}

export type UpdateConfig = ReleaseUpdateConfig | UrlUpdateConfig;

/** `detected` sources come from the app itself and refresh automatically; `user` ones never do. */
export type UpdateConfigOrigin = "detected" | "user";

export type AppKind = "appimage" | "binary";

export interface InstalledApp {
  kind: AppKind;
  reviewed_digest?: string | null;
  launch_config?: LaunchConfig | null;
  update_config?: UpdateConfig | null;
  update_config_origin?: UpdateConfigOrigin | null;
  id: string;
  name: string;
  file_path: string;
  source_path: string | null;
  file_name: string;
  size_bytes: number;
  executable: boolean;
  integrated: boolean;
  version_current: string | null;
  icon_path: string | null;
  desktop_id: string | null;
  launch_count: number;
  last_launched: string | null;
  added_at: string;
  file_created_at?: string | null;
}

export interface ScanWarning {
  path: string;
  message: string;
}

export interface ScanReport {
  apps: InstalledApp[];
  warnings: ScanWarning[];
}

export interface UpdateInfo {
  app_id: string;
  current: string | null;
  latest: string;
  download_url: string;
  asset_name: string;
  published_at: string | null;
  size_bytes: number | null;
  status: "available" | "unknown-current";
}

export interface UpdateReport {
  updates: UpdateInfo[];
  errors: { app_id: string; message: string }[];
  unknown_versions: string[];
  checked_count: number;
  unsupported_count: number;
}

export interface UpdateCheckResult {
  app_id: string;
  status: "available" | "up-to-date" | "unknown-current" | "unsupported" | "error";
  latest: string | null;
  update: UpdateInfo | null;
  error: string | null;
}

export interface Settings {
  scan_dirs: string[];
  managed_dir: string;
}

export interface LaunchLog {
  started_at: string;
  text: string;
}
