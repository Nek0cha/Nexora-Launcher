import { describe, expect, it, vi } from "vitest";
import { detectLanguage, resources } from "./index";

function keys(obj: unknown, prefix = ""): string[] {
  if (typeof obj !== "object" || obj === null) return [prefix];
  return Object.entries(obj).flatMap(([k, v]) => keys(v, prefix ? `${prefix}.${k}` : k));
}

const sources = import.meta.glob(["../**/*.{ts,tsx}", "!../**/*.test.{ts,tsx}", "!../bindings/**"], {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;
const sourceText = Object.values(sources).join("\n");
// t(`prefix.${x}`) のように後ろを組み立てるキーは、前の部分で参照されているとみなす
const dynamicPrefixes = [...sourceText.matchAll(/`([\w.-]*\.)\$\{/g)].map((m) => m[1] as string);

function isReferenced(key: string): boolean {
  // 複数形（_one、_other）と、数を足した形（…Percent、…Count）は、元のキーの参照で使われる
  const plain = key.replace(/_(one|other)$/, "");
  const names = [plain, plain.replace(/(Percent|Count)$/, "")];
  const quoted = names.flatMap((n) => [`"${n}"`, `'${n}'`, `\`${n}\``]);
  return quoted.some((q) => sourceText.includes(q)) || dynamicPrefixes.some((p) => key.startsWith(p));
}

describe("i18n", () => {
  it("ja and en have the same keys", () => {
    expect(keys(resources.en).sort()).toEqual(keys(resources.ja).sort());
  });

  it("every key is used by the UI", () => {
    // errors は、バックエンドが返すエラーコードがキーなので、画面のコードには現れない
    const unused = Object.entries(resources.ja).flatMap(([ns, tree]) =>
      (ns === "errors" ? [] : keys(tree))
        .filter((k) => !isReferenced(k))
        .map((k) => `${ns}:${k}`),
    );
    expect(unused).toEqual([]);
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
