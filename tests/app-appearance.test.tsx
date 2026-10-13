import { renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { IconBackendContext } from "../src/components/icon-backend-context";
import { useAppAppearance } from "../src/components/use-app-appearance";
import { paletteFromName } from "../src/lib/app-palette";
import { createPreviewApi } from "../src/lib/preview";
import type { AppApi } from "../src/lib/api";

beforeEach(() => {
  class TestImage {
    naturalWidth = 32;
    naturalHeight = 32;
    onload: (() => void) | null = null;
    onerror: (() => void) | null = null;
    set src(source: string) {
      queueMicrotask(() => {
        if (source === "broken") this.onerror?.();
        else this.onload?.();
      });
    }
  }
  vi.stubGlobal("Image", TestImage);
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({
    drawImage: () => undefined,
    getImageData: () => ({ data: new Uint8ClampedArray([30, 90, 220, 255]) }),
  } as unknown as CanvasRenderingContext2D);
});

afterEach(() => vi.unstubAllGlobals());

function wrapper(backend: AppApi) {
  return function Backend({ children }: { children: React.ReactNode }) {
    return <IconBackendContext.Provider value={backend}>{children}</IconBackendContext.Provider>;
  };
}

it("derives colors from saved icons, shares loads, and refreshes when the icon changes", async () => {
  const preview = createPreviewApi();
  const [original] = await preview.libraryList();
  const libraryIcon = vi.fn(async () => "data:image/png;base64,example");
  const backend = { ...preview, libraryIcon };
  const app = { ...original, icon_path: "/icons/first.png" };
  const { result, rerender } = renderHook(({ current }) => {
    const first = useAppAppearance(current.name, current);
    const second = useAppAppearance(current.name, current);
    return { first, second };
  }, { wrapper: wrapper(backend), initialProps: { current: app } });

  expect(result.current.first.palette).toEqual(paletteFromName(app.name));
  await waitFor(() => expect(result.current.first.palette.accent).toBe("rgb(30 90 220)"));
  expect(result.current.second.palette).toEqual(result.current.first.palette);
  expect(libraryIcon).toHaveBeenCalledTimes(1);

  libraryIcon.mockResolvedValue("broken");
  rerender({ current: { ...app, icon_path: "/icons/second.png" } });
  expect(result.current.first.palette).toEqual(paletteFromName(app.name));
  await waitFor(() => expect(result.current.first.error).toContain("could not be read"));
  expect(result.current.first.palette).toEqual(paletteFromName(app.name));
  expect(libraryIcon).toHaveBeenCalledTimes(2);
});

it("keeps name colors and exposes an error when loading the saved icon fails", async () => {
  const preview = createPreviewApi();
  const [original] = await preview.libraryList();
  const app = { ...original, icon_path: "/missing.png" };
  const backend = { ...preview, async libraryIcon() { throw new Error("Icon missing"); } };
  const { result } = renderHook(() => useAppAppearance(app.name, app), { wrapper: wrapper(backend) });
  await waitFor(() => expect(result.current.error).toContain("Icon missing"));
  expect(result.current.image).toBeNull();
  expect(result.current.palette).toEqual(paletteFromName(app.name));
});

it("shows the new icon when an update returns a new saved icon path", async () => {
  const preview = createPreviewApi();
  const [original] = await preview.libraryList();
  const app = { ...original, version_current: "1.0.0", icon_path: "/icons/old.svg" };
  const updated = { ...app, version_current: "2.0.0", icon_path: "/icons/new.svg" };
  const oldImage = "data:image/svg+xml;base64,old";
  const newImage = "data:image/svg+xml;base64,new";
  let currentApp = app;
  const backend = {
    ...preview,
    async updatesInstall() { return updated; },
    async libraryIcon() { return currentApp.icon_path === app.icon_path ? oldImage : newImage; },
  };
  const { result, rerender } = renderHook(({ current }) => useAppAppearance(current.name, current), {
    wrapper: wrapper(backend), initialProps: { current: app },
  });
  await waitFor(() => expect(result.current.image).toBe(oldImage));

  currentApp = await backend.updatesInstall();
  rerender({ current: currentApp });

  await waitFor(() => expect(result.current.image).toBe(newImage));
});

it("assigns name colors when there is no icon, including after renaming", async () => {
  const preview = createPreviewApi();
  const { result, rerender } = renderHook(({ name }) => useAppAppearance(name), {
    wrapper: wrapper(preview), initialProps: { name: "Example" },
  });
  expect(result.current.palette).toEqual(paletteFromName("Example"));
  rerender({ name: "Another app" });
  expect(result.current.palette).toEqual(paletteFromName("Another app"));
  await waitFor(() => expect(result.current.image).toBeNull());
});
