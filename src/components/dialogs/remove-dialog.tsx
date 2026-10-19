import { useId, useState } from "react";
import type { InstalledApp } from "@/lib/types";
import { Button } from "../ui/button";
import { Checkbox } from "../ui/checkbox";
import { Dialog } from "./dialog";

function removeLabel(isRemoving: boolean, deleteFile: boolean) {
  if (isRemoving) return "Removing…";
  return deleteFile ? "Delete file and remove" : "Remove entry";
}

export function RemoveDialog({ app, onClose, onRemove, isRemoving, error, returnFocusElement }: {
  app: InstalledApp;
  onClose: () => void;
  onRemove: (deleteFile: boolean) => Promise<boolean>;
  isRemoving: boolean;
  error: string | null;
  returnFocusElement: HTMLElement | null;
}) {
  const [deleteFile, setDeleteFile] = useState(false);
  const deletionHintId = useId();
  return (
    <Dialog title={`Remove ${app.name}?`} onClose={onClose} canClose={!isRemoving} returnFocusElement={returnFocusElement}>
      <p className="text-[14px]">Remove this app from your library and delete its AppBento-created app menu shortcut, if it has one.</p>
      <p className="text-[13px] text-[var(--muted)] break-all">{app.file_path}</p>
      <label className="flex gap-2 items-start text-[14px]">
        <Checkbox className="mt-0.5" aria-describedby={deletionHintId} checked={deleteFile} disabled={isRemoving || !app.integrated} onChange={(event) => setDeleteFile(event.target.checked)} />
        Also delete the AppImage file permanently
      </label>
      <p id={deletionHintId} className="text-[13px] text-[var(--muted)]">{app.integrated
        ? "This AppImage is managed by AppBento. You can also delete its file when removing the app."
        : "This AppImage is outside AppBento’s managed folder. To delete its file, use your file manager at the location shown above."}</p>
      {!deleteFile ? <p className="text-[13px] text-[var(--muted)]">The file stays on disk. A later scan can add it again.</p> : null}
      {error ? <p role="alert" className="text-[var(--bad)] text-[14px]">{error}</p> : null}
      <div className="flex justify-end gap-2">
        <Button variant="ghost" disabled={isRemoving} onClick={onClose}>Cancel</Button>
        <Button variant="danger" disabled={isRemoving} onClick={async () => {
          if (await onRemove(deleteFile)) onClose();
        }}>{removeLabel(isRemoving, deleteFile)}</Button>
      </div>
    </Dialog>
  );
}
