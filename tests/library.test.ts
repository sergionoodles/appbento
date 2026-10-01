import assert from "node:assert/strict";
import { test } from "vitest";
import { createLibraryController, recentApps } from "../src/lib/library-controller.ts";
import { createPreviewApi, createPreviewApp } from "../src/lib/preview.ts";
import type { UpdateReport } from "../src/lib/types.ts";

function deferred<Value>() {
  let resolve: (value: Value) => void = () => { throw new Error("Deferred promise not initialized"); };
  const promise = new Promise<Value>((complete) => { resolve = complete; });
  return { promise, resolve };
}

const emptyReport: UpdateReport = { updates: [], errors: [], unknown_versions: [], checked_count: 0, unsupported_count: 0 };

test("local library appears before network update checks complete", async () => {
  const updates = deferred<UpdateReport>();
  const controller = createLibraryController({ ...createPreviewApi(), updatesCheckAll: () => updates.promise });
  await controller.initialize();
  assert.equal(controller.getState().apps.length, 2);
  assert.equal(controller.getState().isLoading, false);
  assert.equal(controller.getState().isCheckingUpdates, true);
  updates.resolve(emptyReport);
});

test("saving folders discovers apps through the new scan directory", async () => {
  const preview = createPreviewApi();
  const app = createPreviewApp("New app", null);
  let discovered = false;
  const controller = createLibraryController({
    ...preview,
    async libraryScan() {
      const settings = await preview.settingsGet();
      discovered = settings.scan_dirs.includes("/new");
      return { apps: discovered ? [app] : [], warnings: [] };
    },
    async libraryList() { return discovered ? [app] : []; },
  });
  await controller.initialize();
  assert.equal(await controller.saveFolders({ managed_dir: "/managed", scan_dirs: ["/new"] }), true);
  assert.deepEqual(controller.getState().apps, [app]);
});

test("failed saves preserve dialog state through a false result and visible error", async () => {
  const controller = createLibraryController({ ...createPreviewApi(), async settingsSet() { throw new Error("Permission denied"); } });
  await controller.initialize();
  assert.equal(await controller.saveFolders({ managed_dir: "/restricted", scan_dirs: [] }), false);
  assert.match(controller.getState().error ?? "", /Permission denied/);
  assert.equal(controller.getState().operation, null);
});

test("overlapping mutations are refused until the first operation completes", async () => {
  const pendingLaunch = deferred<ReturnType<typeof createPreviewApp>>();
  const preview = createPreviewApi();
  const controller = createLibraryController({ ...preview, libraryLaunch: () => pendingLaunch.promise });
  await controller.initialize();
  const app = controller.getState().apps[0];
  const launch = controller.launch(app.id);
  assert.equal(await controller.remove(app.id, false), false);
  assert.equal(controller.getState().apps.length, 2);
  pendingLaunch.resolve({ ...app, launch_count: 1 });
  assert.equal(await launch, true);
});

test("an older update check cannot repopulate a removed app", async () => {
  const updates = deferred<UpdateReport>();
  const preview = createPreviewApi();
  const report = await preview.updatesCheckAll();
  let hasRequested = false;
  const controller = createLibraryController({ ...preview, updatesCheckAll: () => {
    if (hasRequested) return Promise.resolve(emptyReport);
    hasRequested = true;
    return updates.promise;
  } });
  await controller.initialize();
  await controller.remove(report.updates[0].app_id, false);
  updates.resolve(report);
  await Promise.resolve();
  assert.equal(controller.getState().updateReport?.updates.some((update) => update.app_id === report.updates[0].app_id) ?? false, false);
  assert.equal(controller.getState().apps.length, 1);
});

test("recent apps use last launch time rather than lifetime frequency without mutating input", () => {
  const old = { ...createPreviewApp("Old", null), launch_count: 100, last_launched: "2025-01-01T00:00:00Z" };
  const recent = { ...createPreviewApp("Recent", null), launch_count: 1, last_launched: "2026-01-01T00:00:00Z" };
  const apps = [old, recent];
  assert.deepEqual(recentApps(apps, 1), [recent]);
  assert.deepEqual(apps, [old, recent]);
});

test("launches preserve an update check in progress and do not perform another check", async () => {
  const updates = deferred<UpdateReport>();
  const preview = createPreviewApi();
  const report = await preview.updatesCheckAll();
  const controller = createLibraryController({ ...preview, updatesCheckAll: () => updates.promise });
  await controller.initialize();
  await controller.launch(controller.getState().apps[0].id);
  updates.resolve(report);
  await Promise.resolve();
  assert.deepEqual(controller.getState().updateReport, report);
});

test("a failed newer check clears stale successful release data and shows an error", async () => {
  const preview = createPreviewApi();
  let hasChecked = false;
  const controller = createLibraryController({ ...preview, async updatesCheckAll() {
    if (hasChecked) throw new Error("Network unavailable");
    hasChecked = true;
    return preview.updatesCheckAll();
  } });
  await controller.initialize();
  await controller.checkUpdates();
  assert.equal(controller.getState().updateReport, null);
  assert.match(controller.getState().error ?? "", /Network unavailable/);
});

test("successful scans retain readable apps and surface invalid file warnings", async () => {
  const preview = createPreviewApi();
  const controller = createLibraryController({ ...preview, async libraryScan() {
    return { apps: await preview.libraryList(), warnings: [{ path: "/bad.AppImage", message: "Invalid executable format" }] };
  } });
  await controller.initialize();
  assert.equal(await controller.scan(), true);
  assert.equal(controller.getState().apps.length, 2);
  assert.equal(controller.getState().error, null);
  assert.deepEqual(controller.getState().scanWarnings, [{ path: "/bad.AppImage", message: "Invalid executable format" }]);
});
