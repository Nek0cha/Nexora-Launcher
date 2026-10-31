import { describe, expect, it } from "vitest";
import { isMrpackPath } from "./mrpackPath";

describe("isMrpackPath", () => {
  it("accepts .mrpack files in any case, with spaces and non-ASCII names", () => {
    for (const path of ["a.mrpack", "A.MRPACK", "C:\\x y\\日本語.mrpack", "/home/me/pack.MrPack"]) {
      expect(isMrpackPath(path), path).toBe(true);
    }
  });

  it("rejects everything else", () => {
    for (const path of ["a.zip", "mrpack", ".mrpack.txt", "a.mrpack.txt", "C:\\x\\mrpack", ""]) {
      expect(isMrpackPath(path), path).toBe(false);
    }
  });
});
