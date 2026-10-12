import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "../src/App";
import { createPreviewApi, createPreviewApp } from "../src/lib/preview";
import type { AppApi } from "../src/lib/api";
import type { InstalledApp, LaunchConfig } from "../src/lib/types";
import { chooseFromMenu, chooseOption } from "./menu-helpers";

function unreviewedBackend(patch: Partial<AppApi> = {}) {
  let app: InstalledApp = createPreviewApp("Obsidian", "1");
  const libraryLaunch = vi.fn(async () => {
    if (!app.launch_config) throw new Error("Review required");
    app = { ...app, launch_count: app.launch_count + 1 };
    return app;
  });
  const libraryConfigure = vi.fn(async (_id: string, config: LaunchConfig) => {
    app = { ...app, launch_config: config };
    return app;
  });
  const backend: AppApi = { ...createPreviewApi(), async libraryList() { return [app]; }, libraryLaunch, libraryConfigure, ...patch };
  return { backend, libraryLaunch, libraryConfigure };
}

async function openReview(backend: AppApi) {
  const user = userEvent.setup();
  render(<App backend={backend} />);
  await screen.findByRole("heading", { name: "All apps" });
  await user.click(screen.getByRole("button", { name: "Launch Obsidian" }));
  const dialog = screen.getByRole("dialog", { name: "Settings for Obsidian" });
  return { user, dialog };
}

async function restrict(user: ReturnType<typeof userEvent.setup>, dialog: HTMLElement) {
  await user.click(within(dialog).getByRole("radio", { name: "Restrict with bubblewrap" }));
  await waitFor(() => expect(within(dialog).getByRole("button", { name: "Recheck bubblewrap" })).toBeEnabled());
}

