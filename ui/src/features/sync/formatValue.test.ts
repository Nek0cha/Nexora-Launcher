import { beforeEach, describe, expect, it } from "vitest";
import i18n from "../../i18n";
import { formatKey, formatValue } from "./formatValue";

const t = i18n.t.bind(i18n);

describe("formatValue", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("ja");
  });

  it("shows booleans as on and off", () => {
    expect(formatValue("autoJump", "true", t)).toBe("オン");
    expect(formatValue("autoJump", "false", t)).toBe("オフ");
  });

  it("rounds plain numbers to two decimals", () => {
    expect(formatValue("mouseWheelSensitivity", "1.0", t)).toBe("1");
    expect(formatValue("mouseWheelSensitivity", "1.2345678", t)).toBe("1.23");
    expect(formatValue("renderDistance", "12", t)).toBe("12");
  });

  it("shows volumes and other 0 to 1 scales as percentages", () => {
    expect(formatValue("soundCategory_master", "0.4507042253521127", t)).toBe("45%");
    expect(formatValue("soundCategory_music", "1.0", t)).toBe("100%");
    expect(formatValue("chatOpacity", "0.25", t)).toBe("25%");
    expect(formatValue("gamma", "0.5", t)).toBe("50%");
  });

  it("shows the mouse sensitivity as the game does (0.5 is 100%)", () => {
    expect(formatValue("mouseSensitivity", "0.5", t)).toBe("100%");
    expect(formatValue("mouseSensitivity", "0.2535211", t)).toBe("51%");
  });

  it("shows the field of view in degrees", () => {
    expect(formatValue("fov", "0.0", t)).toBe("70°");
    expect(formatValue("fov", "1.0", t)).toBe("110°");
    expect(formatValue("fov", "0.375", t)).toBe("85°");
  });

  it("names the special numbers of the GUI scale and the frame rate", () => {
    expect(formatValue("guiScale", "0", t)).toBe("自動");
    expect(formatValue("guiScale", "3", t)).toBe("3");
    expect(formatValue("maxFps", "260", t)).toBe("無制限");
    expect(formatValue("maxFps", "120", t)).toBe("120");
  });

  it("names the main hand and drops the quotes of older files", () => {
    expect(formatValue("mainHand", '"left"', t)).toBe("左");
    expect(formatValue("mainHand", "right", t)).toBe("右");
  });

  it("formats key bindings as key names", () => {
    expect(formatValue("key_key.jump", "key.keyboard.space", t)).toBe("スペース");
    expect(formatValue("key_some_mod.action", "key.keyboard.r", t)).toBe("R");
  });

  it("keeps other text as it is", () => {
    expect(formatValue("musicFrequency", "DEFAULT", t)).toBe("DEFAULT");
  });

  it("uses English words in English", async () => {
    await i18n.changeLanguage("en");
    expect(formatValue("autoJump", "true", t)).toBe("On");
    expect(formatValue("guiScale", "0", t)).toBe("Auto");
    expect(formatValue("key_key.sneak", "key.keyboard.left.shift", t)).toBe("Left Shift");
  });
});

describe("formatKey", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("ja");
  });

  it.each([
    ["key.keyboard.w", "W"],
    ["key.keyboard.1", "1"],
    ["key.keyboard.f3", "F3"],
    ["key.keyboard.left.shift", "左 Shift"],
    ["key.keyboard.right.control", "右 Ctrl"],
    ["key.keyboard.left.alt", "左 Alt"],
    ["key.keyboard.tab", "Tab"],
    ["key.keyboard.escape", "Esc"],
    ["key.keyboard.caps.lock", "Caps Lock"],
    ["key.keyboard.page.up", "Page Up"],
    ["key.keyboard.up", "↑"],
    ["key.keyboard.left", "←"],
    ["key.keyboard.grave.accent", "`"],
    ["key.keyboard.slash", "/"],
    ["key.keyboard.keypad.1", "テンキー 1"],
    ["key.keyboard.keypad.add", "テンキー +"],
    ["key.keyboard.unknown", "未割り当て"],
    ["key.mouse.left", "左クリック"],
    ["key.mouse.right", "右クリック"],
    ["key.mouse.middle", "ホイールクリック"],
    ["key.mouse.4", "マウスボタン 4"],
    ["key.keyboard.world.1", "World 1"],
  ])("formats %s", (value, expected) => {
    expect(formatKey(value, t)).toBe(expected);
  });

  it("keeps a value that is not a key name", () => {
    expect(formatKey("57", t)).toBe("57");
  });
});
