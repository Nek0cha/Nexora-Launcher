import { describe, expect, it } from "vitest";
import { detectLanguage, resources } from "./index";

function keys(obj: unknown, prefix = ""): string[] {
  if (typeof obj !== "object" || obj === null) return [prefix];
  return Object.entries(obj).flatMap(([k, v]) => keys(v, prefix ? `${prefix}.${k}` : k));
}

describe("i18n", () => {
  it("ja and en have the same keys", () => {
    expect(keys(resources.en).sort()).toEqual(keys(resources.ja).sort());
  });

  it("detects Japanese from navigator language", () => {
    expect(detectLanguage("ja")).toBe("ja");
    expect(detectLanguage("ja-JP")).toBe("ja");
    expect(detectLanguage("en-US")).toBe("en");
    expect(detectLanguage("fr")).toBe("en");
  });
});
