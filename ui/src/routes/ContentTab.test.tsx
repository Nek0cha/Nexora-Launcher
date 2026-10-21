import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import {
  addContentFiles,
  changeContentVersion,
  checkContentUpdates,
  contentTaskReport,
  getContentIcon,
  getTask,
  identifyContent,
  listContent,
  listContentCategories,
  listContentVersions,
  removeContent,
  setContentEnabled,
  updateContent,
} from "../api/commands";
import { type DragDropHandlers, onFilesDropped } from "../api/dragDrop";
import { IpcError } from "../api/ipc";
import type { ContentTaskReport } from "../bindings/ContentTaskReport";
import type { TaskSnapshot } from "../bindings/TaskSnapshot";
import type { UpdateCheck } from "../bindings/UpdateCheck";
import type { ContentItem } from "../bindings/ContentItem";
import type { ContentKind } from "../bindings/ContentKind";
import type { Instance } from "../bindings/Instance";
import { ToastRegion } from "../components/toast/ToastRegion";
import { clearToasts } from "../components/toast/useToast";
import i18n from "../i18n";
import { ContentTab } from "./ContentTab";

vi.mock("../api/commands", () => ({
  listContent: vi.fn(),
  identifyContent: vi.fn(),
  setContentEnabled: vi.fn(),
  removeContent: vi.fn(),
  addContentFiles: vi.fn(),
  listContentCategories: vi.fn(),
  getContentIcon: vi.fn(),
  checkContentUpdates: vi.fn(),
  updateContent: vi.fn(),
  listContentVersions: vi.fn(),
  changeContentVersion: vi.fn(),
  getTask: vi.fn(),
  contentTaskReport: vi.fn(),
}));

vi.mock("../api/dragDrop", () => ({ onFilesDropped: vi.fn() }));

const instance: Instance = {
  id: "i1", name: "Survival", dirName: "Survival", mcVersion: "1.21.5", state: "ready",
  createdAt: 0, lastPlayedAt: null, playTimeMs: 0,
  memoryMode: "auto", memoryMb: null, javaMode: "auto", javaPath: null, jvmArgs: "",
  loader: "fabric", loaderVersion: "0.16.0",
};

function item(over: Partial<ContentItem> = {}): ContentItem {
  return {
    kind: "mod", relPath: "sodium.jar", fileName: "sodium.jar", enabled: true, size: 1, sha1: "s1",
    provider: "modrinth", projectId: "p-sodium", versionId: "v1", pinned: false, installedAs: "explicit",
    name: "Sodium", versionNumber: "0.6.0", description: null, iconUrl: null, hasLocalIcon: false,
    clientSide: "required", serverSide: "unsupported", categories: ["optimization"], dependencies: [], identified: true,
    ...over,
  };
}

const fabricApi = item({
  relPath: "fabric-api.jar", fileName: "fabric-api.jar", sha1: "s2", projectId: "p-fapi", name: "Fabric API",
  installedAs: "dependency", categories: ["library"],
});
const modMenu = item({
  relPath: "modmenu.jar", fileName: "modmenu.jar", sha1: "s3", projectId: "p-modmenu", name: "Mod Menu",
  dependencies: ["p-fapi"], categories: ["utility"],
});
const localMod = item({
  relPath: "mystery.jar", fileName: "mystery.jar", sha1: "s4", provider: "local", projectId: null, versionId: null,
  name: "Mystery", clientSide: null, serverSide: null, categories: [], identified: false,
});
const pack = item({
  kind: "resourcepack", relPath: "faithful.zip", fileName: "faithful.zip", sha1: "r1", projectId: "p-faithful",
  name: "Faithful", categories: ["simplistic"],
});

let lists: Record<ContentKind, ContentItem[]>;
const noUpdates: UpdateCheck = { updates: [], noCompatible: [] };
const report = (over: Partial<ContentTaskReport> = {}): ContentTaskReport => ({
  installed: 0, dependenciesAdded: 0, updated: 0, failures: [], ...over,
});

