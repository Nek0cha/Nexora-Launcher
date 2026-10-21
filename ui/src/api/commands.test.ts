import { invoke } from "@tauri-apps/api/core";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  beginLogin,
  changeSkin,
  createInstance,
  deleteInstance,
  duplicateInstance,
  getSkinVariant,
  addContentFiles,
  getContentIcon,
  identifyContent,
  launchInstance,
  listContent,
  listContentCategories,
  removeContent,
  setContentEnabled,
  listLoaderGameVersions,
  listLoaderVersions,
  openInstanceFolder,
  stopInstance,
  updateInstance,
} from "./commands";
import type { InstanceSettings } from "../bindings/InstanceSettings";
import type { NewInstance } from "../bindings/NewInstance";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn().mockResolvedValue("t1") }));

afterEach(() => vi.clearAllMocks());

describe("createInstance", () => {
  it("sends the whole request", async () => {
    const request: NewInstance = {
      name: "World",
      mcVersion: "1.21.5",
      loader: "fabric",
      loaderVersion: "0.16.0",
      memoryMode: "auto",
      memoryMb: null,
      javaMode: "auto",
      javaPath: null,
      jvmArgs: "",
    };
    await createInstance(request);
    expect(invoke).toHaveBeenLastCalledWith("create_instance", { request });
  });
});

describe("loader versions", () => {
  it("lists game versions for a loader", async () => {
    await listLoaderGameVersions("fabric");
    expect(invoke).toHaveBeenLastCalledWith("list_loader_game_versions", { loader: "fabric" });
  });

  it("lists loader versions for a game version", async () => {
    await listLoaderVersions("quilt", "1.20.1");
    expect(invoke).toHaveBeenLastCalledWith("list_loader_versions", { loader: "quilt", mcVersion: "1.20.1" });
  });
});

describe("launchInstance", () => {
  it("passes offline, false by default", async () => {
    await launchInstance("i1");
    expect(invoke).toHaveBeenLastCalledWith("launch_instance", { instanceId: "i1", offline: false });
    await launchInstance("i1", true);
    expect(invoke).toHaveBeenLastCalledWith("launch_instance", { instanceId: "i1", offline: true });
  });
});

describe("stopInstance", () => {
  it("passes instanceId and force", async () => {
    await stopInstance("i1", true);
    expect(invoke).toHaveBeenLastCalledWith("stop_instance", { instanceId: "i1", force: true });
  });
});

describe("openInstanceFolder", () => {
  it("passes instanceId and kind", async () => {
    await openInstanceFolder("i1", "saves");
    expect(invoke).toHaveBeenLastCalledWith("open_instance_folder", { instanceId: "i1", kind: "saves" });
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

describe("content commands", () => {
  it("passes the camelCase argument names", async () => {
    await listContent("i1", "mod");
    expect(invoke).toHaveBeenLastCalledWith("list_content", { instanceId: "i1", kind: "mod" });
    await identifyContent("i1");
    expect(invoke).toHaveBeenLastCalledWith("identify_content", { instanceId: "i1" });
    await setContentEnabled("i1", "shader", "a.zip", false);
    expect(invoke).toHaveBeenLastCalledWith("set_content_enabled", {
      instanceId: "i1",
      kind: "shader",
      relPath: "a.zip",
      enabled: false,
    });
    await removeContent("i1", "resourcepack", "a.zip");
    expect(invoke).toHaveBeenLastCalledWith("remove_content", { instanceId: "i1", kind: "resourcepack", relPath: "a.zip" });
    await addContentFiles("i1", "mod", ["C:\a.jar"], true);
    expect(invoke).toHaveBeenLastCalledWith("add_content_files", {
      instanceId: "i1",
      kind: "mod",
      paths: ["C:\a.jar"],
      overwrite: true,
    });
    await listContentCategories("mod");
    expect(invoke).toHaveBeenLastCalledWith("list_content_categories", { kind: "mod" });
    await getContentIcon("k");
    expect(invoke).toHaveBeenLastCalledWith("get_content_icon", { key: "k" });
  });
});
