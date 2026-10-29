import { invoke } from "@tauri-apps/api/core";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  beginLogin,
  createInstance,
  deleteInstance,
  duplicateInstance,
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
  searchModrinth,
  previewContentInstall,
  installContent,
  checkContentUpdates,
  updateContent,
  listContentVersions,
  changeContentVersion,
  contentTaskReport,
  openModrinthProject,
  listSyncProfiles,
  createSyncProfileFromInstance,
  renameSyncProfile,
  deleteSyncProfile,
  setSyncCategories,
  previewJoinSyncProfile,
  joinSyncProfile,
  leaveSyncProfile,
  getInstanceSync,
  setSyncExcluded,
  pendingSyncProposals,
  resolveSyncProposal,
  listSyncFields,
  getSyncProfileValues,
  setSyncOverride,
  getCurrentSkin, setCape, listSavedSkins, getSavedSkinImage, addSavedSkin, addSavedSkinFromPath,
  updateSavedSkin, deleteSavedSkin, applySavedSkin, importCurrentSkin, listDefaultSkins, applyDefaultSkin,
} from "./commands";
import type { SearchQuery } from "../bindings/SearchQuery";
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

describe("modrinth commands", () => {
  it("passes the camelCase argument names", async () => {
    const query: SearchQuery = {
      query: "x", mcVersion: null, loaders: [], categories: [], clientSides: [], serverSides: [], sort: "relevance",
    };
    await searchModrinth(null, query, 20);
    expect(invoke).toHaveBeenLastCalledWith("search_modrinth", { instanceId: null, query, offset: 20 });
    await previewContentInstall("i1", "p1");
    expect(invoke).toHaveBeenLastCalledWith("preview_content_install", { instanceId: "i1", projectId: "p1" });
    await installContent("i1", "p1", true);
    expect(invoke).toHaveBeenLastCalledWith("install_content", { instanceId: "i1", projectId: "p1", acceptIncompatible: true });
    await checkContentUpdates("i1", true);
    expect(invoke).toHaveBeenLastCalledWith("check_content_updates", { instanceId: "i1", force: true });
    await updateContent("i1", ["mods/a.jar"]);
    expect(invoke).toHaveBeenLastCalledWith("update_content", { instanceId: "i1", relPaths: ["mods/a.jar"] });
    await listContentVersions("i1", "mods/a.jar");
    expect(invoke).toHaveBeenLastCalledWith("list_content_versions", { instanceId: "i1", relPath: "mods/a.jar" });
    await changeContentVersion("i1", "mods/a.jar", "v2");
    expect(invoke).toHaveBeenLastCalledWith("change_content_version", {
      instanceId: "i1",
      relPath: "mods/a.jar",
      versionId: "v2",
    });
    await contentTaskReport("t1");
    expect(invoke).toHaveBeenLastCalledWith("content_task_report", { taskId: "t1" });
    await openModrinthProject("sodium");
    expect(invoke).toHaveBeenLastCalledWith("open_modrinth_project", { slug: "sodium" });
  });
});

