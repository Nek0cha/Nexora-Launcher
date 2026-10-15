import { describe, expect, it, vi } from "vitest";
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

describe("language hint", () => {
  it("setLanguage writes the hint and the html lang", async () => {
    localStorage.clear();
    const { setLanguage } = await import("./index");
    await setLanguage("en");
    expect(localStorage.getItem("nexora.lang")).toBe("en");
    expect(document.documentElement.lang).toBe("en");
  });

  it("uses the hint for the initial language when present", async () => {
    localStorage.setItem("nexora.lang", "ja");
    vi.resetModules();
    const fresh = await import("./index");
    expect(fresh.default.language).toBe("ja");
    localStorage.setItem("nexora.lang", "en");
    vi.resetModules();
    expect((await import("./index")).default.language).toBe("en");
    localStorage.clear();
  });

  it("ignores an unknown hint", async () => {
    localStorage.setItem("nexora.lang", "fr");
    vi.resetModules();
    expect(["ja", "en"]).toContain((await import("./index")).default.language);
    localStorage.clear();
  });
});
