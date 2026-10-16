import { describe, expect, it } from "vitest";
import { nextName } from "./createName";

describe("nextName", () => {
  it("follows the version while the name is empty or still the previous version", () => {
    expect(nextName("", null, "1.21.5")).toBe("1.21.5");
    expect(nextName("1.21.5", "1.21.5", "1.8.9")).toBe("1.8.9");
    expect(nextName("   ", "1.21.5", "1.8.9")).toBe("1.8.9");
  });
  it("keeps a name the user typed", () => {
    expect(nextName("My World", "1.21.5", "1.8.9")).toBe("My World");
  });
});
