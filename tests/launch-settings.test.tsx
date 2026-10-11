import { describe, expect, it } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "../src/App";
import { createPreviewApi } from "../src/lib/preview";
import { chooseFromMenu } from "./menu-helpers";

async function openLaunchSettings() {
  const backend = createPreviewApi();
  const user = userEvent.setup();
  render(<App backend={backend} />);
  await screen.findByRole("heading", { name: "All apps" });
  await chooseFromMenu(user, "More options for Obsidian", "Settings");
  await user.click(screen.getByRole("button", { name: "Launch" }));
  return { backend, user };
}

describe("per-app launch settings", () => {
  it("saves, reopens, and clears options for only the selected app", async () => {
    const { backend, user } = await openLaunchSettings();
    await user.type(screen.getByLabelText("Command-line arguments"), "--profile\nMy profile");
    await user.type(screen.getByLabelText("Environment variables"), "GTK_THEME=Adwaita:dark\nEMPTY=");
    await user.click(screen.getByRole("button", { name: "Permissions" }));
    await user.click(screen.getByRole("button", { name: "Launch" }));
    expect(screen.getByLabelText("Command-line arguments")).toHaveValue("--profile\nMy profile");
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    const apps = await backend.libraryList();
    expect(apps.find((app) => app.name === "Obsidian")?.launch_config).toMatchObject({ arguments: ["--profile", "My profile"], environment: { GTK_THEME: "Adwaita:dark", EMPTY: "" } });
    expect(apps.find((app) => app.name === "Joplin")?.launch_config?.arguments).toBeUndefined();
    await chooseFromMenu(user, "More options for Obsidian", "Settings");
    await user.click(screen.getByRole("button", { name: "Launch" }));
    expect(screen.getByLabelText("Environment variables")).toHaveValue("GTK_THEME=Adwaita:dark\nEMPTY=");
    await user.clear(screen.getByLabelText("Command-line arguments"));
    await user.clear(screen.getByLabelText("Environment variables"));
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect((await backend.libraryList()).find((app) => app.name === "Obsidian")?.launch_config).toMatchObject({ arguments: [], environment: {} });
  });

  it("retains invalid input and leaves saved settings untouched", async () => {
    const { backend, user } = await openLaunchSettings();
    await user.type(screen.getByLabelText("Environment variables"), "INVALID-NAME=value");
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    expect(within(screen.getByRole("dialog")).getByRole("alert")).toHaveTextContent("NAME=value");
    expect(screen.getByLabelText("Environment variables")).toHaveValue("INVALID-NAME=value");
    expect((await backend.libraryList()).find((app) => app.name === "Obsidian")?.launch_config?.environment).toBeUndefined();
  });
});
