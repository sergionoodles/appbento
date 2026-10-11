export interface SandboxPath {
  path: string;
  access: "read-only" | "read-write";
}

export interface SandboxPermissions {
  network: boolean;
  display: boolean;
  audio: boolean;
  gpu: boolean;
  persistent_data: boolean;
  paths: SandboxPath[];
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

export type UpdateConfig = ReleaseUpdateConfig | { strategy: "direct-url"; url: string };

export interface InstalledApp {
  launch_config?: LaunchConfig | null;
  update_config?: UpdateConfig | null;
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
  update: UpdateInfo | null;
  error: string | null;
}

export interface Settings {
  scan_dirs: string[];
  managed_dir: string;
}
