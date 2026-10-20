import { describe, expect, it } from "vitest";
import type { ContentItem } from "../../bindings/ContentItem";
import { applyFilter, dependentsOf, emptyFilter, isMetadataFiltered } from "./contentFilter";

function item(over: Partial<ContentItem>): ContentItem {
  return {
    kind: "mod",
    relPath: "a.jar",
    fileName: "a.jar",
    enabled: true,
    size: 1,
    sha1: "x",
    provider: "modrinth",
    projectId: null,
    versionId: null,
    pinned: false,
    installedAs: "explicit",
    name: null,
    versionNumber: null,
    description: null,
    iconUrl: null,
    hasLocalIcon: false,
    clientSide: null,
    serverSide: null,
    categories: [],
    dependencies: [],
    identified: true,
    ...over,
  };
}

describe("applyFilter", () => {
  it("matches the name case-insensitively and falls back to the file name", () => {
    const a = item({ relPath: "a", name: "Sodium" });
    const b = item({ relPath: "b", name: null, fileName: "IrisShaders.jar" });
    const c = item({ relPath: "c", name: "Lithium" });
    expect(applyFilter([a, b, c], { ...emptyFilter, query: "sod" }).visible).toEqual([a]);
    expect(applyFilter([a, b, c], { ...emptyFilter, query: "IRIS" }).visible).toEqual([b]);
  });

  it("requires every selected category", () => {
    const a = item({ relPath: "a", categories: ["x", "y"] });
    const b = item({ relPath: "b", categories: ["x"] });
    expect(applyFilter([a, b], { ...emptyFilter, categories: ["x", "y"] }).visible).toEqual([a]);
  });

  it("client and server accept any of the selected sides", () => {
    const a = item({ relPath: "a", clientSide: "required", serverSide: "optional" });
    const b = item({ relPath: "b", clientSide: "optional", serverSide: "unsupported" });
    const c = item({ relPath: "c", clientSide: null, serverSide: null });
    const client = applyFilter([a, b, c], { ...emptyFilter, client: ["required", "optional"] });
    expect(client.visible).toEqual([a, b]);
    const server = applyFilter([a, b, c], { ...emptyFilter, server: ["unsupported"] });
    expect(server.visible).toEqual([b]);
  });

  it("hides unidentified items only while a metadata filter is active and reports them as excluded", () => {
    const a = item({ relPath: "a", clientSide: "required" });
    const local = item({ relPath: "l", provider: "local" });
    expect(applyFilter([a, local], emptyFilter).visible).toEqual([a, local]);
    const result = applyFilter([a, local], { ...emptyFilter, client: ["required"] });
    expect(result.visible).toEqual([a]);
    expect(result.excluded).toEqual([local]);
    expect(isMetadataFiltered(emptyFilter)).toBe(false);
    expect(isMetadataFiltered({ ...emptyFilter, categories: ["x"] })).toBe(true);
  });

  it("does not count name mismatches as excluded", () => {
    const local = item({ relPath: "l", provider: "local", name: "Foo" });
    const result = applyFilter([local], { ...emptyFilter, query: "bar", server: ["required"] });
    expect(result.visible).toEqual([]);
    expect(result.excluded).toEqual([]);
  });

  it("status filter combines with the others", () => {
    const on = item({ relPath: "1", name: "Aa" });
    const off = item({ relPath: "2", name: "Ab", enabled: false });
    const dep = item({ relPath: "3", name: "Ac", installedAs: "dependency" });
    const other = item({ relPath: "4", name: "Zz", enabled: false });
    const all = [on, off, dep, other];
    expect(applyFilter(all, { ...emptyFilter, status: ["disabled"] }).visible).toEqual([off, other]);
    expect(applyFilter(all, { ...emptyFilter, status: ["disabled", "dependency"], query: "a" }).visible).toEqual([off, dep]);
  });
});

describe("dependentsOf", () => {
  it("lists only enabled items that depend on the project", () => {
    const target = item({ relPath: "t", projectId: "P" });
    const yes = item({ relPath: "y", dependencies: ["P"] });
    const off = item({ relPath: "o", dependencies: ["P"], enabled: false });
    const no = item({ relPath: "n", dependencies: ["Q"] });
    expect(dependentsOf([target, yes, off, no], target)).toEqual([yes]);
  });

  it("is empty for an item without a project id", () => {
    const target = item({ relPath: "t", projectId: null });
    const other = item({ relPath: "o", dependencies: ["P"] });
    expect(dependentsOf([target, other], target)).toEqual([]);
  });
});
