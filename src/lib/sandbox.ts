import type { SandboxPermissions } from "./types";

export function restrictedPermissions(): SandboxPermissions {
  return { network: false, display: true, audio: false, gpu: false, persistent_data: true, paths: [] };
}

export function validateSandboxPaths(permissions: SandboxPermissions): string | null {
  if (permissions.paths.some(({ path }) => !path.startsWith("/") || path.includes("\0") || path.split("/").includes(".."))) {
    return "Choose absolute paths without parent directory segments.";
  }
  if (new Set(permissions.paths.map(({ path }) => path)).size !== permissions.paths.length) {
    return "Each path can only be added once.";
  }
  return null;
}
