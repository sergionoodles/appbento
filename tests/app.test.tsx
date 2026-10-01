import { Profiler } from "react";
import { describe, expect, it } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "../src/App";
import { createPreviewApi, createPreviewApp } from "../src/lib/preview";
import type { AppApi } from "../src/lib/api";
import type { UpdateReport } from "../src/lib/types";

const emptyReport: UpdateReport = { updates: [], errors: [], checked_count: 0, unknown_versions: [], unsupported_count: 0 };

async function openLibrary(backend: AppApi) {
  const user = userEvent.setup();
  render(<App backend={backend} />);
  await screen.findByRole("heading", { name: "Recent applications" });
  await user.click(screen.getByRole("button", { name: "library" }));
  return user;
}

describe("library management", () => {
  it("loads local apps while release checking is still pending", async () => {
    const user = await openLibrary({ ...createPreviewApi(), updatesCheckAll: () => new Promise(() => {}) });
    expect(screen.getByText("Obsidian")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "scan" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "Launch Obsidian" }));
    await waitFor(() => expect(screen.getByText("×1")).toBeInTheDocument());
  });

  it("keeps failed removal open and displays the error", async () => {
    const user = await openLibrary({ ...createPreviewApi(), async libraryRemove() { throw new Error("Permission denied"); } });
    await user.click(screen.getByRole("button", { name: "Remove Obsidian" }));
    const dialog = screen.getByRole("dialog", { name: "Remove Obsidian?" });
    await user.click(within(dialog).getByRole("button", { name: "remove entry" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Permission denied");
    expect(screen.getByText("Obsidian")).toBeInTheDocument();
  });

  it("retains files by default and requires an explicit checkbox to delete", async () => {
    const preview = createPreviewApi();
    const choices: boolean[] = [];
    const user = await openLibrary({ ...preview, async libraryRemove(id, deleteFile) {
      choices.push(deleteFile);
      await preview.libraryRemove(id, deleteFile);
    } });
    await user.click(screen.getByRole("button", { name: "Remove Obsidian" }));
    await user.click(screen.getByRole("button", { name: "remove entry" }));
    await waitFor(() => expect(screen.queryByText("Obsidian")).not.toBeInTheDocument());
    expect(choices).toEqual([false]);
    await user.click(screen.getByRole("button", { name: "Remove Joplin" }));
    await user.click(screen.getByRole("checkbox", { name: "Also delete the AppImage file permanently" }));
    await user.click(screen.getByRole("button", { name: "delete file + remove" }));
    await waitFor(() => expect(screen.queryByText("Joplin")).not.toBeInTheDocument());
    expect(choices).toEqual([false, true]);
  });

  it("save plus rescan displays apps discovered in the chosen scan folder", async () => {
    const preview = createPreviewApi();
    const app = createPreviewApp("Discovered", null);
    let discovered = false;
    const backend: AppApi = {
      ...preview, isTauri: true,
      async pickFolder() { return "/new"; },
      async libraryScan() {
        const settings = await preview.settingsGet();
        discovered = settings.scan_dirs.includes("/new");
        return { apps: discovered ? [app] : [], warnings: [] };
      },
      async libraryList() { return discovered ? [app] : []; },
      async updatesCheckAll() { return emptyReport; },
    };
    const user = await openLibrary(backend);
    await user.click(screen.getByRole("button", { name: "folders" }));
    await user.click(screen.getByRole("button", { name: "+ add folder" }));
    expect(await screen.findByText("/new")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "save + rescan" }));
    await screen.findByText("Discovered");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect((await preview.settingsGet()).scan_dirs).toEqual(["/new"]);
  });

  it("failed settings saves keep user input visible and report the failure", async () => {
    const user = await openLibrary({ ...createPreviewApi(), async settingsSet() { throw new Error("Folder is not writable"); } });
    await user.click(screen.getByRole("button", { name: "folders" }));
    const dialog = screen.getByRole("dialog", { name: "Scan and managed folders" });
    const managed = within(dialog).getByRole("textbox", { name: "Managed folder" });
    await user.clear(managed);
    await user.type(managed, "/restricted");
    await user.click(within(dialog).getByRole("button", { name: "save + rescan" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Folder is not writable");
    expect(managed).toHaveValue("/restricted");
  });

  it("distinguishes unknown installed versions from confirmed updates", async () => {
    const preview = createPreviewApi();
    const report = await preview.updatesCheckAll();
    report.updates[0] = { ...report.updates[0], current: null, status: "unknown-current" };
    report.unknown_versions = [report.updates[0].app_id];
    const user = await openLibrary({ ...preview, async updatesCheckAll() { return report; } });
    expect(await screen.findByLabelText("Current version unknown")).toBeInTheDocument();
    expect(screen.getByText("0 confirmed updates")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "updates" }));
    expect(screen.getByText("current version unknown")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "download latest" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "download update" })).not.toBeInTheDocument();
  });
});

it("partial scan success keeps discovered apps usable and displays skipped-file warnings", async () => {
  const preview = createPreviewApi();
  const user = await openLibrary({ ...preview, async libraryScan() {
    return { apps: await preview.libraryList(), warnings: [{ path: "/bad.AppImage", message: "Invalid executable format" }] };
  } });
  await user.click(screen.getByRole("button", { name: "scan" }));
  const warnings = await screen.findByRole("region", { name: "Scan warnings" });
  expect(warnings).toHaveTextContent("Some files were skipped");
  expect(warnings).toHaveTextContent("/bad.AppImage: Invalid executable format");
  expect(screen.getByRole("button", { name: "Launch Obsidian" })).toBeEnabled();
  await user.click(screen.getByRole("button", { name: "dismiss scan warnings" }));
  expect(screen.queryByRole("region", { name: "Scan warnings" })).not.toBeInTheDocument();
});

it("restores the actual app trigger after inert clears focus during dialog commit", async () => {
  const user = userEvent.setup();
  render(<Profiler id="inert-focus-simulation" onRender={() => {
    const dialog = document.querySelector('[role="dialog"]');
    const focused = document.activeElement;
    if (dialog && focused instanceof HTMLElement && !dialog.contains(focused)) focused.blur();
  }}><App backend={createPreviewApi()} /></Profiler>);
  await screen.findByRole("heading", { name: "Recent applications" });
  const folders = screen.getByRole("button", { name: "folders" });
  await user.click(folders);
  expect(screen.getByRole("textbox", { name: "Managed folder" })).toHaveFocus();
  await user.keyboard("{Escape}");
  expect(folders).toHaveFocus();
  await user.click(screen.getByRole("button", { name: "library" }));
  const remove = screen.getByRole("button", { name: "Remove Obsidian" });
  await user.click(remove);
  await user.keyboard("{Escape}");
  expect(remove).toHaveFocus();
});
