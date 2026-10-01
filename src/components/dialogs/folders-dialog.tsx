import { useState } from "react";
import { X } from "lucide-react";
import type { Settings } from "@/lib/types";
import { Button } from "../ui/button";
import { Input } from "../ui/card";
import { Dialog } from "./dialog";

export function FoldersDialog({ pickFolder, isNative, initial, onClose, onSaved, isSaving, error, returnFocusElement }: {
  pickFolder: () => Promise<string | null>;
  isNative: boolean;
  initial: Settings;
  onClose: () => void;
  onSaved: (settings: Settings) => Promise<boolean>;
  isSaving: boolean;
  error: string | null;
  returnFocusElement: HTMLElement | null;
}) {
  const [managed, setManaged] = useState(initial.managed_dir);
  const [directories, setDirectories] = useState(initial.scan_dirs);
  const [browseError, setBrowseError] = useState<string | null>(null);
  const browse = async (isManaged: boolean) => {
    setBrowseError(null);
    try {
      const directory = await pickFolder();
      if (!directory) return;
      if (isManaged) {
        setManaged(directory);
        return;
      }
      setDirectories((existing) => existing.includes(directory) ? existing : [...existing, directory]);
    } catch (failure) {
      setBrowseError(failure instanceof Error ? failure.message : String(failure));
    }
  };

  return (
    <Dialog title="Scan and managed folders" onClose={onClose} canClose={!isSaving} returnFocusElement={returnFocusElement}>
      <form className="flex flex-col gap-4" onSubmit={async (event) => {
        event.preventDefault();
        if (await onSaved({ managed_dir: managed, scan_dirs: directories })) onClose();
      }}>
        <fieldset disabled={isSaving} className="flex flex-col gap-4">
          <div>
            <label htmlFor="managed-folder" className="block font-medium mb-1">Managed folder</label>
            <p className="text-[13px] text-[var(--muted)] mb-2">Integrated AppImages are copied here. Enter an absolute path.</p>
            <div className="flex gap-2">
              <Input id="managed-folder" required value={managed} onChange={(event) => setManaged(event.target.value)} />
              <Button type="button" onClick={() => void browse(true)} disabled={!isNative}>browse</Button>
            </div>
          </div>
          <div>
            <div className="font-medium mb-1">Extra scan folders</div>
            <p className="text-[13px] text-[var(--muted)] mb-2">Scanned together with the managed folder, ~/Applications, ~/Downloads and other defaults.</p>
            <div className="flex flex-col gap-1">
              {directories.map((directory) => (
                <div key={directory} className="flex items-center gap-2 text-[14px] bg-[var(--panel2)] rounded-md pl-4 pr-1 py-1">
                  <span className="truncate flex-1">{directory}</span>
                  <Button type="button" variant="ghost" size="icon" aria-label={`Remove scan folder ${directory}`}
                    onClick={() => setDirectories((existing) => existing.filter((entry) => entry !== directory))}><X size={14} /></Button>
                </div>
              ))}
              <Button type="button" onClick={() => void browse(false)} disabled={!isNative} className="self-start">+ add folder</Button>
            </div>
          </div>
        </fieldset>
        {error || browseError ? <p role="alert" className="text-[var(--bad)] text-[14px]">{browseError ?? error}</p> : null}
        <div className="flex justify-end gap-2">
          <Button type="button" variant="ghost" disabled={isSaving} onClick={onClose}>cancel</Button>
          <Button type="submit" variant="accent" disabled={isSaving}>{isSaving ? "saving…" : "save + rescan"}</Button>
        </div>
      </form>
    </Dialog>
  );
}
