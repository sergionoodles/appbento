import { describe, expect, it } from "vitest";
import { paletteFromName, paletteFromPixels } from "../src/lib/app-palette";

describe("name palettes", () => {
  it("assigns stable, pleasant colors from app names", () => {
    expect(paletteFromName(" Obsidian ")).toEqual(paletteFromName("obsidian"));
    expect(paletteFromName("Obsidian")).not.toEqual(paletteFromName("FreeCAD"));
    expect(paletteFromName("Obsidian").accent).toMatch(/^hsl\(\d+ 62% 64%\)$/);
    expect(paletteFromName("").accent).toMatch(/^hsl\(\d+ 62% 64%\)$/);
  });
});

describe("icon palettes", () => {
  it("extracts the dominant logo color while ignoring transparent pixels and white padding", () => {
    const pixels = [
      ...Array.from({ length: 30 }, () => [255, 255, 255, 255]).flat(),
      ...Array.from({ length: 50 }, () => [0, 255, 0, 0]).flat(),
      220, 28, 40, 255,
      222, 30, 42, 255,
      20, 30, 220, 255,
    ];
    expect(paletteFromPixels(pixels)).toEqual({ accent: "rgb(221 29 41)", onAccent: "#ffffff" });
  });

  it("weights partially transparent colors by their visibility", () => {
    expect(paletteFromPixels([255, 0, 0, 40, 0, 0, 255, 255])?.accent).toBe("rgb(0 0 255)");
  });

  it("handles monochrome icons with contrasting foreground colors", () => {
    expect(paletteFromPixels([255, 255, 255, 255])).toEqual({ accent: "rgb(255 255 255)", onAccent: "#000000" });
    expect(paletteFromPixels([0, 0, 0, 255])).toEqual({ accent: "rgb(0 0 0)", onAccent: "#ffffff" });
  });

  it("falls back for missing or fully transparent image content", () => {
    expect(paletteFromPixels([])).toBeNull();
    expect(paletteFromPixels([200, 50, 20, 0])).toBeNull();
  });
});
