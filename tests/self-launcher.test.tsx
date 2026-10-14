import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "../src/App";
import { createPreviewApi } from "../src/lib/preview";
import type { AppApi } from "../src/lib/api";
import { chooseFromMenu } from "./menu-helpers";

const TITLE = "Add appbento to your system launcher?";

function nativeBackend(): AppApi {
  const preview = createPreviewApi();
  let dismissed = false;
  return {
    ...preview,
    isTauri: true,
    async selfLauncherStatus() {
      return { ...await preview.selfLauncherStatus(), dismissed };
    },
    async selfLauncherDismiss() {
      dismissed = true;
      return preview.selfLauncherDismiss();
    },
    async selfLauncherInstall() {
      dismissed = true;
      return preview.selfLauncherInstall();
    },
  };
}

beforeEach(() => localStorage.clear());
afterEach(cleanup);

describe("appbento launcher registration", () => {
  it("prompts on first launch, remembers dismissal, and installs later from Settings", async () => {
    const backend = nativeBackend();
    const user = userEvent.setup();
    const firstLaunch = render(<App backend={backend} />);
    const dialog = await screen.findByRole("dialog", { name: TITLE });
    await user.click(within(dialog).getByRole("button", { name: "Not now" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: TITLE })).not.toBeInTheDocument());
    firstLaunch.unmount();
    render(<App backend={backend} />);
    await screen.findByText("Obsidian");
    expect(screen.queryByRole("dialog", { name: TITLE })).not.toBeInTheDocument();
    await chooseFromMenu(user, "Settings", "Add appbento to launcher");
    await user.click(screen.getByRole("button", { name: "Add to launcher" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: TITLE })).not.toBeInTheDocument());
    await user.click(screen.getByRole("button", { name: "Settings" }));
    expect(screen.queryByRole("menuitem", { name: "Add appbento to launcher" })).not.toBeInTheDocument();
  });

  it("adds directly from the first-launch prompt and skips the prompt on restart", async () => {
    const backend = nativeBackend();
    const user = userEvent.setup();
    const firstLaunch = render(<App backend={backend} />);
    await screen.findByRole("dialog", { name: TITLE });
    await user.click(screen.getByRole("button", { name: "Add to launcher" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: TITLE })).not.toBeInTheDocument());
    firstLaunch.unmount();
    render(<App backend={backend} />);
    await screen.findByText("Obsidian");
    expect(screen.queryByRole("dialog", { name: TITLE })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Settings" }));
    expect(screen.queryByRole("menuitem", { name: "Add appbento to launcher" })).not.toBeInTheDocument();
  });

  it("shows installation errors and lets the user retry", async () => {
    const native = nativeBackend();
    let shouldFail = true;
    const backend = { ...native, async selfLauncherInstall() {
      if (shouldFail) throw new Error("Permission denied");
      return native.selfLauncherInstall();
    } };
    const user = userEvent.setup();
    render(<App backend={backend} />);
    await screen.findByRole("dialog", { name: TITLE });
    await user.click(screen.getByRole("button", { name: "Add to launcher" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Permission denied");
    expect((await backend.selfLauncherStatus()).installed).toBe(false);
    shouldFail = false;
    await user.click(screen.getByRole("button", { name: "Add to launcher" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: TITLE })).not.toBeInTheDocument());
  });

  it("remembers Escape dismissal and keeps the settings option", async () => {
    const backend = nativeBackend();
    const user = userEvent.setup();
    render(<App backend={backend} />);
    await screen.findByRole("dialog", { name: TITLE });
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog", { name: TITLE })).not.toBeInTheDocument());
    expect((await backend.selfLauncherStatus()).dismissed).toBe(true);
    await chooseFromMenu(user, "Settings", "Add appbento to launcher");
    expect(screen.getByRole("dialog", { name: TITLE })).toBeInTheDocument();
  });

  it("does not offer system integration in the browser preview", async () => {
    const user = userEvent.setup();
    render(<App backend={createPreviewApi()} />);
    await screen.findByText("Obsidian");
    expect(screen.queryByRole("dialog", { name: TITLE })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Settings" }));
    expect(screen.queryByRole("menuitem", { name: "Add appbento to launcher" })).not.toBeInTheDocument();
  });
});
