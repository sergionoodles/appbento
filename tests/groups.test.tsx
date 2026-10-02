import { beforeEach, describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "../src/App";
import { createPreviewApi } from "../src/lib/preview";
import { chooseFromMenu } from "./menu-helpers";

async function openHome() {
  const user = userEvent.setup();
  const view = render(<App backend={createPreviewApi()} />);
  await screen.findByRole("heading", { name: "All apps" });
  return { user, view };
}

function appOrder(container: HTMLElement = document.body) {
  return within(container).getAllByRole("button", { name: /^Open / }).map((button) => button.getAttribute("aria-label"));
}

function sidebar() {
  return screen.getByRole("navigation", { name: "Library" });
}

describe("arranging apps", () => {
  beforeEach(() => localStorage.clear());

  it("starts alphabetical and keeps a custom order after reopening the app", async () => {
    const { user, view } = await openHome();
    expect(appOrder()).toEqual(["Open Joplin", "Open Obsidian"]);
    expect(screen.queryByText("Most used")).not.toBeInTheDocument();

    await chooseFromMenu(user, "More options for Obsidian", "Move earlier");
    expect(appOrder()).toEqual(["Open Obsidian", "Open Joplin"]);

    view.unmount();
    await openHome();
    expect(appOrder()).toEqual(["Open Obsidian", "Open Joplin"]);
  });

  it("creates a named group from an app and moves other apps into it", async () => {
    const { user } = await openHome();
    await chooseFromMenu(user, "More options for Joplin", "Move to new group");
    const name = screen.getByRole("textbox", { name: "Group name" });
    expect(name).toHaveFocus();
    await user.clear(name);
    await user.type(name, "Notes{Enter}");

    const notes = screen.getByRole("region", { name: "Notes" });
    expect(within(notes).getByRole("heading", { name: "Notes" })).toBeInTheDocument();
    expect(appOrder(notes)).toEqual(["Open Joplin"]);
    expect(within(sidebar()).getByRole("button", { name: /Notes/ })).toBeInTheDocument();

    await chooseFromMenu(user, "More options for Obsidian", "Move to Notes");
    expect(appOrder(notes)).toEqual(["Open Joplin", "Open Obsidian"]);
    expect(screen.queryByRole("region", { name: "Ungrouped" })).not.toBeInTheDocument();
  });

  it("filters to one group from the sidebar and deleting it keeps its apps", async () => {
    const { user } = await openHome();
    await user.click(within(sidebar()).getByRole("button", { name: "New group" }));
    await user.type(screen.getByRole("textbox", { name: "Group name" }), "Video tools{Enter}", { skipClick: true });
    await chooseFromMenu(user, "More options for Obsidian", "Move to Video tools");

    await user.click(within(sidebar()).getByRole("button", { name: /Video tools/ }));
    expect(screen.getByRole("heading", { level: 1, name: "Video tools" })).toBeInTheDocument();
    expect(appOrder()).toEqual(["Open Obsidian"]);

    await chooseFromMenu(user, "Options for Video tools group", "Delete Video tools group");
    expect(await screen.findByRole("heading", { name: "All apps" })).toBeInTheDocument();
    expect(appOrder()).toEqual(["Open Joplin", "Open Obsidian"]);
    expect(within(sidebar()).queryByRole("button", { name: /Video tools/ })).not.toBeInTheDocument();
  });

  it("creating a group while another group is open returns to all apps to name it", async () => {
    const { user } = await openHome();
    await user.click(within(sidebar()).getByRole("button", { name: "New group" }));
    await user.type(screen.getByRole("textbox", { name: "Group name" }), "Dev{Enter}", { skipClick: true });
    await user.click(within(sidebar()).getByRole("button", { name: /Dev/ }));
    await user.click(within(sidebar()).getByRole("button", { name: "New group" }));
    expect(screen.getByRole("heading", { level: 1, name: "All apps" })).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Group name" })).toHaveFocus();
  });

  it("cancels a rename with Escape and keeps the previous name", async () => {
    const { user } = await openHome();
    await user.click(within(sidebar()).getByRole("button", { name: "New group" }));
    await user.type(screen.getByRole("textbox", { name: "Group name" }), "Dev{Enter}", { skipClick: true });
    await chooseFromMenu(user, "Options for Dev group", "Rename");
    await user.type(screen.getByRole("textbox", { name: "Group name" }), "Discarded{Escape}", { skipClick: true });
    expect(screen.getByRole("heading", { name: "Dev" })).toBeInTheDocument();
  });
});