function snapshot(state: TaskSnapshot["state"], error: TaskSnapshot["error"] = null): TaskSnapshot {
  return {
    id: "t1", kind: "content", instanceId: "i1", state, phase: null, progress: { current: 0, total: 0 },
    revision: 1, error, createdAt: 0, updatedAt: 0,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}
let handlers: DragDropHandlers | null;
const unlisten = vi.fn();

// jsdom は要素の大きさを持たないので、Virtualizer が窓を計算できるよう一定の大きさを返す。
// afterEach の resetAllMocks で元に戻るので、テストごとに付け直す
const spies: { mockRestore(): void }[] = [];
function mockSizes() {
  const proto = HTMLElement.prototype;
  spies.push(
    vi.spyOn(proto, "clientWidth", "get").mockReturnValue(800),
    vi.spyOn(proto, "clientHeight", "get").mockReturnValue(600),
    vi.spyOn(proto, "offsetWidth", "get").mockReturnValue(800),
    vi.spyOn(proto, "offsetHeight", "get").mockReturnValue(600),
    vi.spyOn(proto, "scrollHeight", "get").mockReturnValue(600),
    vi.spyOn(proto, "getBoundingClientRect").mockReturnValue(DOMRect.fromRect({ x: 0, y: 0, width: 800, height: 600 })),
  );
}

beforeAll(async () => {
  await i18n.changeLanguage("en");
});

afterAll(() => {
  for (const spy of spies) spy.mockRestore();
});

beforeEach(() => {
  mockSizes();
  lists = { mod: [item(), fabricApi, modMenu, localMod], resourcepack: [pack], shader: [] };
  handlers = null;
  vi.mocked(listContent).mockImplementation(async (_id, kind) => lists[kind]);
  vi.mocked(identifyContent).mockResolvedValue({ identified: 0, local: 0 });
  vi.mocked(listContentCategories).mockResolvedValue(["optimization", "library", "utility"]);
  vi.mocked(getContentIcon).mockResolvedValue(null);
  vi.mocked(setContentEnabled).mockResolvedValue(null);
  vi.mocked(removeContent).mockResolvedValue(null);
  vi.mocked(addContentFiles).mockResolvedValue({ added: [], conflicts: [], rejected: [] });
  vi.mocked(checkContentUpdates).mockResolvedValue(noUpdates);
  vi.mocked(updateContent).mockResolvedValue("t1");
  vi.mocked(getTask).mockResolvedValue(snapshot("succeeded"));
  vi.mocked(contentTaskReport).mockResolvedValue(report({ updated: 1 }));
  vi.mocked(onFilesDropped).mockImplementation(async (h) => {
    handlers = h;
    return unlisten;
  });
});

afterEach(() => {
  act(() => clearToasts());
  cleanup();
  vi.resetAllMocks();
});

function mount(busy = false, target: Instance = instance) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  const tree = (b: boolean) => (
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={["/instances/i1"]}>
        <Routes>
          <Route path="/instances/:id" element={<ContentTab instance={target} busy={b} />} />
          <Route path="/instances/:id/modrinth" element={<p>modrinth page</p>} />
        </Routes>
      </MemoryRouter>
      <ToastRegion />
    </QueryClientProvider>
  );
  let view = render(tree(busy));
  return {
    setBusy: (b: boolean) => view.rerender(tree(b)),
    // 同じ QueryClient のまま、タブを閉じて開き直す
    remount: () => {
      view.unmount();
      view = render(tree(busy));
    },
  };
}

const subTab = (name: RegExp) => screen.getByRole("tab", { name });
const toasts = () => screen.getByRole("region", { name: "Notifications" });

async function ready() {
  await screen.findByRole("button", { name: "Enable Sodium" });
  await waitFor(() => expect(handlers).not.toBeNull());
}

async function press(el: HTMLElement) {
  await act(async () => {
    fireEvent.click(el);
    await Promise.resolve();
  });
}

