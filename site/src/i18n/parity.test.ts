import { describe, expect, it } from "vitest";
import { en } from "./en";
import { ja } from "./ja";

/** 入れ子の辞書のキーを `a.b.c` の形で集める。配列は要素の数もキーに含める */
function keys(value: unknown, prefix = ""): string[] {
  if (Array.isArray(value)) return value.flatMap((v, i) => keys(v, `${prefix}[${i}]`));
  if (value && typeof value === "object")
    return Object.entries(value).flatMap(([k, v]) => keys(v, prefix ? `${prefix}.${k}` : k));
  return [prefix];
}

describe("i18n", () => {
  it("has the same keys in English and Japanese", () => {
    expect(keys(ja).sort()).toEqual(keys(en).sort());
  });

  it("writes バージョン, not 版, in Japanese", () => {
    expect(JSON.stringify(ja)).not.toMatch(/版/);
  });

  it("keeps the version placeholder in both languages", () => {
    expect(en.hero.version).toContain("{v}");
    expect(ja.hero.version).toContain("{v}");
  });
});
