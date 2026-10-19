import { invoke } from "@tauri-apps/api/core";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  beginLogin,
  changeSkin,
  deleteInstance,
  duplicateInstance,
  getSkinVariant,
  launchInstance,
  updateInstance,
} from "./commands";
import type { InstanceSettings } from "../bindings/InstanceSettings";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue("t1") }));

afterEach(() => vi.clearAllMocks());

describe("launchInstance", () => {
  it("passes offline, false by default", async () => {
    await launchInstance("i1");
    expect(invoke).toHaveBeenLastCalledWith("launch_instance", { instanceId: "i1", offline: false });
    await launchInstance("i1", true);
    expect(invoke).toHaveBeenLastCalledWith("launch_instance", { instanceId: "i1", offline: true });
  });
});

describe("beginLogin", () => {
  it("passes the method", async () => {
    await beginLogin("deviceCode");
    expect(invoke).toHaveBeenLastCalledWith("begin_login", { method: "deviceCode" });
  });
});

describe("instance and skin commands", () => {
  it("passes the camelCase argument names", async () => {
    const settings = {} as InstanceSettings;
    await updateInstance("i1", settings);
    expect(invoke).toHaveBeenLastCalledWith("update_instance", { instanceId: "i1", settings });
    await deleteInstance("i1");
    expect(invoke).toHaveBeenLastCalledWith("delete_instance", { instanceId: "i1" });
    await duplicateInstance("i1");
    expect(invoke).toHaveBeenLastCalledWith("duplicate_instance", { instanceId: "i1" });
    await getSkinVariant("a1");
    expect(invoke).toHaveBeenLastCalledWith("get_skin_variant", { accountId: "a1" });
    await changeSkin("a1", "slim", [1, 2]);
    expect(invoke).toHaveBeenLastCalledWith("change_skin", { accountId: "a1", variant: "slim", png: [1, 2] });
  });
});
