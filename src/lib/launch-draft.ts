import type { LaunchConfig } from "./types";

export interface LaunchDraft {
  arguments: string;
  environment: string;
}

export function launchDraftFromConfig(config: LaunchConfig): LaunchDraft {
  return {
    arguments: (config.arguments ?? []).join("\n"),
    environment: Object.entries(config.environment ?? {}).map(([name, value]) => `${name}=${value}`).join("\n"),
  };
}

export function parseLaunchDraft(draft: LaunchDraft): Pick<LaunchConfig, "arguments" | "environment"> {
  const argumentsList = draft.arguments.split("\n").filter((line) => line !== "");
  if (argumentsList.some((argument) => argument.includes("\0"))) {
    throw new Error("Arguments cannot contain null characters.");
  }
  const entries = draft.environment.split("\n").filter((line) => line.trim() !== "").map((line) => {
    const separator = line.indexOf("=");
    const name = line.slice(0, separator);
    const value = line.slice(separator + 1);
    if (separator < 0 || !/^[A-Za-z_][A-Za-z0-9_]*$/.test(name)) {
      throw new Error("Enter each environment variable as NAME=value. Names can contain letters, digits, and underscores, and cannot start with a digit.");
    }
    if (value.includes("\0")) throw new Error("Environment values cannot contain null characters.");
    return [name, value] as const;
  });
  if (new Set(entries.map(([name]) => name)).size !== entries.length) {
    throw new Error("Each environment variable name must appear only once.");
  }
  return { arguments: argumentsList, environment: Object.fromEntries(entries) };
}
