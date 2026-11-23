import { describe, expect, it } from "vitest";
import { en } from "./en";
import { ja } from "./ja";

describe("factual claims", () => {
  it("limits the non-GPL claim to the launchers in the table", () => {
    expect(en.compare.lead).toMatch(/in this table/);
    expect(ja.compare.lead).toMatch(/この表/);
  });

  it("names the architecture on the Apple Silicon button", () => {
    expect(en.hero.downloadFor["macos-arm"]).toContain("Apple Silicon");
    expect(ja.hero.downloadFor["macos-arm"]).toContain("Apple シリコン");
  });

  it("tells macOS users how to open an app that is reported as damaged", () => {
    const mac = (d: typeof en) => d.install.items.find((i) => i.os === "macOS")!.body;
    expect(mac(en)).toContain("xattr -dr com.apple.quarantine /Applications/Nexora.app");
    expect(mac(ja)).toContain("xattr -dr com.apple.quarantine /Applications/Nexora.app");
  });
});
