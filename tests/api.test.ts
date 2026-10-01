import assert from "node:assert/strict";
import { test } from "vitest";
import { api } from "../src/lib/api.ts";

test("native library failures remain errors instead of displaying sample apps", async () => {
  const originalWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
  Object.defineProperty(globalThis, "window", { configurable: true, value: { __TAURI_INTERNALS__: {} } });
  try {
    await assert.rejects(api.libraryList());
  } finally {
    if (originalWindow) Object.defineProperty(globalThis, "window", originalWindow);
    else Reflect.deleteProperty(globalThis, "window");
  }
});

import { createNativeApi, validateExternalUrl } from "../src/lib/api.ts";
import { createPreviewApi } from "../src/lib/preview.ts";

test("all native read failures propagate without fabricating state", async () => {
  const backend = createNativeApi(async () => { throw new Error("IPC unavailable"); });
  for (const read of [backend.libraryList, backend.catalogList, backend.settingsGet, backend.updatesCheckAll]) {
    await assert.rejects(read(), /IPC unavailable/);
  }
});

test("preview removal and launches modify sample state without native commands", async () => {
  const preview = createPreviewApi();
  const [app] = await preview.libraryList();
  const launched = await preview.libraryLaunch(app.id);
  assert.equal(launched.launch_count, app.launch_count + 1);
  await preview.libraryRemove(app.id, false);
  assert.equal((await preview.libraryList()).some((entry) => entry.id === app.id), false);
  assert.equal((await preview.updatesCheckAll()).updates.some((entry) => entry.app_id === app.id), false);
  await assert.rejects(preview.libraryLaunch(app.id), /not found/);
});

test("external URL guard permits HTTPS and rejects dangerous protocols or credentials", () => {
  assert.equal(validateExternalUrl("https://example.org/releases"), "https://example.org/releases");
  for (const url of ["javascript:alert(1)", "file:///etc/passwd", "http://example.org", "https://user:password@example.org", "not a url"]) {
    assert.throws(() => validateExternalUrl(url));
  }
});

import type { Commands, CommandTransport } from "../src/lib/api.ts";
import { createPreviewApp } from "../src/lib/preview.ts";
import { restrictedPermissions } from "../src/lib/sandbox.ts";

test("native sandbox commands preserve config and return backend status", async () => {
  const config = { mode: "bubblewrap" as const, permissions: restrictedPermissions() };
  const app = { ...createPreviewApp("Configured", null), launch_config: config };
  const status = { available: true, version: "0.11", error: null };
  const calls: unknown[] = [];
  const transport: CommandTransport = async <Command extends keyof Commands>(command: Command, args: Commands[Command]["args"]) => {
    calls.push({ command, args });
    return (command === "sandbox_status" ? status : app) as Commands[Command]["result"];
  };
  const backend = createNativeApi(transport);
  assert.deepEqual(await backend.sandboxStatus(), status);
  assert.deepEqual(await backend.libraryConfigure(app.id, config), app);
  assert.deepEqual(calls, [
    { command: "sandbox_status", args: undefined },
    { command: "library_configure", args: { id: app.id, config } },
  ]);
});
