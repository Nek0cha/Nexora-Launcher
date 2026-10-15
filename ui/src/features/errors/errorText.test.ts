import { describe, expect, it } from "vitest";
import { errorKey } from "./errorText";

describe("errorKey", () => {
  const known = new Set(["errors:demo.failed", "errors:generic"]);
  const exists = (k: string) => known.has(k);

  it("uses the code when a translation exists", () => {
    expect(errorKey("demo.failed", exists)).toBe("errors:demo.failed");
  });

  it("falls back to the generic message for unknown codes", () => {
    expect(errorKey("future.unknown_code", exists)).toBe("errors:generic");
  });
});
