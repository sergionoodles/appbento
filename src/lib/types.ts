export type UpdateStrategy = "github-release" | "manual";

export interface CatalogEntry {
  app: {
    id: string;
    name: string;
    summary: string;
    description: string;
    website: string;
    docs_url: string | null;
    repo: string | null;
    license: string | null;
    categories: string[];
    verified: boolean;
  };
  branding: {
    logo: string;
    logo_dark: string | null;
    cover: string | null;
    screenshots: string[];
    accent: string | null;
  };
  update: {
    strategy: UpdateStrategy | null;
    github: {
      repo: string;
      asset_regex: string;
      prerelease: boolean;
      channel: string;
      architecture: string;
      tag_regex: string;
    } | null;
  };
  desktop: { categories: string | null; terminal: boolean; mime_type: string | null };
  version_regex: string;
  match_filenames: string[];
  match_binaries: string[];
}

export interface InstalledApp {
  id: string;
  name: string;
  file_path: string;
  source_path: string | null;
  file_name: string;
  size_bytes: number;
  executable: boolean;
  integrated: boolean;
  catalog_id: string | null;
  version_current: string | null;
  icon_path: string | null;
  desktop_id: string | null;
  launch_count: number;
  last_launched: string | null;
  added_at: string;
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
