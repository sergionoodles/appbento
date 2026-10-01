import { useState } from "react";
import { api, type AppApi } from "@/lib/api";
import { useLibrary } from "@/lib/use-library";
import type { LibraryState } from "@/lib/library-controller";
import { ThemeProvider } from "@/components/theme";
import { Header, type Tab } from "@/components/header";
import { BentoView, CatalogView, LibraryView, UpdatesView } from "@/components/tab-views";
import { FoldersDialog } from "@/components/dialogs/folders-dialog";
import { RemoveDialog } from "@/components/dialogs/remove-dialog";
import { Button } from "@/components/ui/button";

function libraryStatus({ operation, isLoading, isCheckingUpdates }: Pick<LibraryState, "operation" | "isLoading" | "isCheckingUpdates">) {
  if (operation) return operation;
  if (isLoading) return "loading";
  return isCheckingUpdates ? "checking releases" : "idle";
}

function Shell({ backend }: { backend: AppApi }) {
  const [tab, setTab] = useState<Tab>("bento");
  const [query, setQuery] = useState("");
  const [showFolders, setShowFolders] = useState(false);
  const [returnFocusElement, setReturnFocusElement] = useState<HTMLElement | null>(null);
  const [removingId, setRemovingId] = useState<string | null>(null);
  const { apps, catalog, settings, updateReport, scanWarnings, isLoading, isCheckingUpdates, operation, error, controller } = useLibrary(backend);
  const filter = query.toLowerCase();
  const filteredApps = apps.filter((app) => app.name.toLowerCase().includes(filter));
  const filteredCatalog = catalog.filter((entry) => `${entry.app.name} ${entry.app.summary}`.toLowerCase().includes(filter));
  const updates = updateReport?.updates ?? [];
  const disabled = isLoading || operation !== null;
  const removingApp = apps.find((app) => app.id === removingId);
  const onLaunch = (id: string) => { void controller.launch(id); };
  const onOpen = (url: string) => { void controller.openExternal(url); };
  const onCheck = () => { void controller.checkUpdates(); };

  return (
    <>
      <div className="h-full flex flex-col" inert={showFolders || !!removingApp}>
        <Header isNative={backend.isTauri} tab={tab} onTab={setTab} query={query} onQuery={setQuery} disabled={disabled} canEditFolders={settings !== null}
          isChecking={isCheckingUpdates} onFolders={() => {
            setReturnFocusElement(document.activeElement instanceof HTMLElement ? document.activeElement : null);
            controller.dismissError();
            setShowFolders(true);
          }}
          onAdd={() => { void controller.integrate(); }} onScan={() => { void controller.scan(); }}
          onCheck={() => {
            setTab("updates");
            onCheck();
          }} />
        <main className="flex-1 overflow-auto p-4 max-w-7xl w-full mx-auto">
          {!backend.isTauri ? <p className="text-[12px] text-[var(--warn)] mb-3">Sample preview: changes affect sample data only. Launch and file management run in the desktop app.</p> : null}
          {error ? <div role="alert" className="mb-4 flex items-center gap-3 border border-[var(--bad)] rounded-md p-3 text-[var(--bad)]">
            <span className="flex-1">{error}</span><Button onClick={controller.dismissError}>dismiss</Button><Button disabled={disabled} onClick={() => void controller.reload()}>reload</Button>
          </div> : null}
          {scanWarnings.length ? <section aria-label="Scan warnings" className="mb-4 border border-[var(--warn)] rounded-md p-3">
            <div className="flex items-center justify-between gap-3"><h2 className="text-[var(--warn)]">Some files were skipped</h2><Button onClick={controller.dismissScanWarnings}>dismiss scan warnings</Button></div>
            <ul className="text-[12px] text-[var(--muted)] mt-2">{scanWarnings.map((warning) => <li key={warning.path} className="break-all">{warning.path}: {warning.message}</li>)}</ul>
          </section> : null}
          {isLoading ? <p role="status">Loading local library…</p> : (
            <>
              {tab === "bento" ? <BentoView apps={filteredApps} catalog={catalog} updates={updates} onLaunch={onLaunch} disabled={disabled} /> : null}
              {tab === "library" ? <LibraryView apps={filteredApps} settings={settings} updates={updates} onLaunch={onLaunch} disabled={disabled}
                onRemove={(id) => {
                  setReturnFocusElement(document.activeElement instanceof HTMLElement ? document.activeElement : null);
                  controller.dismissError();
                  setRemovingId(id);
                }} /> : null}
              {tab === "catalog" ? <CatalogView catalog={filteredCatalog} apps={apps} isPreview={!backend.isTauri} onOpen={onOpen} /> : null}
              {tab === "updates" ? <UpdatesView report={updateReport} isChecking={isCheckingUpdates || disabled} onCheck={onCheck} onOpen={onOpen} /> : null}
            </>
          )}
        </main>
        <footer className="statusbar flex flex-wrap items-center gap-4 px-4 py-1.5 text-[12px]" aria-live="polite">
          <span>{apps.length} apps</span><span>{updates.filter((update) => update.status === "available").length} confirmed updates</span><span>{catalog.length} catalog entries</span>
          <span className="ml-auto">{backend.isTauri ? "desktop" : "sample preview"} · {libraryStatus({ operation, isLoading, isCheckingUpdates })}</span>
        </footer>
      </div>
      {showFolders && settings ? <FoldersDialog returnFocusElement={returnFocusElement} pickFolder={backend.pickFolder} isNative={backend.isTauri} initial={settings} onClose={() => setShowFolders(false)} onSaved={controller.saveFolders} isSaving={operation !== null} error={error} /> : null}
      {removingApp ? <RemoveDialog returnFocusElement={returnFocusElement} app={removingApp} onClose={() => setRemovingId(null)} onRemove={(deleteFile) => controller.remove(removingApp.id, deleteFile)} isRemoving={operation !== null} error={error} /> : null}
    </>
  );
}

export default function App({ backend = api }: { backend?: AppApi }) {
  return <ThemeProvider><Shell backend={backend} /></ThemeProvider>;
}
