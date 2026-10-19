import type { InstalledApp } from "@/lib/types";
import { Button } from "../ui/button";
import { Dialog } from "./dialog";

export function ImportLocationDialog({ app, managedDir, onKeep, onMove, isMoving, error, returnFocusElement }: {
  app: InstalledApp;
  managedDir: string;
  onKeep: () => void;
  onMove: () => Promise<boolean>;
  isMoving: boolean;
  error: string | null;
  returnFocusElement: HTMLElement | null;
}) {
  return (
    <Dialog title={`Move ${app.name} to your managed folder?`} onClose={onKeep} canClose={!isMoving} returnFocusElement={returnFocusElement}>
      <p className="text-[14px]">{app.name} has been added to your library. Move its AppImage to your managed folder to keep your apps together, or leave it where it is.</p>
      <div className="space-y-2 text-[13px]">
        <p className="text-[var(--muted)]">Current location</p>
        <p className="break-all selectable">{app.file_path}</p>
        <p className="text-[var(--muted)]">Managed folder</p>
        <p className="break-all selectable">{managedDir}</p>
      </div>
      <p className="text-[13px] text-[var(--muted)]">Moving removes the file from its current location after it has been safely saved in the managed folder. Its filename stays the same.</p>
      {error ? <p role="alert" className="text-[var(--bad)] text-[14px]">{error}</p> : null}
      <div className="flex justify-end gap-2">
        <Button variant="ghost" disabled={isMoving} onClick={onKeep}>Keep current location</Button>
        <Button variant="accent" disabled={isMoving} onClick={() => { void onMove(); }}>{isMoving ? "Moving…" : "Move to managed folder"}</Button>
      </div>
    </Dialog>
  );
}
