import { expect, it } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "../src/App";
import type { AppApi } from "../src/lib/api";
import { createPreviewApi } from "../src/lib/preview";
import type { LaunchConfig } from "../src/lib/types";

async function openBubblewrapPermissions(librarySandboxCommand: AppApi["librarySandboxCommand"]) {
  const user = userEvent.setup();
  render(<App backend={{ ...createPreviewApi(true), librarySandboxCommand }} />);
  await screen.findByRole("heading", { name: "All apps" });
  await user.click(screen.getByRole("button", { name: "Launch Native Editor" }));
  const dialog = screen.getByRole("dialog", { name: "Settings for Native Editor" });
  await user.click(within(dialog).getByRole("radio", { name: "Restrict with bubblewrap" }));
  return { user, section: within(dialog).getByRole("region", { name: "Run from a terminal" }), dialog };
}

const describeNetwork = async (_id: string, config: LaunchConfig) =>
  `bwrap ${config.permissions.network ? "--share-net" : "--unshare-net"} -- /app/editor`;

async function copyCommand(user: ReturnType<typeof userEvent.setup>, section: HTMLElement) {
  const button = within(section).getByRole("button", { name: "Copy command" });
  await waitFor(() => expect(button).toBeEnabled());
  await user.click(button);
  return navigator.clipboard.readText();
}

it("copies the sandbox command for unsaved permissions without showing it", async () => {
  const { user, section, dialog } = await openBubblewrapPermissions(describeNetwork);
  expect(await copyCommand(user, section)).toBe("bwrap --unshare-net -- /app/editor");
  expect(within(section).getByRole("button", { name: "Copied" })).toBeInTheDocument();
  expect(within(dialog).queryByText(/bwrap/)).not.toBeInTheDocument();

  await user.click(within(dialog).getByRole("checkbox", { name: "Internet and network" }));
  expect(await copyCommand(user, section)).toBe("bwrap --share-net -- /app/editor");
});

it("explains why the command cannot be built", async () => {
  const { section } = await openBubblewrapPermissions(async () => { throw new Error("folder grant is outside your home"); });
  expect(await within(section).findByText("Cannot build the command: folder grant is outside your home")).toBeInTheDocument();
  expect(within(section).getByRole("button", { name: "Copy command" })).toBeDisabled();
});
