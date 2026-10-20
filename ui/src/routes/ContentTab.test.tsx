import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import {
  addContentFiles,
  getContentIcon,
  identifyContent,
  listContent,
  listContentCategories,
  removeContent,
  setContentEnabled,
} from "../api/commands";
import { type DragDropHandlers, onFilesDropped } from "../api/dragDrop";
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

function mount(busy = false) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  const tree = (b: boolean) => (
    <QueryClientProvider client={client}>
      <ContentTab instance={instance} busy={b} />
      <ToastRegion />
    </QueryClientProvider>
  );
  const { rerender } = render(tree(busy));
  return { setBusy: (b: boolean) => rerender(tree(b)) };
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
    expect(screen.getByText("You can't change content while the instance is running, installing or being duplicated.")).toBeTruthy();
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
});
