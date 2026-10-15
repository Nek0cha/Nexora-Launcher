import { describe, expect, it } from "vitest";
import { filterCommands, modifierLabel, nextTheme, type PaletteCommand } from "./commands";

const noop = () => {};
const cmds: PaletteCommand[] = [
  { id: "go-home", label: "ホーム を開く", run: noop },
  { id: "go-settings", label: "設定 を開く", run: noop },
  { id: "theme", label: "テーマを切り替える", run: noop },
];

describe("palette commands", () => {
  it("returns everything for an empty query", () => {
    expect(filterCommands(cmds, "")).toHaveLength(3);
  });
  it("matches case-insensitively by substring", () => {
    expect(filterCommands(cmds, "設定").map((c) => c.id)).toEqual(["go-settings"]);
    expect(filterCommands([{ id: "x", label: "Open Settings", run: noop }], "settings")).toHaveLength(1);
  });
  it("uses the OS modifier label", () => {
    expect(modifierLabel("MacIntel")).toBe("⌘");
    expect(modifierLabel("Win32")).toBe("Ctrl");
  });
  it("toggles the resolved theme", () => {
    expect(nextTheme("dark")).toBe("light");
    expect(nextTheme("light")).toBe("dark");
  });
});
