import { Profiler } from "react";
import { beforeEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "../src/App";
import { createPreviewApi, createPreviewApp } from "../src/lib/preview";
import type { AppApi } from "../src/lib/api";
import type { UpdateReport } from "../src/lib/types";
import { chooseFromMenu, chooseOption } from "./menu-helpers";

const emptyReport: UpdateReport = { updates: [], errors: [], checked_count: 0, unknown_versions: [], unsupported_count: 0 };

async function openHome(backend: AppApi) {
  const user = userEvent.setup();
  render(<App backend={backend} />);
  await screen.findByRole("heading", { name: "All apps" });
  return user;
}

describe("library management", () => {
  it("creates a menu shortcut from a card and removes it from Integration settings", async () => {
    const preview = createPreviewApi();
    const user = await openHome(preview);
    const original = (await preview.libraryList())[0];
    await chooseFromMenu(user, "More options for Obsidian", "Add to app menu");
    await waitFor(async () => expect((await preview.libraryList())[0].desktop_id).not.toBeNull());
    await chooseFromMenu(user, "More options for Obsidian", "Settings");
    const dialog = screen.getByRole("dialog", { name: "Settings for Obsidian" });
    await user.click(within(dialog).getByRole("button", { name: "Integration" }));
    expect(within(dialog).getByText("This app has a shortcut in your system app menu.")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Remove from app menu" }));
    expect(await within(dialog).findByText("This app has no shortcut in your system app menu.")).toBeInTheDocument();
    expect((await preview.libraryList())[0]).toEqual(original);
    await user.click(within(dialog).getByRole("button", { name: "Add to app menu" }));
    expect(await within(dialog).findByText("This app has a shortcut in your system app menu.")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Close settings" }));
    await chooseFromMenu(user, "More options for Obsidian", "Remove from app menu");
    await waitFor(async () => expect((await preview.libraryList())[0]).toEqual(original));
  });

  it("shows menu integration failures without claiming a shortcut was created", async () => {
    const user = await openHome({ ...createPreviewApi(), async librarySetMenuEntry() { throw new Error("Permission denied"); } });
    await chooseFromMenu(user, "More options for Obsidian", "Settings");
    const dialog = screen.getByRole("dialog", { name: "Settings for Obsidian" });
    await user.click(within(dialog).getByRole("button", { name: "Integration" }));
    await user.click(within(dialog).getByRole("button", { name: "Add to app menu" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Permission denied");
    expect(within(dialog).getByText("This app has no shortcut in your system app menu.")).toBeInTheDocument();
  });

  it("loads local apps while release checking is still pending", async () => {
    const user = await openHome({ ...createPreviewApi(), updatesCheckAll: () => new Promise(() => {}) });
    expect(screen.getByText("Obsidian")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add apps" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "Launch Obsidian" }));
    await chooseFromMenu(user, "More options for Obsidian", "Settings");
    expect(screen.getByText("Launch count").nextElementSibling).toHaveTextContent("1");
  });

  it("keeps failed removal open and displays the error", async () => {
    const user = await openHome({ ...createPreviewApi(), async libraryRemove() { throw new Error("Permission denied"); } });
    await chooseFromMenu(user, "More options for Obsidian", "Remove Obsidian");
    const dialog = screen.getByRole("dialog", { name: "Remove Obsidian?" });
    await user.click(within(dialog).getByRole("button", { name: "Remove entry" }));
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
    await user.click(screen.getByRole("button", { name: "Remove entry" }));
    await waitFor(() => expect(screen.queryByText("Obsidian")).not.toBeInTheDocument());
    expect(choices).toEqual([false]);
    await chooseFromMenu(user, "More options for Joplin", "Remove Joplin");
    await user.click(screen.getByRole("checkbox", { name: "Also delete the AppImage file permanently" }));
    await user.click(screen.getByRole("button", { name: "Delete file and remove" }));
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
    await user.click(screen.getByRole("button", { name: "+ Add folder" }));
    expect(await screen.findByText("/new")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Save and rescan" }));
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
    await user.click(within(dialog).getByRole("button", { name: "Save and rescan" }));
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
  await screen.findByRole("heading", { name: "All apps" });
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

it("leaves card body clicks inactive while keeping launch and settings accessible", async () => {
  const user = await openHome(createPreviewApi());
  const tile = screen.getByRole("article", { name: "Obsidian" });
  expect(within(tile).queryByRole("button", { name: "Open Obsidian" })).not.toBeInTheDocument();
  await user.click(within(tile).getByRole("heading", { name: "Obsidian" }));
  expect(screen.getByRole("heading", { name: "All apps" })).toBeInTheDocument();
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  await chooseFromMenu(user, "More options for Obsidian", "Settings");
  expect(screen.getByText("Launch count").nextElementSibling).toHaveTextContent("0");
});

it("shows portable app status and launch count in General settings", async () => {
  const app = { ...createPreviewApp("Portable", "1.0"), integrated: false, launch_count: 3 };
  const user = await openHome({ ...createPreviewApi(), async libraryList() { return [app]; }, async updatesCheckAll() { return emptyReport; } });
  await chooseFromMenu(user, "More options for Portable", "Settings");
  expect(screen.getByText("No shortcut")).toBeInTheDocument();
  expect(screen.getByText("Launch count").nextElementSibling).toHaveTextContent("3");
});

async function openAppUpdates(user: ReturnType<typeof userEvent.setup>, name: string) {
  await chooseFromMenu(user, `More options for ${name}`, "Settings");
  await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Updates" }));
}

describe("custom app icons", () => {
  const imageUrl = "data:image/png;base64,iVBORw0KGgo=";

  it("saves a pasted URL and displays the stored image in the tile and settings", async () => {
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
    await chooseFromMenu(user, "More options for Obsidian", "Settings");
    await user.click(screen.getByRole("button", { name: "Change icon" }));
    await user.type(screen.getByRole("textbox", { name: "Image URL" }), "https://example.com/icon.png");
    await user.click(screen.getByRole("button", { name: "Save icon" }));
    await waitFor(() => expect(screen.queryByRole("form", { name: "Change icon" })).not.toBeInTheDocument());
    expect(sources).toEqual(["https://example.com/icon.png"]);
    await waitFor(() => expect(screen.getByRole("dialog").querySelector("img")).toHaveAttribute("src", imageUrl));
    await user.click(screen.getByRole("button", { name: "Close settings" }));
    await waitFor(() => expect(document.querySelector("[data-tile-id] img")).toHaveAttribute("src", imageUrl));
    await chooseFromMenu(user, "More options for Obsidian", "Settings");
    await waitFor(() => expect(screen.getByRole("dialog").querySelector("img")).toHaveAttribute("src", imageUrl));
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
    await chooseFromMenu(user, "More options for Obsidian", "Settings");
    await user.click(screen.getByRole("button", { name: "Change icon" }));
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
    await chooseFromMenu(user, "More options for Obsidian", "Settings");
    await user.click(screen.getByRole("button", { name: "Change icon" }));
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
    await openAppUpdates(user, "Joplin");
    expect(screen.getByText("Updates are disabled for this app.")).toBeInTheDocument();
    await chooseOption(user, screen.getByRole("combobox", { name: "Source" }), "Direct HTTPS download");
    await user.type(screen.getByRole("textbox", { name: "Download URL" }), "https://example.org/Joplin.AppImage");
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    await openAppUpdates(user, "Joplin");
    await user.click(screen.getByRole("button", { name: "Check for updates" }));
    await user.click(await screen.findByRole("button", { name: "Install update" }));
    await waitFor(() => expect(screen.queryByRole("button", { name: "Install update" })).not.toBeInTheDocument());
    const installed = (await preview.libraryList()).find((entry) => entry.id === app.id);
    expect(installed?.launch_config).toEqual(app.launch_config);
    expect(installed?.update_config).toEqual({ strategy: "direct-url", url: "https://example.org/Joplin.AppImage" });
    expect(installed?.file_path).toEqual(app.file_path);
    expect(installed?.version_current).toEqual(app.version_current);
    expect((await preview.updatesCheck(app.id)).status).toEqual("up-to-date");
    expect(screen.getByRole("dialog", { name: "Settings for Joplin" })).toBeInTheDocument();
  });

  it("blocks update actions until source edits are saved or reverted", async () => {
    const user = await openHome(createPreviewApi());
    await openAppUpdates(user, "Obsidian");
    const repository = screen.getByRole("textbox", { name: "Repository" });
    const original = "obsidianmd/obsidian-releases";
    await user.clear(repository);
    await user.type(repository, "owner/other");
    expect(screen.getByRole("button", { name: "Check for updates" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Install update" })).toBeDisabled();
    expect(screen.getByText("Save your source changes before checking or installing updates.")).toBeInTheDocument();
    await user.clear(repository);
    await user.type(repository, original);
    expect(screen.getByRole("button", { name: "Check for updates" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Install update" })).toBeEnabled();
  });

  it("reports per-app check failures in settings and allows retry", async () => {
    const user = await openHome({ ...createPreviewApi(), async updatesCheck() { throw new Error("Release server unavailable"); } });
    await openAppUpdates(user, "Obsidian");
    await user.click(screen.getByRole("button", { name: "Check for updates" }));
    const dialog = screen.getByRole("dialog", { name: "Settings for Obsidian" });
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Release server unavailable");
    expect(within(dialog).getByRole("button", { name: "Check for updates" })).toBeEnabled();
  });

  it("offers Install latest in settings when the installed version is unknown", async () => {
    const preview = createPreviewApi();
    const report = await preview.updatesCheckAll();
    report.updates[0] = { ...report.updates[0], current: null, status: "unknown-current" };
    const user = await openHome({ ...preview, async updatesCheckAll() { return report; } });
    await openAppUpdates(user, "Obsidian");
    expect(screen.getByRole("button", { name: "Install latest" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "Install update" })).not.toBeInTheDocument();
  });

  it("disables updates and excludes the app from release checks", async () => {
    const preview = createPreviewApi();
    const user = await openHome(preview);
    await openAppUpdates(user, "Obsidian");
    await chooseOption(user, screen.getByRole("combobox", { name: "Source" }), "Disabled");
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    await openAppUpdates(user, "Obsidian");
    expect(screen.getByText("Updates are disabled for this app.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Check for updates" })).not.toBeInTheDocument();
    expect((await preview.updatesCheckAll()).checked_count).toBe(0);
  });

  it("extracts a pasted GitHub release URL and saves its repository", async () => {
    const preview = createPreviewApi();
    const user = await openHome(preview);
    await openAppUpdates(user, "Joplin");
    await chooseOption(user, screen.getByRole("combobox", { name: "Source" }), "GitHub releases");
    const repository = screen.getByRole("textbox", { name: "Repository" });
    await user.click(repository);
    await user.paste("https://github.com/stablyai/orca/releases");
    expect(repository).toHaveValue("stablyai/orca");
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect((await preview.libraryList()).find((app) => app.name === "Joplin")?.update_config).toMatchObject({ repo: "stablyai/orca" });
  });

  it("retains source inputs and reports a configuration error", async () => {
    const user = await openHome({ ...createPreviewApi(), async updatesConfigure() { throw new Error("Invalid filename matcher"); } });
    await openAppUpdates(user, "Joplin");
    await chooseOption(user, screen.getByRole("combobox", { name: "Source" }), "GitHub releases");
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
  await openAppUpdates(user, "Obsidian");
  await user.click(await screen.findByRole("button", { name: "Install update" }));
  expect(await within(screen.getByRole("dialog")).findByRole("alert")).toHaveTextContent("Download interrupted");
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
    await chooseOption(user, within(dialog).getByRole("combobox", { name: "Source" }), "Direct HTTPS download");
    await user.type(within(dialog).getByRole("textbox", { name: "Download URL" }), "https://example.org/Joplin.AppImage");
    await user.click(within(dialog).getByRole("button", { name: "General" }));
    expect(within(dialog).getByRole("textbox", { name: "Name" })).toHaveValue("Notes");
    await user.click(within(dialog).getByRole("button", { name: "Save changes" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(screen.getByRole("article", { name: "Notes" })).toBeInTheDocument();
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
    expect(await screen.findByRole("article", { name: "Joplin" })).toBeInTheDocument();
  });
});


it("shows only the local library and its update tools", async () => {
  render(<App backend={createPreviewApi()} />);
  await screen.findByRole("heading", { name: "All apps" });
  expect(screen.queryByRole("button", { name: "Discover" })).not.toBeInTheDocument();
  expect(screen.getByRole("textbox", { name: "Filter applications" })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Updates" })).toBeInTheDocument();
});

it("offers add shortcuts in the sidebar only in the desktop app", async () => {
  const user = await openHome({ ...createPreviewApi(), isTauri: true });
  const sidebar = screen.getByRole("navigation", { name: "Library" });
  await user.click(within(sidebar).getByRole("button", { name: "Download from URL" }));
  expect(screen.getByRole("dialog", { name: "Download from URL" })).toBeInTheDocument();
  cleanup();

  await openHome(createPreviewApi());
  const browserSidebar = screen.getByRole("navigation", { name: "Library" });
  expect(within(browserSidebar).getByRole("button", { name: "Add an AppImage" })).toBeDisabled();
  expect(within(browserSidebar).getByRole("button", { name: "Download from URL" })).toBeDisabled();
});

it("downloads an AppImage from the add menu and refreshes the library", async () => {
  const preview = createPreviewApi();
  const imported = createPreviewApp("Downloaded", null);
  const urls: string[] = [];
  let hasImported = false;
  let finishDownload: (() => void) | undefined;
  const user = await openHome({
    ...preview, isTauri: true,
    async libraryAddUrl(url) {
      urls.push(url);
      await new Promise<void>((resolve) => { finishDownload = resolve; });
      hasImported = true;
      return imported;
    },
    async libraryList() { return [...await preview.libraryList(), ...(hasImported ? [imported] : [])]; },
  });
  await chooseFromMenu(user, "Add apps", "Download from URL");
  const dialog = screen.getByRole("dialog", { name: "Download from URL" });
  expect(within(dialog).getByText(/\/preview\/\.local\/share\/appbento\/apps/)).toBeInTheDocument();
  await user.type(within(dialog).getByRole("textbox", { name: "AppImage URL" }), "https://example.org/Downloaded.AppImage");
  await user.click(within(dialog).getByRole("button", { name: "Download and add" }));
  expect(within(dialog).getByRole("status")).toHaveTextContent("Downloading");
  expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeDisabled();
  await user.keyboard("{Escape}");
  expect(dialog).toBeInTheDocument();
  finishDownload?.();
  await screen.findByText("Downloaded");
  expect(screen.queryByRole("dialog", { name: "Download from URL" })).not.toBeInTheDocument();
  expect(urls).toEqual(["https://example.org/Downloaded.AppImage"]);
});

it("validates URL imports and preserves the URL after a failed download", async () => {
  const urls: string[] = [];
  const user = await openHome({ ...createPreviewApi(), isTauri: true, async libraryAddUrl(url) {
    urls.push(url);
    throw new Error("Download failed: 404");
  } });
  await chooseFromMenu(user, "Add apps", "Download from URL");
  const dialog = screen.getByRole("dialog", { name: "Download from URL" });
  const input = within(dialog).getByRole("textbox", { name: "AppImage URL" });
  await user.type(input, "http://example.org/App.AppImage");
  await user.click(within(dialog).getByRole("button", { name: "Download and add" }));
  expect(within(dialog).getByRole("alert")).toHaveTextContent("HTTPS");
  expect(urls).toEqual([]);
  await user.clear(input);
  await user.type(input, "https://example.org/App.AppImage");
  await user.click(within(dialog).getByRole("button", { name: "Download and add" }));
  expect(await within(dialog).findByRole("alert")).toHaveTextContent("Download failed: 404");
  expect(input).toHaveValue("https://example.org/App.AppImage");
  expect(within(dialog).getByRole("button", { name: "Download and add" })).toBeEnabled();
});
