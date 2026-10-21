import { describe, expect, it } from "vitest";
import type { ContentItem } from "../../bindings/ContentItem";
import { applyFilter, dependentsOf, emptyFilter, isMetadataFiltered, sortByName } from "./contentFilter";

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

  it("status_filter_has_update_available", () => {
    const a = item({ relPath: "a", name: "Sodium" });
    const b = item({ relPath: "b", name: "Lithium", enabled: false });
    const c = item({ relPath: "c", name: "Iris" });
    const updatable = new Set(["a", "b"]);
    expect(applyFilter([a, b, c], { ...emptyFilter, status: ["update"] }, updatable).visible).toEqual([a, b]);
    // 他の状態とは「どれか」で組み合わせる
    expect(applyFilter([a, b, c], { ...emptyFilter, status: ["update", "enabled"] }, updatable).visible).toEqual([a, b, c]);
    expect(applyFilter([a, b, c], { ...emptyFilter, status: ["update"], query: "lith" }, updatable).visible).toEqual([b]);
    // 確認の結果が無ければ、どの行も「更新あり」に当たらない
    expect(applyFilter([a, b, c], { ...emptyFilter, status: ["update"] }).visible).toEqual([]);
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

  it("leaves out the target itself even if it lists its own project id", () => {
    const target = item({ relPath: "t", projectId: "P", dependencies: ["P"] });
    expect(dependentsOf([target], target)).toEqual([]);
  });

  it("is empty for an item without a project id", () => {
    const target = item({ relPath: "t", projectId: null });
    const other = item({ relPath: "o", dependencies: ["P"] });
    expect(dependentsOf([target, other], target)).toEqual([]);
  });
});

describe("sortByName", () => {
  it("orders by the shown name, ignoring case, and falls back to the file name", () => {
    const zeta = item({ relPath: "mods/a.jar", fileName: "a.jar", name: "zeta" });
    const alpha = item({ relPath: "mods/z.jar", fileName: "z.jar", name: "Alpha" });
    const unnamed = item({ relPath: "mods/m.jar", fileName: "mystery.jar", name: null });
    const beta = item({ relPath: "mods/b.jar", fileName: "b.jar", name: "beta" });
    const input = [zeta, alpha, unnamed, beta];
    expect(sortByName(input)).toEqual([alpha, beta, unnamed, zeta]);
    // 元の配列は並べ替えない
    expect(input).toEqual([zeta, alpha, unnamed, beta]);
  });

  it("breaks ties by the path so the order is stable", () => {
    const b = item({ relPath: "mods/b.jar", name: "Same" });
    const a = item({ relPath: "mods/a.jar", name: "same" });
    expect(sortByName([b, a])).toEqual([a, b]);
  });
});
