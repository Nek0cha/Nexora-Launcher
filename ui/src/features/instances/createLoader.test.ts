import { describe, expect, it } from "vitest";
import { fallbackLoader, LOADER_TABS, pickLoaderVersion, tabState, versionsFor } from "./createLoader";

describe("LOADER_TABS", () => {
  it("lists the tabs in display order", () => {
    expect(LOADER_TABS).toEqual(["vanilla", "fabric", "forge", "neoforge", "quilt"]);
  });
});

describe("tabState", () => {
  it("always enables vanilla", () => {
    expect(tabState("vanilla", null, {})).toEqual({ enabled: true });
    expect(tabState("vanilla", "1.8.9", { fabric: [] })).toEqual({ enabled: true });
  });
  it("marks forge and neoforge as coming soon", () => {
    expect(tabState("forge", "1.20.1", {})).toEqual({ enabled: false, reason: "soon" });
    expect(tabState("neoforge", "1.20.1", { fabric: ["1.20.1"] })).toEqual({ enabled: false, reason: "soon" });
  });
  it("is loading while the game version list is not fetched", () => {
    expect(tabState("fabric", "1.20.1", {})).toEqual({ enabled: false, reason: "loading" });
    expect(tabState("quilt", "1.20.1", { quilt: undefined })).toEqual({ enabled: false, reason: "loading" });
  });
  it("is unsupported when the version is not in the list", () => {
    expect(tabState("fabric", "1.8.9", { fabric: ["1.20.1"] })).toEqual({ enabled: false, reason: "unsupportedVersion" });
  });
  it("is enabled when the version is in the list", () => {
    expect(tabState("quilt", "1.20.1", { quilt: ["1.20.1"] })).toEqual({ enabled: true });
  });
});

describe("versionsFor", () => {
  const vanilla = ["1.21.5", "1.20.1", "1.8.9"];
  it("returns vanilla unchanged for vanilla", () => {
    expect(versionsFor("vanilla", vanilla, ["1.20.1"])).toEqual(vanilla);
  });
  it("intersects with the vanilla order kept", () => {
    expect(versionsFor("fabric", vanilla, ["1.20.1", "1.21.5"])).toEqual(["1.21.5", "1.20.1"]);
  });
  it("is empty while the list is not fetched", () => {
    expect(versionsFor("fabric", vanilla, undefined)).toEqual([]);
  });
});

describe("fallbackLoader", () => {
  it("falls back to vanilla when the tab is no longer selectable", () => {
    expect(fallbackLoader("fabric", "1.8.9", { fabric: ["1.20.1"] })).toBe("vanilla");
  });
  it("keeps the current loader when selectable", () => {
    expect(fallbackLoader("fabric", "1.20.1", { fabric: ["1.20.1"] })).toBe("fabric");
    expect(fallbackLoader("vanilla", "1.8.9", {})).toBe("vanilla");
  });
});

describe("pickLoaderVersion", () => {
  const entries = [
    { version: "0.17.0-beta", stable: false },
    { version: "0.16.9", stable: true },
    { version: "0.16.8", stable: true },
  ];
  it("keeps the current version when present", () => {
    expect(pickLoaderVersion(entries, "0.16.8")).toBe("0.16.8");
    expect(pickLoaderVersion(entries, "0.17.0-beta")).toBe("0.17.0-beta");
  });
  it("prefers the first stable entry otherwise", () => {
    expect(pickLoaderVersion(entries, null)).toBe("0.16.9");
    expect(pickLoaderVersion(entries, "gone")).toBe("0.16.9");
  });
  it("falls back to the first entry when none is stable", () => {
    expect(pickLoaderVersion([{ version: "a", stable: false }, { version: "b", stable: false }], null)).toBe("a");
  });
  it("returns null for an empty list", () => {
    expect(pickLoaderVersion([], "x")).toBeNull();
  });
});
