import type { InstalledApp } from "@/lib/types";
import { Button } from "../ui/button";
import { Dialog } from "./dialog";

export function ExecutableDialog({ app, onClose, onConfirm, isSaving, error, returnFocusElement }: {
  app: InstalledApp;
  onClose: () => void;
  onConfirm: () => Promise<boolean>;
  isSaving: boolean;
  error: string | null;
  returnFocusElement: HTMLElement | null;
}) {
  return (
    <Dialog title={`Make ${app.name} executable?`} onClose={onClose} canClose={!isSaving} returnFocusElement={returnFocusElement}>
      <p className="text-[14px]">This AppImage needs execute permission to run. Allow AppBento to make the file executable and launch it?</p>
      <p className="text-[13px] text-[var(--muted)] break-all selectable">{app.file_path}</p>
      {error ? <p role="alert" className="text-[var(--bad)] text-[14px]">{error}</p> : null}
      <div className="flex justify-end gap-2">
        <Button variant="ghost" disabled={isSaving} onClick={onClose}>Cancel</Button>
        <Button disabled={isSaving} onClick={() => { void onConfirm(); }}>{isSaving ? "Launching…" : "Make executable and launch"}</Button>
      </div>
    </Dialog>
  );
}
