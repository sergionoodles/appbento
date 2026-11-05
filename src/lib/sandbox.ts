import type { DbusRule, DesktopIntegration, SandboxPermissions, SandboxStatus } from "./types";

/** Desktop integration defaults for a newly reviewed app; saved apps keep whatever they stored. */
export function recommendedDesktopIntegration(): DesktopIntegration {
  return { portals: true, notifications: true, tray: false, media_controls: false, secrets: false, custom: [] };
}

export function restrictedPermissions(): SandboxPermissions {
  return {
    network: false,
    display: true,
    audio: false,
    gpu: false,
    persistent_data: true,
    paths: [],
    desktop_integration: recommendedDesktopIntegration(),
  };
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

export function unavailableSandboxStatus(error: string): SandboxStatus {
  return { available: false, version: null, error, desktop_integration_available: false, desktop_integration_error: null };
}

const DBUS_NAME_MAX_LENGTH = 255;
const DBUS_ELEMENT_PATTERN = /^[A-Za-z_-][A-Za-z0-9_-]*$/;
const DBUS_DAEMON_ELEMENTS = ["org", "freedesktop", "DBus"];

export function validateDbusName(name: string, existing: DbusRule[], access: DbusRule["access"]): string | null {
  if (!name || name.length > DBUS_NAME_MAX_LENGTH) return "Enter a D-Bus name of at most 255 characters.";
  if (name.startsWith(":")) return "Unique connection names (starting with a colon) are not supported.";

  const hasWildcard = name.endsWith(".*");
  const elements = (hasWildcard ? name.slice(0, -2) : name).split(".");
  if (elements.length < 2 || !elements.every((element) => DBUS_ELEMENT_PATTERN.test(element))) {
    return "Use a name like org.example.Service: at least two dot-separated parts of letters, digits, hyphens or underscores, none starting with a digit.";
  }

  const isDaemonPrefix = elements.length <= DBUS_DAEMON_ELEMENTS.length && elements.every((element, index) => element === DBUS_DAEMON_ELEMENTS[index]);
  const coversDaemon = hasWildcard ? isDaemonPrefix : isDaemonPrefix && elements.length === DBUS_DAEMON_ELEMENTS.length;
  if (coversDaemon) return "This name would cover the D-Bus service itself and cannot be allowed.";

  if (existing.some((rule) => rule.name === name && rule.access === access)) return "This name is already added with this access.";
  return null;
}
