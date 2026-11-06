import { useEffect, useState } from "react";
import { Check, Copy } from "lucide-react";
import type { AppApi } from "@/lib/api";
import type { LaunchConfig } from "@/lib/types";
import { isDesktopIntegrationEnabled } from "@/lib/sandbox";
import { Button } from "../../ui/button";

const COPIED_RESET_DELAY_MS = 2000;
const COPIED_MESSAGE = "Command copied.";

type CommandResult =
  | { status: "ready"; command: string }
  | { status: "error"; error: string };

type CommandState = CommandResult | { status: "loading" };

function errorMessage(failure: unknown) {
  return failure instanceof Error ? failure.message : String(failure);
}

/** Keeps the last result visible while a changed config is rebuilt, but marks it stale. */
function useSandboxCommand(backend: AppApi, appId: string, config: LaunchConfig) {
  const [latest, setLatest] = useState<{ key: string; result: CommandResult } | null>(null);
  // Serialized so edits that produce an identical config don't refetch.
  const configKey = JSON.stringify(config);

  useEffect(() => {
    let isActive = true;
    backend.librarySandboxCommand(appId, JSON.parse(configKey) as LaunchConfig)
      .then((command) => { if (isActive) setLatest({ key: configKey, result: { status: "ready", command } }); })
      .catch((failure: unknown) => { if (isActive) setLatest({ key: configKey, result: { status: "error", error: errorMessage(failure) } }); });
    return () => { isActive = false; };
  }, [backend, appId, configKey]);

  const state: CommandState = latest?.result ?? { status: "loading" };
  return { state, configKey, isCurrent: latest?.key === configKey };
}

function notesFor(config: LaunchConfig) {
  return [
    "AppBento's file check and launch logs are skipped.",
    ...(isDesktopIntegrationEnabled(config.permissions.desktop_integration)
      ? ["Desktop integration is left out because AppBento only provides it when it launches the app."]
      : []),
    ...(config.permissions.persistent_data
      ? ["Launch the app from AppBento once first so its private data folder exists."]
      : []),
  ];
}

export function ManualCommand({ backend, appId, config }: {
  backend: AppApi;
  appId: string;
  config: LaunchConfig;
}) {
  const { state, configKey, isCurrent } = useSandboxCommand(backend, appId, config);
  // Tied to the config it was copied for, so editing permissions clears it.
  const [copyResult, setCopyResult] = useState<{ key: string; message: string } | null>(null);
  const copyMessage = copyResult?.key === configKey ? copyResult.message : "";

  useEffect(() => {
    if (copyResult?.message !== COPIED_MESSAGE) return;
    const timer = window.setTimeout(() => setCopyResult(null), COPIED_RESET_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [copyResult]);

  const copy = async () => {
    if (state.status !== "ready" || !isCurrent) return;
    try {
      await navigator.clipboard.writeText(state.command);
      setCopyResult({ key: configKey, message: COPIED_MESSAGE });
    } catch {
      setCopyResult({ key: configKey, message: "Could not copy the command to the clipboard." });
    }
  };

  const isCopied = copyMessage === COPIED_MESSAGE;
  const copyButton = (
    <Button size="sm" variant="outline" className="shrink-0 whitespace-nowrap" disabled={state.status !== "ready" || !isCurrent} onClick={() => void copy()}>
      {isCopied ? <Check size={14} /> : <Copy size={14} />}
      {isCopied ? "Copied" : "Copy command"}
    </Button>
  );

  const notes = notesFor(config);

  return (
    <section aria-label="Run from a terminal" className="flex items-start justify-between gap-4">
      <div className="min-w-0">
        <h4 className="text-[14px] font-semibold">Run from a terminal</h4>
        <p className="text-[12.5px] leading-snug text-[var(--muted)] mt-0.5">Copy the command that starts this app with the access above, including unsaved changes.</p>
        {notes.map((note) => <p key={note} className="text-[12.5px] leading-snug text-[var(--muted)] mt-1">{note}</p>)}
        {state.status === "error" ? <p className="text-[12.5px] leading-snug text-[var(--warn)] mt-1">Cannot build the command: {state.error}</p> : null}
        <p role="status" className={!copyMessage || isCopied ? "sr-only" : "text-[12.5px] leading-snug text-[var(--warn)] mt-1"}>{copyMessage}</p>
      </div>
      {copyButton}
    </section>
  );
}
