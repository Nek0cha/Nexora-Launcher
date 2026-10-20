import { describe, expect, it } from "vitest";
import { errorKey, errorSentence } from "./errorText";

describe("errorKey", () => {
  const known = new Set(["errors:net.http_status", "errors:generic"]);
  const exists = (k: string) => known.has(k);

  it("uses the code when a translation exists", () => {
    expect(errorKey("net.http_status", exists)).toBe("errors:net.http_status");
  });

  it("falls back to the generic message for unknown codes", () => {
    expect(errorKey("future.unknown_code", exists)).toBe("errors:generic");
  });
});

describe("errorSentence", () => {
  const known = new Set(["errors:auth.xbox", "errors:auth.xbox_2148916233", "errors:generic"]);
  const exists = (k: string) => known.has(k);
  const t = (key: string, options: Record<string, unknown>) => `${key}|${String(options.code)}`;

  it("prefers the XErr specific message", () => {
    expect(errorSentence({ code: "auth.xbox", params: { xerr: 2148916233 }, detail: null }, t, exists)).toBe(
      "errors:auth.xbox_2148916233|auth.xbox",
    );
  });

  it("uses the generic message with code unknown when there is no error", () => {
    expect(errorSentence(null, t, exists)).toBe("errors:generic|unknown");
  });
});
