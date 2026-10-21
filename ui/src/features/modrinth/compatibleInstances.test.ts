import { describe, expect, it } from "vitest";
import type { Instance } from "../../bindings/Instance";
import type { LoaderKind } from "../../bindings/LoaderKind";
import type { SearchHit } from "../../bindings/SearchHit";
import { compatibleInstances } from "./compatibleInstances";

const inst = (id: string, mcVersion: string, loader: LoaderKind): Instance => ({ id, mcVersion, loader }) as Instance;

const hit = (categories: string[], versions: string[]): SearchHit =>
  ({ projectId: "p", slug: "s", title: "t", description: "", iconUrl: null, downloads: 0, categories, versions, clientSide: null, serverSide: null }) as SearchHit;

describe("compatibleInstances", () => {
  const instances = [
    inst("fabric120", "1.20.1", "fabric"),
    inst("quilt120", "1.20.1", "quilt"),
    inst("forge120", "1.20.1", "forge"),
    inst("fabric121", "1.21", "fabric"),
    inst("vanilla120", "1.20.1", "vanilla"),
  ];

  it("returns fabric and quilt instances of the same Minecraft version for a fabric mod", () => {
    const ids = compatibleInstances(hit(["fabric", "utility"], ["1.20.1"]), instances).map((i) => i.id);
    expect(ids).toEqual(["fabric120", "quilt120"]);
  });

  it("never returns vanilla instances", () => {
    expect(compatibleInstances(hit(["fabric", "forge"], ["1.20.1"]), instances).map((i) => i.id)).not.toContain("vanilla120");
  });

  it("matches the loader exactly for forge", () => {
    expect(compatibleInstances(hit(["forge"], ["1.20.1"]), instances).map((i) => i.id)).toEqual(["forge120"]);
  });

  it("returns nothing when the version is missing", () => {
    expect(compatibleInstances(hit(["fabric"], ["1.19"]), instances)).toEqual([]);
  });
});
