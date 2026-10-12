import { beforeEach, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "../src/App";
import { ConfigurationDialog } from "../src/components/dialogs/configuration-dialog";
import { createPreviewApi, createPreviewApp } from "../src/lib/preview";
import { readConfigurationPreferences, restoreConfigurationPreferences, type ConfigurationPreferences, type ConfigurationPreview } from "../src/lib/configuration";

const preferences: ConfigurationPreferences = {
  theme: "paper",
  display_names: { "/new/Example.AppImage": "My example" },
  layout: { ungrouped: [], groups: [{ id: "work", name: "Work", appIds: ["/new/Example.AppImage"], isCollapsed: true }] },
};

function importPreview(): ConfigurationPreview {
  const app = createPreviewApp("Example", "1.2.3");
  return { backup: { format: "appbento-configuration", version: 1, home_directory: "/old",
    settings: { managed_dir: "/old/Applications", scan_dirs: ["/old/Downloads"] },
    preferences, apps: [{ app, icon: null }] },
    settings: { managed_dir: "/new/Applications", scan_dirs: ["/new/Downloads"] },
    app_paths: { [app.id]: "/new/Example.AppImage" },
    permission_paths: { "/old/Documents": "/new/Documents" },
  };
}

beforeEach(() => localStorage.clear());

it("exports theme, names, and complete group layout including collapsed state", async () => {
  restoreConfigurationPreferences(preferences);
  const exported: ConfigurationPreferences[] = [];
  render(<ConfigurationDialog backend={{ ...createPreviewApi(),
    async configurationExport(value) { exported.push(value); return true; },
  }} onClose={() => {}} onRestored={async () => {}} returnFocusElement={null} />);
  await userEvent.setup().click(screen.getByRole("button", { name: "Export settings" }));
  expect(await screen.findByRole("status")).toHaveTextContent("Settings exported");
  expect(exported).toEqual([preferences]);
});

it("allows relocating AppImages and permission folders before applying all settings", async () => {
  const user = userEvent.setup();
  const requests: ConfigurationPreview[] = [];
  const restored: ConfigurationPreferences[] = [];
  const selected = importPreview();
  render(<ConfigurationDialog backend={{ ...createPreviewApi(),
    async configurationRead() { return selected; },
    async pickAppImage() { return "/elsewhere/Example.AppImage"; },
    async configurationRestore(request) { requests.push(request); return preferences; },
  }} onClose={() => {}} onRestored={async (value) => { restored.push(value); }} returnFocusElement={null} />);
  await user.click(screen.getByRole("button", { name: "Choose settings file" }));
  await user.click(await screen.findByRole("button", { name: "Browse" }));
  const grant = screen.getByRole("textbox", { name: "Permission folder: /old/Documents" });
  await user.clear(grant);
  await user.type(grant, "/elsewhere/Documents");
  await user.click(screen.getByRole("button", { name: "Restore settings" }));
  expect(requests).toHaveLength(1);
  expect(requests[0].app_paths[selected.backup.apps[0].app.id]).toBe("/elsewhere/Example.AppImage");
  expect(requests[0].permission_paths["/old/Documents"]).toBe("/elsewhere/Documents");
  expect(restored).toEqual([preferences]);
});

it("keeps restore choices and previous interface settings when restore fails", async () => {
  const user = userEvent.setup();
  localStorage.setItem("appbento-theme", "midnight");
  render(<ConfigurationDialog backend={{ ...createPreviewApi(),
    async configurationRead() { return importPreview(); },
    async configurationRestore() { throw new Error("AppImage file is missing"); },
  }} onClose={() => { throw new Error("Should remain open"); }} onRestored={async (value) => restoreConfigurationPreferences(value)} returnFocusElement={null} />);
  await user.click(screen.getByRole("button", { name: "Choose settings file" }));
  await user.click(await screen.findByRole("button", { name: "Restore settings" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("AppImage file is missing");
  expect(screen.getByRole("button", { name: "Restore settings" })).toBeEnabled();
  expect(localStorage.getItem("appbento-theme")).toBe("midnight");
});

it("cancelled file selection leaves the dialog ready without success or errors", async () => {
  const user = userEvent.setup();
  render(<ConfigurationDialog backend={{ ...createPreviewApi(),
    async configurationRead() { return null; },
    async configurationExport() { return false; },
  }} onClose={() => {}} onRestored={async () => {}} returnFocusElement={null} />);
  await user.click(screen.getByRole("button", { name: "Choose settings file" }));
  await user.click(screen.getByRole("button", { name: "Export settings" }));
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  expect(screen.queryByRole("status")).not.toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Restore settings" })).not.toBeInTheDocument();
});

it("resumes pending interface restoration on startup and refreshes theme and groups", async () => {
  const app = { ...createPreviewApp("Example", "1.2.3"), id: "/new/Example.AppImage", file_path: "/new/Example.AppImage" };
  let pending: ConfigurationPreferences | null = preferences;
  render(<App backend={{ ...createPreviewApi(),
    async libraryList() { return [app]; },
    async configurationPreferences(completed) {
      if (completed) pending = null;
      return pending;
    },
  }} />);
  const group = await screen.findByRole("button", { name: /^Work/ });
  await userEvent.setup().click(group);
  expect(await screen.findByText("My example")).toBeInTheDocument();
  expect(document.documentElement.dataset.theme).toBe("paper");
  expect(pending).toBeNull();
  expect(readConfigurationPreferences()).toEqual(preferences);
});

it("reports unreadable import files without entering restore", async () => {
  render(<ConfigurationDialog backend={{ ...createPreviewApi(),
    async configurationRead() { throw new Error("Unsupported AppBento configuration format or version"); },
  }} onClose={() => {}} onRestored={async () => {}} returnFocusElement={null} />);
  const dialog = within(screen.getByRole("dialog"));
  await userEvent.setup().click(dialog.getByRole("button", { name: "Choose settings file" }));
  expect(await dialog.findByRole("alert")).toHaveTextContent("Unsupported");
  expect(dialog.queryByRole("button", { name: "Restore settings" })).not.toBeInTheDocument();
});

it("lets restored launch settings be confirmed unchanged before launching a changed AppImage", async () => {
  const preview = createPreviewApi();
  let app = { ...(await preview.libraryList())[0], reviewed_digest: null as string | null };
  const saved: unknown[] = [];
  render(<App backend={{ ...preview,
    async libraryList() { return [app]; },
    async libraryConfigure(_id, config) {
      saved.push(config);
      app = { ...app, launch_config: config, reviewed_digest: "confirmed" };
      return app;
    },
    async libraryLaunch() { return { ...app, launch_count: 1 }; },
  }} />);
  await screen.findByRole("heading", { name: "All apps" });
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "Launch Obsidian" }));
  const dialog = screen.getByRole("dialog", { name: "Settings for Obsidian" });
  const save = within(dialog).getByRole("button", { name: "Save and launch" });
  expect(save).toBeEnabled();
  await user.click(save);
  expect(saved).toEqual([app.launch_config]);
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});
