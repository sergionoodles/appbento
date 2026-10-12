import { act, render, renderHook, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import App from "../src/App";
import { createPreviewApi } from "../src/lib/preview";
import { createSelfUpdateController, SELF_UPDATE_INTERVAL, type SelfUpdateCheck, type SelfUpdateProgress } from "../src/lib/self-update";
import { useSelfUpdate } from "../src/lib/use-self-update";
import { SelfUpdateDialog } from "../src/components/dialogs/self-update-dialog";

const available: SelfUpdateCheck = {
  currentVersion: "0.1.0", unavailableReason: null,
  update: { version: "0.2.0", notes: "Improved updates", downloadUrl: "https://example.com/AppBento.AppImage", installDisabledReason: null },
};

afterEach(() => vi.useRealTimers());

it("checks once at startup, polls, and stops polling on unmount", async () => {
  vi.useFakeTimers();
  let checks = 0;
  const backend = { ...createPreviewApi(), isTauri: true, async selfUpdateCheck() { checks++; return available; } };
  const { result, unmount } = renderHook(() => useSelfUpdate(backend));
  await act(async () => {});
  expect(result.current.result).toEqual(available);
  expect(checks).toBe(1);
  await act(async () => { await vi.advanceTimersByTimeAsync(SELF_UPDATE_INTERVAL); });
  expect(checks).toBe(2);
  unmount();
  await vi.advanceTimersByTimeAsync(SELF_UPDATE_INTERVAL);
  expect(checks).toBe(2);
});

it("does not check or install in browser preview", async () => {
  const controller = createSelfUpdateController(createPreviewApi());
  await controller.check();
  await controller.install(false);
  expect(controller.getState()).toEqual({ result: null, isChecking: false, progress: null, error: null });
});

it("retains an available update after a failed check and allows retrying", async () => {
  let hasFailed = false;
  const controller = createSelfUpdateController({ ...createPreviewApi(), isTauri: true,
    async selfUpdateCheck() { if (hasFailed) throw new Error("Offline"); return available; },
  });
  await controller.check();
  hasFailed = true;
  await controller.check();
  expect(controller.getState()).toMatchObject({ result: available, error: "Offline", isChecking: false });
  hasFailed = false;
  await controller.check();
  expect(controller.getState().error).toBeNull();
});

it("blocks installs during library activity and prevents duplicate installs or checks during download", async () => {
  let installs = 0;
  let finishInstall: () => void = () => {};
  const controller = createSelfUpdateController({ ...createPreviewApi(), isTauri: true,
    async selfUpdateCheck() { return available; },
    async selfUpdateInstall(onProgress) {
      installs++;
      onProgress({ phase: "downloading", downloaded: 50, total: 100 });
      await new Promise<void>((resolve) => { finishInstall = resolve; });
    },
  });
  await controller.check();
  await controller.install(true);
  expect(installs).toBe(0);
  const installation = controller.install(false);
  await controller.install(false);
  await controller.check();
  expect(installs).toBe(1);
  expect(controller.getState().progress).toEqual({ phase: "downloading", downloaded: 50, total: 100 });
  finishInstall();
  await installation;
  expect(controller.getState().progress).toEqual({ phase: "restarting" });
});

it("preserves the available update after installation failure and can retry", async () => {
  let shouldFail = true;
  const controller = createSelfUpdateController({ ...createPreviewApi(), isTauri: true,
    async selfUpdateCheck() { return available; },
    async selfUpdateInstall() { if (shouldFail) throw new Error("Signature verification failed"); },
  });
  await controller.check();
  await controller.install(false);
  expect(controller.getState()).toMatchObject({ result: available, progress: null, error: "Signature verification failed" });
  shouldFail = false;
  await controller.install(false);
  expect(controller.getState()).toMatchObject({ progress: { phase: "restarting" }, error: null });
});

it("shows the cog badge, release notes, download progress, and one-click install from Settings", async () => {
  const user = userEvent.setup();
  let reportProgress: (progress: SelfUpdateProgress) => void = () => {};
  let finishInstall: () => void = () => {};
  render(<App backend={{ ...createPreviewApi(), isTauri: true,
    async selfUpdateCheck() { return available; },
    async selfUpdateInstall(onProgress) {
      reportProgress = onProgress;
      onProgress({ phase: "downloading", downloaded: 50, total: 100 });
      await new Promise<void>((resolve) => { finishInstall = resolve; });
    },
  }} />);
  expect(await screen.findByRole("status", { name: "AppBento 0.2.0 update available" })).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Settings" }));
  await user.click(screen.getByRole("menuitem", { name: "Update available · v0.2.0" }));
  expect(screen.getByLabelText("Release notes")).toHaveTextContent("Improved updates");
  await waitFor(() => expect(screen.getByRole("button", { name: "Install update and restart" })).toBeEnabled());
  await user.click(screen.getByRole("button", { name: "Install update and restart" }));
  expect(screen.getByText("Downloading update… 50%")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Close" })).toBeDisabled();
  await user.keyboard("{Escape}");
  expect(screen.getByRole("dialog", { name: "AppBento updates" })).toBeInTheDocument();
  act(() => reportProgress({ phase: "installing" }));
  expect(screen.getByText("Installing update…")).toBeInTheDocument();
  await act(async () => { finishInstall(); });
  expect(screen.getByText("Restarting AppBento…")).toBeInTheDocument();
});

it("offers a manual download for a protected AppImage and blocks automatic installation", async () => {
  const user = userEvent.setup();
  const downloads: string[] = [];
  const result = { ...available, update: { ...available.update, version: "0.2.0", notes: null, downloadUrl: "https://example.com/AppBento.AppImage", installDisabledReason: "Move the AppImage to a writable folder." } };
  const controller = createSelfUpdateController({ ...createPreviewApi(), isTauri: true,
    async selfUpdateCheck() { return result; },
    async selfUpdateInstall() { throw new Error("Must not install"); },
  });
  await controller.check();
  await controller.install(false);
  expect(controller.getState().progress).toBeNull();
  render(<SelfUpdateDialog state={controller.getState()} isBlocked={false} onCheck={() => {}} onInstall={() => {}}
    onDownload={async (url) => { downloads.push(url); }} onClose={() => {}} returnFocusElement={null} />);
  expect(screen.getByRole("button", { name: "Install update and restart" })).toBeDisabled();
  await user.click(screen.getByRole("button", { name: "Download manually" }));
  expect(downloads).toEqual([result.update.downloadUrl]);
});

it("reports up-to-date and failed checks with a retry action", async () => {
  const user = userEvent.setup();
  let shouldFail = false;
  render(<App backend={{ ...createPreviewApi(), isTauri: true,
    async selfUpdateCheck() {
      if (shouldFail) throw new Error("Network unavailable");
      return { currentVersion: "0.1.0", update: null, unavailableReason: null };
    },
  }} />);
  await user.click(screen.getByRole("button", { name: "Settings" }));
  await user.click(screen.getByRole("menuitem", { name: "Check for AppBento updates" }));
  expect(await screen.findByText("AppBento is up to date.")).toBeInTheDocument();
  shouldFail = true;
  await user.click(screen.getByRole("button", { name: "Check for updates" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Network unavailable");
  shouldFail = false;
  await user.click(screen.getByRole("button", { name: "Check for updates" }));
  await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
});
