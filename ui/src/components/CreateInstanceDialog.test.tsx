import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  createInstance,
  joinSyncProfile,
  listLoaderGameVersions,
  listLoaderVersions,
  listSyncProfiles,
  listVersions,
} from "../api/commands";
import { IpcError } from "../api/ipc";
import type { CreatedInstance } from "../bindings/CreatedInstance";
import type { LoaderKind } from "../bindings/LoaderKind";
import type { SyncProfile } from "../bindings/SyncProfile";
import { INSTANCES_KEY } from "../features/instances/useInstances";
import i18n from "../i18n";
import { clearToasts } from "./toast/useToast";
import { ToastRegion } from "./toast/ToastRegion";
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
  { id: "1.8.9", releaseTime: "2015-12-09T00:00:00Z" },
];

const created: CreatedInstance = {
  instance: {
    id: "i1", name: "1.21.5", dirName: "1.21.5", mcVersion: "1.21.5", state: "installing",
    createdAt: 0, lastPlayedAt: null, playTimeMs: 0,
    memoryMode: "auto", memoryMb: null, javaMode: "auto", javaPath: null, jvmArgs: "",
  loader: "vanilla", loaderVersion: null,
  },
  taskId: "t1",
};

// ローダーのタブの試験に使う一覧。Forge は実物と同じく 1.8.9〜1.12.2 と 1.16.5 以降だけ。Fabric はこの試験では 1.16.5 と 1.8.9 に対応しないものとする
const LOADER_TEST_VERSIONS = [
  { id: "1.21.5", releaseTime: "2025-03-25T00:00:00Z" },
  { id: "1.21.4", releaseTime: "2024-12-03T00:00:00Z" },
  { id: "1.16.5", releaseTime: "2021-01-14T00:00:00Z" },
  { id: "1.8.9", releaseTime: "2015-12-09T00:00:00Z" },
];
const GAME_VERSIONS: Partial<Record<LoaderKind, string[]>> = {
  fabric: ["1.21.5", "1.21.4", "1.20.1"],
  quilt: ["1.21.5", "1.21.4"],
  forge: ["1.21.5", "1.21.4", "1.16.5"],
  neoforge: ["1.21.5"],
};
const FORGE_VERSIONS = [
  { version: "1.21.5-54.0.1", stable: false },
  { version: "1.21.5-54.0.0", stable: true },
];
const FABRIC_VERSIONS = [
  { version: "0.17.0-beta.1", stable: false },
  { version: "0.16.10", stable: true },
  { version: "0.16.9", stable: true },
];

let client: QueryClient;

function mount() {
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  client.setQueryData(INSTANCES_KEY, []);
  render(
    <QueryClientProvider client={client}>
      <CreateInstanceDialog trigger="quiet" />
      <ToastRegion />
    </QueryClientProvider>,
  );
}

async function open() {
  fireEvent.click(screen.getByRole("button", { name: "Create instance" }));
  return screen.findByRole("dialog");
}

const nameInput = (dialog: HTMLElement) => within(dialog).getByRole("textbox", { name: "Name" }) as HTMLInputElement;

async function pickVersion(dialog: HTMLElement, id: string) {
  fireEvent.click(within(dialog).getByRole("button", { name: /Version/ }));
  fireEvent.click(await screen.findByRole("option", { name: id }));
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
  vi.mocked(listLoaderGameVersions).mockImplementation(async (loader) => GAME_VERSIONS[loader] ?? []);
  vi.mocked(listLoaderVersions).mockResolvedValue(FABRIC_VERSIONS);
  vi.mocked(listSyncProfiles).mockResolvedValue([]);
});

const tab = (dialog: HTMLElement, name: string) => within(dialog).getByRole("tab", { name });
const isDisabledTab = (el: HTMLElement) => el.getAttribute("aria-disabled") === "true";
const isSelectedTab = (el: HTMLElement) => el.getAttribute("aria-selected") === "true";
const createButton = (dialog: HTMLElement) => within(dialog).getByRole("button", { name: "Create" }) as HTMLButtonElement;
const advancedButton = (dialog: HTMLElement) => within(dialog).getByRole("button", { name: "Advanced settings" });

