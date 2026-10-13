import { useState } from "react";
import type { SelfUpdateProgress, SelfUpdateState } from "@/lib/self-update";
import { Button } from "../ui/button";
import { Dialog } from "./dialog";

function progressLabel(progress: SelfUpdateProgress | null) {
  if (progress?.phase === "installing") return "Installing update…";
  if (progress?.phase === "restarting") return "Restarting AppBento…";
  return "Downloading update…";
}

function checkLabel(state: SelfUpdateState) {
  if (state.isChecking) return "Checking for updates…";
  if (state.result?.unavailableReason) return state.result.unavailableReason;
  if (state.error) return "Could not check for updates. Try again.";
  return state.result ? "AppBento is up to date." : "Check for a newer version of AppBento.";
}

export function SelfUpdateDialog({ state, isBlocked, onCheck, onInstall, onDownload, onClose, returnFocusElement }: {
  state: SelfUpdateState;
  isBlocked: boolean;
  onCheck: () => void;
  onInstall: () => void;
  onDownload: (url: string) => Promise<void>;
  onClose: () => void;
  returnFocusElement: HTMLElement | null;
}) {
  const [downloadError, setDownloadError] = useState<string | null>(null);
  const update = state.result?.update;
  const progress = state.progress;
  const isInstalling = progress !== null;
  const percentage = progress?.phase === "downloading" && progress.total
    ? Math.min(100, Math.floor(progress.downloaded / progress.total * 100)) : null;

  return <Dialog title="AppBento updates" onClose={onClose} canClose={!isInstalling} returnFocusElement={returnFocusElement}>
    {state.result ? <p className="text-[13px] text-[var(--muted)]">Current version: {state.result.currentVersion}</p> : null}
    {update ? <>
      <p className="text-[14px]">AppBento {update.version} is available.</p>
      {update.notes ? <div className="max-h-48 overflow-auto whitespace-pre-wrap selectable text-[13px] text-[var(--muted)]" aria-label="Release notes">{update.notes}</div> : null}
      {update.installDisabledReason ? <p className="text-[13px] text-[var(--warn)]">{update.installDisabledReason}</p> : <p className="text-[13px] text-[var(--muted)]">AppBento will restart after the update is installed.</p>}
      {isBlocked && !isInstalling ? <p className="text-[13px] text-[var(--warn)]">Wait for the current library operation to finish before restarting.</p> : null}
    </> : <p role="status" className="text-[14px]">{checkLabel(state)}</p>}
    {progress ? <div role="status" className="space-y-2 text-[14px]">
      <p>{progressLabel(progress)}{percentage !== null ? ` ${percentage}%` : ""}</p>
      {progress.phase === "downloading" ? <div
        role="progressbar"
        aria-label="Update download"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={percentage ?? undefined}
        className="h-3.5 w-full overflow-hidden rounded-full"
        style={{ backgroundColor: "var(--panel2)" }}
      >
        <div
          className={`h-full rounded-full${percentage === null ? " animate-pulse motion-reduce:animate-none" : ""}`}
          style={{ backgroundColor: "var(--accent)", width: `${percentage ?? 100}%` }}
        />
      </div> : null}
    </div> : null}
    {state.error || downloadError ? <p role="alert" className="text-[13px] text-[var(--bad)] selectable">{state.error ?? downloadError}</p> : null}
    <div className="flex flex-wrap justify-end gap-2">
      <Button variant="ghost" disabled={isInstalling} onClick={onClose}>Close</Button>
      {update ? <Button variant="outline" disabled={isInstalling} onClick={() => {
        setDownloadError(null);
        void onDownload(update.downloadUrl).catch((error: unknown) => setDownloadError(error instanceof Error ? error.message : String(error)));
      }}>Download manually</Button> : null}
      {update ? <Button disabled={isInstalling || state.isChecking || isBlocked || !!update.installDisabledReason} onClick={onInstall}>Install update and restart</Button>
        : <Button disabled={state.isChecking || !!state.result?.unavailableReason} onClick={onCheck}>Check for updates</Button>}
    </div>
  </Dialog>;
}
