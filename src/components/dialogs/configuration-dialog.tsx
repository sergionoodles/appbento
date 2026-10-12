import { useState } from "react";
import type { AppApi } from "@/lib/api";
import { readConfigurationPreferences, type ConfigurationPreferences, type ConfigurationPreview } from "@/lib/configuration";
import { Button } from "../ui/button";
import { Input } from "../ui/card";
import { Dialog } from "./dialog";

export function ConfigurationDialog({ backend, onClose, onRestored, returnFocusElement }: {
  backend: AppApi;
  onClose: () => void;
  onRestored: (preferences: ConfigurationPreferences) => Promise<void>;
  returnFocusElement: HTMLElement | null;
}) {
  const [preview, setPreview] = useState<ConfigurationPreview | null>(null);
  const [isBusy, setIsBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  const run = async (operation: () => Promise<void>) => {
    setIsBusy(true);
    setError(null);
    setMessage(null);
    try { await operation(); }
    catch (failure) { setError(failure instanceof Error ? failure.message : String(failure)); }
    finally { setIsBusy(false); }
  };

  const browseApp = async (id: string) => {
    const path = await backend.pickAppImage();
    if (path) setPreview((current) => current ? { ...current, app_paths: { ...current.app_paths, [id]: path } } : null);
  };

  return (
    <Dialog title="Export and restore settings" onClose={onClose} canClose={!isBusy} returnFocusElement={returnFocusElement}>
      <div className="flex flex-col gap-4">
        <p className="text-[13px] text-[var(--muted)]">Save all AppBento settings, custom icons, names, groups, app order, theme, launch history, launch permissions, and update sources. Copy your AppImage files and the apps’ own data separately.</p>
        <div className="flex gap-2">
          <Button disabled={isBusy} onClick={() => void run(async () => {
            if (await backend.configurationExport(readConfigurationPreferences())) setMessage("Settings exported.");
          })}>Export settings</Button>
          <Button disabled={isBusy} onClick={() => void run(async () => {
            const selected = await backend.configurationRead();
            if (selected) setPreview(selected);
          })}>Choose settings file</Button>
        </div>
        {preview ? <form className="flex flex-col gap-4" onSubmit={(event) => {
          event.preventDefault();
          void run(async () => {
            const preferences = await backend.configurationRestore(preview);
            await onRestored(preferences);
            onClose();
          });
        }}>
          <p className="text-[13px] text-[var(--muted)]">Restore {preview.backup.apps.length} apps. Home paths have been adapted to this computer. Review the folders and locate each copied AppImage. Restore applies saved app and global settings; other library entries stay available.</p>
          <fieldset disabled={isBusy} className="flex flex-col gap-3">
            <label className="flex flex-col gap-1 text-[13px]">Managed folder
              <Input required value={preview.settings.managed_dir} onChange={(event) => setPreview({ ...preview, settings: { ...preview.settings, managed_dir: event.target.value } })} />
            </label>
            {preview.settings.scan_dirs.map((directory, index) => <label key={index} className="flex flex-col gap-1 text-[13px]">Scan folder {index + 1}
              <Input required value={directory} onChange={(event) => setPreview({ ...preview, settings: { ...preview.settings, scan_dirs: preview.settings.scan_dirs.map((value, position) => position === index ? event.target.value : value) } })} />
            </label>)}
            {preview.backup.apps.map(({ app }) => <label key={app.id} className="flex flex-col gap-1 text-[13px]">
              AppImage for {preview.backup.preferences.display_names[app.id] || app.name}
              <div className="flex gap-2">
                <Input required value={preview.app_paths[app.id] ?? ""} onChange={(event) => setPreview({ ...preview, app_paths: { ...preview.app_paths, [app.id]: event.target.value } })} />
                <Button type="button" onClick={() => void run(() => browseApp(app.id))}>Browse</Button>
              </div>
            </label>)}
            {Object.entries(preview.permission_paths).map(([source, destination]) => <label key={source} className="flex flex-col gap-1 text-[13px]">Permission folder: {source}
              <Input required value={destination} onChange={(event) => setPreview({ ...preview, permission_paths: { ...preview.permission_paths, [source]: event.target.value } })} />
            </label>)}
          </fieldset>
          <p className="text-[13px] text-[var(--muted)]">Permission folders must exist. Launch arguments and environment values that are home paths are adapted too. If an AppImage has changed, its launch settings are retained and require confirmation before launch.</p>
          <Button type="submit" variant="accent" disabled={isBusy}>{isBusy ? "Working…" : "Restore settings"}</Button>
        </form> : null}
        {error ? <p role="alert" className="text-[var(--bad)] text-[14px] selectable">{error}</p> : null}
        {message ? <p role="status" className="text-[13px]">{message}</p> : null}
        <div className="flex justify-end"><Button variant="ghost" disabled={isBusy} onClick={onClose}>Close</Button></div>
      </div>
    </Dialog>
  );
}
