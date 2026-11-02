import { expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "../src/App";
import type { AppApi } from "../src/lib/api";
import { createPreviewApi } from "../src/lib/preview";
import { chooseFromMenu } from "./menu-helpers";

async function binaryBackend(patch: Partial<AppApi> = {}) {
  const preview = createPreviewApi(true);
  const binary = (await preview.libraryList()).find((app) => app.kind === "binary");
  if (!binary) throw new Error("Binary fixture is missing");
  return { binary, backend: { ...preview, ...patch } };
}

it("imports an extensionless executable in place without offering a managed move", async () => {
  const { binary, backend } = await binaryBackend();
  let isImported = false;
  const libraryImport = vi.fn(async () => { isImported = true; return binary; });
  const pickExecutable = vi.fn(async () => binary.file_path);
  const user = userEvent.setup();
  render(<App backend={{ ...backend, isTauri: true, libraryImport, pickExecutable,
    async libraryList() { return (await backend.libraryList()).filter((app) => app.kind !== "binary" || isImported); },
  }} />);
  await screen.findByRole("heading", { name: "All apps" });
  await chooseFromMenu(user, "Add apps", "Add an executable");
  await screen.findByRole("button", { name: "Launch Native Editor" });
  expect(pickExecutable).toHaveBeenCalledOnce();
  expect(libraryImport).toHaveBeenCalledWith(binary.file_path, false);
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});

it("requires first-launch review in direct mode, loads backend folder defaults, and saves a sandbox launch", async () => {
  const { binary, backend } = await binaryBackend();
  const libraryLaunch = vi.fn(backend.libraryLaunch);
  const libraryDefaultPermissions = vi.fn(backend.libraryDefaultPermissions);
  const user = userEvent.setup();
  render(<App backend={{ ...backend, libraryLaunch, libraryDefaultPermissions }} />);
  await screen.findByRole("heading", { name: "All apps" });
  await user.click(screen.getByRole("button", { name: "Launch Native Editor" }));
  const dialog = screen.getByRole("dialog", { name: "Settings for Native Editor" });
  expect(within(dialog).getByRole("radio", { name: "Run without restrictions" })).toBeChecked();
  expect(libraryLaunch).not.toHaveBeenCalled();
  expect(await within(dialog).findByText(/Files in granted folders are not covered/)).toBeInTheDocument();
  await waitFor(() => expect(within(dialog).getByRole("button", { name: "Save and launch" })).toBeEnabled());
  expect(libraryDefaultPermissions).toHaveBeenCalledWith(binary.id);
  await user.click(within(dialog).getByRole("radio", { name: "Restrict with bubblewrap" }));
  expect(within(dialog).getByText("/preview/Applications")).toBeInTheDocument();
  expect(within(dialog).getByRole("combobox", { name: "Access for /preview/Applications" })).toHaveTextContent("Read only");
  await user.click(within(dialog).getByRole("button", { name: "Save and launch" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  const saved = (await backend.libraryList()).find((app) => app.id === binary.id);
  expect(saved?.launch_config).toMatchObject({ mode: "bubblewrap", permissions: { paths: [{ path: "/preview/Applications", access: "read-only" }] } });
  expect(saved?.launch_count).toBe(1);
});

it("blocks saving when backend default permissions cannot be loaded", async () => {
  const { backend } = await binaryBackend({ async libraryDefaultPermissions() { throw new Error("Cannot read library"); } });
  const user = userEvent.setup();
  render(<App backend={backend} />);
  await screen.findByRole("heading", { name: "All apps" });
  await user.click(screen.getByRole("button", { name: "Launch Native Editor" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Cannot load default permissions: Cannot read library");
  expect(screen.getByRole("button", { name: "Save and launch" })).toBeDisabled();
});

it("keeps binary update settings unavailable and supports desktop menu integration", async () => {
  const { binary, backend } = await binaryBackend();
  const user = userEvent.setup();
  render(<App backend={backend} />);
  await screen.findByRole("heading", { name: "All apps" });
  await chooseFromMenu(user, "More options for Native Editor", "Settings");
  await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Updates" }));
  expect(screen.getByText(/Executable updates are not supported yet/)).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Check for updates" })).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Integration" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Add to app menu" })).toBeEnabled());
  await user.click(screen.getByRole("button", { name: "Add to app menu" }));
  await screen.findByRole("button", { name: "Remove from app menu" });
  expect((await backend.libraryList()).find((app) => app.id === binary.id)?.desktop_id).toBeTruthy();
  expect((await backend.updatesCheck(binary.id)).status).toBe("unsupported");
  expect((await backend.updatesCheckAll()).updates.every((update) => update.app_id !== binary.id)).toBe(true);
  await expect(backend.updatesConfigure(binary.id, null)).rejects.toThrow("Executable updates");
  await expect(backend.updatesInstall(binary.id)).rejects.toThrow("Executable updates");
});

it("offers removal without file deletion and explains how to add a moved binary again", async () => {
  const { backend } = await binaryBackend();
  const libraryRemove = vi.fn(backend.libraryRemove);
  const user = userEvent.setup();
  render(<App backend={{ ...backend, libraryRemove }} />);
  await screen.findByRole("heading", { name: "All apps" });
  await chooseFromMenu(user, "More options for Native Editor", "Remove Native Editor");
  const dialog = screen.getByRole("dialog", { name: "Remove Native Editor?" });
  expect(within(dialog).queryByRole("checkbox")).not.toBeInTheDocument();
  expect(within(dialog).getByText(/Moving or renaming it requires a new entry/)).toBeInTheDocument();
  await user.click(within(dialog).getByRole("button", { name: "Remove entry" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  expect(libraryRemove.mock.calls[0][1]).toBe(false);
});

it("shows unavailable Bubblewrap and blocks sandbox saving without launching unrestricted", async () => {
  const { backend } = await binaryBackend({ async sandboxStatus() { return { available: false, version: null, error: "bwrap not installed" }; } });
  const libraryLaunch = vi.fn(backend.libraryLaunch);
  const user = userEvent.setup();
  render(<App backend={{ ...backend, libraryLaunch }} />);
  await screen.findByRole("heading", { name: "All apps" });
  await user.click(screen.getByRole("button", { name: "Launch Native Editor" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Save and launch" })).toBeEnabled());
  await user.click(screen.getByRole("radio", { name: "Restrict with bubblewrap" }));
  expect(await screen.findByText(/Bubblewrap is unavailable: bwrap not installed/)).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Save and launch" })).toBeDisabled();
  expect(libraryLaunch).not.toHaveBeenCalled();
});

it("reopens binary permission review when changed content invalidates approval", async () => {
  const { binary, backend } = await binaryBackend();
  await backend.libraryConfigure(binary.id, { mode: "direct", permissions: await backend.libraryDefaultPermissions(binary.id) }, await backend.libraryPrepareReview(binary.id));
  let hasChanged = false;
  const user = userEvent.setup();
  render(<App backend={{ ...backend,
    async libraryList() { return (await backend.libraryList()).map((app) => app.id === binary.id && hasChanged ? { ...app, reviewed_digest: null } : app); },
    async libraryLaunch() { hasChanged = true; throw new Error("App changed since review. Review launch permissions again"); },
  }} />);
  await screen.findByRole("heading", { name: "All apps" });
  await user.click(screen.getByRole("button", { name: "Launch Native Editor" }));
  const dialog = await screen.findByRole("dialog", { name: "Settings for Native Editor" });
  expect(within(dialog).getByRole("alert")).toHaveTextContent("App changed since review");
  expect(within(dialog).getByRole("radio", { name: "Run without restrictions" })).toBeChecked();
  expect(within(dialog).getByRole("button", { name: "Save and launch" })).toBeEnabled();
});
