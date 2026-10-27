import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import {
  checkContentUpdates,
  getContentIcon,
  identifyContent,
  listContent,
  listContentCategories,
} from "../api/commands";
import { onFilesDropped } from "../api/dragDrop";
import { IpcError } from "../api/ipc";
import type { ContentItem } from "../bindings/ContentItem";
import type { Instance } from "../bindings/Instance";
import i18n from "../i18n";
import { ContentTab } from "./ContentTab";

vi.mock("../api/commands", () => ({
  listContent: vi.fn(),
  identifyContent: vi.fn(),
  listContentCategories: vi.fn(),
  getContentIcon: vi.fn(),
  checkContentUpdates: vi.fn(),
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

function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

// jsdom は要素の大きさを持たないので、Virtualizer が窓を計算できるよう一定の大きさを返す
const spies: { mockRestore(): void }[] = [];
beforeAll(async () => {
  await i18n.changeLanguage("en");
  const proto = HTMLElement.prototype;
  spies.push(
    vi.spyOn(proto, "clientWidth", "get").mockReturnValue(800),
    vi.spyOn(proto, "clientHeight", "get").mockReturnValue(600),
    vi.spyOn(proto, "offsetWidth", "get").mockReturnValue(800),
    vi.spyOn(proto, "offsetHeight", "get").mockReturnValue(600),
    vi.spyOn(proto, "scrollHeight", "get").mockReturnValue(600),
    vi.spyOn(proto, "getBoundingClientRect").mockReturnValue(DOMRect.fromRect({ x: 0, y: 0, width: 800, height: 600 })),
  );
});
afterAll(() => {
  for (const spy of spies) spy.mockRestore();
});

beforeEach(() => {
  vi.mocked(onFilesDropped).mockResolvedValue(() => {});
  vi.mocked(listContent).mockImplementation(async (_id, kind) => (kind === "mod" ? [item()] : []));
  vi.mocked(identifyContent).mockResolvedValue({ identified: 0, local: 0 });
  vi.mocked(getContentIcon).mockResolvedValue(null);
  vi.mocked(checkContentUpdates).mockResolvedValue({ updates: [], noCompatible: [] });
});
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

function mount() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={["/instances/i1"]}>
        <Routes>
          <Route path="/instances/:id" element={<ContentTab instance={instance} busy={false} />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
  return client;
}

const wait = (ms: number) => act(async () => void (await new Promise((r) => setTimeout(r, ms))));
const toggle = () => screen.queryByRole("button", { name: "Enable Sodium" });

describe("ContentTab loading", () => {
  it("shows_a_skeleton_and_no_partial_content_until_the_list_and_the_categories_are_both_ready", async () => {
    const cats = deferred<string[]>();
    vi.mocked(listContentCategories).mockReturnValue(cats.promise);
    mount();
    // 一覧は届いたが、カテゴリがまだ。途中の姿は出さない
    await wait(250);
    expect(toggle()).toBeNull();
    expect(screen.queryByTestId("content-facets")).toBeNull();
    expect(screen.getByTestId("content-skeleton")).toBeTruthy();
    await act(async () => cats.resolve(["optimization"]));
    // 出したスケルトンは 300ms は保つ
    await wait(350);
    expect(screen.queryByTestId("content-skeleton")).toBeNull();
    expect(toggle()).toBeTruthy();
    expect(screen.getByRole("button", { name: "Optimization" })).toBeTruthy();
  });

  it("keeps_the_content_when_a_settled_query_goes_back_to_pending", async () => {
    vi.mocked(listContentCategories).mockResolvedValue(["optimization"]);
    const client = mount();
    await screen.findByRole("button", { name: "Enable Sodium" });
    // 取り直しの途中で pending に戻っても、枠には戻さない
    vi.mocked(listContentCategories).mockReturnValue(deferred<string[]>().promise);
    await act(async () => void client.resetQueries({ queryKey: ["content-categories"] }));
    await wait(450);
    expect(toggle()).toBeTruthy();
    expect(screen.queryByTestId("content-skeleton")).toBeNull();
  });

  it("shows_a_failed_category_list_with_its_reason_at_the_same_time_as_the_rows", async () => {
    vi.mocked(listContentCategories).mockRejectedValue(new IpcError({ code: "net.offline", params: {}, detail: null }));
    mount();
    await screen.findByRole("button", { name: "Enable Sodium" });
    expect(screen.getByTestId("categories-failed")).toBeTruthy();
  });

  it("reserves_the_update_slot_on_every_mod_row_before_any_update_is_known", async () => {
    vi.mocked(listContentCategories).mockResolvedValue([]);
    mount();
    await screen.findByRole("button", { name: "Enable Sodium" });
    expect(screen.getAllByTestId("row-update-slot").length).toBe(1);
  });
});
