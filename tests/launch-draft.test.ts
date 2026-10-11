import { describe, expect, it } from "vitest";
import { launchDraftFromConfig, parseLaunchDraft } from "../src/lib/launch-draft";
import { restrictedPermissions } from "../src/lib/sandbox";

describe("launch settings", () => {
  it("preserves literal arguments and environment values", () => {
    expect(parseLaunchDraft({ arguments: "--profile\nMy profile\n$(literal)\n", environment: "LABEL= two words = value \nEMPTY=\n\n" })).toEqual({
      arguments: ["--profile", "My profile", "$(literal)"],
      environment: { LABEL: " two words = value ", EMPTY: "" },
    });
  });

  it("reads old configurations and clears options", () => {
    expect(launchDraftFromConfig({ mode: "direct", permissions: restrictedPermissions() })).toEqual({ arguments: "", environment: "" });
    expect(parseLaunchDraft({ arguments: "", environment: "" })).toEqual({ arguments: [], environment: {} });
  });

  it.each(["MISSING", "=value", "1NAME=value", "BAD-NAME=value", "NAME=one\nNAME=two", "NAME=bad\0value"])("rejects invalid environment settings: %s", (environment) => {
    expect(() => parseLaunchDraft({ arguments: "", environment })).toThrow();
  });

  it("rejects null characters in arguments", () => {
    expect(() => parseLaunchDraft({ arguments: "bad\0argument", environment: "" })).toThrow();
  });
});
