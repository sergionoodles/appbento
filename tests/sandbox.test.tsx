import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "../src/App";
import { createPreviewApi, createPreviewApp } from "../src/lib/preview";
import type { AppApi } from "../src/lib/api";
import type { InstalledApp, LaunchConfig } from "../src/lib/types";
import { chooseFromMenu } from "./menu-helpers";

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
  const dialog = await screen.findByRole("dialog", { name: "Launch permissions for Obsidian" });
  await waitFor(() => expect(within(dialog).getByRole("button", { name: "Recheck bubblewrap" })).toBeEnabled());
  return { user, dialog };
}

describe("AppImage permission review", () => {

  it("keeps reviewed configuration visible after a save-and-launch failure", async () => {
    const { backend } = unreviewedBackend({ async libraryLaunch() { throw new Error("Sandbox launch failed"); } });
    const { user, dialog } = await openReview(backend);
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await user.click(screen.getByRole("button", { name: "Launch Obsidian" }));
    const reopened = screen.getByRole("dialog");
    await waitFor(() => expect(within(reopened).getByRole("button", { name: "Save and launch" })).toBeEnabled());
    await user.click(within(reopened).getByRole("checkbox", { name: /Display/ }));
    await user.click(within(reopened).getByRole("button", { name: "Save and launch" }));
    expect(await within(reopened).findByRole("alert")).toHaveTextContent("Sandbox launch failed");
    expect(within(reopened).getByRole("checkbox", { name: /Display/ })).not.toBeChecked();
  });

  it("moves to the next unreviewed app when a queued review is canceled", async () => {
    const { backend } = unreviewedBackend({ async libraryList() { return [createPreviewApp("Obsidian", "1"), createPreviewApp("Second", null)]; } });
    const { user, dialog } = await openReview(backend);
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    const next = screen.getByRole("dialog", { name: "Launch permissions for Second" });
    await user.click(within(next).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getAllByText(/^Needs setup/)).toHaveLength(2);
  });

  it.each([["Add apps", "Scan folders"], ["Add apps", "Add an AppImage"]])("opens review for an app discovered by %s > %s", async (menu, item) => {
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
    await screen.findByRole("heading", { name: "Recent applications" });
    await chooseFromMenu(user, menu, item);
    expect(await screen.findByRole("dialog", { name: "Launch permissions for Imported" })).toBeInTheDocument();
    expect(app.launch_count).toBe(0);
  });

  it("cancellation leaves an app unreviewed and launching asks again before explicit direct launch", async () => {
    const { backend, libraryLaunch, libraryConfigure } = unreviewedBackend();
    const { user, dialog } = await openReview(backend);
    expect(within(dialog).getByRole("radio", { name: "Restrict with bubblewrap" })).toBeChecked();
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(libraryLaunch).not.toHaveBeenCalled();
    expect(libraryConfigure).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Launch Obsidian" }));
    const reopened = screen.getByRole("dialog");
    expect(libraryLaunch).not.toHaveBeenCalled();
    await user.click(within(reopened).getByRole("radio", { name: "Run without restrictions" }));
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
    const { user, dialog } = await openReview(backend);
    expect(within(dialog).getByText("sudo pacman -S bubblewrap")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Save permissions" })).toBeDisabled();
    expect(within(dialog).getByRole("checkbox", { name: /Internet and network/ })).not.toBeChecked();
    expect(within(dialog).getByRole("checkbox", { name: /Display/ })).toBeChecked();
    expect(within(dialog).getByRole("checkbox", { name: /Keep private app data/ })).toBeChecked();
    await user.click(within(dialog).getByRole("button", { name: "Recheck bubblewrap" }));
    await waitFor(() => expect(within(dialog).getByRole("button", { name: "Save permissions" })).toBeEnabled());
    await user.click(within(dialog).getByRole("button", { name: "Save permissions" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(libraryConfigure.mock.calls[0][1]).toEqual({ mode: "bubblewrap", permissions: { network: false, display: true, audio: false, gpu: false, persistent_data: true, paths: [] } });
    expect(libraryLaunch).not.toHaveBeenCalled();
  });

  it("requires explicit catalog suggestions and preserves editable grants when saving fails", async () => {
    const libraryConfigure = vi.fn<(id: string, config: LaunchConfig) => Promise<InstalledApp>>(async () => { throw new Error("Cannot save configuration"); });
    const { backend, libraryLaunch } = unreviewedBackend({ libraryConfigure });
    const { user, dialog } = await openReview(backend);
    expect(within(dialog).getByRole("checkbox", { name: /Internet and network/ })).not.toBeChecked();
    await user.click(within(dialog).getByRole("checkbox", { name: /Display/ }));
    expect(within(dialog).getByRole("checkbox", { name: /Display/ })).not.toBeChecked();
    await user.click(within(dialog).getByRole("button", { name: "Use catalog suggestion" }));
    expect(within(dialog).getByRole("checkbox", { name: /Display/ })).toBeChecked();
    expect(libraryConfigure).not.toHaveBeenCalled();
    await user.type(within(dialog).getByRole("textbox", { name: "Absolute path" }), "/home/me/Notes");
    await user.click(within(dialog).getByRole("button", { name: "Add path" }));
    await user.selectOptions(within(dialog).getByRole("combobox", { name: "Access for /home/me/Notes" }), "read-write");
    await user.click(within(dialog).getByRole("button", { name: "Save permissions" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Cannot save configuration");
    expect(within(dialog).getByRole("combobox", { name: "Access for /home/me/Notes" })).toHaveValue("read-write");
    expect(libraryConfigure.mock.calls[0][1].permissions.paths).toEqual([{ path: "/home/me/Notes", access: "read-write" }]);
    expect(libraryLaunch).not.toHaveBeenCalled();
    await user.click(within(dialog).getByRole("button", { name: "Remove path /home/me/Notes" }));
    expect(within(dialog).getByText("No personal paths allowed.")).toBeInTheDocument();
  });

  it("keeps sandbox save blocked on a failed availability probe but allows explicit direct choice", async () => {
    const { backend, libraryConfigure } = unreviewedBackend({ async sandboxStatus() { throw new Error("Probe failed"); } });
    const { user, dialog } = await openReview(backend);
    expect(within(dialog).getByText(/Probe failed/)).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Save permissions" })).toBeDisabled();
    await user.click(within(dialog).getByRole("radio", { name: "Run without restrictions" }));
    await user.click(within(dialog).getByRole("button", { name: "Save permissions" }));
    await waitFor(() => expect(libraryConfigure).toHaveBeenCalledOnce());
  });

  it("rejects relative filesystem grants without adding them", async () => {
    const { backend } = unreviewedBackend();
    const { user, dialog } = await openReview(backend);
    await user.type(within(dialog).getByRole("textbox", { name: "Absolute path" }), "Documents");
    await user.click(within(dialog).getByRole("button", { name: "Add path" }));
    expect(within(dialog).getByRole("alert")).toHaveTextContent("Choose absolute paths");
    expect(within(dialog).getByText("No personal paths allowed.")).toBeInTheDocument();
  });
});