async function choose(dialog: HTMLElement, select: RegExp, option: string) {
  fireEvent.click(within(dialog).getByRole("button", { name: select }));
  fireEvent.click(await screen.findByRole("option", { name: option }));
}

/** ローダーの一覧が届き、Fabric のタブが選べるようになるまで待つ。 */
async function openWithLoaders() {
  vi.mocked(listVersions).mockResolvedValue(LOADER_TEST_VERSIONS);
  mount();
  const dialog = await open();
  await waitFor(() => expect(isDisabledTab(tab(dialog, "Fabric"))).toBe(false));
  return dialog;
}

afterEach(() => {
  cleanup();
  clearToasts();
  vi.clearAllMocks();
});

describe("CreateInstanceDialog", () => {
  it("selects the newest version, names the instance after it and focuses the name", async () => {
    vi.mocked(listVersions).mockResolvedValue(VERSIONS);
    mount();
    const dialog = await open();
    await waitFor(() => expect(nameInput(dialog).value).toBe("1.21.5"));
    // 名前の欄は中身と同時に出るため、フォーカスは少し遅れて移る
    await waitFor(() => expect(document.activeElement).toBe(nameInput(dialog)));
    expect(within(dialog).getByRole("button", { name: /Version/ }).textContent).toContain("1.21.5");
  });

  it("keeps row labels from shrinking or wrapping and reserves the scrollbar's place, so the form does not reflow when it scrolls", async () => {
    mount();
    const dialog = await open();
    await waitFor(() => expect(nameInput(dialog)).toBeTruthy());
    const form = dialog.querySelector("form") as HTMLElement;
    const scroller = form.querySelector(":scope > div.overflow-y-auto") as HTMLElement;
    expect(scroller.className).toContain("[scrollbar-gutter:stable]");
    for (const name of ["Name", "Version"]) {
      const label = within(dialog).getByText(name, { selector: "label, span" });
      expect(label.className.split(/\s+/)).toEqual(expect.arrayContaining(["shrink-0", "whitespace-nowrap"]));
    }
  });

  it("shows the skeleton, not a half-built form, while versions load", async () => {
    vi.mocked(listVersions).mockReturnValue(new Promise(() => {}));
    mount();
    const dialog = await open();
    expect(await within(dialog).findByTestId("create-skeleton")).toBeTruthy();
    expect(within(dialog).queryByText("Loading versions")).toBeNull();
    expect(within(dialog).getByRole("button", { name: "Create" }).hasAttribute("disabled")).toBe(true);
  });

  it("renames with the version until the user types a name", async () => {
    vi.mocked(listVersions).mockResolvedValue(VERSIONS);
    mount();
    const dialog = await open();
    await waitFor(() => expect(nameInput(dialog).value).toBe("1.21.5"));
    await pickVersion(dialog, "1.8.9");
    await waitFor(() => expect(nameInput(dialog).value).toBe("1.8.9"));

    fireEvent.change(nameInput(dialog), { target: { value: "My World" } });
    await pickVersion(dialog, "1.21.5");
    await waitFor(() => expect(within(dialog).getByRole("button", { name: /Version/ }).textContent).toContain("1.21.5"));
    expect(nameInput(dialog).value).toBe("My World");
  });

  it("creates the instance, refetches the list and closes", async () => {
    vi.mocked(listVersions).mockResolvedValue(VERSIONS);
    vi.mocked(createInstance).mockResolvedValue(created);
    mount();
    const dialog = await open();
    await waitFor(() => expect(nameInput(dialog).value).toBe("1.21.5"));
    fireEvent.change(nameInput(dialog), { target: { value: "My World" } });
    await act(async () => fireEvent.click(within(dialog).getByRole("button", { name: "Create" })));
    expect(createInstance).toHaveBeenCalledWith({
      name: "My World",
      mcVersion: "1.21.5",
      loader: "vanilla",
      loaderVersion: null,
      memoryMode: "auto",
      memoryMb: null,
      javaMode: "auto",
      javaPath: null,
      jvmArgs: "",
    });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    // 一覧を見ている画面（ホーム）があれば取り直される
    expect(client.getQueryState(INSTANCES_KEY)?.isInvalidated).toBe(true);
  });

  it("keeps the dialog open and shows the error when creating fails", async () => {
    vi.mocked(listVersions).mockResolvedValue(VERSIONS);
    vi.mocked(createInstance).mockRejectedValue(new IpcError({ code: "db.failed", params: {}, detail: null }));
    mount();
    const dialog = await open();
    await waitFor(() => expect(nameInput(dialog).value).toBe("1.21.5"));
    await act(async () => fireEvent.click(within(dialog).getByRole("button", { name: "Create" })));
    expect(await within(dialog).findByText("A database operation failed")).toBeTruthy();
    expect(screen.getByRole("dialog")).toBe(dialog);
  });

  it("shows the error and retries when versions cannot be loaded", async () => {
    vi.mocked(listVersions).mockRejectedValueOnce(new IpcError({ code: "net.offline", params: {}, detail: null }));
    mount();
    const dialog = await open();
    expect(await within(dialog).findByText("Can't connect to the internet")).toBeTruthy();
    vi.mocked(listVersions).mockResolvedValueOnce(VERSIONS);
    await act(async () => fireEvent.click(within(dialog).getByRole("button", { name: "Retry" })));
    await waitFor(() => expect(nameInput(dialog).value).toBe("1.21.5"));
    expect(listVersions).toHaveBeenCalledTimes(2);
  });

  it("closes with the cancel button without creating", async () => {
    vi.mocked(listVersions).mockResolvedValue(VERSIONS);
    mount();
    const dialog = await open();
    fireEvent.click(await within(dialog).findByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(createInstance).not.toHaveBeenCalled();
  });

  it("shows five loader tabs and enables the ones whose version lists arrive", async () => {
    const dialog = await openWithLoaders();
    const list = within(dialog).getByRole("tablist", { name: "Loader" });
    expect(within(list).getAllByRole("tab").map((t) => t.textContent)).toEqual([
      "Vanilla", "Fabric", "Forge", "NeoForge", "Quilt",
    ]);
    expect(isSelectedTab(tab(dialog, "Vanilla"))).toBe(true);
    await waitFor(() => expect(isDisabledTab(tab(dialog, "Forge"))).toBe(false));
    expect(isDisabledTab(tab(dialog, "NeoForge"))).toBe(false);
    expect(isDisabledTab(tab(dialog, "Quilt"))).toBe(false);
    // Vanilla ではローダーのバージョンの欄を出さない
    expect(within(dialog).queryByRole("button", { name: /Loader version/ })).toBeNull();
  });

  it("forge tab lists its loader versions with the recommended one first by default and names the instance after it", async () => {
    vi.mocked(listLoaderVersions).mockImplementation(async (loader) => (loader === "forge" ? FORGE_VERSIONS : FABRIC_VERSIONS));
    const dialog = await openWithLoaders();
    await waitFor(() => expect(isDisabledTab(tab(dialog, "Forge"))).toBe(false));
    fireEvent.click(tab(dialog, "Forge"));
    const loaderVersion = await within(dialog).findByRole("button", { name: /Loader version/ });
    await waitFor(() => expect(loaderVersion.textContent).toContain("1.21.5-54.0.0"));
    expect(listLoaderVersions).toHaveBeenCalledWith("forge", "1.21.5");
    await waitFor(() => expect(nameInput(dialog).value).toBe("Forge 1.21.5"));
  });

  it("neoforge tab narrows the Minecraft versions to the ones NeoForge supports", async () => {
    const dialog = await openWithLoaders();
    fireEvent.click(tab(dialog, "NeoForge"));
    await waitFor(() => expect(isSelectedTab(tab(dialog, "NeoForge"))).toBe(true));
    fireEvent.click(within(dialog).getByRole("button", { name: /Version/ }));
    const options = (await screen.findAllByRole("option")).map((o) => o.textContent);
    expect(options).toEqual(["1.21.5"]);
  });

  it("gives a disabled tab's reason to assistive tech through aria-describedby", async () => {
    // Forge の一覧に、ダイアログが知るどの正式版も含まれない
    vi.mocked(listLoaderGameVersions).mockImplementation(async (loader) => (loader === "forge" ? ["1.7.10"] : GAME_VERSIONS[loader] ?? []));
    const dialog = await openWithLoaders();
    await waitFor(() => expect(isDisabledTab(tab(dialog, "Forge"))).toBe(true));
    const forge = tab(dialog, "Forge");
    const id = forge.getAttribute("aria-describedby");
    expect(id).toBeTruthy();
    expect(document.getElementById(id as string)?.textContent).toBe("No supported versions");
    // 選べるタブには参照を付けない
    expect(isDisabledTab(tab(dialog, "Fabric"))).toBe(false);
    expect(tab(dialog, "Fabric").getAttribute("aria-describedby")).toBeNull();
  });

  it("fabric tab lists only versions the loader supports and defaults to the newest stable loader version", async () => {
    const dialog = await openWithLoaders();
    fireEvent.click(tab(dialog, "Fabric"));
    await waitFor(() => expect(isSelectedTab(tab(dialog, "Fabric"))).toBe(true));
    const loaderVersion = await within(dialog).findByRole("button", { name: /Loader version/ });
    await waitFor(() => expect(loaderVersion.textContent).toContain("0.16.10"));
    expect(listLoaderVersions).toHaveBeenCalledWith("fabric", "1.21.5");

    fireEvent.click(within(dialog).getByRole("button", { name: /Version/ }));
    const options = (await screen.findAllByRole("option")).map((o) => o.textContent);
    expect(options).toEqual(["1.21.5", "1.21.4"]);
  });

  it("keeps a loader tab enabled for an unsupported selected version and falls back to Vanilla when the selected loader loses support", async () => {
    const dialog = await openWithLoaders();
    await choose(dialog, /Version/, "1.8.9");
    await waitFor(() => expect(nameInput(dialog).value).toBe("1.8.9"));
    expect(isDisabledTab(tab(dialog, "Fabric"))).toBe(false);
    expect(isDisabledTab(tab(dialog, "Forge"))).toBe(false);

    await choose(dialog, /Version/, "1.21.5");
    fireEvent.click(tab(dialog, "Fabric"));
    await waitFor(() => expect(nameInput(dialog).value).toBe("Fabric 1.21.5"));
    // 一覧が取り直され、選んでいるバージョンに Fabric が対応しなくなった
    act(() => {
      client.setQueryData(["loader-game-versions", "fabric"], ["1.20.1"]);
    });
    await waitFor(() => expect(isSelectedTab(tab(dialog, "Vanilla"))).toBe(true));
    expect(nameInput(dialog).value).toBe("1.21.5");
    expect(within(dialog).queryByRole("button", { name: /Loader version/ })).toBeNull();
  });

  it("moves to the newest version a loader supports when the latest release is unsupported", async () => {
    vi.mocked(listVersions).mockResolvedValue([{ id: "26.3", releaseTime: "2026-09-01T00:00:00Z" }, ...LOADER_TEST_VERSIONS]);
    vi.mocked(listLoaderGameVersions).mockImplementation(async (loader) =>
      loader === "fabric" ? ["26.3", ...(GAME_VERSIONS.fabric ?? [])] : (GAME_VERSIONS[loader] ?? []),
    );
    mount();
    const dialog = await open();
    await waitFor(() => expect(nameInput(dialog).value).toBe("26.3"));
    await waitFor(() => expect(isDisabledTab(tab(dialog, "Forge"))).toBe(false));
    expect(isDisabledTab(tab(dialog, "NeoForge"))).toBe(false);
    expect(isDisabledTab(tab(dialog, "Fabric"))).toBe(false);

    fireEvent.click(tab(dialog, "Forge"));
    await waitFor(() => expect(isSelectedTab(tab(dialog, "Forge"))).toBe(true));
    await waitFor(() => expect(within(dialog).getByRole("button", { name: /Version/ }).textContent).toContain("1.21.5"));
    await waitFor(() => expect(nameInput(dialog).value).toBe("Forge 1.21.5"));

    fireEvent.click(tab(dialog, "Vanilla"));
    await waitFor(() => expect(isSelectedTab(tab(dialog, "Vanilla"))).toBe(true));
    expect(within(dialog).getByRole("button", { name: /Version/ }).textContent).toContain("1.21.5");
    await waitFor(() => expect(nameInput(dialog).value).toBe("1.21.5"));
  });

  it('names the instance "Fabric 1.21.4" until the user edits the name', async () => {
    const dialog = await openWithLoaders();
    await choose(dialog, /Version/, "1.21.4");
    await waitFor(() => expect(nameInput(dialog).value).toBe("1.21.4"));
    fireEvent.click(tab(dialog, "Fabric"));
    await waitFor(() => expect(nameInput(dialog).value).toBe("Fabric 1.21.4"));
    fireEvent.click(tab(dialog, "Quilt"));
    await waitFor(() => expect(nameInput(dialog).value).toBe("Quilt 1.21.4"));

    fireEvent.change(nameInput(dialog), { target: { value: "Modded" } });
    fireEvent.click(tab(dialog, "Vanilla"));
    await waitFor(() => expect(isSelectedTab(tab(dialog, "Vanilla"))).toBe(true));
    expect(nameInput(dialog).value).toBe("Modded");
  });

  it("advanced settings are collapsed by default and creating without opening them sends the defaults", async () => {
    vi.mocked(createInstance).mockResolvedValue(created);
    const dialog = await openWithLoaders();
    expect(advancedButton(dialog).getAttribute("aria-expanded")).toBe("false");
    expect(within(dialog).queryByRole("button", { name: /Memory$/ })).toBeNull();
    await act(async () => fireEvent.click(createButton(dialog)));
    expect(createInstance).toHaveBeenCalledWith({
      name: "1.21.5",
      mcVersion: "1.21.5",
      loader: "vanilla",
      loaderVersion: null,
      memoryMode: "auto",
      memoryMb: null,
      javaMode: "auto",
      javaPath: null,
      jvmArgs: "",
    });
  });

  it("sends the advanced values and the loader version when set", async () => {
    vi.mocked(createInstance).mockResolvedValue(created);
    const dialog = await openWithLoaders();
    fireEvent.click(tab(dialog, "Fabric"));
    const loaderVersion = await within(dialog).findByRole("button", { name: /Loader version/ });
    await waitFor(() => expect(loaderVersion.textContent).toContain("0.16.10"));
    await choose(dialog, /Loader version/, "0.16.9");
    fireEvent.click(advancedButton(dialog));
    expect(advancedButton(dialog).getAttribute("aria-expanded")).toBe("true");
    await choose(dialog, /Memory$/, "Manual");
    // 閉じても入力は残る
    fireEvent.click(advancedButton(dialog));
    expect(advancedButton(dialog).getAttribute("aria-expanded")).toBe("false");
    await act(async () => fireEvent.click(createButton(dialog)));
    expect(createInstance).toHaveBeenCalledWith({
      name: "Fabric 1.21.5",
      mcVersion: "1.21.5",
      loader: "fabric",
      loaderVersion: "0.16.9",
      memoryMode: "manual",
      memoryMb: 4096,
      javaMode: "auto",
      javaPath: null,
      jvmArgs: "",
    });
  });

  it("opens the advanced section and blocks Create when an advanced value is invalid", async () => {
    const dialog = await openWithLoaders();
    fireEvent.click(advancedButton(dialog));
    // 手動の Java でパスが空のときは作れない（手動のメモリは入力欄が範囲に丸めるため、画面からは範囲外にできない）
    await choose(dialog, /Java$/, "Manual");
    await waitFor(() => expect(createButton(dialog).disabled).toBe(true));
    fireEvent.click(advancedButton(dialog));
    await waitFor(() => expect(advancedButton(dialog).getAttribute("aria-expanded")).toBe("true"));
    expect(createButton(dialog).disabled).toBe(true);
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Java path" }), {
      target: { value: "C:/java/bin/javaw.exe" },
    });
    await waitFor(() => expect(createButton(dialog).disabled).toBe(false));
  });

  it("shows the failure with a retry when loader versions cannot be loaded, and Vanilla still works", async () => {
    vi.mocked(listVersions).mockResolvedValue(LOADER_TEST_VERSIONS);
    vi.mocked(listLoaderGameVersions).mockRejectedValue(new IpcError({ code: "net.offline", params: {}, detail: null }));
    // 作成は終わらせず、ダイアログを開いたままにして再試行まで確かめる
    vi.mocked(createInstance).mockReturnValue(new Promise(() => {}));
    mount();
    const dialog = await open();
    expect(await within(dialog).findByText("Can't connect to the internet")).toBeTruthy();
    // 読み込み中のままにはしない
    expect(tab(dialog, "Fabric").querySelector("[title]")?.getAttribute("title")).not.toBe("Loading loader versions");
    expect(isDisabledTab(tab(dialog, "Fabric"))).toBe(true);
    // 失敗したままでも Vanilla は作れる
    await waitFor(() => expect(createButton(dialog).disabled).toBe(false));
    await act(async () => fireEvent.click(createButton(dialog)));
    expect(createInstance).toHaveBeenCalledWith(expect.objectContaining({ loader: "vanilla", mcVersion: "1.21.5" }));
    // 再試行で取り直しに成功すれば、タブが選べるようになり、失敗の表示は消える
    vi.mocked(listLoaderGameVersions).mockImplementation(async (loader) => GAME_VERSIONS[loader] ?? []);
    await act(async () => fireEvent.click(within(dialog).getByRole("button", { name: "Retry loading loaders" })));
    await waitFor(() => expect(isDisabledTab(tab(dialog, "Fabric"))).toBe(false));
    expect(isDisabledTab(tab(dialog, "Quilt"))).toBe(false);
    expect(within(dialog).queryByText("Can't connect to the internet")).toBeNull();
  });

  it("explains an empty loader version list instead of loading forever, and keeps Create disabled", async () => {
    vi.mocked(listLoaderVersions).mockResolvedValue([]);
    const dialog = await openWithLoaders();
    fireEvent.click(tab(dialog, "Fabric"));
    await waitFor(() => expect(isSelectedTab(tab(dialog, "Fabric"))).toBe(true));
    expect(
      await within(dialog).findByText("No loader versions are available for this Minecraft version"),
    ).toBeTruthy();
    expect(within(dialog).queryByText("Loading loader versions")).toBeNull();
    expect(within(dialog).queryByRole("button", { name: /Loader version/ })).toBeNull();
    expect(createButton(dialog).disabled).toBe(true);
  });

  it("shows the failure of the loader version list with a retry that recovers", async () => {
    vi.mocked(listLoaderVersions).mockRejectedValueOnce(new IpcError({ code: "net.offline", params: {}, detail: null }));
    const dialog = await openWithLoaders();
    fireEvent.click(tab(dialog, "Fabric"));
    expect(await within(dialog).findByText("Can't connect to the internet")).toBeTruthy();
    expect(createButton(dialog).disabled).toBe(true);
    vi.mocked(listLoaderVersions).mockResolvedValue(FABRIC_VERSIONS);
    await act(async () => fireEvent.click(within(dialog).getByRole("button", { name: "Retry" })));
    const loaderVersion = await within(dialog).findByRole("button", { name: /Loader version/ });
    await waitFor(() => expect(loaderVersion.textContent).toContain("0.16.10"));
    expect(within(dialog).queryByText("Can't connect to the internet")).toBeNull();
    expect(listLoaderVersions).toHaveBeenCalledTimes(2);
    await waitFor(() => expect(createButton(dialog).disabled).toBe(false));
  });

  it("retries the loader lists from the failure notice", async () => {
    vi.mocked(listVersions).mockResolvedValue(LOADER_TEST_VERSIONS);
    vi.mocked(listLoaderGameVersions).mockRejectedValue(new IpcError({ code: "net.offline", params: {}, detail: null }));
    mount();
    const dialog = await open();
    expect(await within(dialog).findByText("Can't connect to the internet")).toBeTruthy();
    vi.mocked(listLoaderGameVersions).mockImplementation(async (loader) => GAME_VERSIONS[loader] ?? []);
    await act(async () => fireEvent.click(within(dialog).getByRole("button", { name: "Retry loading loaders" })));
    await waitFor(() => expect(isDisabledTab(tab(dialog, "Fabric"))).toBe(false));
    expect(isDisabledTab(tab(dialog, "Quilt"))).toBe(false);
    expect(within(dialog).queryByText("Can't connect to the internet")).toBeNull();
  });
});

