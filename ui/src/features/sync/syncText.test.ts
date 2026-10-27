import { describe, expect, it } from "vitest";
import type { SyncField } from "../../bindings/SyncField";
import { defaultProfileName, isKnownFieldId, reasonKey } from "./syncText";

describe("defaultProfileName", () => {
  it("uses the base name when it is free", () => {
    expect(defaultProfileName("共通設定", [])).toBe("共通設定");
    expect(defaultProfileName("共通設定", ["別の名前"])).toBe("共通設定");
  });

  it("adds the next free number when the name is taken", () => {
    expect(defaultProfileName("共通設定", ["共通設定"])).toBe("共通設定 2");
    expect(defaultProfileName("共通設定", ["共通設定", "共通設定 2"])).toBe("共通設定 3");
    expect(defaultProfileName("共通設定", ["共通設定", "共通設定 3"])).toBe("共通設定 2");
  });
});

describe("isKnownFieldId", () => {
  const fields: SyncField[] = [
    { id: "fov", category: "graphics" },
    { id: "key_key.jump", category: "keys" },
  ];

  it("accepts registered fields and key bindings added by mods", () => {
    expect(isKnownFieldId("fov", fields)).toBe(true);
    expect(isKnownFieldId("key_key.jump", fields)).toBe(true);
    expect(isKnownFieldId("key_some_mod.action", fields)).toBe(true);
  });

  it("rejects anything else", () => {
    expect(isKnownFieldId("lastServer", fields)).toBe(false);
    expect(isKnownFieldId("key_", fields)).toBe(false);
    expect(isKnownFieldId("key_a:b", fields)).toBe(false);
    expect(isKnownFieldId("key_a\nb", fields)).toBe(false);
  });
});

describe("reasonKey", () => {
  it("maps known reason codes and falls back to unexpected", () => {
    expect(reasonKey("writeFailed")).toBe("sync.reason.writeFailed");
    expect(reasonKey("noOptionsFile")).toBe("sync.reason.noOptionsFile");
    expect(reasonKey("somethingNew")).toBe("sync.reason.unexpected");
    expect(reasonKey(null)).toBe("sync.reason.unexpected");
  });
});
