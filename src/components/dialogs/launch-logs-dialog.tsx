import { useEffect, useState } from "react";
import { RefreshCw } from "lucide-react";
import type { AppApi } from "@/lib/api";
import type { InstalledApp, LaunchLog } from "@/lib/types";
import { createLaunchFixPrompt } from "@/lib/launch-fix-prompt";
import { Button } from "../ui/button";
import { Dialog } from "./dialog";

export function LaunchLogsDialog({ app, backend, onClose, returnFocusElement }: {
  app: InstalledApp;
  backend: AppApi;
  onClose: () => void;
  returnFocusElement?: HTMLElement | null;
}) {
  const [log, setLog] = useState<LaunchLog | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [revision, setRevision] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [copyMessage, setCopyMessage] = useState("");
  const [prompt, setPrompt] = useState<string | null>(null);

  useEffect(() => {
    let isActive = true;
    backend.libraryLaunchLog(app.id).then((result) => {
      if (!isActive) return;
      setLog(result);
      setIsLoading(false);
    }).catch((failure: unknown) => {
      if (!isActive) return;
      setError(failure instanceof Error ? failure.message : String(failure));
      setIsLoading(false);
    });
    return () => { isActive = false; };
  }, [app.id, backend, revision]);

  const refresh = () => {
    setIsLoading(true);
    setError(null);
    setCopyMessage("");
    setRevision((value) => value + 1);
  };

  const copyLogs = async () => {
    if (!log?.text) return;
    try {
      await navigator.clipboard.writeText(log.text);
      setCopyMessage("Logs copied.");
    } catch {
      setCopyMessage("Could not copy automatically. Select the log text and copy it manually.");
    }
  };

  const copyPrompt = async () => {
    if (!prompt?.trim()) return;
    try {
      await navigator.clipboard.writeText(prompt);
      setCopyMessage("Prompt copied. Paste it into your AI agent.");
    } catch {
      setCopyMessage("Could not copy automatically. Select the prompt text and copy it manually.");
    }
  };

  return (
    <Dialog title={`${app.name} logs`} onClose={onClose} returnFocusElement={returnFocusElement} className="w-[800px]"
      titleAction={prompt === null ? <Button variant="outline" size="icon" aria-label="Refresh" title="Refresh" onClick={refresh} disabled={isLoading}><RefreshCw size={16} aria-hidden="true" /></Button> : null}>
      {prompt !== null ? <>
        <p id="fix-prompt-help" className="text-[13px] text-[var(--muted)]">Copy this prompt into your external AI agent for help diagnosing the launch. Review or edit it first: logs, paths, and environment variables may contain private information.</p>
        <textarea aria-label="Troubleshooting prompt" aria-describedby="fix-prompt-help" tabIndex={0} rows={16} autoFocus
          className="selectable min-h-48 max-h-[50vh] w-full shrink-0 resize-y rounded-md border border-solid border-[var(--border)] bg-[var(--bg)] p-4 text-[12px] leading-relaxed font-mono outline-none focus:border-[var(--accent)]"
          value={prompt} onChange={(event) => { setPrompt(event.target.value); setCopyMessage(""); }} />
      </> : <>
        {error ? <p role="alert" className="text-[var(--bad)] selectable">{error}</p> : null}
        {isLoading ? <p role="status">Loading logs…</p> : null}
        {!isLoading && !error && !log ? <p>No launch logs yet. Launch this app to record its output.</p> : null}
        {log ? <pre aria-label="Launch log output" tabIndex={0} className="selectable min-h-48 max-h-[50vh] overflow-auto rounded-md border border-solid border-[var(--border)] bg-[var(--bg)] p-4 text-[12px] leading-relaxed font-mono">{log.text || "No output was produced by this launch."}</pre> : null}
      </>}
      <p role="status" className="text-[13px] text-[var(--muted)]">{copyMessage}</p>
      <div className="flex flex-wrap justify-end gap-2">
        {prompt !== null ? <>
          <Button variant="outline" className="mr-auto" onClick={() => { setPrompt(null); setCopyMessage(""); }}>Back to logs</Button>
          <Button variant="accent" onClick={() => void copyPrompt()} disabled={!prompt.trim()}>Copy prompt</Button>
        </> : <>
          <Button variant="accent" className="mr-auto" disabled={isLoading || !!error || !log?.text.trim()} onClick={() => { setPrompt(createLaunchFixPrompt(app, log)); setCopyMessage(""); }}>Prompt your agent</Button>
          <Button variant="accent" onClick={() => void copyLogs()} disabled={isLoading || !log?.text}>Copy logs</Button>
        </>}
        <Button onClick={onClose}>Close</Button>
      </div>
    </Dialog>
  );
}
