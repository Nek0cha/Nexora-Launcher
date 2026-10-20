import { describe, expect, it } from "vitest";
import { fallbackLoader, LOADER_TABS, pickLoaderVersion, tabState, versionForLoader, versionsFor } from "./createLoader";

describe("LOADER_TABS", () => {
  it("lists the tabs in display order", () => {
    expect(LOADER_TABS).toEqual(["vanilla", "fabric", "forge", "neoforge", "quilt"]);
  });
});

describe("tabState", () => {
  const vanilla = ["26.3", "1.21.5", "1.20.1", "1.8.9"];
  it("always enables vanilla", () => {
    expect(tabState("vanilla", [], {})).toEqual({ enabled: true });
    expect(tabState("vanilla", vanilla, { fabric: [] })).toEqual({ enabled: true });
  });
  it("is loading while the game version list is not fetched", () => {
    expect(tabState("fabric", vanilla, {})).toEqual({ enabled: false, reason: "loading" });
    expect(tabState("quilt", vanilla, { quilt: undefined })).toEqual({ enabled: false, reason: "loading" });
    expect(tabState("neoforge", vanilla, {})).toEqual({ enabled: false, reason: "loading" });
  });
  it("has no versions when nothing in the list matches a known vanilla version", () => {
    expect(tabState("fabric", vanilla, { fabric: ["1.7.10"] })).toEqual({ enabled: false, reason: "noVersions" });
    expect(tabState("forge", vanilla, { forge: [] })).toEqual({ enabled: false, reason: "noVersions" });
  });
  it("is enabled when the loader supports any known version, even if the newest is unsupported", () => {
    expect(tabState("forge", vanilla, { forge: ["1.21.5", "1.20.1"] })).toEqual({ enabled: true });
    expect(tabState("neoforge", vanilla, { neoforge: ["1.21.5"] })).toEqual({ enabled: true });
  });
});

describe("versionForLoader", () => {
  const vanilla = ["26.3", "1.21.5", "1.20.1"];
  it("keeps the current version when the loader supports it", () => {
    expect(versionForLoader("forge", "1.20.1", vanilla, { forge: ["1.21.5", "1.20.1"] })).toBe("1.20.1");
  });
  it("always keeps the current version for vanilla", () => {
    expect(versionForLoader("vanilla", "26.3", vanilla, {})).toBe("26.3");
  });
  it("moves to the newest supported version otherwise", () => {
    expect(versionForLoader("forge", "26.3", vanilla, { forge: ["1.20.1", "1.21.5"] })).toBe("1.21.5");
  });
  it("returns null when nothing is supported or the list is not fetched", () => {
    expect(versionForLoader("forge", "26.3", vanilla, { forge: ["1.7.10"] })).toBeNull();
    expect(versionForLoader("forge", "26.3", vanilla, {})).toBeNull();
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
  const vanilla = ["1.21.5", "1.20.1", "1.8.9"];
  it("falls back to vanilla when the loader no longer supports the version", () => {
    expect(fallbackLoader("fabric", "1.8.9", vanilla, { fabric: ["1.20.1"] })).toBe("vanilla");
  });
  it("keeps the current loader when it supports the version", () => {
    expect(fallbackLoader("fabric", "1.20.1", vanilla, { fabric: ["1.20.1"] })).toBe("fabric");
    expect(fallbackLoader("vanilla", "1.8.9", vanilla, {})).toBe("vanilla");
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
