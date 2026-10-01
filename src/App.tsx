import { useCallback, useState } from "react";
import { Loader2 } from "lucide-react";
import { api, type AppApi } from "@/lib/api";
import { useLibrary } from "@/lib/use-library";
import { transitionTo } from "@/lib/view-transition";
import { ThemeProvider } from "@/components/theme";
import { Header } from "@/components/header";
import { Bento, BentoSkeleton, tileSelector } from "@/components/bento";
import { AppDetail, STAGE_SELECTOR } from "@/components/app-detail";
import { CatalogView } from "@/components/catalog-view";
import { UpdatesView } from "@/components/updates-view";
import { FoldersDialog } from "@/components/dialogs/folders-dialog";
import { PermissionsDialog } from "@/components/dialogs/permissions-dialog";
import { IconDialog } from "@/components/dialogs/icon-dialog";
import { IconBackendContext } from "@/components/app-icon";
import { RemoveDialog } from "@/components/dialogs/remove-dialog";
import { Button } from "@/components/ui/button";

type View = { name: "home" } | { name: "app"; id: string } | { name: "discover" } | { name: "updates" };

const HOME: View = { name: "home" };
const LAUNCH_OPERATION = "Launch app";

function Shell({ backend }: { backend: AppApi }) {
  const [view, setView] = useState<View>(HOME);
  const [query, setQuery] = useState("");
  const [showFolders, setShowFolders] = useState(false);
  const [returnFocusElement, setReturnFocusElement] = useState<HTMLElement | null>(null);
  const [iconId, setIconId] = useState<string | null>(null);
  const [removingId, setRemovingId] = useState<string | null>(null);
  const [hasEntered, setHasEntered] = useState(false);
  const { apps, catalog, settings, updateReport, scanWarnings, isLoading, isCheckingUpdates, operation, error, reviewIds, launchAfterReview, controller } = useLibrary(backend);
  const markEntered = useCallback(() => setHasEntered(true), []);
  const filter = query.toLowerCase();
  const filteredApps = apps.filter((app) => app.name.toLowerCase().includes(filter));
  const filteredCatalog = catalog.filter((entry) => `${entry.app.name} ${entry.app.summary}`.toLowerCase().includes(filter));
  const updates = updateReport?.updates ?? [];
  const confirmedUpdateCount = updates.filter((update) => update.status === "available").length;
  const disabled = isLoading || operation !== null;
  const removingApp = apps.find((app) => app.id === removingId);
  const reviewingApp = apps.find((app) => app.id === reviewIds[0]);
  const detailApp = view.name === "app" ? apps.find((app) => app.id === view.id) : undefined;
  const iconApp = apps.find((app) => app.id === iconId);
  const hasModal = !!iconApp || showFolders || !!removingApp || !!reviewingApp;
  const captureFocus = () => setReturnFocusElement(document.activeElement instanceof HTMLElement ? document.activeElement : null);
  const onLaunch = (id: string) => { captureFocus(); void controller.launch(id); };
  const onPermissions = (id: string) => { captureFocus(); controller.requestReview(id); };
  const onIcon = (id: string) => {
    captureFocus();
    controller.dismissError();
    setIconId(id);
  };
  const onRemove = (id: string) => {
    captureFocus();
    controller.dismissError();
    setRemovingId(id);
  };
  const onOpenUrl = (url: string) => { void controller.openExternal(url); };
  const goHome = () => {
    const fromId = view.name === "app" ? view.id : null;
    transitionTo(() => setView(HOME), { fromSelector: fromId ? STAGE_SELECTOR : null, toSelector: fromId ? tileSelector(fromId) : null });
  };
  const goTo = (next: View) => transitionTo(() => setView(next));
  const openApp = (id: string) => transitionTo(() => setView({ name: "app", id }), { fromSelector: tileSelector(id), toSelector: STAGE_SELECTOR });
  const statusText = operation ?? (isCheckingUpdates ? "Checking for updates" : null);

  return (
    <>
      <div className="min-h-full flex flex-col" inert={hasModal}>
        <Header isNative={backend.isTauri} query={query} onQuery={setQuery} disabled={disabled} canEditFolders={settings !== null} updateCount={confirmedUpdateCount}
          onHome={goHome} onUpdates={() => goTo({ name: "updates" })}
          onFolders={() => {
            captureFocus();
            controller.dismissError();
            setShowFolders(true);
          }}
          onAdd={() => { captureFocus(); void controller.integrate(); }} onScan={() => { captureFocus(); void controller.scan(); }} />
        <main className="flex-1 px-6 pb-24 max-w-[1240px] w-full mx-auto">
          {error ? <div role="alert" className="mb-4 flex items-center gap-3 rounded-lg border border-[var(--bad)] p-4 text-[var(--bad)]">
            <span className="flex-1">{error}</span><Button variant="outline" onClick={controller.dismissError}>Dismiss</Button><Button variant="outline" disabled={disabled} onClick={() => void controller.reload()}>Reload</Button>
          </div> : null}
          {scanWarnings.length ? <section aria-label="Scan warnings" className="mb-4 rounded-lg border border-[var(--warn)] p-4">
            <div className="flex items-center justify-between gap-3"><h2 className="text-[var(--warn)] font-medium">Some files were skipped</h2><Button variant="outline" size="sm" onClick={controller.dismissScanWarnings}>Dismiss scan warnings</Button></div>
            <ul className="text-[13px] text-[var(--muted)] mt-2">{scanWarnings.map((warning) => <li key={warning.path} className="break-all">{warning.path}: {warning.message}</li>)}</ul>
          </section> : null}
          {isLoading ? <BentoSkeleton /> : (
            <>
              {view.name === "home" || (view.name === "app" && !detailApp) ? <Bento apps={filteredApps} catalog={catalog} updates={updates} isAnimated={!hasEntered} onEntered={markEntered}
                onOpen={openApp} onLaunch={onLaunch} onPermissions={onPermissions} onIcon={onIcon} onRemove={onRemove} onDiscover={() => goTo({ name: "discover" })}
                onAdd={() => { captureFocus(); void controller.integrate(); }} canAdd={backend.isTauri} disabled={disabled} /> : null}
              {detailApp ? <AppDetail app={detailApp} entry={catalog.find((entry) => entry.app.id === detailApp.catalog_id)} update={updates.find((update) => update.app_id === detailApp.id)}
                isLaunching={operation === LAUNCH_OPERATION} disabled={disabled} onBack={goHome} onLaunch={onLaunch} onPermissions={onPermissions} onIcon={onIcon} onRemove={onRemove} onOpenUrl={onOpenUrl} /> : null}
              {view.name === "discover" ? <CatalogView catalog={filteredCatalog} apps={apps} isPreview={!backend.isTauri} onBack={goHome} onOpen={onOpenUrl} /> : null}
              {view.name === "updates" ? <UpdatesView report={updateReport} isChecking={isCheckingUpdates || disabled} onBack={goHome} onCheck={() => void controller.checkUpdates()} onOpen={onOpenUrl} /> : null}
            </>
          )}
        </main>
        <div aria-live="polite" className="fixed bottom-5 inset-x-0 flex justify-center pointer-events-none">
          {statusText ? <p role="status" className="menu flex items-center gap-2 rounded-md px-4 h-10 text-[14px]"><Loader2 size={15} className="animate-spin" />{statusText}…</p> : null}
        </div>
      </div>
      {reviewingApp && !iconApp && !showFolders && !removingApp ? <PermissionsDialog key={reviewingApp.id} app={reviewingApp} catalog={catalog.find((entry) => entry.app.id === reviewingApp.catalog_id)} backend={backend} launchAfterSave={launchAfterReview} onClose={controller.dismissReview} onSave={(config, launch) => controller.configure(reviewingApp.id, config, launch)} isSaving={operation !== null} error={error} returnFocusElement={returnFocusElement} /> : null}
      {iconApp ? <IconDialog key={iconApp.id} app={iconApp} backend={backend} onClose={() => setIconId(null)} onSave={(source) => controller.setIcon(iconApp.id, source)} isSaving={operation !== null} error={error} returnFocusElement={returnFocusElement} /> : null}
      {showFolders && settings ? <FoldersDialog returnFocusElement={returnFocusElement} pickFolder={backend.pickFolder} isNative={backend.isTauri} initial={settings} onClose={() => setShowFolders(false)} onSaved={controller.saveFolders} isSaving={operation !== null} error={error} /> : null}
      {removingApp ? <RemoveDialog returnFocusElement={returnFocusElement} app={removingApp} onClose={() => setRemovingId(null)} onRemove={(deleteFile) => controller.remove(removingApp.id, deleteFile)} isRemoving={operation !== null} error={error} /> : null}
    </>
  );
}

export default function App({ backend = api }: { backend?: AppApi }) {
  return <ThemeProvider><IconBackendContext.Provider value={backend}><Shell backend={backend} /></IconBackendContext.Provider></ThemeProvider>;
}