const shared = (id: string, name: string): SyncProfile => ({ id, name, revision: 1, categories: ["keys"], members: [] });
const sharedCheckbox = (dialog: HTMLElement) => within(dialog).queryByRole("checkbox", { name: "Use shared settings" });

describe("CreateInstanceDialog shared settings", () => {
  it("hides the checkbox when there are no shared settings", async () => {
    vi.mocked(listVersions).mockResolvedValue(VERSIONS);
    mount();
    const dialog = await open();
    await waitFor(() => expect(nameInput(dialog).value).toBe("1.21.5"));
    await waitFor(() => expect(listSyncProfiles).toHaveBeenCalled());
    expect(sharedCheckbox(dialog)).toBeNull();
  });

  it("shows the checkbox with no picker for a single shared setting, and joins after creating", async () => {
    vi.mocked(listVersions).mockResolvedValue(VERSIONS);
    vi.mocked(listSyncProfiles).mockResolvedValue([shared("p1", "Main")]);
    vi.mocked(createInstance).mockResolvedValue(created);
    vi.mocked(joinSyncProfile).mockResolvedValue({ adopted: 0 } as never);
    mount();
    const dialog = await open();
    const box = await within(dialog).findByRole("checkbox", { name: "Use shared settings" });
    expect((box as HTMLInputElement).checked).toBe(false);
    expect(within(dialog).queryByRole("button", { name: /Shared settings to join/ })).toBeNull();
    fireEvent.click(box);
    await waitFor(() => expect(nameInput(dialog).value).toBe("1.21.5"));
    await act(async () => fireEvent.click(createButton(dialog)));
    await waitFor(() => expect(joinSyncProfile).toHaveBeenCalledWith("i1", "p1", "keepProfile"));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("does not join when the checkbox is left unchecked", async () => {
    vi.mocked(listVersions).mockResolvedValue(VERSIONS);
    vi.mocked(listSyncProfiles).mockResolvedValue([shared("p1", "Main")]);
    vi.mocked(createInstance).mockResolvedValue(created);
    mount();
    const dialog = await open();
    await within(dialog).findByRole("checkbox", { name: "Use shared settings" });
    await waitFor(() => expect(nameInput(dialog).value).toBe("1.21.5"));
    await act(async () => fireEvent.click(createButton(dialog)));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(joinSyncProfile).not.toHaveBeenCalled();
  });

  it("lets the user pick one of several shared settings", async () => {
    vi.mocked(listVersions).mockResolvedValue(VERSIONS);
    vi.mocked(listSyncProfiles).mockResolvedValue([shared("p1", "Main"), shared("p2", "Speedrun")]);
    vi.mocked(createInstance).mockResolvedValue(created);
    vi.mocked(joinSyncProfile).mockResolvedValue({ adopted: 0 } as never);
    mount();
    const dialog = await open();
    fireEvent.click(await within(dialog).findByRole("checkbox", { name: "Use shared settings" }));
    await choose(dialog, /Shared settings to join/, "Speedrun");
    await waitFor(() => expect(nameInput(dialog).value).toBe("1.21.5"));
    await act(async () => fireEvent.click(createButton(dialog)));
    await waitFor(() => expect(joinSyncProfile).toHaveBeenCalledWith("i1", "p2", "keepProfile"));
  });

  it("keeps the creation successful and shows a toast when joining fails", async () => {
    vi.mocked(listVersions).mockResolvedValue(VERSIONS);
    vi.mocked(listSyncProfiles).mockResolvedValue([shared("p1", "Main")]);
    vi.mocked(createInstance).mockResolvedValue(created);
    vi.mocked(joinSyncProfile).mockRejectedValue(new IpcError({ code: "db.failed", params: {}, detail: null }));
    mount();
    const dialog = await open();
    fireEvent.click(await within(dialog).findByRole("checkbox", { name: "Use shared settings" }));
    await waitFor(() => expect(nameInput(dialog).value).toBe("1.21.5"));
    await act(async () => fireEvent.click(createButton(dialog)));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(await screen.findByText(/could not join the shared settings/)).toBeTruthy();
    expect(client.getQueryState(INSTANCES_KEY)?.isInvalidated).toBe(true);
  });
});
