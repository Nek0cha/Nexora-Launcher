import { describe, expect, it } from "vitest";
import { figureParts, pngHeight } from "./skinFigure";

// 署名 + IHDR（幅 64、高さ h）だけの PNG の先頭
function png(h: number): string {
  const bytes = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, 0x49, 0x48, 0x44, 0x52, 0, 0, 0, 64, 0, 0, 0, h, 8, 6, 0, 0, 0];
  return `data:image/png;base64,${btoa(String.fromCharCode(...bytes))}`;
}

describe("pngHeight", () => {
  it("reads the height of 64x64 and 64x32 images", () => {
    expect(pngHeight(png(64))).toBe(64);
    expect(pngHeight(png(32))).toBe(32);
  });

  it("returns null for anything that is not a PNG data URL", () => {
    expect(pngHeight(null)).toBeNull();
    expect(pngHeight("data:image/png;base64,AAAA")).toBeNull();
    expect(pngHeight("https://example.com/a.png")).toBeNull();
    expect(pngHeight("data:image/png;base64,!!!!")).toBeNull();
  });
});

describe("figureParts", () => {
  it("uses 4px arms for classic and 3px arms for slim", () => {
    const arms = (v: "classic" | "slim") => figureParts(64, v).filter((p) => !p.overlay && (p.tex.x === 44 || p.tex.x === 36));
    expect(arms("classic").map((p) => p.tex.w)).toEqual([4, 4]);
    expect(arms("slim").map((p) => p.tex.w)).toEqual([3, 3]);
  });

  it("puts the arms against the torso", () => {
    const left = figureParts(64, "slim").find((p) => !p.overlay && p.tex.x === 44);
    expect(left?.box).toEqual({ x: 1, y: 8, w: 3, h: 12 });
  });

  it("reuses the right limbs mirrored and has only the hat layer for 64x32", () => {
    const parts = figureParts(32, "classic");
    expect(parts.filter((p) => p.mirror)).toHaveLength(2);
    expect(parts.filter((p) => p.overlay).map((p) => p.tex)).toEqual([{ x: 40, y: 8, w: 8, h: 8 }]);
  });

  it("draws every base part before any overlay", () => {
    const parts = figureParts(64, "classic");
    const firstOverlay = parts.findIndex((p) => p.overlay);
    expect(parts.slice(firstOverlay).every((p) => p.overlay)).toBe(true);
    expect(parts).toHaveLength(12);
  });
});
