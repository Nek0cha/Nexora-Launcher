import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import {
  contentTaskReport,
  getSettings,
  getTask,
  installContent,
  listContent,
  listContentCategories,
  listInstances,
  listVersions,
  openModrinthProject,
  previewContentInstall,
  searchModrinth,
} from "../../api/commands";
import { IpcError } from "../../api/ipc";
import type { Instance } from "../../bindings/Instance";
import type { SearchHit } from "../../bindings/SearchHit";
import type { SearchPage } from "../../bindings/SearchPage";
import type { SearchQuery } from "../../bindings/SearchQuery";
import type { TaskSnapshot } from "../../bindings/TaskSnapshot";
import i18n from "../../i18n";
import { ToastRegion } from "../toast/ToastRegion";
import { clearToasts } from "../toast/useToast";
import { ModrinthBrowser, SEARCH_DEBOUNCE_MS } from "./ModrinthBrowser";

vi.mock("../../api/commands", () => ({
  identifyContent: vi.fn(),
  searchModrinth: vi.fn(),
  previewContentInstall: vi.fn(),
  installContent: vi.fn(),
  getTask: vi.fn(),
  contentTaskReport: vi.fn(),
  openModrinthProject: vi.fn(),
  listInstances: vi.fn(),
  getSettings: vi.fn(),
  listContentCategories: vi.fn(),
  listContent: vi.fn(),
  listVersions: vi.fn(),
}));

function inst(patch: Partial<Instance>): Instance {
  return {
    id: "i1", name: "Survival", dirName: "Survival", mcVersion: "1.20.1", state: "ready",
    createdAt: 0, lastPlayedAt: null, playTimeMs: 0,
    memoryMode: "auto", memoryMb: null, javaMode: "auto", javaPath: null, jvmArgs: "",
    loader: "fabric", loaderVersion: "0.16.0",
    ...patch,
  };
}

const fabric120 = inst({ id: "f120", name: "Fabric 1.20.1" });
const quilt120 = inst({ id: "q120", name: "Quilt 1.20.1", loader: "quilt" });
const forge120 = inst({ id: "fo120", name: "Forge 1.20.1", loader: "forge" });
const fabric121 = inst({ id: "f121", name: "Fabric 1.21", mcVersion: "1.21" });
const vanilla = inst({ id: "v", name: "Vanilla", loader: "vanilla", loaderVersion: null });

function hit(patch: Partial<SearchHit> = {}): SearchHit {
  return {
    projectId: "AANobbMI", slug: "sodium", title: "Sodium", description: "Rendering engine",
    iconUrl: null, downloads: 12_345_678, categories: ["fabric", "optimization"], versions: ["1.20.1"],
    clientSide: "required", serverSide: "unsupported",
    ...patch,
  };
}

const page = (hits: SearchHit[], offset = 0, total = hits.length): SearchPage => ({ hits, offset, total });

function snapshot(state: TaskSnapshot["state"], error: TaskSnapshot["error"] = null): TaskSnapshot {
  return {
    id: "t1", kind: "content", instanceId: "f120", state, phase: null, progress: { current: 0, total: 0 },
    revision: 1, error, createdAt: 0, updatedAt: 0,
  };
}

let lastAdded: string | null;

beforeAll(async () => {
  await i18n.changeLanguage("en");
});

