import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { getSettings, listContentCategories, listInstances, listVersions, searchModrinth } from "../../api/commands";
import type { SearchHit } from "../../bindings/SearchHit";
import type { SearchPage } from "../../bindings/SearchPage";
import { IpcError } from "../../api/ipc";
import i18n from "../../i18n";
import { ModrinthBrowser, SEARCH_DEBOUNCE_MS } from "./ModrinthBrowser";

vi.mock("../../api/commands", () => ({
  searchModrinth: vi.fn(),
  listInstances: vi.fn(),
  getSettings: vi.fn(),
  listContentCategories: vi.fn(),
  listContent: vi.fn(),
  listVersions: vi.fn(),
  openModrinthProject: vi.fn(),
}));

const hit = (over: Partial<SearchHit> = {}): SearchHit => ({
  projectId: "AANobbMI", slug: "sodium", title: "Sodium", description: "Rendering engine", iconUrl: null,
  downloads: 12_345_678, categories: ["fabric", "optimization"], versions: ["1.20.1"], clientSide: "required",
  serverSide: "unsupported", ...over,
});
const page = (hits: SearchHit[]): SearchPage => ({ hits, offset: 0, total: hits.length });

function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

beforeAll(async () => {
  await i18n.changeLanguage("en");
});
beforeEach(() => {
  vi.mocked(listInstances).mockResolvedValue([]);
  vi.mocked(getSettings).mockResolvedValue({ theme: "system", language: null, maxConcurrentDownloads: 8, lastAddedInstance: null });
  vi.mocked(listVersions).mockResolvedValue([{ id: "1.21", releaseTime: "2024-06-13T00:00:00Z" }]);
  vi.mocked(listContentCategories).mockResolvedValue(["optimization"]);
});
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

function mount() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <ModrinthBrowser />
    </QueryClientProvider>,
  );
  return client;
}
const wait = (ms: number) => act(async () => void (await new Promise((r) => setTimeout(r, ms))));

describe("ModrinthBrowser loading", () => {
  it("shows_a_skeleton_and_no_partial_content_until_search_and_facets_are_all_ready", async () => {
    const first = deferred<SearchPage>();
    vi.mocked(searchModrinth).mockReturnValue(first.promise);
    mount();
    await wait(250);
    // 絞り込みの札は届いているが、結果がまだ。途中の姿は出さない
    expect(screen.queryByTestId("modrinth-facets")).toBeNull();
    expect(screen.getByTestId("modrinth-skeleton")).toBeTruthy();
    await act(async () => first.resolve(page([hit()])));
    await wait(350);
    expect(screen.queryByTestId("modrinth-skeleton")).toBeNull();
    expect(screen.getByRole("article", { name: "Sodium" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Optimization" })).toBeTruthy();
  });

  it("keeps_the_previous_rows_and_dims_them_only_when_a_new_search_takes_long", async () => {
    vi.mocked(searchModrinth).mockResolvedValueOnce(page([hit()]));
    mount();
    await screen.findByRole("article", { name: "Sodium" });
    const next = deferred<SearchPage>();
    vi.mocked(searchModrinth).mockReturnValue(next.promise);
    fireEvent.change(screen.getByRole("searchbox", { name: "Search mods" }), { target: { value: "iris" } });
    await wait(SEARCH_DEBOUNCE_MS + 100);
    // 一覧は空にせず、スケルトンにも戻さない
    expect(screen.getByRole("article", { name: "Sodium" })).toBeTruthy();
    expect(screen.queryByTestId("modrinth-skeleton")).toBeNull();
    expect(screen.queryByText("Searching...")).toBeNull();
    // すぐ返る検索では薄くしない（薄くして戻すとちらつく）。待ちが 150ms を超えたら薄くする
    expect(screen.getByTestId("modrinth-results").getAttribute("aria-busy")).not.toBe("true");
    await wait(200);
    expect(screen.getByTestId("modrinth-results").getAttribute("aria-busy")).toBe("true");
    await act(async () => next.resolve(page([hit({ projectId: "x", title: "Iris", slug: "iris" })])));
    expect(await screen.findByRole("article", { name: "Iris" })).toBeTruthy();
    expect(screen.getByTestId("modrinth-results").getAttribute("aria-busy")).not.toBe("true");
  });

  it("keeps_the_total_slot_so_the_search_field_does_not_change_width", async () => {
    vi.mocked(searchModrinth).mockResolvedValue(page([hit()]));
    mount();
    await screen.findByRole("article", { name: "Sodium" });
    // 件数の場所は、件数が決まる前から（空でも）同じ幅で確保する
    expect(screen.getByTestId("modrinth-total").className).toContain("min-w-");
  });

  it("announces_loading_to_screen_readers_in_the_skeleton", async () => {
    vi.mocked(searchModrinth).mockReturnValue(deferred<SearchPage>().promise);
    mount();
    await wait(250);
    expect(within(screen.getByTestId("modrinth-skeleton")).getByRole("status").textContent).toBe("Searching...");
  });

  it("shows_why_the_version_list_is_missing_and_retries_it", async () => {
    // 枠を出したあとに中身が付く側の問い合わせも、取り直すたびに失敗させる
    vi.mocked(listVersions).mockRejectedValue(new IpcError({ code: "net.offline", params: {}, detail: null }));
    vi.mocked(searchModrinth).mockResolvedValue(page([hit()]));
    mount();
    const failed = await screen.findByTestId("versions-failed");
    expect(failed.textContent).toContain("Retry");
    const calls = vi.mocked(listVersions).mock.calls.length;
    vi.mocked(listVersions).mockResolvedValue([{ id: "1.21", releaseTime: "2024-06-13T00:00:00Z" }]);
    await act(async () => fireEvent.click(within(failed).getByRole("button", { name: "Retry" })));
    await waitFor(() => expect(screen.queryByTestId("versions-failed")).toBeNull());
    expect(vi.mocked(listVersions).mock.calls.length).toBe(calls + 1);
  });
});