describe("AppImage permission review", () => {

  it("keeps reviewed configuration visible after a save-and-launch failure", async () => {
    const { backend } = unreviewedBackend({ async libraryLaunch() { throw new Error("Sandbox launch failed"); } });
    const { user, dialog } = await openReview(backend);
    await restrict(user, dialog);
    await waitFor(() => expect(within(dialog).getByRole("button", { name: "Save and launch" })).toBeEnabled());
    await user.click(within(dialog).getByRole("checkbox", { name: /Display/ }));
    await user.click(within(dialog).getByRole("button", { name: "Save and launch" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Sandbox launch failed");
    expect(within(dialog).getByRole("checkbox", { name: /Display/ })).not.toBeChecked();
  });

  it("only asks for a review when an unreviewed app is launched", async () => {
    const { backend, libraryLaunch } = unreviewedBackend({ async libraryList() { return [createPreviewApp("Obsidian", "1"), createPreviewApp("Second", null)]; } });
    const user = userEvent.setup();
    render(<App backend={backend} />);
    await screen.findByRole("heading", { name: "All apps" });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Launch Second" }));
    const dialog = screen.getByRole("dialog", { name: "Settings for Second" });
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(libraryLaunch).not.toHaveBeenCalled();
    expect(screen.getAllByText(/^Needs setup/)).toHaveLength(2);
  });

  it("shows the version of an app that still needs setup", async () => {
    const { backend } = unreviewedBackend();
    render(<App backend={backend} />);
    const tile = await screen.findByRole("article");
    expect(within(tile).getByText("v1")).toBeInTheDocument();
    expect(within(tile).getByText(/^Needs setup/)).toBeInTheDocument();
  });

  it.each([["Add apps", "Scan folders"], ["Add apps", "Add an AppImage"]])("does not open a review for an app discovered by %s > %s", async (menu, item) => {
    const preview = createPreviewApi();
    const app = createPreviewApp("Imported", null);
    let isDiscovered = false;
    const backend: AppApi = {
      ...preview, isTauri: true,
      async libraryList() { return [...await preview.libraryList(), ...(isDiscovered ? [app] : [])]; },
      async libraryScan() { isDiscovered = true; return { apps: [app], warnings: [] }; },
      async pickAppImage() { return app.file_path; },
      async libraryIntegrate() { isDiscovered = true; return app; },
    };
    const user = userEvent.setup();
    render(<App backend={backend} />);
    await screen.findByRole("heading", { name: "All apps" });
    await chooseFromMenu(user, menu, item);
    expect(await screen.findByText("Imported")).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(app.launch_count).toBe(0);
  });

  it("cancellation leaves an app unreviewed and launching asks again before explicit direct launch", async () => {
    const { backend, libraryLaunch, libraryConfigure } = unreviewedBackend();
    const { user, dialog } = await openReview(backend);
    expect(within(dialog).getByRole("radio", { name: "Run without restrictions" })).toBeChecked();
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(libraryLaunch).not.toHaveBeenCalled();
    expect(libraryConfigure).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Launch Obsidian" }));
    const reopened = screen.getByRole("dialog");
    expect(libraryLaunch).not.toHaveBeenCalled();
    expect(within(reopened).getByRole("radio", { name: "Run without restrictions" })).toBeChecked();
    expect(within(reopened).getByText(/normal account access/)).toBeInTheDocument();
    await user.click(within(reopened).getByRole("button", { name: "Save and launch" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(libraryConfigure.mock.calls[0][1].mode).toBe("direct");
    expect(libraryLaunch).toHaveBeenCalledOnce();
    await user.click(screen.getByRole("button", { name: "Open Obsidian" }));
    expect(await screen.findByText("Launched once")).toBeInTheDocument();
  });

  it("blocks unavailable bubblewrap until a successful recheck, then saves restricted permissions without launching", async () => {
    const sandboxStatus = vi.fn().mockResolvedValueOnce({ available: false, version: null, error: "Not installed" }).mockResolvedValue({ available: true, version: "0.11", error: null });
    const { backend, libraryConfigure, libraryLaunch } = unreviewedBackend({ sandboxStatus });
    const user = userEvent.setup();
    render(<App backend={backend} />);
    await screen.findByRole("heading", { name: "All apps" });
    await chooseFromMenu(user, "More options for Obsidian", "Settings");
    const dialog = screen.getByRole("dialog", { name: "Settings for Obsidian" });
    await user.click(within(dialog).getByRole("button", { name: "Permissions" }));
    await user.click(within(dialog).getByRole("radio", { name: "Restrict with bubblewrap" }));
    expect(await within(dialog).findByText("sudo pacman -S bubblewrap")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Save changes" })).toBeDisabled();
    expect(within(dialog).getByRole("checkbox", { name: /Internet and network/ })).not.toBeChecked();
    expect(within(dialog).getByRole("checkbox", { name: /Display/ })).toBeChecked();
    expect(within(dialog).getByRole("checkbox", { name: /Keep private app data/ })).toBeChecked();
    await user.click(within(dialog).getByRole("button", { name: "Recheck bubblewrap" }));
    await waitFor(() => expect(within(dialog).getByRole("button", { name: "Save changes" })).toBeEnabled());
    await user.click(within(dialog).getByRole("button", { name: "Save changes" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(libraryConfigure.mock.calls[0][1]).toEqual({ mode: "bubblewrap", permissions: { network: false, display: true, audio: false, gpu: false, persistent_data: true, paths: [] } });
    expect(libraryLaunch).not.toHaveBeenCalled();
  });

  it("preserves editable grants when saving fails", async () => {
    const libraryConfigure = vi.fn<(id: string, config: LaunchConfig) => Promise<InstalledApp>>(async () => { throw new Error("Cannot save configuration"); });
    const imported = createPreviewApp("Obsidian", "1");
    const { backend, libraryLaunch } = unreviewedBackend({ libraryConfigure, async libraryList() { return [imported]; } });
    const user = userEvent.setup();
    render(<App backend={backend} />);
    await screen.findByRole("heading", { name: "All apps" });
    await chooseFromMenu(user, "More options for Obsidian", "Settings");
    const dialog = screen.getByRole("dialog", { name: "Settings for Obsidian" });
    await user.click(within(dialog).getByRole("button", { name: "Permissions" }));
    await user.click(within(dialog).getByRole("radio", { name: "Restrict with bubblewrap" }));
    await waitFor(() => expect(within(dialog).getByRole("button", { name: "Save changes" })).toBeEnabled());
    expect(within(dialog).getByRole("checkbox", { name: /Internet and network/ })).not.toBeChecked();
    await user.click(within(dialog).getByRole("checkbox", { name: /Display/ }));
    expect(within(dialog).getByRole("checkbox", { name: /Display/ })).not.toBeChecked();
    await user.click(within(dialog).getByRole("checkbox", { name: /Display/ }));
    expect(within(dialog).getByRole("checkbox", { name: /Display/ })).toBeChecked();
    expect(libraryConfigure).not.toHaveBeenCalled();
    await user.type(within(dialog).getByRole("textbox", { name: "Absolute path" }), "/home/me/Notes");
    await user.click(within(dialog).getByRole("button", { name: "Add path" }));
    await chooseOption(user, within(dialog).getByRole("combobox", { name: "Access for /home/me/Notes" }), "Read and write");
    await user.click(within(dialog).getByRole("button", { name: "Save changes" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Cannot save configuration");
    expect(within(dialog).getByRole("combobox", { name: "Access for /home/me/Notes" })).toHaveTextContent("Read and write");
    expect(libraryConfigure.mock.calls[0][1].permissions.paths).toEqual([{ path: "/home/me/Notes", access: "read-write" }]);
    expect(libraryLaunch).not.toHaveBeenCalled();
    await user.click(within(dialog).getByRole("button", { name: "Remove path /home/me/Notes" }));
    expect(within(dialog).getByText("No personal paths allowed.")).toBeInTheDocument();
  });

  it("keeps sandbox save blocked on a failed availability probe but allows explicit direct choice", async () => {
    const { backend, libraryConfigure } = unreviewedBackend({ async sandboxStatus() { throw new Error("Probe failed"); } });
    const { user, dialog } = await openReview(backend);
    expect(within(dialog).getByRole("button", { name: "Save and launch" })).toBeEnabled();
    await user.click(within(dialog).getByRole("radio", { name: "Restrict with bubblewrap" }));
    expect(await within(dialog).findByText(/Probe failed/)).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Save and launch" })).toBeDisabled();
    await user.click(within(dialog).getByRole("radio", { name: "Run without restrictions" }));
    await user.click(within(dialog).getByRole("button", { name: "Save and launch" }));
    await waitFor(() => expect(libraryConfigure).toHaveBeenCalledOnce());
  });

  it("rejects relative filesystem grants without adding them", async () => {
    const { backend } = unreviewedBackend();
    const { user, dialog } = await openReview(backend);
    await restrict(user, dialog);
    await user.type(within(dialog).getByRole("textbox", { name: "Absolute path" }), "Documents");
    await user.click(within(dialog).getByRole("button", { name: "Add path" }));
    expect(within(dialog).getByRole("alert")).toHaveTextContent("Choose absolute paths");
    expect(within(dialog).getByText("No personal paths allowed.")).toBeInTheDocument();
  });
});


it("ignores old import suggestions and starts with generic permissions", async () => {
  const permissions = { network: true, display: true, audio: false, gpu: false, persistent_data: true, paths: [] };
  const imported = { ...createPreviewApp("Obsidian", "1"), permission_defaults: permissions };
  const preview = createPreviewApi();
  const user = userEvent.setup();
  render(<App backend={{ ...preview, async libraryList() { return [imported]; } }} />);
  await screen.findByRole("heading", { name: "All apps" });
  await chooseFromMenu(user, "More options for Obsidian", "Settings");
  const dialog = screen.getByRole("dialog", { name: "Settings for Obsidian" });
  await user.click(within(dialog).getByRole("button", { name: "Permissions" }));
  await user.click(within(dialog).getByRole("radio", { name: "Restrict with bubblewrap" }));
  expect(within(dialog).getByRole("checkbox", { name: /Internet and network/ })).not.toBeChecked();
  expect(within(dialog).queryByRole("button", { name: "Use import defaults" })).not.toBeInTheDocument();
});