beforeEach(() => {
  lastAdded = null;
  vi.mocked(searchModrinth).mockResolvedValue(page([hit()]));
  vi.mocked(listInstances).mockResolvedValue([fabric120, quilt120, forge120, fabric121, vanilla]);
  vi.mocked(getSettings).mockImplementation(async () => ({
    theme: "system", language: null, maxConcurrentDownloads: 8, lastAddedInstance: lastAdded,
  }));
  vi.mocked(listContentCategories).mockResolvedValue(["optimization", "library"]);
  vi.mocked(listVersions).mockResolvedValue([
    { id: "1.21", releaseTime: "2024-06-13T00:00:00Z" },
    { id: "1.20.1", releaseTime: "2023-06-12T00:00:00Z" },
  ]);
  vi.mocked(previewContentInstall).mockResolvedValue({
    versionNumber: "0.5.3", dependencyCount: 0, incompatible: [], enablesInstalled: false,
  });
  vi.mocked(listContent).mockResolvedValue([]);
  vi.mocked(installContent).mockResolvedValue("t1");
  vi.mocked(getTask).mockResolvedValue(snapshot("succeeded"));
  vi.mocked(contentTaskReport).mockResolvedValue({ installed: 1, dependenciesAdded: 0, updated: 0, failures: [] });
  vi.mocked(openModrinthProject).mockResolvedValue(null);
});

afterEach(() => {
  act(() => clearToasts());
  cleanup();
  vi.resetAllMocks();
});

function mount(instance?: Instance) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <ModrinthBrowser instance={instance} />
      <ToastRegion />
    </QueryClientProvider>,
  );
}

