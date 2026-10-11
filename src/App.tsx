import { useCallback, useEffect, useState } from "react";
import { api, type AppApi } from "@/lib/api";
import { useLibrary } from "@/lib/use-library";
import { useAppLayout } from "@/lib/use-app-layout";
import { useTileDrag } from "@/lib/use-tile-drag";
import { transitionTo } from "@/lib/view-transition";
import { ThemeProvider } from "@/components/theme";
import { Header } from "@/components/header";
import { Bento, BentoSkeleton, tileSelector } from "@/components/bento";
import { Sidebar, type SidebarPage } from "@/components/sidebar";
import { StatusBar } from "@/components/status-bar";
import type { TileActions } from "@/components/app-tile";
import type { GroupActions } from "@/components/group-header";
import { AppDetail, STAGE_SELECTOR } from "@/components/app-detail";
import { UpdatesView } from "@/components/updates-view";
import { FoldersDialog } from "@/components/dialogs/folders-dialog";
import { AppSettingsDialog, type SettingsTarget } from "@/components/dialogs/app-settings-dialog";
import { IconBackendContext } from "@/components/icon-backend-context";
import { RemoveDialog } from "@/components/dialogs/remove-dialog";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

/** The app view remembers which group it was opened from so going back returns there. */
type View = { name: "home"; groupId: string | null } | { name: "app"; id: string; groupId: string | null } | { name: "updates" };

const HOME: View = { name: "home", groupId: null };
const LAUNCH_OPERATION = "Launch app";
const DISPLAY_NAMES_KEY = "appbento-display-names";

function sidebarPage(view: View): SidebarPage {
  if (view.name === "home") return view.groupId ? { name: "group", id: view.groupId } : { name: "all" };
  if (view.name === "app") return { name: "other" };
  return { name: view.name };
}

/** Replaces the web view's own context menu (Reload, Inspect…) outside text fields, as a native window would. */
function useNativeContextMenu(isNative: boolean) {
  useEffect(() => {
    if (!isNative) return;
    const preventWebMenu = (event: MouseEvent) => {
      const isEditable = event.target instanceof HTMLElement && event.target.closest("input, textarea, .selectable");
      if (!isEditable) event.preventDefault();
    };
    document.addEventListener("contextmenu", preventWebMenu);
    return () => document.removeEventListener("contextmenu", preventWebMenu);
  }, [isNative]);
}

