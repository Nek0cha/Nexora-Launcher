import { describe, expect, it } from "vitest";
import { upgradeTargets } from "./upgradeVersions";

const vanilla = ["26.3", "1.21.5", "1.21.1", "1.20.1", "1.8.9"];

describe("upgradeTargets", () => {
  it("lists only the releases newer than the current one, newest first", () => {
    expect(upgradeTargets("1.20.1", "vanilla", vanilla, undefined)).toEqual(["26.3", "1.21.5", "1.21.1"]);
  });
  it("is empty on the newest release", () => {
    expect(upgradeTargets("26.3", "vanilla", vanilla, undefined)).toEqual([]);
  });
  it("is empty when the current version is not a known release", () => {
    expect(upgradeTargets("24w01a", "vanilla", vanilla, undefined)).toEqual([]);
  });
  it("keeps only the versions the loader supports", () => {
    expect(upgradeTargets("1.20.1", "fabric", vanilla, ["1.21.5", "1.20.1"])).toEqual(["1.21.5"]);
  });
  it("is empty while the loader's versions are not fetched", () => {
    expect(upgradeTargets("1.20.1", "forge", vanilla, undefined)).toEqual([]);
  });
});
