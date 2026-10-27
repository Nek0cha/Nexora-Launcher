import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { Profiler } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { listLoaderGameVersions, listLoaderVersions, listSyncProfiles, listVersions } from "../api/commands";
import { IpcError } from "../api/ipc";
import type { LoaderKind } from "../bindings/LoaderKind";
import { INSTANCES_KEY } from "../features/instances/useInstances";
import i18n from "../i18n";
import { CreateInstanceDialog } from "./CreateInstanceDialog";

vi.mock("../api/commands", () => ({
  listVersions: vi.fn(),
  createInstance: vi.fn(),
  listInstances: vi.fn().mockResolvedValue([]),
  listLoaderGameVersions: vi.fn(),
  listLoaderVersions: vi.fn(),
  systemMemoryMb: vi.fn().mockResolvedValue(16384),
  listSyncProfiles: vi.fn(),
  joinSyncProfile: vi.fn(),
}));

const VERSIONS = [
  { id: "1.21.5", releaseTime: "2025-03-25T00:00:00Z" },
  { id: "1.21.4", releaseTime: "2024-12-03T00:00:00Z" },
  { id: "1.16.5", releaseTime: "2021-01-14T00:00:00Z" },
];
const GAME_VERSIONS: Partial<Record<LoaderKind, string[]>> = {
  fabric: ["1.21.5", "1.21.4"],
  quilt: ["1.21.5", "1.21.4"],
  forge: ["1.21.5", "1.16.5"],
  neoforge: ["1.21.5"],
};
const LOADERS = ["fabric", "quilt", "forge", "neoforge"] as const;
const TAB_NAMES = ["Vanilla", "Fabric", "Forge", "NeoForge", "Quilt"];

let client: QueryClient;

/** 保留にして、テストから届けたり失敗させたりできる問い合わせ */
function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

const offline = () => new IpcError({ code: "net.offline", params: {}, detail: null });
const tabs = () => screen.queryAllByRole("tab");
const tab = (dialog: HTMLElement, name: string) => within(dialog).getByRole("tab", { name });
const isDisabledTab = (el: HTMLElement) => el.getAttribute("aria-disabled") === "true";
const nameInput = (dialog: HTMLElement) => within(dialog).getByRole("textbox", { name: "Name" }) as HTMLInputElement;
const createButton = (dialog: HTMLElement) => within(dialog).getByRole("button", { name: "Create" }) as HTMLButtonElement;

function newClient() {
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  client.setQueryData(INSTANCES_KEY, []);
}

/** コミットのたびにタブの無効の状態を記録する。描画の途中の見え方（無効から有効への切り替わり）を確かめるため */
function mountRecording() {
  const frames: Record<string, boolean>[] = [];
  const record = () => {
    const now = Object.fromEntries(tabs().map((el) => [el.textContent ?? "", isDisabledTab(el)]));
    if (Object.keys(now).length > 0) frames.push(now);
  };
  newClient();
  render(
    <QueryClientProvider client={client}>
      <Profiler id="create" onRender={record}>
        <CreateInstanceDialog trigger="quiet" />
      </Profiler>
    </QueryClientProvider>,
  );
  return frames;
}

function mount() {
  newClient();
  render(
    <QueryClientProvider client={client}>
      <CreateInstanceDialog trigger="quiet" />
    </QueryClientProvider>,
  );
}