function Shell({ backend }: { backend: AppApi }) {
  const [view, setView] = useState<View>(HOME);
  const [query, setQuery] = useState("");
  const [showFolders, setShowFolders] = useState(false);
  const [returnFocusElement, setReturnFocusElement] = useState<HTMLElement | null>(null);
  const [settingsId, setSettingsId] = useState<string | null>(null);
  const [settingsTarget, setSettingsTarget] = useState<SettingsTarget>("permissions");
  const [displayNames, setDisplayNames] = useState<Record<string, string>>(() => {
    try { return JSON.parse(localStorage.getItem(DISPLAY_NAMES_KEY) ?? "{}"); } catch { return {}; }
  });
  const [removingId, setRemovingId] = useState<string | null>(null);
  const [hasEntered, setHasEntered] = useState(false);
  const [editingGroupId, setEditingGroupId] = useState<string | null>(null);
  const { apps, settings, updateReport, scanWarnings, isLoading, isCheckingUpdates, operation, error, reviewIds, launchAfterReview, controller } = useLibrary(backend);
  const markEntered = useCallback(() => setHasEntered(true), []);
  const installedIds = [...apps].sort((first, second) => first.name.localeCompare(second.name)).map((app) => app.id);
  const { layout, error: layoutError, actions: layoutActions } = useAppLayout(installedIds);
  const { drag, startDrag, suppressClickAfterDrag } = useTileDrag(layout, (appId, target) => layoutActions.moveApp(appId, target.sectionId, target.index));
  useNativeContextMenu(backend.isTauri);
  const filter = query.toLowerCase();
  const displayApps = apps.map((app) => ({ ...app, name: displayNames[app.id] || app.name }));
  const filteredApps = displayApps.filter((app) => app.name.toLowerCase().includes(filter));
  const updates = updateReport?.updates ?? [];
  const confirmedUpdateCount = updates.filter((update) => update.status === "available").length;
  const disabled = isLoading || operation !== null;
  const removingApp = apps.find((app) => app.id === removingId);
  const reviewingApp = apps.find((app) => app.id === reviewIds[0]);
  const detailApp = view.name === "app" ? displayApps.find((app) => app.id === view.id) : undefined;
  const settingsApp = displayApps.find((app) => app.id === settingsId);
  const configuringApp = settingsApp ?? (reviewingApp ? displayApps.find((app) => app.id === reviewingApp.id) : undefined);
  const configuringOriginalName = apps.find((app) => app.id === configuringApp?.id)?.name ?? "";
  const hasModal = !!settingsApp || showFolders || !!removingApp || !!reviewingApp;
  const captureFocus = () => setReturnFocusElement(document.activeElement instanceof HTMLElement ? document.activeElement : null);
  const onLaunch = (id: string) => { captureFocus(); void controller.launch(id); };
  const openSettings = (id: string, target: SettingsTarget) => {
    captureFocus();
    controller.dismissError();
    setSettingsTarget(target);
    setSettingsId(id);
  };
  const onSettings = (id: string) => openSettings(id, "general");
  const onIcon = (id: string) => openSettings(id, "icon");
  const onUpdateSource = (id: string) => openSettings(id, "updates");
  const saveDisplayName = (id: string, originalName: string, name: string) => {
    const next = { ...displayNames };
    if (name === originalName) delete next[id];
    else next[id] = name;
    setDisplayNames(next);
    localStorage.setItem(DISPLAY_NAMES_KEY, JSON.stringify(next));
  };
  const onRemove = (id: string) => {
    captureFocus();
    controller.dismissError();
    setRemovingId(id);
  };
  const activeGroupId = view.name === "home" && layout.groups.some((group) => group.id === view.groupId) ? view.groupId : null;
  const goHome = () => {
    if (view.name !== "app") {
      transitionTo(() => setView(HOME));
      return;
    }
    transitionTo(() => setView({ name: "home", groupId: view.groupId }), { fromSelector: STAGE_SELECTOR, toSelector: tileSelector(view.id) });
  };
  const goTo = (next: View) => transitionTo(() => setView(next));
  const openApp = (id: string) => transitionTo(() => setView({ name: "app", id, groupId: activeGroupId }), { fromSelector: tileSelector(id), toSelector: STAGE_SELECTOR });
  const createGroup = (appIds: string[] = []) => {
    setEditingGroupId(layoutActions.addGroup(appIds));
    setView(HOME);
  };
  const tileActions: TileActions = {
    onOpen: openApp, onLaunch, onSettings, onIcon, onRemove,
    onShift: layoutActions.shiftApp,
    onMoveTo: (id, sectionId) => layoutActions.moveApp(id, sectionId, Number.MAX_SAFE_INTEGER),
    onNewGroup: (id) => createGroup([id]),
    onDragStart: startDrag,
  };
  const groupActions: GroupActions = {
    onToggle: layoutActions.toggleGroup, onRename: layoutActions.renameGroup, onShift: layoutActions.shiftGroup, onEdit: setEditingGroupId,
    onDelete: (groupId) => {
      layoutActions.deleteGroup(groupId);
      if (activeGroupId === groupId) setView(HOME);
    },
  };
  const statusText = operation ?? (isCheckingUpdates ? "Checking for updates" : null);

  return (
    <>
      <div className={cn("h-full flex flex-col", drag && "is-dragging")} inert={hasModal} onClickCapture={suppressClickAfterDrag}>
        <div className="flex-1 min-h-0 flex">
          <Sidebar page={sidebarPage(view)} appCount={apps.length} groups={layout.groups} updateCount={confirmedUpdateCount} dropSectionId={drag?.target.sectionId ?? null}
            onHome={() => goTo(HOME)} onGroup={(groupId) => goTo({ name: "home", groupId })} onNewGroup={() => createGroup()}
            onUpdates={() => goTo({ name: "updates" })} />
          <div className="flex-1 min-w-0 flex flex-col">
            <Header isNative={backend.isTauri} query={query} onQuery={setQuery} disabled={disabled} canEditFolders={settings !== null}
              onFolders={() => {
                captureFocus();
                controller.dismissError();
                setShowFolders(true);
              }}
              onAdd={() => { captureFocus(); void controller.integrate(); }} onScan={() => { captureFocus(); void controller.scan(); }} />
            <main data-scroll-container className="flex-1 min-h-0 overflow-y-auto">
              <div className="px-6 pt-5 pb-10 max-w-[1600px]">
                {error ? <div role="alert" className="mb-4 flex items-center gap-3 rounded-lg border border-[var(--bad)] p-4 text-[var(--bad)]">
                  <span className="flex-1 selectable">{error}</span><Button variant="outline" onClick={controller.dismissError}>Dismiss</Button><Button variant="outline" disabled={disabled} onClick={() => void controller.reload()}>Reload</Button>
                </div> : null}
                {scanWarnings.length ? <section aria-label="Scan warnings" className="mb-4 rounded-lg border border-[var(--warn)] p-4">
                  <div className="flex items-center justify-between gap-3"><h2 className="text-[var(--warn)] font-medium">Some files were skipped</h2><Button variant="outline" size="sm" onClick={controller.dismissScanWarnings}>Dismiss scan warnings</Button></div>
                  <ul className="text-[13px] text-[var(--muted)] mt-2 selectable">{scanWarnings.map((warning) => <li key={warning.path} className="break-all">{warning.path}: {warning.message}</li>)}</ul>
                </section> : null}
                {isLoading ? <BentoSkeleton /> : (
                  <>
                    {view.name === "home" || (view.name === "app" && !detailApp) ? <Bento layout={layout} apps={filteredApps} updates={updates} activeGroupId={activeGroupId}
                      isFiltering={filter.length > 0} drag={drag} editingGroupId={editingGroupId} isAnimated={!hasEntered} onEntered={markEntered}
                      tileActions={tileActions} groupActions={groupActions} disabled={disabled} /> : null}
                    {detailApp ? <AppDetail app={detailApp} update={updates.find((update) => update.app_id === detailApp.id)}
                      isLaunching={operation === LAUNCH_OPERATION} disabled={disabled} onBack={goHome} onLaunch={onLaunch} onSettings={onSettings} onIcon={onIcon} onRemove={onRemove} onUpdateSource={onUpdateSource} onCheckUpdate={(id) => void controller.checkAppUpdates(id)} onInstallUpdate={(id) => void controller.installUpdate(id)} isCheckingUpdates={isCheckingUpdates} /> : null}
                    {view.name === "updates" ? <UpdatesView report={updateReport} apps={apps} disabled={disabled} isChecking={isCheckingUpdates} onBack={goHome} onCheck={() => void controller.checkUpdates()} onInstall={(id) => void controller.installUpdate(id)} /> : null}
                  </>
                )}
              </div>
            </main>
          </div>
        </div>
        <StatusBar status={statusText} appCount={apps.length} groupCount={layout.groups.length} warning={layoutError} />
      </div>
      {configuringApp && !showFolders && !removingApp ? <AppSettingsDialog key={configuringApp.id} app={configuringApp} backend={backend}
        openTo={settingsApp ? settingsTarget : "permissions"} originalName={configuringOriginalName} launchAfterSave={launchAfterReview}
        onClose={() => { setSettingsId(null); if (reviewingApp) controller.dismissReview(); }} onSaved={() => setSettingsId(null)}
        onSavePermissions={(config, launch) => controller.configure(configuringApp.id, config, launch)} onSaveUpdates={(config) => controller.configureUpdates(configuringApp.id, config)}
        onSaveIcon={(source) => controller.setIcon(configuringApp.id, source)} onSaveName={(name) => saveDisplayName(configuringApp.id, configuringOriginalName, name)}
        isSaving={operation !== null} error={error} returnFocusElement={returnFocusElement} /> : null}
      {showFolders && settings ? <FoldersDialog returnFocusElement={returnFocusElement} pickFolder={backend.pickFolder} isNative={backend.isTauri} initial={settings} onClose={() => setShowFolders(false)} onSaved={controller.saveFolders} isSaving={operation !== null} error={error} /> : null}
      {removingApp ? <RemoveDialog returnFocusElement={returnFocusElement} app={removingApp} onClose={() => setRemovingId(null)} onRemove={(deleteFile) => controller.remove(removingApp.id, deleteFile)} isRemoving={operation !== null} error={error} /> : null}
    </>
  );
}

export default function App({ backend = api }: { backend?: AppApi }) {
  return <ThemeProvider><IconBackendContext.Provider value={backend}><Shell backend={backend} /></IconBackendContext.Provider></ThemeProvider>;
}
