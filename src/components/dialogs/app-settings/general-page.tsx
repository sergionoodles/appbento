import { useState } from "react";
import { Pencil, RotateCcw } from "lucide-react";
import type { AppApi } from "@/lib/api";
import type { InstalledApp } from "@/lib/types";
import { formatAge } from "@/lib/relative-time";
import { AppIcon } from "../../app-icon";
import { versionLabel } from "../../app-status";
import { Button } from "../../ui/button";
import { Input } from "../../ui/card";
import { Field, FieldGroup } from "../../ui/field";

const BYTES_PER_MEGABYTE = 1e6;

export function GeneralPage({ app, backend, name, originalName, onNameChange, isEditingIcon, onEditIcon, onSaveIcon, isSaving }: {
  app: InstalledApp;
  backend: AppApi;
  name: string;
  originalName: string;
  onNameChange: (name: string) => void;
  isEditingIcon: boolean;
  onEditIcon: (isEditing: boolean) => void;
  onSaveIcon: (source: string) => Promise<boolean>;
  isSaving: boolean;
}) {
  const fileAge = formatAge(app.file_created_at);
  const details = [
    ["Version", versionLabel(app)],
    ["File", app.file_path],
    ["Launch count", String(app.launch_count)],
    ...(!app.integrated ? [["Integration", "Portable, not in the app menu"]] : []),
    ["Size", `${(app.size_bytes / BYTES_PER_MEGABYTE).toFixed(0)} MB`],
    ...(fileAge ? [["Downloaded", fileAge]] : []),
  ];

  return (
    <div className="flex flex-col gap-7">
      <div className="flex items-start gap-5">
        <div className="relative shrink-0">
          <AppIcon app={app} name={name || originalName} className="size-20 rounded-xl" />
          <button type="button" aria-label="Change icon" aria-expanded={isEditingIcon} disabled={isSaving} onClick={() => onEditIcon(!isEditingIcon)}
            className="absolute -bottom-1.5 -right-1.5 grid size-7 place-items-center rounded-full bg-[var(--accent)] text-[var(--on-accent)] shadow-md ring-[3px] ring-[var(--panel)] transition-transform hover:scale-110 disabled:opacity-45 disabled:cursor-not-allowed">
            <Pencil size={13} />
          </button>
        </div>
        <Field label="Name" htmlFor="app-display-name" className="flex-1" hint={name.trim() === originalName ? "Shown in your library." : <>
          Original name: {originalName}.{" "}
          <button type="button" className="inline-flex items-center gap-1 text-[var(--accent)] hover:underline" onClick={() => onNameChange(originalName)}><RotateCcw size={11} />Reset</button>
        </>}>
          <Input id="app-display-name" value={name} disabled={isSaving} onChange={(event) => onNameChange(event.target.value)} />
        </Field>
      </div>

      {isEditingIcon ? <IconEditor backend={backend} isSaving={isSaving} onCancel={() => onEditIcon(false)} onSave={onSaveIcon} /> : null}

      <FieldGroup title="Details">
        {details.map(([label, value]) => (
          <div key={label} className="flex items-baseline gap-4 px-4 py-2.5 text-[13.5px]">
            <span className="w-28 shrink-0 text-[var(--muted)]">{label}</span>
            <span className="min-w-0 break-all">{value}</span>
          </div>
        ))}
      </FieldGroup>
    </div>
  );
}

function IconEditor({ backend, isSaving, onCancel, onSave }: {
  backend: AppApi;
  isSaving: boolean;
  onCancel: () => void;
  onSave: (source: string) => Promise<boolean>;
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
      if (!selected) return;
      setFile(selected);
      setUrl("");
    } catch (failure) {
      setBrowseError(failure instanceof Error ? failure.message : String(failure));
    } finally {
      setIsPicking(false);
    }
  };

  return (
    <form aria-label="Change icon" className="flex flex-col gap-4 rounded-lg border border-[var(--accent)] bg-[color-mix(in_oklab,var(--accent)_5%,transparent)] p-4" onSubmit={async (event) => {
      event.preventDefault();
      const source = file ?? url.trim();
      if (!source || isBusy) return;
      if (await onSave(source)) onCancel();
    }}>
      <Field label="Image URL" htmlFor="icon-url" hint={backend.isTauri ? "A copy is saved with AppBento’s data, so it stays available offline." : "Saving icons is available in the desktop app."}>
        <div className="flex gap-2">
          <Input id="icon-url" type="url" placeholder="https://example.com/icon.png" disabled={isBusy} value={url} onChange={(event) => {
            setUrl(event.target.value);
            setFile(null);
          }} />
          <Button variant="outline" className="shrink-0" disabled={isBusy || !backend.isTauri} onClick={() => void browse()}>Choose local image</Button>
        </div>
      </Field>
      {file ? <p className="text-[13px] break-all">Selected: {file}</p> : null}
      {browseError ? <p role="alert" className="text-[13px] text-[var(--bad)]">{browseError}</p> : null}
      <div className="flex justify-end gap-2">
        <Button variant="ghost" size="sm" disabled={isBusy} onClick={onCancel}>Cancel</Button>
        <Button type="submit" size="sm" variant="accent" disabled={isBusy || !backend.isTauri || (!file && !url.trim())}>{isSaving ? "Saving…" : "Save icon"}</Button>
      </div>
    </form>
  );
}
