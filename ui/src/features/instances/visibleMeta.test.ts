import { describe, expect, it } from "vitest";
import { META_MIN_WIDTH, visibleMeta } from "./visibleMeta";

describe("visibleMeta", () => {
  it("shows everything in display order when there is room", () => {
    expect(visibleMeta(10_000)).toEqual(["version", "state", "lastPlayed", "playTime"]);
  });

  it("always keeps the state, even with no room at all", () => {
    expect(visibleMeta(0)).toEqual(["state"]);
    expect(visibleMeta(-5)).toEqual(["state"]);
  });

  it("hides play time first, then last played, then version", () => {
    const order = ["playTime", "lastPlayed", "version"] as const;
    // 幅を狭めていくと、この順に消える
    const hiddenAt = (key: (typeof order)[number]) => META_MIN_WIDTH[key];
    expect(hiddenAt("playTime")).toBeGreaterThan(hiddenAt("lastPlayed"));
    expect(hiddenAt("lastPlayed")).toBeGreaterThan(hiddenAt("version"));
    let shown = visibleMeta(10_000).length;
    for (const key of order) {
      const next = visibleMeta(META_MIN_WIDTH[key] - 1);
      expect(next).not.toContain(key);
      expect(next.length).toBeLessThan(shown);
      shown = next.length;
    }
  });

  it("shows a piece exactly at its minimum width", () => {
    for (const key of ["version", "lastPlayed", "playTime"] as const) {
      expect(visibleMeta(META_MIN_WIDTH[key])).toContain(key);
      expect(visibleMeta(META_MIN_WIDTH[key] - 1)).not.toContain(key);
    }
  });

  it("is monotonic: a wider row never shows less", () => {
    let prev = 0;
    for (let w = 0; w <= 1000; w += 5) {
      const n = visibleMeta(w).length;
      expect(n).toBeGreaterThanOrEqual(prev);
      prev = n;
    }
  });
});
