import { useEffect, useState } from "react";
import type { AppApi } from "@/lib/api";
import type { InstalledApp, LaunchLog } from "@/lib/types";
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

  return (
    <Dialog title={`${app.name} latest launch logs`} onClose={onClose} returnFocusElement={returnFocusElement} className="w-[800px]">
      {error ? <p role="alert" className="text-[var(--bad)] selectable">{error}</p> : null}
      {isLoading ? <p role="status">Loading logs…</p> : null}
      {!isLoading && !error && !log ? <p>No launch logs yet. Launch this app to record its output.</p> : null}
      {log ? <>
        <pre aria-label="Launch log output" tabIndex={0} className="selectable min-h-48 max-h-[50vh] overflow-auto rounded-md border border-solid border-[var(--border)] bg-[var(--bg)] p-4 text-[12px] leading-relaxed font-mono">{log.text || "No output was produced by this launch."}</pre>
        <p role="status" className="text-[13px] text-[var(--muted)]">{copyMessage}</p>
      </> : null}
      <div className="flex justify-end gap-2">
        <Button variant="outline" onClick={refresh} disabled={isLoading}>Refresh</Button>
        <Button variant="outline" onClick={() => void copyLogs()} disabled={isLoading || !log?.text}>Copy logs</Button>
        <Button onClick={onClose}>Close</Button>
      </div>
    </Dialog>
  );
}
