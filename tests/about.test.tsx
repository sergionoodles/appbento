import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it } from "vitest";
import App from "../src/App";
import { createPreviewApi } from "../src/lib/preview";
import { AboutDialog, REPOSITORY_URL } from "../src/components/dialogs/about-dialog";
import { version } from "../package.json";
import { chooseFromMenu } from "./menu-helpers";

it("opens the about dialog from the settings menu and closes it", async () => {
  const user = userEvent.setup();
  render(<App backend={createPreviewApi()} />);

  await chooseFromMenu(user, "Settings", "About AppBento");
  const about = await screen.findByRole("dialog", { name: "About AppBento" });
  expect(within(about).getByLabelText(`Version ${version}`)).toBeInTheDocument();

  await user.click(within(about).getByRole("button", { name: "Close" }));
  expect(screen.queryByRole("dialog", { name: "About AppBento" })).not.toBeInTheDocument();
});

it("opens the repository and the release notes for the running version", async () => {
  const user = userEvent.setup();
  const opened: string[] = [];
  render(<AboutDialog onOpenExternal={async (url) => { opened.push(url); }} onClose={() => {}} returnFocusElement={null} />);

  await user.click(screen.getByRole("button", { name: /Source code/ }));
  await user.click(screen.getByRole("button", { name: /Release notes/ }));

  expect(opened).toEqual([REPOSITORY_URL, `${REPOSITORY_URL}/releases/tag/v${version}`]);
});

it("shows an error when a link cannot be opened", async () => {
  const user = userEvent.setup();
  render(<AboutDialog onOpenExternal={async () => { throw new Error("No browser available"); }} onClose={() => {}} returnFocusElement={null} />);

  await user.click(screen.getByRole("button", { name: /Source code/ }));

  expect(await screen.findByRole("alert")).toHaveTextContent("No browser available");
});
