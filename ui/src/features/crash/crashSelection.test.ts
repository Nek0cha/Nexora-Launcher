import { describe, expect, it } from "vitest";
import type { CrashCandidate } from "../../bindings/CrashCandidate";
import { initialChecked } from "./crashSelection";

const candidate = (relPath: string, confidence: "high" | "low"): CrashCandidate => ({
  relPath, name: relPath, modId: relPath, reasons: ["mixinFailure"], confidence, evidence: "", missing: null,
});

describe("initialChecked", () => {
  it("checks the high confidence candidates only", () => {
    const list = [candidate("mods/a.jar", "high"), candidate("mods/b.jar", "low"), candidate("mods/c.jar", "high")];
    expect(initialChecked(list)).toEqual(["mods/a.jar", "mods/c.jar"]);
  });

  it("checks nothing when every candidate has low confidence", () => {
    expect(initialChecked([candidate("mods/b.jar", "low")])).toEqual([]);
  });
});
