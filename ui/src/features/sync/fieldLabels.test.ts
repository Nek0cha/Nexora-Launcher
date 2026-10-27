import { describe, expect, it } from "vitest";
import registry from "../../../../crates/nexora-core/src/sync/registry.rs?raw";
import i18n, { resources } from "../../i18n";
import { fieldLabel } from "./fieldLabels";

/** 登録表（core）の項目の ID。表の行 `f("id", ...)`、`f_since("id", ...)` と `key_field("id")` から読む */
function registeredIds(): string[] {
  const table = registry.slice(0, registry.indexOf("pub fn fields()"));
  return [...table.matchAll(/\b(?:f|f_since|key_field)\("([^"]+)"/g)].map((m) => m[1]!);
}

describe("fieldLabel", () => {
  it("reads the field ids from the registry", () => {
    const ids = registeredIds();
    expect(ids.length).toBeGreaterThan(80);
    expect(ids).toContain("fov");
    expect(ids).toContain("lang");
    expect(ids).toContain("key_key.hotbar.9");
  });

  it.each(["ja", "en"] as const)("has a name for every registered field in %s", (lang) => {
    const fields = resources[lang].common.sync.fields as Record<string, string>;
    const missing = registeredIds().filter((id) => typeof fields[id] !== "string" || fields[id] === "");
    expect(missing).toEqual([]);
  });

  it("has no names for fields that are not registered", () => {
    const ids = new Set(registeredIds());
    const extra = Object.keys(resources.ja.common.sync.fields).filter((id) => !ids.has(id));
    expect(extra).toEqual([]);
  });

  it("looks up names, including ids that contain dots", async () => {
    await i18n.changeLanguage("ja");
    expect(fieldLabel("fov", i18n)).toBe("視野角");
    expect(fieldLabel("key_key.jump", i18n)).toBe("ジャンプ");
    expect(fieldLabel("key_key.hotbar.3", i18n)).toBe("ホットバー 3");
    await i18n.changeLanguage("en");
    expect(fieldLabel("key_key.hotbar.3", i18n)).toBe("Hotbar Slot 3");
  });

  it("never shows an unknown id", async () => {
    await i18n.changeLanguage("ja");
    expect(fieldLabel("lastServer", i18n)).toBe("その他の設定");
    expect(fieldLabel("key_some_mod.action", i18n)).toBe("その他の設定");
    expect(fieldLabel("key_key", i18n)).toBe("その他の設定");
    expect(fieldLabel("", i18n)).toBe("その他の設定");
  });
});