const lastQuery = (): SearchQuery => vi.mocked(searchModrinth).mock.lastCall![1];
const searchbox = () => screen.getByRole("searchbox", { name: "Search mods" });
const card = (name: string) => screen.getByRole("article", { name });
const toasts = () => screen.getByRole("region", { name: "Notifications" });

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe("ModrinthBrowser", () => {
  it("typing_sends_one_search_after_the_debounce", async () => {
    mount();
    await screen.findByRole("article", { name: "Sodium" });
    expect(searchModrinth).toHaveBeenCalledTimes(1);
    for (const value of ["s", "so", "sod"]) fireEvent.change(searchbox(), { target: { value } });
    // 入力の直後はまだ送らない
    expect(searchModrinth).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(searchModrinth).toHaveBeenCalledTimes(2));
    expect(lastQuery().query).toBe("sod");
    // 止まったあとに余分な検索が続かない
    await new Promise((r) => setTimeout(r, SEARCH_DEBOUNCE_MS + 100));
    expect(searchModrinth).toHaveBeenCalledTimes(2);
  });

  it("stale_results_do_not_overwrite_newer_input", async () => {
    const first = deferred<SearchPage>();
    const second = deferred<SearchPage>();
    vi.mocked(searchModrinth)
      .mockResolvedValueOnce(page([hit()]))
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(second.promise);
    mount();
    await screen.findByRole("article", { name: "Sodium" });
    fireEvent.change(searchbox(), { target: { value: "iris" } });
    await waitFor(() => expect(searchModrinth).toHaveBeenCalledTimes(2));
    fireEvent.change(searchbox(), { target: { value: "lithium" } });
    await waitFor(() => expect(searchModrinth).toHaveBeenCalledTimes(3));
    // 新しい入力の結果が先に届き、古い入力の結果があとから届く
    await act(async () => second.resolve(page([hit({ projectId: "li", slug: "lithium", title: "Lithium" })])));
    await screen.findByRole("article", { name: "Lithium" });
    await act(async () => first.resolve(page([hit({ projectId: "ir", slug: "iris", title: "Iris" })])));
    expect(screen.queryByRole("article", { name: "Iris" })).toBeNull();
    expect(screen.getByRole("article", { name: "Lithium" })).toBeTruthy();
  });

  it("instance_mode_fixes_version_and_loader_and_hides_those_filters", async () => {
    mount(quilt120);
    await screen.findByRole("article", { name: "Sodium" });
    expect(searchModrinth).toHaveBeenLastCalledWith("q120", expect.anything(), 0);
    expect(lastQuery().mcVersion).toBe("1.20.1");
    expect(lastQuery().loaders).toEqual(["quilt", "fabric"]);
    expect(screen.getByText("Fixed: 1.20.1 / Quilt")).toBeTruthy();
    expect(screen.queryByRole("toolbar", { name: "Loader" })).toBeNull();
    expect(screen.queryByRole("button", { name: /Minecraft version/ })).toBeNull();
    // 項目の絞り込みは使える
    expect(screen.getByRole("toolbar", { name: "Client side" })).toBeTruthy();
  });

  it("global_mode_sends_the_chosen_loaders_and_categories", async () => {
    mount();
    await screen.findByRole("article", { name: "Sodium" });
    expect(searchModrinth).toHaveBeenLastCalledWith(null, expect.anything(), 0);
    expect(lastQuery().mcVersion).toBeNull();
    expect(lastQuery().loaders).toEqual([]);
    fireEvent.click(within(screen.getByRole("toolbar", { name: "Loader" })).getByRole("button", { name: "Forge" }));
    await waitFor(() => expect(lastQuery().loaders).toEqual(["forge"]));
    fireEvent.click(
      within(await screen.findByRole("toolbar", { name: "Categories" })).getByRole("button", { name: "Optimization" }),
    );
    await waitFor(() => expect(lastQuery().categories).toEqual(["optimization"]));
  });

  it("load_more_appends_the_next_page", async () => {
    const first = Array.from({ length: 20 }, (_, i) => hit({ projectId: `p${i}`, slug: `m${i}`, title: `Mod ${i}` }));
    vi.mocked(searchModrinth)
      .mockResolvedValueOnce(page(first, 0, 21))
      .mockResolvedValueOnce(page([hit({ projectId: "p20", slug: "m20", title: "Mod 20" })], 20, 21));
    mount();
    await screen.findByRole("article", { name: "Mod 0" });
    expect(screen.getAllByRole("article")).toHaveLength(20);
    fireEvent.click(screen.getByRole("button", { name: "Load more" }));
    await screen.findByRole("article", { name: "Mod 20" });
    expect(screen.getAllByRole("article")).toHaveLength(21);
    expect(searchModrinth).toHaveBeenLastCalledWith(null, expect.anything(), 20);
    // 最後のページのあとは出さない
    expect(screen.queryByRole("button", { name: "Load more" })).toBeNull();
  });

  it("empty_result_and_offline_states_are_shown", async () => {
    vi.mocked(searchModrinth).mockResolvedValueOnce(page([]));
    mount();
    await screen.findByText("No mods match this search.");
    vi.mocked(searchModrinth).mockRejectedValue(new IpcError({ code: "net.offline", params: {}, detail: null }));
    fireEvent.change(searchbox(), { target: { value: "x" } });
    await screen.findByText("Can't reach Modrinth. Searching and adding mods work when you're online.");
    vi.mocked(searchModrinth).mockResolvedValue(page([hit()]));
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await screen.findByRole("article", { name: "Sodium" });
  });

  it("add_to_instance_menu_lists_only_compatible_instances", async () => {
    mount();
    await screen.findByRole("article", { name: "Sodium" });
    fireEvent.click(within(card("Sodium")).getByRole("button", { name: "Add to instance" }));
    const menu = await screen.findByRole("menu");
    const names = within(menu).getAllByRole("menuitem").map((el) => el.textContent);
    expect(names).toEqual(["Fabric 1.20.1", "Quilt 1.20.1"]);
  });

  it("last_added_button_is_disabled_when_incompatible_or_unset", async () => {
    // 未設定
    mount();
    await screen.findByRole("article", { name: "Sodium" });
    const unset = within(card("Sodium")).getByRole("button", { name: "Add to last used" }) as HTMLButtonElement;
    expect(unset.disabled).toBe(true);
    cleanup();
    // 合わない（Forge）
    lastAdded = "fo120";
    mount();
    await screen.findByRole("article", { name: "Sodium" });
    const forge = (await within(card("Sodium")).findByRole("button", { name: "Add to Forge 1.20.1" })) as HTMLButtonElement;
    expect(forge.disabled).toBe(true);
    cleanup();
    // 合う
    lastAdded = "q120";
    mount();
    await screen.findByRole("article", { name: "Sodium" });
    const quilt = (await within(card("Sodium")).findByRole("button", { name: "Add to Quilt 1.20.1" })) as HTMLButtonElement;
    expect(quilt.disabled).toBe(false);
    fireEvent.click(quilt);
    await waitFor(() => expect(installContent).toHaveBeenCalledWith("q120", "AANobbMI", false));
  });

  it("incompatible_installed_asks_for_confirmation_before_installing", async () => {
    vi.mocked(previewContentInstall).mockResolvedValue({
      versionNumber: "0.5.3", dependencyCount: 1, incompatible: ["OptiFabric"], enablesInstalled: false,
    });
    mount(fabric120);
    await screen.findByRole("article", { name: "Sodium" });
    fireEvent.click(within(card("Sodium")).getByRole("button", { name: "Add" }));
    const dialog = await screen.findByRole("alertdialog", { name: "Add Sodium?" });
    expect(within(dialog).getByText("OptiFabric")).toBeTruthy();
    // 依存が 1 件なら単数で書く
    expect(within(dialog).getByText("Version 0.5.3 will be installed with 1 required mod.")).toBeTruthy();
    // 拒否すると何も入れない
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(installContent).not.toHaveBeenCalled();
    // 了承すると、非互換を受け入れて入れる
    fireEvent.click(within(card("Sodium")).getByRole("button", { name: "Add" }));
    fireEvent.click(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Add anyway" }));
    await waitFor(() => expect(installContent).toHaveBeenCalledWith("f120", "AANobbMI", true));
  });

  it("installing_with_dependencies_shows_the_dependency_toast", async () => {
    vi.mocked(contentTaskReport).mockResolvedValue({ installed: 3, dependenciesAdded: 2, updated: 0, failures: [] });
    mount(fabric120);
    await screen.findByRole("article", { name: "Sodium" });
    fireEvent.click(within(card("Sodium")).getByRole("button", { name: "Add" }));
    await within(toasts()).findByText("Added 2 required mods");
    expect(installContent).toHaveBeenCalledWith("f120", "AANobbMI", false);
    expect(within(toasts()).getByText("Added Sodium to Fabric 1.20.1")).toBeTruthy();
  });

  it("installing_without_dependencies_shows_no_dependency_toast", async () => {
    mount(fabric120);
    await screen.findByRole("article", { name: "Sodium" });
    fireEvent.click(within(card("Sodium")).getByRole("button", { name: "Add" }));
    await within(toasts()).findByText("Added Sodium to Fabric 1.20.1");
    expect(within(toasts()).queryByText(/required mods/)).toBeNull();
  });

  it("a_failed_install_shows_the_error", async () => {
    vi.mocked(getTask).mockResolvedValue(
      snapshot("failed", { code: "content.dependency", params: { projectId: "P7", name: "Lib Seven" }, detail: null }),
    );
    mount(fabric120);
    await screen.findByRole("article", { name: "Sodium" });
    fireEvent.click(within(card("Sodium")).getByRole("button", { name: "Add" }));
    // project の ID ではなく表示名で知らせる
    await within(toasts()).findByText(/Lib Seven/);
    expect(within(toasts()).queryByText(/Added Sodium/)).toBeNull();
  });

  it("one_dependency_is_written_in_the_singular", async () => {
    vi.mocked(contentTaskReport).mockResolvedValue({ installed: 1, dependenciesAdded: 1, updated: 0, failures: [] });
    mount(fabric120);
    await screen.findByRole("article", { name: "Sodium" });
    fireEvent.click(within(card("Sodium")).getByRole("button", { name: "Add" }));
    await within(toasts()).findByText("Added 1 required mod");
  });

  it("a_disabled_installed_copy_is_enabled_instead_of_added", async () => {
    vi.mocked(previewContentInstall).mockResolvedValue({
      versionNumber: "0.5.3", dependencyCount: 0, incompatible: [], enablesInstalled: true,
    });
    mount(fabric120);
    await screen.findByRole("article", { name: "Sodium" });
    fireEvent.click(within(card("Sodium")).getByRole("button", { name: "Add" }));
    await within(toasts()).findByText("Enabled Sodium in Fabric 1.20.1");
    expect(within(toasts()).queryByText(/Added Sodium/)).toBeNull();
  });

  it("an_enabled_installed_mod_shows_installed_instead_of_add", async () => {
    vi.mocked(listContent).mockResolvedValue([
      {
        kind: "mod", relPath: "mods/sodium.jar", fileName: "sodium.jar", enabled: true, size: 1, sha1: "",
        provider: "modrinth", projectId: "AANobbMI", versionId: "v1", pinned: false, installedAs: "explicit",
        name: "Sodium", versionNumber: "0.5.3", description: null, iconUrl: null, hasLocalIcon: false,
        clientSide: null, serverSide: null, categories: [], dependencies: [], identified: true,
      },
    ]);
    mount(fabric120);
    await screen.findByRole("article", { name: "Sodium" });
    const installed = await within(card("Sodium")).findByRole("button", { name: "Installed" });
    expect(installed.hasAttribute("disabled")).toBe(true);
    expect(listContent).toHaveBeenCalledWith("f120", "mod");
  });

  it("an_already_installed_error_is_shown_by_name", async () => {
    vi.mocked(previewContentInstall).mockRejectedValue(
      new IpcError({ code: "content.already_installed", params: { name: "Sodium" }, detail: null }),
    );
    mount(fabric120);
    await screen.findByRole("article", { name: "Sodium" });
    fireEvent.click(within(card("Sodium")).getByRole("button", { name: "Add" }));
    await within(toasts()).findByText("Sodium is already installed in this instance.");
    expect(installContent).not.toHaveBeenCalled();
  });

  it("open_in_browser_calls_open_modrinth_project_with_the_slug", async () => {
    mount();
    await screen.findByRole("article", { name: "Sodium" });
    fireEvent.click(within(card("Sodium")).getByRole("button", { name: "Open on Modrinth" }));
    expect(openModrinthProject).toHaveBeenCalledWith("sodium");
  });

  it("sorts_by_relevance_by_default_and_by_downloads_when_chosen", async () => {
    mount();
    await screen.findByRole("article", { name: "Sodium" });
    expect(lastQuery().sort).toBe("relevance");
    const sort = screen.getByRole("radiogroup", { name: "Sort by" });
    expect(within(sort).getByRole("radio", { name: "Relevance" }).getAttribute("aria-checked")).toBe("true");
    fireEvent.click(within(sort).getByRole("radio", { name: "Downloads" }));
    await waitFor(() => expect(lastQuery().sort).toBe("downloads"));
    // 選んでいる方をもう一度押しても、選択は外れない
    fireEvent.click(within(sort).getByRole("radio", { name: "Downloads" }));
    expect(within(sort).getByRole("radio", { name: "Downloads" }).getAttribute("aria-checked")).toBe("true");
    fireEvent.click(within(sort).getByRole("radio", { name: "Relevance" }));
    await waitFor(() => expect(lastQuery().sort).toBe("relevance"));
  });

  it("the_sort_control_is_also_shown_for_an_instance", async () => {
    mount(fabric120);
    await screen.findByRole("article", { name: "Sodium" });
    fireEvent.click(within(screen.getByRole("radiogroup", { name: "Sort by" })).getByRole("radio", { name: "Downloads" }));
    await waitFor(() => expect(lastQuery().sort).toBe("downloads"));
    expect(lastQuery().mcVersion).toBe("1.20.1");
  });

  it("shows the name, description, downloads and categories of a hit", async () => {
    mount();
    const sodium = await screen.findByRole("article", { name: "Sodium" });
    expect(within(sodium).getByText("Rendering engine")).toBeTruthy();
    expect(within(sodium).getByText("12M downloads")).toBeTruthy();
    expect(within(sodium).getByText("Optimization")).toBeTruthy();
  });
});
