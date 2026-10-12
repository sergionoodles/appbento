import { beforeEach, describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "../src/App";
import { createPreviewApi } from "../src/lib/preview";
import { dragTile, tileOf } from "./drag-helpers";
import { chooseFromMenu } from "./menu-helpers";

async function openHome() {
  const user = userEvent.setup();
  const view = render(<App backend={createPreviewApi()} />);
  await screen.findByRole("heading", { name: "All apps" });
  return { user, view };
}

function appOrder(container: HTMLElement = document.body) {
  return within(container).getAllByRole("article").map((button) => button.getAttribute("aria-label"));
}

function sidebar() {
  return screen.getByRole("navigation", { name: "Library" });
}

function dropZoneOf(groupName: string) {
  const zone = screen.getByRole("heading", { name: groupName }).closest("[data-drop-section]");
  if (!zone) throw new Error(`No drop zone for ${groupName}`);
  return zone;
}

async function addGroup(user: ReturnType<typeof userEvent.setup>, name: string) {
  await user.click(within(sidebar()).getByRole("button", { name: "New group" }));
  await user.type(screen.getByRole("textbox", { name: "Group name" }), `${name}{Enter}`, { skipClick: true });
}

describe("arranging apps", () => {
  beforeEach(() => localStorage.clear());

  it("starts alphabetical and keeps a custom order after reopening the app", async () => {
    const { view } = await openHome();
    expect(appOrder()).toEqual(["Joplin", "Obsidian"]);
    expect(screen.queryByText("Most used")).not.toBeInTheDocument();

    dragTile("Obsidian", tileOf("Joplin"));
    expect(appOrder()).toEqual(["Obsidian", "Joplin"]);

    view.unmount();
    await openHome();
    expect(appOrder()).toEqual(["Obsidian", "Joplin"]);
  });

  it("drags apps onto a group header to move them into the group", async () => {
    const { user } = await openHome();
    await addGroup(user, "Notes");
    expect(within(sidebar()).getByRole("button", { name: /Notes/ })).toBeInTheDocument();

    dragTile("Joplin", dropZoneOf("Notes"));
    const notes = screen.getByRole("region", { name: "Notes" });
    expect(appOrder(notes)).toEqual(["Joplin"]);

    dragTile("Obsidian", dropZoneOf("Notes"));
    expect(appOrder(notes)).toEqual(["Joplin", "Obsidian"]);
    expect(screen.queryByRole("region", { name: "Ungrouped" })).not.toBeInTheDocument();
  });

  it("leaves arranging to drag and drop instead of the app menu", async () => {
    const { user } = await openHome();
    await addGroup(user, "Notes");
    await user.click(screen.getByRole("button", { name: "More options for Obsidian" }));
    const items = screen.getAllByRole("menuitem").map((item) => item.textContent);
    expect(items).toEqual(["Settings", "View logs", "Remove"]);
  });

  it("filters to one group from the sidebar and deleting it keeps its apps", async () => {
    const { user } = await openHome();
    await addGroup(user, "Video tools");
    dragTile("Obsidian", within(sidebar()).getByRole("button", { name: /Video tools/ }));

    await user.click(within(sidebar()).getByRole("button", { name: /Video tools/ }));
    expect(screen.getByRole("heading", { level: 1, name: "Video tools" })).toBeInTheDocument();
    expect(appOrder()).toEqual(["Obsidian"]);

    await chooseFromMenu(user, "Options for Video tools group", "Delete Video tools group");
    expect(await screen.findByRole("heading", { name: "All apps" })).toBeInTheDocument();
    expect(appOrder()).toEqual(["Joplin", "Obsidian"]);
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
