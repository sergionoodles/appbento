import { expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "../src/App";
import { createPreviewApi, createPreviewApp } from "../src/lib/preview";
import { LaunchLogsDialog } from "../src/components/dialogs/launch-logs-dialog";
import { restrictedPermissions } from "../src/lib/sandbox";
import { chooseFromMenu } from "./menu-helpers";

it("opens the latest launch log from the app menu, refreshes, and copies output", async () => {
  const user = userEvent.setup();
  let text = "bwrap: permission denied\nApplication exited with status 1";
  const backend = { ...createPreviewApi(), async libraryLaunchLog() {
    return { started_at: "2026-10-12T10:00:00Z", text };
  } };
  render(<App backend={backend} />);
  await screen.findByRole("heading", { name: "All apps" });
  await chooseFromMenu(user, "More options for Obsidian", "View logs");
  const dialog = screen.getByRole("dialog", { name: "Obsidian latest launch logs" });
  const output = await within(dialog).findByLabelText("Launch log output");
  expect(output).toHaveTextContent("bwrap: permission denied");
  await user.click(within(dialog).getByRole("button", { name: "Copy logs" }));
  expect(await navigator.clipboard.readText()).toBe(text);
  expect(within(dialog).getByText("Logs copied.")).toBeInTheDocument();
  text = "New output from the running app";
  await user.click(within(dialog).getByRole("button", { name: "Refresh" }));
  await waitFor(() => expect(output).toHaveTextContent(text));
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "More options for Obsidian" })).toHaveFocus();
});

it("explains missing logs and disables copying", async () => {
  const user = userEvent.setup();
  render(<App backend={createPreviewApi()} />);
  await screen.findByRole("heading", { name: "All apps" });
  await chooseFromMenu(user, "More options for Joplin", "View logs");
  expect(await screen.findByText("No launch logs yet. Launch this app to record its output.")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Copy logs" })).toBeDisabled();
});

it("displays read failures and lets the user retry", async () => {
  const user = userEvent.setup();
  let hasFailed = true;
  const backend = { ...createPreviewApi(), async libraryLaunchLog() {
    if (hasFailed) throw new Error("Cannot read saved log");
    return { started_at: "2026-10-12T10:00:00Z", text: "Recovered output" };
  } };
  render(<App backend={backend} />);
  await screen.findByRole("heading", { name: "All apps" });
  await chooseFromMenu(user, "More options for Obsidian", "View logs");
  expect(await screen.findByRole("alert")).toHaveTextContent("Cannot read saved log");
  hasFailed = false;
  await user.click(screen.getByRole("button", { name: "Refresh" }));
  expect(await screen.findByLabelText("Launch log output")).toHaveTextContent("Recovered output");
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
});

it("keeps logs selectable when clipboard access fails", async () => {
  const user = userEvent.setup();
  vi.spyOn(navigator.clipboard, "writeText").mockRejectedValue(new Error("Clipboard denied"));
  render(<App backend={{ ...createPreviewApi(), async libraryLaunchLog() {
    return { started_at: "2026-10-12T10:00:00Z", text: "Error to copy manually" };
  } }} />);
  await screen.findByRole("heading", { name: "All apps" });
  await chooseFromMenu(user, "More options for Obsidian", "View logs");
  await screen.findByLabelText("Launch log output");
  await user.click(screen.getByRole("button", { name: "Copy logs" }));
  expect(await screen.findByText("Could not copy automatically. Select the log text and copy it manually.")).toBeInTheDocument();
  expect(screen.getByLabelText("Launch log output")).toHaveTextContent("Error to copy manually");
});

it("shows an editable prompt with app settings and logs, copies edits, and returns to logs", async () => {
  const user = userEvent.setup();
  const app = { ...createPreviewApp("Example", "2.0"), launch_config: {
    mode: "bubblewrap" as const,
    permissions: { ...restrictedPermissions(), paths: [{ path: "/home/test/Documents", access: "read-only" as const }] },
    arguments: ["--profile", "My profile"],
    environment: { LANG: "en_US.UTF-8" },
  } };
  const log = { started_at: "2026-10-12T10:00:00Z", text: "bwrap: permission denied\nApplication exited with status 1" };
  render(<LaunchLogsDialog app={app} backend={{ ...createPreviewApi(), async libraryLaunchLog() { return log; } }} onClose={() => {}} />);
  await screen.findByLabelText("Launch log output");
  await user.click(screen.getByRole("button", { name: "Fix with a prompt" }));
  const editor = screen.getByRole("textbox", { name: "Troubleshooting prompt" });
  const prompt = (editor as HTMLTextAreaElement).value;
  const diagnostics = JSON.parse(prompt.split("Diagnostics (JSON):\n")[1]);
  expect(diagnostics).toEqual({
    app: { name: app.name, version: app.version_current, file_path: app.file_path },
    current_launch_config: app.launch_config,
    latest_launch: log,
  });
  expect(prompt).toContain("smallest settings change");
  expect(prompt).toContain('refresh "View logs"');
  expect(prompt).toContain('"Graphics acceleration" (gpu)');
  expect(prompt).toContain("may have changed since the recorded launch");
  await user.click(screen.getByRole("button", { name: "Copy prompt" }));
  expect(await navigator.clipboard.readText()).toBe(prompt);
  expect(screen.getByRole("status")).toHaveTextContent("Prompt copied.");
  await user.clear(editor);
  expect(screen.getByRole("button", { name: "Copy prompt" })).toBeDisabled();
  await user.type(editor, "Edited prompt with private details removed");
  await user.click(screen.getByRole("button", { name: "Copy prompt" }));
  expect(await navigator.clipboard.readText()).toBe("Edited prompt with private details removed");
  await user.click(screen.getByRole("button", { name: "Back to logs" }));
  expect(screen.getByLabelText("Launch log output")).toHaveTextContent("bwrap: permission denied");
});

it("can prepare a prompt when logs and saved configuration are missing", async () => {
  const user = userEvent.setup();
  render(<LaunchLogsDialog app={createPreviewApp("Example", null)} backend={{ ...createPreviewApi(), async libraryLaunchLog() { return null; } }} onClose={() => {}} />);
  await screen.findByText("No launch logs yet. Launch this app to record its output.");
  await user.click(screen.getByRole("button", { name: "Fix with a prompt" }));
  const prompt = (screen.getByRole("textbox", { name: "Troubleshooting prompt" }) as HTMLTextAreaElement).value;
  expect(prompt).toContain('"current_launch_config": null');
  expect(prompt).toContain("No launch logs recorded yet.");
});

it("keeps the prompt available for manual copying when clipboard access fails", async () => {
  const user = userEvent.setup();
  vi.spyOn(navigator.clipboard, "writeText").mockRejectedValue(new Error("Clipboard denied"));
  render(<LaunchLogsDialog app={createPreviewApp("Example", null)} backend={{ ...createPreviewApi(), async libraryLaunchLog() {
    return { started_at: "2026-10-12T10:00:00Z", text: "" };
  } }} onClose={() => {}} />);
  await screen.findByText("No output was produced by this launch.");
  await user.click(screen.getByRole("button", { name: "Fix with a prompt" }));
  await user.click(screen.getByRole("button", { name: "Copy prompt" }));
  expect(screen.getByRole("status")).toHaveTextContent("Could not copy automatically. Select the prompt text and copy it manually.");
  expect((screen.getByRole("textbox", { name: "Troubleshooting prompt" }) as HTMLTextAreaElement).value).toContain('"text": ""');
});
