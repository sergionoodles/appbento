import { Profiler } from "react";
import { beforeEach, describe, expect, it } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "../src/App";
import { createPreviewApi, createPreviewApp } from "../src/lib/preview";
import type { AppApi } from "../src/lib/api";
import type { UpdateReport } from "../src/lib/types";
import { chooseFromMenu } from "./menu-helpers";

const emptyReport: UpdateReport = { updates: [], errors: [], checked_count: 0, unknown_versions: [], unsupported_count: 0 };

async function openHome(backend: AppApi) {
  const user = userEvent.setup();
  render(<App backend={backend} />);
  await screen.findByRole("heading", { name: "Recent applications" });
  return user;
}

describe("library management", () => {
  it("loads local apps while release checking is still pending", async () => {
    const user = await openHome({ ...createPreviewApi(), updatesCheckAll: () => new Promise(() => {}) });
    expect(screen.getByText("Obsidian")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add apps" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "Launch Obsidian" }));
    await user.click(screen.getByRole("button", { name: "Open Obsidian" }));
    expect(await screen.findByText("Launched once")).toBeInTheDocument();
  });

  it("keeps failed removal open and displays the error", async () => {
    const user = await openHome({ ...createPreviewApi(), async libraryRemove() { throw new Error("Permission denied"); } });
    await chooseFromMenu(user, "More options for Obsidian", "Remove Obsidian");
    const dialog = screen.getByRole("dialog", { name: "Remove Obsidian?" });
    await user.click(within(dialog).getByRole("button", { name: "remove entry" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Permission denied");
    expect(screen.getByText("Obsidian")).toBeInTheDocument();
  });

  it("retains files by default and requires an explicit checkbox to delete", async () => {
    const preview = createPreviewApi();
    const choices: boolean[] = [];
    const user = await openHome({ ...preview, async libraryRemove(id, deleteFile) {
      choices.push(deleteFile);
      await preview.libraryRemove(id, deleteFile);
    } });
    await chooseFromMenu(user, "More options for Obsidian", "Remove Obsidian");
    await user.click(screen.getByRole("button", { name: "remove entry" }));
    await waitFor(() => expect(screen.queryByText("Obsidian")).not.toBeInTheDocument());
    expect(choices).toEqual([false]);
    await chooseFromMenu(user, "More options for Joplin", "Remove Joplin");
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
    const user = await openHome(backend);
    await chooseFromMenu(user, "Settings", "Folders");
    await user.click(screen.getByRole("button", { name: "+ add folder" }));
    expect(await screen.findByText("/new")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "save + rescan" }));
    await screen.findByText("Discovered");
    expect(screen.queryByRole("dialog", { name: "Scan and managed folders" })).not.toBeInTheDocument();
    expect(screen.queryByRole("dialog", { name: "Settings for Discovered" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Launch Discovered" }));
    const permissions = screen.getByRole("dialog", { name: "Settings for Discovered" });
    expect(within(permissions).getByRole("radio", { name: "Run without restrictions" })).toBeChecked();
    expect((await preview.settingsGet()).scan_dirs).toEqual(["/new"]);
  });

  it("failed settings saves keep user input visible and report the failure", async () => {
    const user = await openHome({ ...createPreviewApi(), async settingsSet() { throw new Error("Folder is not writable"); } });
    await chooseFromMenu(user, "Settings", "Folders");
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
    const user = await openHome({ ...preview, async updatesCheckAll() { return report; } });
    expect(await screen.findByText(/Version unknown/)).toHaveTextContent("Runs as is");
    await user.click(screen.getByRole("button", { name: "Updates" }));
    expect(screen.getByText("current version unknown")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Install latest" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Install update" })).not.toBeInTheDocument();
  });
});

it("partial scan success keeps discovered apps usable and displays skipped-file warnings", async () => {
  const preview = createPreviewApi();
  const user = await openHome({ ...preview, async libraryScan() {
    return { apps: await preview.libraryList(), warnings: [{ path: "/bad.AppImage", message: "Invalid executable format" }] };
  } });
  await chooseFromMenu(user, "Add apps", "Scan folders");
  const warnings = await screen.findByRole("region", { name: "Scan warnings" });
  expect(warnings).toHaveTextContent("Some files were skipped");
  expect(warnings).toHaveTextContent("/bad.AppImage: Invalid executable format");
  expect(screen.getByRole("button", { name: "Launch Obsidian" })).toBeEnabled();
  await user.click(screen.getByRole("button", { name: "Dismiss scan warnings" }));
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
  const settings = screen.getByRole("button", { name: "Settings" });
  await chooseFromMenu(user, "Settings", "Folders");
  expect(screen.getByRole("textbox", { name: "Managed folder" })).toHaveFocus();
  await user.keyboard("{Escape}");
  expect(settings).toHaveFocus();
  const remove = screen.getByRole("button", { name: "More options for Obsidian" });
  await chooseFromMenu(user, "More options for Obsidian", "Remove Obsidian");
  await user.keyboard("{Escape}");
  expect(remove).toHaveFocus();
});

it("opens an app into its launch view and returns to the grid", async () => {
  const user = await openHome(createPreviewApi());
  await user.click(screen.getByRole("button", { name: "Open Obsidian" }));
  expect(await screen.findByRole("heading", { name: "Obsidian" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Launch" })).toBeEnabled();
  expect(screen.getByRole("button", { name: "Settings for Obsidian" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Back to all apps" }));
  expect(await screen.findByRole("button", { name: "Open Obsidian" })).toBeInTheDocument();
});

describe("custom app icons", () => {
  const imageUrl = "data:image/png;base64,iVBORw0KGgo=";

  it("saves a pasted URL and displays the stored image in the tile and detail view", async () => {
    const preview = createPreviewApi();
    const sources: string[] = [];
    const user = await openHome({
      ...preview, isTauri: true,
      async librarySetIcon(id, source) {
        sources.push(source);
        const app = (await preview.libraryList()).find((entry) => entry.id === id);
        if (!app) throw new Error("Missing app");
        return { ...app, icon_path: "/data/icons/custom.png" };
      },
      async libraryIcon() { return imageUrl; },
    });
    await chooseFromMenu(user, "More options for Obsidian", "Change icon");
    await user.type(screen.getByRole("textbox", { name: "Image URL" }), "https://example.com/icon.png");
    await user.click(screen.getByRole("button", { name: "Save icon" }));
    await waitFor(() => expect(screen.queryByRole("form", { name: "Change icon" })).not.toBeInTheDocument());
    expect(sources).toEqual(["https://example.com/icon.png"]);
    await waitFor(() => expect(screen.getByRole("dialog").querySelector("img")).toHaveAttribute("src", imageUrl));
    await user.click(screen.getByRole("button", { name: "Close settings" }));
    await waitFor(() => expect(document.querySelector("[data-tile-id] img")).toHaveAttribute("src", imageUrl));
    await user.click(screen.getByRole("button", { name: "Open Obsidian" }));
    await waitFor(() => expect(document.querySelector("[data-stage] img")).toHaveAttribute("src", imageUrl));
  });

  it("uses the selected local file and leaves the dialog open on save failure", async () => {
    const sources: string[] = [];
    const user = await openHome({
      ...createPreviewApi(), isTauri: true,
      async pickIcon() { return "/home/user/Pictures/icon.png"; },
      async librarySetIcon(_id, source) {
        sources.push(source);
        throw new Error("Invalid image");
      },
    });
    await user.click(screen.getByRole("button", { name: "Open Obsidian" }));
    await chooseFromMenu(user, "More options for Obsidian", "Change icon");
    await user.type(screen.getByRole("textbox", { name: "Image URL" }), "https://example.com/unused.png");
    await user.click(screen.getByRole("button", { name: "Choose local image" }));
    expect(await screen.findByText("Selected: /home/user/Pictures/icon.png")).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Image URL" })).toHaveValue("");
    await user.click(screen.getByRole("button", { name: "Save icon" }));
    const dialog = screen.getByRole("dialog", { name: "Settings for Obsidian" });
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Invalid image");
    expect(sources).toEqual(["/home/user/Pictures/icon.png"]);
  });

  it("keeps the pasted URL when the file picker is cancelled and reports picker failures", async () => {
    let shouldFail = false;
    const user = await openHome({
      ...createPreviewApi(), isTauri: true,
      async pickIcon() {
        if (shouldFail) throw new Error("Picker unavailable");
        return null;
      },
    });
    await chooseFromMenu(user, "More options for Obsidian", "Change icon");
    const input = screen.getByRole("textbox", { name: "Image URL" });
    await user.type(input, "https://example.com/icon.png");
    await user.click(screen.getByRole("button", { name: "Choose local image" }));
    expect(input).toHaveValue("https://example.com/icon.png");
    shouldFail = true;
    await user.click(screen.getByRole("button", { name: "Choose local image" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Picker unavailable");
  });
});


describe("app update sources", () => {
  it("configures a direct source, checks it, and installs while retaining settings", async () => {
    const preview = createPreviewApi();
    const app = (await preview.libraryList())[1];
    const user = await openHome(preview);
    await user.click(screen.getByRole("button", { name: "Open Joplin" }));
    expect(screen.getByText("Updates are disabled for this app.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Configure updates" }));
    await user.selectOptions(screen.getByRole("combobox", { name: "Source" }), "direct-url");
    await user.type(screen.getByRole("textbox", { name: "Download URL" }), "https://example.org/Joplin.AppImage");
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    await user.click(screen.getByRole("button", { name: "Check for updates" }));
    await user.click(await screen.findByRole("button", { name: "Install update" }));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Install update" })).not.toBeInTheDocument());
    const installed = (await preview.libraryList()).find((entry) => entry.id === app.id);
    expect(installed?.launch_config).toEqual(app.launch_config);
    expect(installed?.update_config).toEqual({ strategy: "direct-url", url: "https://example.org/Joplin.AppImage" });
    expect(installed?.file_path).toEqual(app.file_path);
    expect(installed?.version_current).toEqual(app.version_current);
    expect((await preview.updatesCheck(app.id)).status).toEqual("up-to-date");
    expect(await screen.findByRole("heading", { name: "Joplin" })).toBeInTheDocument();
  });

  it("disables updates and excludes the app from release checks", async () => {
    const preview = createPreviewApi();
    const user = await openHome(preview);
    await user.click(screen.getByRole("button", { name: "Open Obsidian" }));
    await chooseFromMenu(user, "More options for Obsidian", "Update source");
    await user.selectOptions(screen.getByRole("combobox", { name: "Source" }), "disabled");
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(screen.getByText("Updates are disabled for this app.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Check for updates" })).not.toBeInTheDocument();
    expect((await preview.updatesCheckAll()).checked_count).toBe(0);
  });

  it("retains source inputs and reports a configuration error", async () => {
    const user = await openHome({ ...createPreviewApi(), async updatesConfigure() { throw new Error("Invalid filename matcher"); } });
    await user.click(screen.getByRole("button", { name: "Open Joplin" }));
    await user.click(screen.getByRole("button", { name: "Configure updates" }));
    await user.selectOptions(screen.getByRole("combobox", { name: "Source" }), "github-release");
    await user.type(screen.getByRole("textbox", { name: "Repository" }), "owner/app");
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    const dialog = screen.getByRole("dialog", { name: "Settings for Joplin" });
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Invalid filename matcher");
    expect(within(dialog).getByRole("textbox", { name: "Repository" })).toHaveValue("owner/app");
  });
});


it("failed update installation keeps the installed version visible and reports the error", async () => {
  const preview = createPreviewApi();
  const original = (await preview.libraryList())[0];
  const user = await openHome({ ...preview, async updatesInstall() { throw new Error("Download interrupted"); } });
  await user.click(screen.getByRole("button", { name: "Open Obsidian" }));
  await user.click(await screen.findByRole("button", { name: "Install update" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Download interrupted");
  expect((await preview.libraryList())[0]).toEqual(original);
  expect(screen.getByRole("button", { name: "Install update" })).toBeEnabled();
});

describe("app settings", () => {
  beforeEach(() => localStorage.clear());

  it("saves edits from several pages with one save", async () => {
    const preview = createPreviewApi();
    const user = await openHome(preview);
    await chooseFromMenu(user, "More options for Joplin", "Settings");
    const dialog = screen.getByRole("dialog", { name: "Settings for Joplin" });
    expect(within(dialog).getByRole("button", { name: "Save changes" })).toBeDisabled();

    const name = within(dialog).getByRole("textbox", { name: "Name" });
    await user.clear(name);
    await user.type(name, "Notes");
    await user.click(within(dialog).getByRole("button", { name: "Updates" }));
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Source" }), "direct-url");
    await user.type(within(dialog).getByRole("textbox", { name: "Download URL" }), "https://example.org/Joplin.AppImage");
    await user.click(within(dialog).getByRole("button", { name: "General" }));
    expect(within(dialog).getByRole("textbox", { name: "Name" })).toHaveValue("Notes");
    await user.click(within(dialog).getByRole("button", { name: "Save changes" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Open Notes" })).toBeInTheDocument();
    const joplin = (await preview.libraryList()).find((app) => app.name === "Joplin");
    expect(joplin?.update_config).toEqual({ strategy: "direct-url", url: "https://example.org/Joplin.AppImage" });
  });

  it("resets a custom name back to the original", async () => {
    const user = await openHome(createPreviewApi());
    await chooseFromMenu(user, "More options for Joplin", "Settings");
    const name = screen.getByRole("textbox", { name: "Name" });
    await user.clear(name);
    await user.type(name, "Notes");
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    await chooseFromMenu(user, "More options for Notes", "Settings");
    await user.click(screen.getByRole("button", { name: "Reset" }));
    expect(screen.getByRole("textbox", { name: "Name" })).toHaveValue("Joplin");
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    expect(await screen.findByRole("button", { name: "Open Joplin" })).toBeInTheDocument();
  });
});
