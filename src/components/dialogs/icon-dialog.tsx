import { useState } from "react";
import type { AppApi } from "@/lib/api";
import type { InstalledApp } from "@/lib/types";
import { Button } from "../ui/button";
import { Input } from "../ui/card";
import { Dialog } from "./dialog";

export function IconDialog({ app, backend, onClose, onSave, isSaving, error, returnFocusElement }: {
  app: InstalledApp;
  backend: AppApi;
  onClose: () => void;
  onSave: (source: string) => Promise<boolean>;
  isSaving: boolean;
  error: string | null;
  returnFocusElement: HTMLElement | null;
}) {
  const [url, setUrl] = useState("");
  const [file, setFile] = useState<string | null>(null);
  const [isPicking, setIsPicking] = useState(false);
  const [browseError, setBrowseError] = useState<string | null>(null);
  const isBusy = isSaving || isPicking;
  const browse = async () => {
    setIsPicking(true);
    setBrowseError(null);
    try {
      const selected = await backend.pickIcon();
      if (selected) {
        setFile(selected);
        setUrl("");
      }
    } catch (failure) {
      setBrowseError(failure instanceof Error ? failure.message : String(failure));
    } finally {
      setIsPicking(false);
    }
  };

  return (
    <Dialog title={`Change icon for ${app.name}`} onClose={onClose} canClose={!isBusy} returnFocusElement={returnFocusElement}>
      <p className="text-[14px] text-[var(--muted)]">Paste an image URL or choose a local image. A copy is saved with AppBento’s data, so it stays available offline.</p>
      <form className="flex flex-col gap-4" onSubmit={async (event) => {
        event.preventDefault();
        const source = file ?? url.trim();
        if (!source || isBusy) return;
        if (await onSave(source)) onClose();
      }}>
        <fieldset disabled={isBusy} className="flex flex-col gap-3">
          <label htmlFor="icon-url" className="font-medium">Image URL</label>
          <Input id="icon-url" type="url" placeholder="https://example.com/icon.png" value={url} onChange={(event) => {
            setUrl(event.target.value);
            setFile(null);
          }} />
          <Button type="button" variant="outline" className="self-start" disabled={!backend.isTauri} onClick={() => void browse()}>Choose local image</Button>
          {file ? <p className="text-[13px] break-all">Selected: {file}</p> : null}
        </fieldset>
        {!backend.isTauri ? <p className="text-[13px] text-[var(--muted)]">Saving icons is available in the desktop app.</p> : null}
        {browseError || error ? <p role="alert" className="text-[var(--bad)] text-[14px]">{browseError ?? error}</p> : null}
        <div className="flex justify-end gap-2">
          <Button type="button" variant="ghost" disabled={isBusy} onClick={onClose}>Cancel</Button>
          <Button type="submit" disabled={isBusy || !backend.isTauri || (!file && !url.trim())}>{isSaving ? "Saving…" : "Save icon"}</Button>
        </div>
      </form>
    </Dialog>
  );
}
