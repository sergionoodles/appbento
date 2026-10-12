import assert from "node:assert/strict";
import { test } from "vitest";
import { createLibraryController } from "../src/lib/library-controller.ts";
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

test("changed AppImages reopen review after launch invalidates persisted permissions", async () => {
  const preview = createPreviewApi();
  const apps = await preview.libraryList();
  const original = apps[0];
  let hasChanged = false;
  const controller = createLibraryController({
    ...preview,
    async libraryList() {
      return apps.map((app) => app.id === original.id && hasChanged ? { ...app, launch_config: null } : app);
    },
    async libraryLaunch() {
      hasChanged = true;
      throw new Error("AppImage changed; review permissions again");
    },
  });
  await controller.initialize();
  assert.equal(await controller.launch(original.id), false);
  assert.equal(controller.getState().apps[0].launch_config, null);
  assert.deepEqual(controller.getState().reviewIds, [original.id]);
  assert.match(controller.getState().error ?? "", /AppImage changed; review permissions again/);
});

test("save-and-launch refreshes invalidated permissions without closing review", async () => {
  const preview = createPreviewApi();
  const original = (await preview.libraryList())[0];
  let app = original;
  const controller = createLibraryController({
    ...preview,
    async libraryList() { return [app]; },
    async libraryConfigure(_id, config) {
      app = { ...app, launch_config: config };
      return app;
    },
    async libraryLaunch() {
      app = { ...app, launch_config: null };
      throw new Error("AppImage changed during launch");
    },
  });
  await controller.initialize();
  controller.requestReview(original.id);
  const config = original.launch_config;
  assert.ok(config);
  assert.equal(await controller.configure(original.id, config, true), false);
  assert.equal(controller.getState().apps[0].launch_config, null);
  assert.deepEqual(controller.getState().reviewIds, [original.id]);
  assert.match(controller.getState().error ?? "", /AppImage changed during launch/);
});

test("launch and refresh failures are both reported", async () => {
  const preview = createPreviewApi();
  let isLoaded = false;
  const controller = createLibraryController({
    ...preview,
    async libraryList() {
      if (isLoaded) throw new Error("Cannot read library");
      isLoaded = true;
      return preview.libraryList();
    },
    async libraryLaunch() { throw new Error("Launch failed"); },
  });
  await controller.initialize();
  assert.equal(await controller.launch(controller.getState().apps[0].id), false);
  assert.match(controller.getState().error ?? "", /Launch failed; Library refresh failed: Cannot read library/);
});

test("saving an update source and installing replaces the app without losing settings", async () => {
  const preview = createPreviewApi();
  const controller = createLibraryController(preview);
  await controller.initialize();
  const app = controller.getState().apps[1];
  const config = { strategy: "github-release" as const, repo: "owner/repository", asset_regex: ".*AppImage$", architecture: "x86_64", tag_regex: "^v?(?P<version>.*)$", prerelease: false, channel: "stable" };
  assert.equal(await controller.configureUpdates(app.id, config), true);
  assert.deepEqual(controller.getState().apps[1].update_config, config);
  assert.equal(await controller.checkAppUpdates(app.id), true);
  assert.equal(controller.getState().updateReport?.updates.some((update) => update.app_id === app.id), true);
  assert.equal(await controller.installUpdate(app.id), true);
  const installed = controller.getState().apps[1];
  assert.equal(installed.id, app.id);
  assert.equal(installed.version_current, "1.6.7");
  assert.deepEqual(installed.launch_config, app.launch_config);
  assert.deepEqual(installed.update_config, config);
  assert.equal(controller.getState().apps.length, 2);
});

test("failed update installation preserves the old app and reports the failure", async () => {
  const controller = createLibraryController({ ...createPreviewApi(), async updatesInstall() { throw new Error("Download interrupted"); } });
  await controller.initialize();
  const app = controller.getState().apps[0];
  assert.equal(await controller.installUpdate(app.id), false);
  assert.deepEqual(controller.getState().apps[0], app);
  assert.match(controller.getState().error ?? "", /Download interrupted/);
});


test("failed per-app checks clear stale releases and show the source error", async () => {
  const controller = createLibraryController({ ...createPreviewApi(), async updatesCheck(appId) {
    return { app_id: appId, status: "error", update: null, error: "Repository unavailable" };
  } });
  await controller.initialize();
  const app = controller.getState().apps[0];
  assert.equal(controller.getState().updateReport?.updates.length, 1);
  assert.equal(await controller.checkAppUpdates(app.id), false);
  assert.equal(controller.getState().updateReport?.updates.length, 0);
  assert.match(controller.getState().error ?? "", /Repository unavailable/);
});

test("restored launch settings awaiting review are retained and shown before launch", async () => {
  const preview = createPreviewApi();
  const [configured] = await preview.libraryList();
  const app = { ...configured, reviewed_digest: null };
  const controller = createLibraryController({ ...preview,
    async libraryList() { return [app]; },
    async libraryLaunch() { throw new Error("AppImage changed since review"); },
  });
  await controller.initialize();
  assert.equal(await controller.launch(app.id), false);
  assert.deepEqual(controller.getState().reviewIds, [app.id]);
  assert.deepEqual(controller.getState().apps[0].launch_config, configured.launch_config);
  assert.equal(controller.getState().error, null);
});
