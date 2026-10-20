import { describe, expect, it } from "vitest";
import { defaultName, nextName } from "./createName";

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

describe("defaultName", () => {
  it("is just the version for vanilla", () => {
    expect(defaultName("vanilla", "1.21.4", "Vanilla")).toBe("1.21.4");
  });
  it("prefixes the loader label otherwise", () => {
    expect(defaultName("fabric", "1.21.4", "Fabric")).toBe("Fabric 1.21.4");
  });
});

describe("nextName with loader defaults", () => {
  it("follows the default when it changes from vanilla to a loader", () => {
    expect(nextName("1.21.4", "1.21.4", "Fabric 1.21.4")).toBe("Fabric 1.21.4");
  });
  it("keeps a name the user typed", () => {
    expect(nextName("My World", "Fabric 1.21.4", "1.21.4")).toBe("My World");
  });
});
