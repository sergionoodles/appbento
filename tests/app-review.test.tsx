import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import App from "../src/App";
import type { AppApi } from "../src/lib/api";
import { createPreviewApi, createPreviewApp } from "../src/lib/preview";
import type { LaunchConfig } from "../src/lib/types";
import { chooseFromMenu } from "./menu-helpers";

function deferred<Value>() {
  let resolve: (value: Value) => void = () => { throw new Error("Deferred promise not initialized"); };
  const promise = new Promise<Value>((complete) => { resolve = complete; });
  return { promise, resolve };
}

function reviewBackend(patch: Partial<AppApi> = {}) {
  let app = { ...createPreviewApp("Obsidian", "1"), reviewed_digest: null as string | null };
  const libraryConfigure = vi.fn(async (_id: string, config: LaunchConfig) => {
    app = { ...app, launch_config: config, reviewed_digest: "approved-file" };
    return app;
  });
  const backend: AppApi = {
    ...createPreviewApi(),
    libraryList: async () => [app],
    libraryPrepareReview: async () => "prepared-review",
    libraryPrepareLaunch: async () => app,
    libraryLaunch: async () => app,
    libraryConfigure,
    ...patch,
  };
  return { backend, libraryConfigure, getApp: () => app };
}

async function openReview(backend: AppApi) {
  const user = userEvent.setup();
  render(<App backend={backend} />);
  await screen.findByRole("button", { name: "Launch Obsidian" });
  await user.click(screen.getByRole("button", { name: "Launch Obsidian" }));
  return { user, dialog: screen.getByRole("dialog") };
}

describe("app file approval and save feedback", () => {
  it("prepares approval while settings stay editable and explains verification, saving, and launching", async () => {
    const verification = deferred<string>();
    const saving = deferred<void>();
    const launching = deferred<void>();
    const { backend, getApp } = reviewBackend({
      libraryPrepareReview: () => verification.promise,
      async libraryConfigure(_id, config, reviewToken) {
        expect(reviewToken).toBe("prepared-file");
        await saving.promise;
        return { ...getApp(), launch_config: config, reviewed_digest: "approved-file" };
      },
      async libraryLaunch() {
        await launching.promise;
        return { ...getApp(), reviewed_digest: "approved-file" };
      },
    });
    const { user, dialog } = await openReview(backend);
    expect(within(dialog).getByText("Verifying app file…")).toBeInTheDocument();
    expect(within(dialog).getByText(/Large apps can take a few seconds/)).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Save and launch" })).toBeDisabled();
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeEnabled();
    await user.click(within(dialog).getByRole("button", { name: "Launch" }));
    await user.type(screen.getByLabelText("Command-line arguments"), "--profile=test");

    await act(async () => verification.resolve("prepared-file"));
    expect(within(dialog).getByText("Ready to approve")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Save and launch" }));
    expect(within(dialog).getByText("Saving settings…")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeDisabled();

    await act(async () => saving.resolve());
    expect(within(dialog).getByText("Verifying and starting app…")).toBeInTheDocument();
    expect(within(dialog).getByText(/Settings saved/)).toBeInTheDocument();
    await act(async () => launching.resolve());
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("retries failed verification without losing the settings draft", async () => {
    const libraryPrepareReview = vi.fn()
      .mockRejectedValueOnce(new Error("Cannot read app file"))
      .mockResolvedValueOnce("retried-review");
    const { backend, libraryConfigure } = reviewBackend({ libraryPrepareReview });
    const { user, dialog } = await openReview(backend);
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Cannot read app file");
    expect(within(dialog).getByRole("button", { name: "Save and launch" })).toBeDisabled();
    await user.click(within(dialog).getByRole("button", { name: "Launch" }));
    await user.type(screen.getByLabelText("Command-line arguments"), "--keep-this");
    await user.click(within(dialog).getByRole("button", { name: "Retry verification" }));
    await within(dialog).findByText("Ready to approve");
    expect(screen.getByLabelText("Command-line arguments")).toHaveValue("--keep-this");
    await user.click(within(dialog).getByRole("button", { name: "Save and launch" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(libraryConfigure).toHaveBeenCalledWith(expect.any(String), expect.objectContaining({ arguments: ["--keep-this"] }), "retried-review");
  });

  it("saves ordinary launch edits without preparing another approval", async () => {
    const preview = createPreviewApi();
    const libraryPrepareReview = vi.fn(async () => { throw new Error("Unexpected verification"); });
    const backend = { ...preview, libraryPrepareReview };
    const user = userEvent.setup();
    render(<App backend={backend} />);
    await screen.findByRole("button", { name: "Launch Obsidian" });
    await chooseFromMenu(user, "More options for Obsidian", "Settings");
    await user.click(screen.getByRole("button", { name: "Launch" }));
    await user.type(screen.getByLabelText("Command-line arguments"), "--quick-save");
    await user.click(screen.getByRole("button", { name: "Save changes" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect((await preview.libraryList())[0].launch_config?.arguments).toEqual(["--quick-save"]);
    expect(libraryPrepareReview).not.toHaveBeenCalled();
  });

  it("ignores verification from a closed review when another review opens", async () => {
    const oldVerification = deferred<string>();
    const newVerification = deferred<string>();
    const libraryPrepareReview = vi.fn()
      .mockReturnValueOnce(oldVerification.promise)
      .mockReturnValueOnce(newVerification.promise);
    const { backend, libraryConfigure } = reviewBackend({ libraryPrepareReview });
    const { user, dialog } = await openReview(backend);
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await user.click(screen.getByRole("button", { name: "Launch Obsidian" }));
    const reopened = screen.getByRole("dialog");

    await act(async () => oldVerification.resolve("old-review"));
    expect(within(reopened).getByText("Verifying app file…")).toBeInTheDocument();
    expect(within(reopened).getByRole("button", { name: "Save and launch" })).toBeDisabled();
    await act(async () => newVerification.resolve("new-review"));
    await user.click(within(reopened).getByRole("button", { name: "Save and launch" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(libraryConfigure).toHaveBeenCalledWith(expect.any(String), expect.any(Object), "new-review");
  });
});
