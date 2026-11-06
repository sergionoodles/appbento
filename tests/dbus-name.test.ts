import { describe, expect, it } from "vitest";
import { validateDbusName } from "../src/lib/sandbox";

describe("validateDbusName", () => {
  it.each(["org.example.Service", "com.example-app.Main_1", "org.example.*", "a.b"])("accepts %s", (name) => {
    expect(validateDbusName(name, [], "talk")).toBeNull();
  });

  it.each([
    "", "single", ":1.42", "org.freedesktop.DBus", "org.freedesktop.*", "org.freedesktop.DBus.*",
    "org.1bad.Name", "org..Name", "org.exa mple.Name", "org.example.*.*", `a.${"b".repeat(260)}`,
  ])("rejects %j", (name) => {
    expect(validateDbusName(name, [], "talk")).not.toBeNull();
  });

  it("rejects duplicates of the same name and access only", () => {
    const existing = [{ name: "org.example.Service", access: "talk" as const }];
    expect(validateDbusName("org.example.Service", existing, "talk")).not.toBeNull();
    expect(validateDbusName("org.example.Service", existing, "own")).toBeNull();
  });

  it.each([
    "org.freedesktop.systemd1", "org.freedesktop.systemd1.*", "org.freedesktop.Flatpak",
    "org.kde.KWin", "org.kde.*", "org.gnome.Shell", "org.gnome.Shell.*", "org.gnome.*",
  ])("rejects host execution service grants for %s", (name) => {
    expect(validateDbusName(name, [], "talk")).not.toBeNull();
    expect(validateDbusName(name, [], "own")).not.toBeNull();
  });

  it.each(["org.kde.StatusNotifierWatcher", "org.gnome.ShellExtensions"])("accepts unrelated names %s", (name) => {
    expect(validateDbusName(name, [], "talk")).toBeNull();
  });
});
