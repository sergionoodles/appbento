import { useState } from "react";
import type { InstalledApp } from "@/lib/types";
import { Button } from "../ui/button";
import { Dialog } from "./dialog";

function removeLabel(isRemoving: boolean, deleteFile: boolean) {
  if (isRemoving) return "removing…";
  return deleteFile ? "delete file + remove" : "remove entry";
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
  return (
    <Dialog title={`Remove ${app.name}?`} onClose={onClose} canClose={!isRemoving} returnFocusElement={returnFocusElement}>
      <p className="text-[13px]">Remove this entry and its managed desktop shortcut from the library.</p>
      <p className="text-[12px] text-[var(--muted)] break-all">{app.file_path}</p>
      <label className="flex gap-2 items-start text-[13px]">
        <input type="checkbox" checked={deleteFile} disabled={isRemoving || !app.integrated} onChange={(event) => setDeleteFile(event.target.checked)} />
        Also delete the AppImage file permanently
      </label>
      {!app.integrated ? <p className="text-[12px] text-[var(--muted)]">Portable files can be removed from the library. Delete the file yourself to remove it from disk.</p> : null}
      {!deleteFile ? <p className="text-[12px] text-[var(--muted)]">The file stays on disk. A later scan can add it again.</p> : null}
      {error ? <p role="alert" className="text-[var(--bad)] text-[13px]">{error}</p> : null}
      <div className="flex justify-end gap-2">
        <Button variant="ghost" disabled={isRemoving} onClick={onClose}>cancel</Button>
        <Button variant="danger" disabled={isRemoving} onClick={async () => {
          if (await onRemove(deleteFile)) onClose();
        }}>{removeLabel(isRemoving, deleteFile)}</Button>
      </div>
    </Dialog>
  );
}
