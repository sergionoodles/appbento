import { expect, it } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "../src/App";
import { createPreviewApi } from "../src/lib/preview";
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
  const dialog = screen.getByRole("dialog", { name: "Launch logs for Obsidian" });
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
  Object.defineProperty(navigator.clipboard, "writeText", { configurable: true, value: async () => {
    throw new Error("Clipboard denied");
  } });
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