describe("ContentTab", () => {
  it("shows counts on the three sub-tabs", async () => {
    mount();
    await ready();
    await waitFor(() => expect(subTab(/^Mods/).textContent).toContain("4"));
    expect(subTab(/^Resource packs/).textContent).toContain("1");
    expect(subTab(/^Shaders/).textContent).toContain("0");
  });

  it("calls identifyContent once when opened", async () => {
    mount();
    await ready();
    expect(identifyContent).toHaveBeenCalledTimes(1);
    expect(identifyContent).toHaveBeenCalledWith("i1");
  });

  it("filters by name and shows the excluded count only while a metadata filter is active", async () => {
    mount();
    await ready();
    fireEvent.change(screen.getByRole("searchbox", { name: "Filter by name" }), { target: { value: "menu" } });
    expect(within(screen.getByRole("grid")).getAllByRole("row")).toHaveLength(1);
    expect(screen.getByRole("button", { name: "Enable Mod Menu" })).toBeTruthy();
    // 名前の絞り込みでは、特定できていない行を隠さない
    fireEvent.change(screen.getByRole("searchbox", { name: "Filter by name" }), { target: { value: "" } });
    expect(screen.queryByText(/Not covered by this filter/)).toBeNull();
    const client = screen.getByRole("toolbar", { name: "Client side" });
    fireEvent.click(within(client).getByRole("button", { name: "Required" }));
    expect(screen.getByText("Not covered by this filter: 1")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Enable Mystery" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Show" }));
    expect(screen.getByRole("button", { name: "Enable Mystery" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Hide" }));
    expect(screen.queryByRole("button", { name: "Enable Mystery" })).toBeNull();
    // 絞り込みはサブタブごとに持つ
    fireEvent.click(subTab(/^Resource packs/));
    await screen.findByRole("button", { name: "Enable Faithful" });
    expect(screen.queryByText(/Not covered by this filter/)).toBeNull();
    fireEvent.click(subTab(/^Mods/));
    expect(await screen.findByText("Not covered by this filter: 1")).toBeTruthy();
  });

  it("toggling a mod that others depend on asks for confirmation first", async () => {
    mount();
    await ready();
    await press(screen.getByRole("button", { name: "Enable Fabric API" }));
    const dialog = await screen.findByRole("alertdialog");
    expect(within(dialog).getByText("Mod Menu")).toBeTruthy();
    expect(setContentEnabled).not.toHaveBeenCalled();
    await press(within(dialog).getByRole("button", { name: "Disable" }));
    await waitFor(() => expect(setContentEnabled).toHaveBeenCalledWith("i1", "mod", "fabric-api.jar", false));
    // 依存されていない MOD はそのまま切り替える
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    await press(screen.getByRole("button", { name: "Enable Mod Menu" }));
    await waitFor(() => expect(setContentEnabled).toHaveBeenCalledWith("i1", "mod", "modmenu.jar", false));
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("toggling a resource pack does not ask", async () => {
    lists.resourcepack = [pack, item({ ...pack, relPath: "dep.zip", sha1: "r2", name: "Addon", dependencies: ["p-faithful"] })];
    mount();
    await ready();
    fireEvent.click(subTab(/^Resource packs/));
    await press(await screen.findByRole("button", { name: "Enable Faithful" }));
    await waitFor(() => expect(setContentEnabled).toHaveBeenCalledWith("i1", "resourcepack", "faithful.zip", false));
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("removing always asks", async () => {
    mount();
    await ready();
    await press(screen.getByRole("button", { name: "Remove Sodium" }));
    let dialog = await screen.findByRole("alertdialog");
    expect(within(dialog).getByRole("heading").textContent).toContain("Sodium");
    expect(removeContent).not.toHaveBeenCalled();
    await press(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(removeContent).not.toHaveBeenCalled();
    // 依存されている MOD の削除では、依存している MOD の名前も並べる
    await press(screen.getByRole("button", { name: "Remove Fabric API" }));
    dialog = await screen.findByRole("alertdialog");
    expect(within(dialog).getByText("Mod Menu")).toBeTruthy();
    await press(within(dialog).getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(removeContent).toHaveBeenCalledWith("i1", "mod", "fabric-api.jar"));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    // パックの削除も確認する
    fireEvent.click(subTab(/^Resource packs/));
    await press(await screen.findByRole("button", { name: "Remove Faithful" }));
    expect(await screen.findByRole("alertdialog")).toBeTruthy();
  });

  it("controls and drops are disabled while the instance is busy", async () => {
    mount(true);
    await ready();
    expect(screen.getByText("You can't change content while the instance is running, installing, being duplicated or adding or updating mods.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Enable Sodium" }).hasAttribute("disabled")).toBe(true);
    expect(screen.getByRole("button", { name: "Remove Sodium" }).hasAttribute("disabled")).toBe(true);
    act(() => handlers!.enter());
    expect(screen.queryByText("Drop here to add")).toBeNull();
    await act(async () => handlers!.drop(["C:\\Downloads\\a.jar"]));
    expect(addContentFiles).not.toHaveBeenCalled();
  });

  it("dropping files calls addContentFiles for the open sub-tab with overwrite=false", async () => {
    mount();
    await ready();
    await act(async () => handlers!.drop(["C:\\Downloads\\a.jar", "C:\\Downloads\\b.jar"]));
    expect(addContentFiles).toHaveBeenLastCalledWith("i1", "mod", ["C:\\Downloads\\a.jar", "C:\\Downloads\\b.jar"], false);
    fireEvent.click(subTab(/^Shaders/));
    await waitFor(() => expect(subTab(/^Shaders/).getAttribute("aria-selected")).toBe("true"));
    await act(async () => handlers!.drop(["/home/me/bsl.zip"]));
    await waitFor(() => expect(addContentFiles).toHaveBeenLastCalledWith("i1", "shader", ["/home/me/bsl.zip"], false));
    // 追加の後は一覧を取り直す
    await waitFor(() => expect(vi.mocked(listContent).mock.calls.filter(([, k]) => k === "shader").length).toBeGreaterThan(1));
  });

  it("a conflict opens a confirmation and re-adds only the conflicting paths with overwrite=true", async () => {
    vi.mocked(addContentFiles).mockResolvedValueOnce({ added: [], conflicts: ["sodium.jar"], rejected: [] });
    mount();
    await ready();
    await act(async () => handlers!.drop(["C:\\Downloads\\new.jar", "C:\\Downloads\\sodium.jar"]));
    const dialog = await screen.findByRole("alertdialog");
    expect(within(dialog).getByText("sodium.jar")).toBeTruthy();
    await press(within(dialog).getByRole("button", { name: "Replace" }));
    await waitFor(() =>
      expect(addContentFiles).toHaveBeenLastCalledWith("i1", "mod", ["C:\\Downloads\\sodium.jar"], true));
    expect(addContentFiles).toHaveBeenCalledTimes(2);
  });

  it("rejected files produce a toast with the reason", async () => {
    vi.mocked(addContentFiles).mockResolvedValueOnce({
      added: [], conflicts: [], rejected: [{ name: "notes.txt", reason: "wrongExtension" }],
    });
    mount();
    await ready();
    await act(async () => handlers!.drop(["C:\\Downloads\\notes.txt"]));
    const alert = await within(toasts()).findByRole("alert");
    expect(alert.textContent).toContain("notes.txt");
    expect(alert.textContent).toContain("Only .jar files can be added here.");
  });

  it("the drop target appears on enter and disappears on leave", async () => {
    mount();
    await ready();
    expect(screen.queryByText("Drop here to add")).toBeNull();
    act(() => handlers!.enter());
    expect(screen.getByText("Drop here to add")).toBeTruthy();
    act(() => handlers!.leave());
    expect(screen.queryByText("Drop here to add")).toBeNull();
    act(() => handlers!.enter());
    await act(async () => handlers!.drop(["C:\\Downloads\\a.jar"]));
    expect(screen.queryByText("Drop here to add")).toBeNull();
  });

  it("confirming after the instance became busy skips the change and closes the dialog", async () => {
    const { setBusy } = mount();
    await ready();
    await press(screen.getByRole("button", { name: "Remove Sodium" }));
    const dialog = await screen.findByRole("alertdialog");
    setBusy(true);
    await press(within(dialog).getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(removeContent).not.toHaveBeenCalled();
    setBusy(false);
    await press(screen.getByRole("button", { name: "Enable Fabric API" }));
    const warn = await screen.findByRole("alertdialog");
    setBusy(true);
    await press(within(warn).getByRole("button", { name: "Disable" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(setContentEnabled).not.toHaveBeenCalled();
  });

  it("the name search sits above the list, outside the facet column", async () => {
    mount();
    await ready();
    const search = screen.getByRole("searchbox", { name: "Filter by name" });
    const facets = screen.getByTestId("content-facets");
    expect(facets.contains(screen.getByRole("toolbar", { name: "Client side" }))).toBe(true);
    expect(facets.contains(search)).toBe(false);
    expect(screen.getAllByRole("searchbox")).toHaveLength(1);
  });

  it("the excluded rows close again when the metadata filter is cleared", async () => {
    mount();
    await ready();
    const required = () =>
      within(screen.getByRole("toolbar", { name: "Client side" })).getByRole("button", { name: "Required" });
    fireEvent.click(required());
    fireEvent.click(screen.getByRole("button", { name: "Show" }));
    expect(screen.getByRole("button", { name: "Enable Mystery" })).toBeTruthy();
    // 絞り込みを外してからもう一度付けると、対象外の行は閉じた状態に戻る
    fireEvent.click(required());
    fireEvent.click(required());
    expect(screen.getByRole("button", { name: "Show" })).toBeTruthy();
    expect(screen.queryByRole("grid", { name: "Content not covered by this filter" })).toBeNull();
  });

  it("refetches the list even when identify fails offline", async () => {
    vi.mocked(identifyContent).mockRejectedValue({ kind: "network", message: "offline" });
    mount();
    await ready();
    // 特定は失敗の前に jar の中の名前を書いているので、失敗でも取り直す
    await waitFor(() => expect(vi.mocked(listContent).mock.calls.filter(([, k]) => k === "mod").length).toBeGreaterThan(1));
    expect(within(toasts()).queryByRole("alert")).toBeNull();
  });

  it("identifies again after an add that added files, including the overwrite re-add", async () => {
    const added = item({ relPath: "new.jar", fileName: "new.jar", sha1: "n1", provider: "local", name: null, identified: false });
    vi.mocked(addContentFiles)
      .mockResolvedValueOnce({ added: [added], conflicts: ["sodium.jar"], rejected: [] })
      .mockResolvedValueOnce({ added: [item()], conflicts: [], rejected: [] });
    mount();
    await ready();
    await waitFor(() => expect(identifyContent).toHaveBeenCalledTimes(1));
    await act(async () => handlers!.drop(["C:\\Downloads\\new.jar", "C:\\Downloads\\sodium.jar"]));
    await waitFor(() => expect(identifyContent).toHaveBeenCalledTimes(2));
    const dialog = await screen.findByRole("alertdialog");
    await press(within(dialog).getByRole("button", { name: "Replace" }));
    await waitFor(() => expect(identifyContent).toHaveBeenCalledTimes(3));
    expect(identifyContent).toHaveBeenLastCalledWith("i1");
  });

  it("does not identify again when an add added nothing", async () => {
    vi.mocked(addContentFiles).mockResolvedValueOnce({
      added: [], conflicts: [], rejected: [{ name: "notes.txt", reason: "wrongExtension" }],
    });
    mount();
    await ready();
    await waitFor(() => expect(identifyContent).toHaveBeenCalledTimes(1));
    await act(async () => handlers!.drop(["C:\\Downloads\\notes.txt"]));
    await within(toasts()).findByRole("alert");
    expect(identifyContent).toHaveBeenCalledTimes(1);
  });

  it("lists rows by the shown name, ignoring case", async () => {
    lists.mod = [...lists.mod, item({ relPath: "b.jar", fileName: "b.jar", sha1: "s9", name: null })];
    mount();
    await ready();
    const names = within(screen.getByRole("grid"))
      .getAllByRole("button", { name: /^Enable / })
      .map((b) => b.getAttribute("aria-label"));
    expect(names).toEqual(["Enable b.jar", "Enable Fabric API", "Enable Mod Menu", "Enable Mystery", "Enable Sodium"]);
  });

  it("stops listening for drops when unmounted", async () => {
    mount();
    await ready();
    cleanup();
    await waitFor(() => expect(unlisten).toHaveBeenCalledTimes(1));
  });

  it("content_tab_shows_the_add_button_only_on_the_mod_tab", async () => {
    mount();
    await ready();
    expect(screen.getByRole("link", { name: "Add from Modrinth" })).toBeTruthy();
    fireEvent.click(subTab(/^Resource packs/));
    await screen.findByRole("button", { name: "Enable Faithful" });
    expect(screen.queryByRole("link", { name: "Add from Modrinth" })).toBeNull();
    fireEvent.click(subTab(/^Shaders/));
    expect(screen.queryByRole("link", { name: "Add from Modrinth" })).toBeNull();
    fireEvent.click(subTab(/^Mods/));
    fireEvent.click(await screen.findByRole("link", { name: "Add from Modrinth" }));
    expect(await screen.findByText("modrinth page")).toBeTruthy();
  });
  describe("version change", () => {
    const versions = [
      { versionId: "v2", versionNumber: "0.6.1", versionType: "release", datePublished: "2024-06-01T00:00:00Z", current: false },
      { versionId: "v1", versionNumber: "0.6.0", versionType: "release", datePublished: "2024-05-01T00:00:00Z", current: true },
    ];

    it("choosing a version runs the change, shows the row as updating and refreshes the list and the update check", async () => {
      vi.mocked(listContentVersions).mockResolvedValue(versions);
      vi.mocked(changeContentVersion).mockResolvedValue("t1");
      const task = deferred<TaskSnapshot>();
      vi.mocked(getTask).mockReturnValue(task.promise);
      mount();
      await ready();
      await waitFor(() => expect(checkContentUpdates).toHaveBeenCalledTimes(1));

      await press(screen.getByRole("button", { name: "Change the version of Sodium (now 0.6.0)" }));
      const list = await screen.findByRole("listbox", { name: "Versions of Sodium" });
      expect(listContentVersions).toHaveBeenCalledWith("i1", "sodium.jar");
      await press(within(list).getByRole("option", { name: /0\.6\.1/ }));

      await waitFor(() => expect(changeContentVersion).toHaveBeenCalledWith("i1", "sodium.jar", "v2"));
      // 更新と同じく、行は「更新中…」になり、ほかの操作は止まる
      expect(await screen.findByText("Updating…")).toBeTruthy();
      expect(screen.getByRole("button", { name: "Enable Sodium" }).hasAttribute("disabled")).toBe(true);
      expect(screen.getByRole("button", { name: "Change the version of Mod Menu (now 0.6.0)" }).hasAttribute("disabled")).toBe(true);

      await act(async () => task.resolve(snapshot("succeeded")));
      await within(toasts()).findAllByText("Changed Sodium to 0.6.1");
      await waitFor(() => expect(vi.mocked(listContent).mock.calls.filter(([, k]) => k === "mod").length).toBeGreaterThan(1));
      await waitFor(() => expect(vi.mocked(checkContentUpdates).mock.calls.length).toBeGreaterThan(1));
      expect(screen.queryByText("Updating…")).toBeNull();
    });

    it("a failed change shows the error", async () => {
      vi.mocked(listContentVersions).mockResolvedValue(versions);
      vi.mocked(changeContentVersion).mockResolvedValue("t1");
      vi.mocked(getTask).mockResolvedValue(
        snapshot("failed", { code: "content.version_unavailable", params: { name: "Sodium" }, detail: null }),
      );
      mount();
      await ready();
      await press(screen.getByRole("button", { name: "Change the version of Sodium (now 0.6.0)" }));
      await press(within(await screen.findByRole("listbox")).getByRole("option", { name: /0\.6\.1/ }));
      await within(toasts()).findAllByText(/Sodium can't be changed to that version/);
    });

    it("version buttons are disabled while the instance is busy", async () => {
      mount(true);
      await ready();
      expect(screen.getByRole("button", { name: "Change the version of Sodium (now 0.6.0)" }).hasAttribute("disabled")).toBe(true);
      // Modrinth で特定されていない MOD には出さない
      expect(screen.queryByRole("button", { name: /Change the version of Mystery/ })).toBeNull();
    });
  });

  describe("updates", () => {
    const withUpdates: UpdateCheck = {
      updates: [
        { relPath: "sodium.jar", versionId: "v2", versionNumber: "0.6.1" },
        { relPath: "modmenu.jar", versionId: "v9", versionNumber: "11.0.0" },
      ],
      noCompatible: ["fabric-api.jar"],
    };
    const quiet = "Can't reach Modrinth right now. Updates will be checked later.";

    it("opening_the_mod_tab_checks_updates_once", async () => {
      mount();
      await ready();
      await waitFor(() => expect(checkContentUpdates).toHaveBeenCalledTimes(1));
      expect(checkContentUpdates).toHaveBeenCalledWith("i1", false);
      // サブタブを行き来しても確認し直さない
      fireEvent.click(subTab(/^Resource packs/));
      await screen.findByRole("button", { name: "Enable Faithful" });
      // 更新を確認する部品は MOD のサブタブだけ
      expect(screen.queryByRole("button", { name: "Check for updates" })).toBeNull();
      fireEvent.click(subTab(/^Mods/));
      await screen.findByRole("button", { name: "Enable Sodium" });
      expect(checkContentUpdates).toHaveBeenCalledTimes(1);
    });

    it("does not check updates for a vanilla instance", async () => {
      mount(false, { ...instance, loader: "vanilla", loaderVersion: null });
      await ready();
      expect(checkContentUpdates).not.toHaveBeenCalled();
      expect(screen.queryByRole("button", { name: "Check for updates" })).toBeNull();
    });

    it("update_button_appears_only_for_rows_with_an_update_and_shows_the_target_version_on_hover", async () => {
      vi.mocked(checkContentUpdates).mockResolvedValue({
        updates: [...withUpdates.updates, { relPath: "fabric-api.jar", versionId: "v5", versionNumber: "0.120.0" }],
        noCompatible: [],
      });
      mount();
      await ready();
      const button = await screen.findByRole("button", { name: "Update Sodium to 0.6.1" });
      expect(screen.getByRole("button", { name: "Update Mod Menu to 11.0.0" })).toBeTruthy();
      expect(screen.queryByRole("button", { name: /^Update Mystery/ })).toBeNull();
      // ボタンの文字は短く、行き先の版はツールチップで見せる（先頭の行で確かめる）
      expect(button.textContent).toBe("Update");
      const first = screen.getByRole("button", { name: "Update Fabric API to 0.120.0" });
      fireEvent.keyDown(document.body, { key: "Tab" });
      act(() => first.focus());
      expect((await screen.findByRole("tooltip")).textContent).toBe("Update to 0.120.0");
      await press(button);
      await waitFor(() => expect(updateContent).toHaveBeenCalledWith("i1", ["sodium.jar"]));
      await within(toasts()).findAllByText("Mods updated: 1");
      // 終わったら一覧と更新の確認を取り直す
      await waitFor(() => expect(vi.mocked(listContent).mock.calls.filter(([, k]) => k === "mod").length).toBeGreaterThan(1));
      await waitFor(() => expect(vi.mocked(checkContentUpdates).mock.calls.length).toBeGreaterThan(1));
    });

    it("disabled_mod_with_update_shows_the_button", async () => {
      lists.mod = [item({ enabled: false, relPath: "sodium.jar.disabled", fileName: "sodium.jar.disabled" })];
      vi.mocked(checkContentUpdates).mockResolvedValue({
        updates: [{ relPath: "sodium.jar.disabled", versionId: "v2", versionNumber: "0.6.1" }], noCompatible: [],
      });
      mount();
      await ready();
      await press(await screen.findByRole("button", { name: "Update Sodium to 0.6.1" }));
      await waitFor(() => expect(updateContent).toHaveBeenCalledWith("i1", ["sodium.jar.disabled"]));
    });

    it("check_button_forces_a_recheck", async () => {
      mount();
      await ready();
      await waitFor(() => expect(checkContentUpdates).toHaveBeenCalledTimes(1));
      expect(screen.queryByRole("button", { name: /^Update all/ })).toBeNull();
      vi.mocked(checkContentUpdates).mockResolvedValue(withUpdates);
      await press(screen.getByRole("button", { name: "Check for updates" }));
      await waitFor(() => expect(checkContentUpdates).toHaveBeenLastCalledWith("i1", true));
      expect(await screen.findByRole("button", { name: "Update all (2)" })).toBeTruthy();
      expect(screen.getByRole("button", { name: "Update Sodium to 0.6.1" })).toBeTruthy();
    });

    it("a forced check without updates says everything is up to date", async () => {
      mount();
      await ready();
      await waitFor(() => expect(checkContentUpdates).toHaveBeenCalledTimes(1));
      await press(screen.getByRole("button", { name: "Check for updates" }));
      await within(toasts()).findAllByText("Everything is up to date");
    });

    it("update_all_button_shows_the_count_and_runs_one_task", async () => {
      vi.mocked(checkContentUpdates).mockResolvedValue(withUpdates);
      vi.mocked(contentTaskReport).mockResolvedValue(report({ updated: 2 }));
      const task = deferred<TaskSnapshot>();
      vi.mocked(getTask).mockReturnValue(task.promise);
      mount();
      await ready();
      await press(await screen.findByRole("button", { name: "Update all (2)" }));
      await waitFor(() => expect(updateContent).toHaveBeenCalledTimes(1));
      expect(new Set(vi.mocked(updateContent).mock.calls[0]![1])).toEqual(new Set(["sodium.jar", "modmenu.jar"]));
      expect(vi.mocked(updateContent).mock.calls[0]![1]).toHaveLength(2);
      // 更新の最中は行の操作と、もう一度の更新を止める
      expect(screen.getByRole("button", { name: "Enable Sodium" }).hasAttribute("disabled")).toBe(true);
      expect(screen.getByRole("button", { name: "Remove Mystery" }).hasAttribute("disabled")).toBe(true);
      expect(screen.getAllByText("Updating…")).toHaveLength(2);
      expect(screen.getByRole("button", { name: /^Update all/ }).hasAttribute("disabled")).toBe(true);
      await act(async () => task.resolve(snapshot("succeeded")));
      await within(toasts()).findAllByText("Mods updated: 2");
      expect(updateContent).toHaveBeenCalledTimes(1);
    });

    it("partial_failure_lists_names_and_reasons_after_completion", async () => {
      vi.mocked(checkContentUpdates).mockResolvedValue(withUpdates);
      vi.mocked(contentTaskReport).mockResolvedValue(report({
        updated: 1,
        failures: [
          { name: "Mod Menu", error: { code: "content.incompatible", params: { names: "Sodium" }, detail: null } },
        ],
      }));
      mount();
      await ready();
      await press(await screen.findByRole("button", { name: "Update all (2)" }));
      const summary = await screen.findByRole("alert", { name: "Mods that couldn't be updated: 1" });
      expect(within(summary).getByText("Mod Menu")).toBeTruthy();
      expect(within(summary).getByText(/declared incompatible with installed mods \(Sodium\)/)).toBeTruthy();
      await within(toasts()).findAllByText("Mods updated: 1");
      // 閉じられる
      await press(within(summary).getByRole("button", { name: "Dismiss" }));
      expect(screen.queryByRole("alert", { name: "Mods that couldn't be updated: 1" })).toBeNull();
    });

    it("the failure summary survives reopening the tab", async () => {
      vi.mocked(checkContentUpdates).mockResolvedValue(withUpdates);
      vi.mocked(contentTaskReport).mockResolvedValue(report({
        updated: 1,
        failures: [{ name: "Mod Menu", error: { code: "net.offline", params: {}, detail: null } }],
      }));
      const view = mount();
      await ready();
      await press(await screen.findByRole("button", { name: "Update all (2)" }));
      await screen.findByRole("alert", { name: "Mods that couldn't be updated: 1" });
      handlers = null;
      view.remount();
      await ready();
      const summary = await screen.findByRole("alert", { name: "Mods that couldn't be updated: 1" });
      expect(within(summary).getByText("Mod Menu")).toBeTruthy();
    });

    it("toggling, removing or adding a mod checks updates again", async () => {
      mount();
      await ready();
      await waitFor(() => expect(checkContentUpdates).toHaveBeenCalledTimes(1));
      // 有効、無効の切り替え
      await press(screen.getByRole("button", { name: "Enable Sodium" }));
      await waitFor(() => expect(setContentEnabled).toHaveBeenCalled());
      await waitFor(() => expect(checkContentUpdates).toHaveBeenCalledTimes(2));
      // 削除
      await press(screen.getByRole("button", { name: "Remove Mystery" }));
      await press(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Remove" }));
      await waitFor(() => expect(removeContent).toHaveBeenCalled());
      await waitFor(() => expect(checkContentUpdates).toHaveBeenCalledTimes(3));
      // ファイルの追加
      await act(async () => handlers!.drop(["C:\Downloads\a.jar"]));
      await waitFor(() => expect(addContentFiles).toHaveBeenCalled());
      await waitFor(() => expect(checkContentUpdates).toHaveBeenCalledTimes(4));
    });

    it("a failed or cancelled update task is reported", async () => {
      vi.mocked(checkContentUpdates).mockResolvedValue(withUpdates);
      vi.mocked(contentTaskReport).mockResolvedValue(null);
      vi.mocked(getTask).mockResolvedValueOnce(snapshot("failed", { code: "net.offline", params: {}, detail: null }));
      mount();
      await ready();
      await press(await screen.findByRole("button", { name: "Update Sodium to 0.6.1" }));
      await within(toasts()).findByText("Can't connect to the internet");
      vi.mocked(getTask).mockResolvedValueOnce(snapshot("cancelled"));
      await press(await screen.findByRole("button", { name: "Update Sodium to 0.6.1" }));
      await within(toasts()).findAllByText("The update was cancelled");
    });

    it("a rejected update start shows the error", async () => {
      vi.mocked(checkContentUpdates).mockResolvedValue(withUpdates);
      vi.mocked(updateContent).mockRejectedValue(new IpcError({ code: "net.offline", params: {}, detail: null }));
      mount();
      await ready();
      await press(await screen.findByRole("button", { name: "Update Sodium to 0.6.1" }));
      await within(toasts()).findByText("Can't connect to the internet");
      await waitFor(() =>
        expect(screen.getByRole("button", { name: "Enable Sodium" }).hasAttribute("disabled")).toBe(false));
    });

    it("offline_check_shows_a_quiet_message_and_no_update_buttons", async () => {
      vi.mocked(checkContentUpdates).mockRejectedValue(new IpcError({ code: "net.offline", params: {}, detail: null }));
      mount();
      await ready();
      expect(await screen.findByText(quiet)).toBeTruthy();
      expect(screen.queryByRole("button", { name: /^Update (all|.+ to )/ })).toBeNull();
      expect(within(toasts()).queryByRole("alert")).toBeNull();
      // 押して確かめ直しても、同じ控えめな表示にとどめる
      await press(screen.getByRole("button", { name: "Check for updates" }));
      await waitFor(() => expect(checkContentUpdates).toHaveBeenLastCalledWith("i1", true));
      expect(await screen.findByText(quiet)).toBeTruthy();
      expect(within(toasts()).queryByRole("alert")).toBeNull();
    });

    it("a rate-limited check is also quiet", async () => {
      vi.mocked(checkContentUpdates).mockRejectedValue(
        new IpcError({ code: "net.http_status", params: { status: 429 }, detail: null }),
      );
      mount();
      await ready();
      expect(await screen.findByText(quiet)).toBeTruthy();
      expect(screen.queryByText(/HTTP 429/)).toBeNull();
    });

    it("other check errors are shown as a failure", async () => {
      vi.mocked(checkContentUpdates).mockRejectedValue(
        new IpcError({ code: "net.http_status", params: { status: 500 }, detail: null }),
      );
      mount();
      await ready();
      expect(await screen.findByText(/HTTP 500/)).toBeTruthy();
      expect(screen.queryByText(quiet)).toBeNull();
    });

    it("status_filter_has_update_available", async () => {
      vi.mocked(checkContentUpdates).mockResolvedValue(withUpdates);
      mount();
      await ready();
      await screen.findByRole("button", { name: "Update Sodium to 0.6.1" });
      const status = screen.getByRole("toolbar", { name: "Status" });
      fireEvent.click(within(status).getByRole("button", { name: "Update available" }));
      const names = within(screen.getByRole("grid"))
        .getAllByRole("button", { name: /^Enable / })
        .map((b) => b.getAttribute("aria-label"));
      expect(names).toEqual(["Enable Mod Menu", "Enable Sodium"]);
      // パックのサブタブには出さない
      fireEvent.click(subTab(/^Resource packs/));
      await screen.findByRole("button", { name: "Enable Faithful" });
      expect(
        within(screen.getByRole("toolbar", { name: "Status" })).queryByRole("button", { name: "Update available" }),
      ).toBeNull();
    });

    it("row_shows_a_hint_when_no_compatible_version_is_found", async () => {
      vi.mocked(checkContentUpdates).mockResolvedValue(withUpdates);
      mount();
      await ready();
      const hints = await screen.findAllByRole("img", { name: "No version for this instance was found on Modrinth" });
      expect(hints).toHaveLength(1);
      expect(hints[0]!.closest("[role=row]")!.textContent).toContain("Fabric API");
    });
  });
});