describe("sync commands", () => {
  it("passes the command names and camelCase arguments", async () => {
    await listSyncProfiles();
    expect(invoke).toHaveBeenLastCalledWith("list_sync_profiles", undefined);
    await createSyncProfileFromInstance("i1", "Shared");
    expect(invoke).toHaveBeenLastCalledWith("create_sync_profile_from_instance", { instanceId: "i1", name: "Shared" });
    await renameSyncProfile("p1", "New");
    expect(invoke).toHaveBeenLastCalledWith("rename_sync_profile", { profileId: "p1", name: "New" });
    await deleteSyncProfile("p1");
    expect(invoke).toHaveBeenLastCalledWith("delete_sync_profile", { profileId: "p1" });
    await setSyncCategories("p1", ["keys", "sound"]);
    expect(invoke).toHaveBeenLastCalledWith("set_sync_categories", { profileId: "p1", categories: ["keys", "sound"] });
    await previewJoinSyncProfile("i1", "p1");
    expect(invoke).toHaveBeenLastCalledWith("preview_join_sync_profile", { instanceId: "i1", profileId: "p1" });
    await joinSyncProfile("i1", "p1", "adoptInstance");
    expect(invoke).toHaveBeenLastCalledWith("join_sync_profile", {
      instanceId: "i1",
      profileId: "p1",
      mode: "adoptInstance",
    });
    await leaveSyncProfile("i1");
    expect(invoke).toHaveBeenLastCalledWith("leave_sync_profile", { instanceId: "i1" });
    await getInstanceSync("i1");
    expect(invoke).toHaveBeenLastCalledWith("get_instance_sync", { instanceId: "i1" });
    await setSyncExcluded("i1", ["fov"]);
    expect(invoke).toHaveBeenLastCalledWith("set_sync_excluded", { instanceId: "i1", fieldIds: ["fov"] });
    await pendingSyncProposals("p1");
    expect(invoke).toHaveBeenLastCalledWith("pending_sync_proposals", { profileId: "p1" });
    await resolveSyncProposal("i1", "fov", "applyToProfile", "70");
    expect(invoke).toHaveBeenLastCalledWith("resolve_sync_proposal", {
      instanceId: "i1",
      fieldId: "fov",
      resolution: "applyToProfile",
      chosenValue: "70",
    });
    await resolveSyncProposal("i1", "fov", "ignore");
    expect(invoke).toHaveBeenLastCalledWith("resolve_sync_proposal", {
      instanceId: "i1",
      fieldId: "fov",
      resolution: "ignore",
      chosenValue: null,
    });
    await joinSyncProfile("i2", "p1", "keepProfile");
    expect(invoke).toHaveBeenLastCalledWith("join_sync_profile", {
      instanceId: "i2",
      profileId: "p1",
      mode: "keepProfile",
    });
  });

  it("passes the arguments of the field, value and override commands", async () => {
    await listSyncFields();
    expect(invoke).toHaveBeenLastCalledWith("list_sync_fields", undefined);
    await getSyncProfileValues("p1");
    expect(invoke).toHaveBeenLastCalledWith("get_sync_profile_values", { profileId: "p1" });
    await setSyncOverride("i1", "fov", "0.5");
    expect(invoke).toHaveBeenLastCalledWith("set_sync_override", { instanceId: "i1", fieldId: "fov", value: "0.5" });
    await setSyncOverride("i1", "fov", null);
    expect(invoke).toHaveBeenLastCalledWith("set_sync_override", { instanceId: "i1", fieldId: "fov", value: null });
  });
});

describe("skin commands", () => {
  it("passes the command names and camelCase arguments", async () => {
    await getCurrentSkin("a1");
    expect(invoke).toHaveBeenLastCalledWith("get_current_skin", { accountId: "a1" });
    await setCape("a1", "c1");
    expect(invoke).toHaveBeenLastCalledWith("set_cape", { accountId: "a1", capeId: "c1" });
    await setCape("a1", null);
    expect(invoke).toHaveBeenLastCalledWith("set_cape", { accountId: "a1", capeId: null });
    await listSavedSkins();
    expect(invoke).toHaveBeenLastCalledWith("list_saved_skins", undefined);
    await getSavedSkinImage("s1");
    expect(invoke).toHaveBeenLastCalledWith("get_saved_skin_image", { skinId: "s1" });
    await addSavedSkin("Me", [1, 2]);
    expect(invoke).toHaveBeenLastCalledWith("add_saved_skin", { name: "Me", png: [1, 2] });
    await addSavedSkinFromPath("C:\a.png");
    expect(invoke).toHaveBeenLastCalledWith("add_saved_skin_from_path", { path: "C:\a.png" });
    const patch = { name: "New", variant: null, png: null };
    await updateSavedSkin("s1", patch);
    expect(invoke).toHaveBeenLastCalledWith("update_saved_skin", { skinId: "s1", patch });
    await deleteSavedSkin("s1");
    expect(invoke).toHaveBeenLastCalledWith("delete_saved_skin", { skinId: "s1" });
    await applySavedSkin("a1", "s1");
    expect(invoke).toHaveBeenLastCalledWith("apply_saved_skin", { accountId: "a1", skinId: "s1" });
    await importCurrentSkin("a1");
    expect(invoke).toHaveBeenLastCalledWith("import_current_skin", { accountId: "a1" });
    await listDefaultSkins();
    expect(invoke).toHaveBeenLastCalledWith("list_default_skins", undefined);
    await applyDefaultSkin("a1", "classic-steve");
    expect(invoke).toHaveBeenLastCalledWith("apply_default_skin", { accountId: "a1", key: "classic-steve" });
  });
});
