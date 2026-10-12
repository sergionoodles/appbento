import { useState } from "react";
import { Button } from "../ui/button";
import { Input } from "../ui/card";
import { Dialog } from "./dialog";

export function AddUrlDialog({ managedDir, onClose, onAdd, isDownloading, error, returnFocusElement }: {
  managedDir?: string;
  onClose: () => void;
  onAdd: (url: string) => Promise<boolean>;
  isDownloading: boolean;
  error: string | null;
  returnFocusElement: HTMLElement | null;
}) {
  const [url, setUrl] = useState("");
  const [validationError, setValidationError] = useState<string | null>(null);

  return (
    <Dialog title="Download from URL" onClose={onClose} canClose={!isDownloading} returnFocusElement={returnFocusElement}>
      <form className="flex flex-col gap-4" onSubmit={async (event) => {
        event.preventDefault();
        setValidationError(null);
        try {
          const parsed = new URL(url.trim());
          if (parsed.protocol !== "https:" || parsed.username || parsed.password) {
            throw new Error("Enter an HTTPS download URL without credentials.");
          }
        } catch {
          setValidationError("Enter an HTTPS download URL without credentials.");
          return;
        }
        if (await onAdd(url.trim())) onClose();
      }}>
        <div>
          <label htmlFor="appimage-url" className="block font-medium mb-1">AppImage URL</label>
          <p className="text-[13px] text-[var(--muted)] mb-2">Paste a direct HTTPS download link to an AppImage.</p>
          <Input id="appimage-url" type="url" required value={url} disabled={isDownloading}
            placeholder="https://example.org/MyApp.AppImage" onChange={(event) => { setUrl(event.target.value); setValidationError(null); }} />
        </div>
        <p className="text-[13px] text-[var(--muted)] break-all">The AppImage will be downloaded and added to {managedDir ?? "your managed folder"}.</p>
        {validationError || error ? <p role="alert" className="text-[var(--bad)] text-[14px]">{validationError ?? error}</p> : null}
        {isDownloading ? <p role="status" className="text-[13px] text-[var(--muted)]">Downloading and adding AppImage…</p> : null}
        <div className="flex justify-end gap-2">
          <Button type="button" variant="ghost" disabled={isDownloading} onClick={onClose}>cancel</Button>
          <Button type="submit" variant="accent" disabled={isDownloading || !url.trim()}>{isDownloading ? "downloading…" : "download + add"}</Button>
        </div>
      </form>
    </Dialog>
  );
}
