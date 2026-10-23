import { expect, it } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "../src/App";
import type { AppApi } from "../src/lib/api";
import { createPreviewApi, createPreviewApp } from "../src/lib/preview";
import type { InstalledApp, UpdateReport } from "../src/lib/types";
import { chooseFromMenu } from "./menu-helpers";

const emptyReport: UpdateReport = { updates: [], errors: [], checked_count: 0, unknown_versions: [], unsupported_count: 0 };
const externalPath = "/Downloads/Example.AppImage";

async function importFixture({ isManaged = false, hasMoveError = false, scanApps = [], openRequest = null }: {
  isManaged?: boolean;
  hasMoveError?: boolean;
  scanApps?: InstalledApp[];
  openRequest?: string | null;
} = {}) {
  const preview = createPreviewApi();
  const settings = await preview.settingsGet();
  const selectedPath = isManaged ? `${settings.managed_dir}/Example.AppImage` : externalPath;
  let apps: InstalledApp[] = [];
  const imports: { path: string; move: boolean }[] = [];
  const backend: AppApi = {
    ...preview, isTauri: true,
    async updatesCheckAll() { return emptyReport; },
    async libraryList() { return structuredClone(apps); },
    async pickAppImage() { return selectedPath; },
    async openRequestTake() {
      const requested = openRequest;
      openRequest = null;
      return requested;
    },
    async libraryScan() {
      apps = structuredClone(scanApps);
      return { apps: structuredClone(apps), warnings: [] };
    },
    async libraryImport(path, move) {
      imports.push({ path, move });
      if (move && hasMoveError) throw new Error("Update destination already exists");
      const current = apps.find((app) => app.file_path === path) ?? {
        ...createPreviewApp("Example", null), id: path, file_path: path, integrated: isManaged,
      };
      const imported = move ? { ...current, file_path: `${settings.managed_dir}/${current.file_name}`, integrated: true } : current;
      apps = [...apps.filter((app) => app.id !== imported.id), imported];
      return structuredClone(imported);
    },
  };
  const user = userEvent.setup();
  render(<App backend={backend} />);
  await screen.findByRole("heading", { name: "All apps" });
  return { user, backend, imports, settings };
}

it("offers to move an external manual import and preserves its library identity", async () => {
  const { user, backend, imports, settings } = await importFixture();
  await chooseFromMenu(user, "Add apps", "Add an AppImage");
  const dialog = await screen.findByRole("dialog", { name: "Move Example to your managed folder?" });
  expect(within(dialog).getByText(externalPath)).toBeInTheDocument();
  expect(within(dialog).getByText(settings.managed_dir)).toBeInTheDocument();
  expect(imports).toEqual([{ path: externalPath, move: false }]);

  await user.click(within(dialog).getByRole("button", { name: "Move to managed folder" }));

  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  const [app] = await backend.libraryList();
  expect(app.id).toBe(externalPath);
  expect(app.file_path).toBe(`${settings.managed_dir}/Example.AppImage`);
  expect(imports).toEqual([{ path: externalPath, move: false }, { path: externalPath, move: true }]);
});

it("imports an AppImage opened with AppBento from the file manager", async () => {
  const { user, imports } = await importFixture({ openRequest: externalPath });
  const dialog = await screen.findByRole("dialog", { name: "Move Example to your managed folder?" });
  expect(imports).toEqual([{ path: externalPath, move: false }]);

  await user.click(within(dialog).getByRole("button", { name: "Keep current location" }));

  expect(await screen.findByText("Example")).toBeInTheDocument();
  expect(imports).toEqual([{ path: externalPath, move: false }]);
});

it("reports an AppImage opened with AppBento that cannot be imported", async () => {
  const preview = createPreviewApi();
  const backend: AppApi = {
    ...preview, isTauri: true,
    async updatesCheckAll() { return emptyReport; },
    async openRequestTake() { return "/Downloads/broken.AppImage"; },
    async libraryImport() { throw new Error("not an AppImage"); },
  };
  render(<App backend={backend} />);

  expect(await screen.findByText(/Add AppImage: not an AppImage/)).toBeInTheDocument();
});

it("keeps an external import in place when the user declines the move", async () => {
  const { user, backend, imports } = await importFixture();
  await chooseFromMenu(user, "Add apps", "Add an AppImage");
  await user.click(await screen.findByRole("button", { name: "Keep current location" }));

  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect((await backend.libraryList())[0].file_path).toBe(externalPath);
  expect(imports).toEqual([{ path: externalPath, move: false }]);
});

it("adds manual imports already in the managed folder without a move prompt", async () => {
  const { user, imports, settings } = await importFixture({ isManaged: true });
  await chooseFromMenu(user, "Add apps", "Add an AppImage");
  await screen.findByText("Example");

  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(imports).toEqual([{ path: `${settings.managed_dir}/Example.AppImage`, move: false }]);
});

it("shows a failed move and keeps both choices available without changing the app path", async () => {
  const { user, backend } = await importFixture({ hasMoveError: true });
  await chooseFromMenu(user, "Add apps", "Add an AppImage");
  const dialog = await screen.findByRole("dialog", { name: "Move Example to your managed folder?" });
  await user.click(within(dialog).getByRole("button", { name: "Move to managed folder" }));

  expect(await within(dialog).findByRole("alert")).toHaveTextContent("destination already exists");
  expect((await backend.libraryList())[0].file_path).toBe(externalPath);
  expect(within(dialog).getByRole("button", { name: "Move to managed folder" })).toBeEnabled();
  await user.click(within(dialog).getByRole("button", { name: "Keep current location" }));
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});

it("offers moves only for newly scanned external apps and does not repeat declined prompts", async () => {
  const managed = createPreviewApp("Managed", null);
  const first = { ...createPreviewApp("First", null), id: "/Downloads/First.AppImage", file_path: "/Downloads/First.AppImage", integrated: false };
  const second = { ...createPreviewApp("Second", null), id: "/Downloads/Second.AppImage", file_path: "/Downloads/Second.AppImage", integrated: false };
  const { user, imports } = await importFixture({ scanApps: [managed, first, second] });
  await chooseFromMenu(user, "Add apps", "Scan folders");
  const firstDialog = await screen.findByRole("dialog", { name: "Move First to your managed folder?" });
  await user.click(within(firstDialog).getByRole("button", { name: "Keep current location" }));
  const secondDialog = await screen.findByRole("dialog", { name: "Move Second to your managed folder?" });
  await user.click(within(secondDialog).getByRole("button", { name: "Keep current location" }));

  await chooseFromMenu(user, "Add apps", "Scan folders");

  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(imports).toEqual([]);
});

it("does not prompt when a scan finds only managed AppImages", async () => {
  const { user, imports } = await importFixture({ scanApps: [createPreviewApp("Managed", null)] });
  await chooseFromMenu(user, "Add apps", "Scan folders");
  await screen.findByText("Managed");
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(imports).toEqual([]);
});