async function open() {
  fireEvent.click(screen.getByRole("button", { name: "Create instance" }));
  return screen.findByRole("dialog");
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
  vi.mocked(listVersions).mockResolvedValue(VERSIONS);
  vi.mocked(listLoaderGameVersions).mockImplementation(async (loader) => GAME_VERSIONS[loader] ?? []);
  vi.mocked(listLoaderVersions).mockResolvedValue([]);
  vi.mocked(listSyncProfiles).mockResolvedValue([]);
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("CreateInstanceDialog shows everything at once", () => {
  it("shows no tabs until versions and the four loader lists have finished, then a skeleton, then the content together", async () => {
    const last = deferred<string[]>();
    vi.mocked(listLoaderGameVersions).mockImplementation((loader) =>
      loader === "neoforge" ? last.promise : Promise.resolve(GAME_VERSIONS[loader] ?? []),
    );
    mount();
    const dialog = await open();
    expect(tabs()).toEqual([]);
    expect(within(dialog).queryByRole("textbox", { name: "Name" })).toBeNull();
    // 少し待つとスケルトン。中身（タブ、名前、版）はまだ無い
    expect(await within(dialog).findByTestId("create-skeleton")).toBeTruthy();
    expect(tabs()).toEqual([]);
    // スケルトンの間も閉じられる
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeTruthy();

    await act(async () => last.resolve(GAME_VERSIONS.neoforge ?? []));
    await waitFor(() => expect(tabs().length).toBe(5));
    expect(within(dialog).queryByTestId("create-skeleton")).toBeNull();
    expect(nameInput(dialog).value).toBe("1.21.5");
    for (const name of TAB_NAMES) expect(isDisabledTab(tab(dialog, name))).toBe(false);
  });

  it("never renders a tab disabled and then enabled once the tabs appear", async () => {
    const gates = Object.fromEntries(LOADERS.map((l) => [l, deferred<string[]>()]));
    vi.mocked(listLoaderGameVersions).mockImplementation((loader) => gates[loader]!.promise);
    const frames = mountRecording();
    await open();
    // 届く順をばらばらにする
    for (const l of ["quilt", "fabric", "neoforge", "forge"] as const) {
      await act(async () => gates[l]!.resolve(GAME_VERSIONS[l] ?? []));
    }
    await waitFor(() => expect(tabs().length).toBe(5));
    await act(async () => {
      await new Promise((r) => setTimeout(r, 50));
    });
    expect(frames.length).toBeGreaterThan(0);
    for (const name of TAB_NAMES) {
      const seq = frames.map((f) => f[name]);
      expect(seq.every((v) => v !== undefined)).toBe(true);
      // 最初に見えた状態のまま変わらない（無効から有効へ変わるフレームが無い）
      expect(seq.every((v) => v === seq[0]), `${name}: ${seq.join(",")}`).toBe(true);
    }
    expect(Object.values(frames[0]!).every((disabled) => !disabled)).toBe(true);
  });

  it("decides the selected version and the default name when the content first appears", async () => {
    mount();
    const dialog = await open();
    await waitFor(() => expect(tabs().length).toBe(5));
    expect(nameInput(dialog).value).toBe("1.21.5");
    expect(within(dialog).getByRole("button", { name: /Version/ }).textContent).toContain("1.21.5");
    expect(within(dialog).queryByText("Loading versions")).toBeNull();
  });

  it("shows a failed loader as disabled with a reason while the others stay usable", async () => {
    vi.mocked(listLoaderGameVersions).mockImplementation(async (loader) => {
      if (loader === "forge") throw offline();
      return GAME_VERSIONS[loader] ?? [];
    });
    const frames = mountRecording();
    const dialog = await open();
    await waitFor(() => expect(tabs().length).toBe(5));
    expect(isDisabledTab(tab(dialog, "Forge"))).toBe(true);
    expect(tab(dialog, "Forge").querySelector("[title]")?.getAttribute("title")).toBe("Couldn't load");
    const id = tab(dialog, "Forge").getAttribute("aria-describedby");
    expect(document.getElementById(id as string)?.textContent).toBe("Couldn't load");
    for (const name of ["Vanilla", "Fabric", "NeoForge", "Quilt"]) expect(isDisabledTab(tab(dialog, name))).toBe(false);
    expect(within(dialog).getByRole("button", { name: "Retry loading loaders" })).toBeTruthy();
    expect(frames.every((f) => f.Forge === true && f.Fabric === false)).toBe(true);
    // Vanilla はそのまま作れる
    await waitFor(() => expect(createButton(dialog).disabled).toBe(false));
  });

  it("skips the skeleton when the lists were prefetched", async () => {
    newClient();
    client.setQueryData(["versions"], VERSIONS);
    for (const l of LOADERS) client.setQueryData(["loader-game-versions", l], GAME_VERSIONS[l]);
    render(
      <QueryClientProvider client={client}>
        <CreateInstanceDialog trigger="quiet" />
      </QueryClientProvider>,
    );
    const dialog = await open();
    // 開いた直後の最初の描画から中身がそろっている
    expect(within(dialog).queryByTestId("create-skeleton")).toBeNull();
    expect(tabs().length).toBe(5);
    expect(nameInput(dialog).value).toBe("1.21.5");
    expect(isDisabledTab(tab(dialog, "Fabric"))).toBe(false);
    expect(listVersions).not.toHaveBeenCalled();
    expect(listLoaderGameVersions).not.toHaveBeenCalled();
  });

  it("does not wait forever on a list that never answers", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    try {
      vi.mocked(listLoaderGameVersions).mockImplementation((loader) =>
        loader === "quilt" ? new Promise(() => {}) : Promise.resolve(GAME_VERSIONS[loader] ?? []),
      );
      mount();
      const dialog = await open();
      expect(tabs()).toEqual([]);
      for (let i = 0; i < 40; i++) {
        await act(async () => {
          await vi.advanceTimersByTimeAsync(500);
        });
      }
      expect(tabs().length).toBe(5);
      // 届いていない一覧のタブは読み込み中として選べない。ほかは使える
      expect(isDisabledTab(tab(dialog, "Quilt"))).toBe(true);
      expect(tab(dialog, "Quilt").querySelector("[title]")?.getAttribute("title")).toBe("Loading loader versions");
      expect(isDisabledTab(tab(dialog, "Fabric"))).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });
});
