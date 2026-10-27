import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes, useLocation } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { deleteInstance, duplicateInstance, launchInstance, listInstances, openInstanceFolder, listRunning } from "../api/commands";
import { IpcError } from "../api/ipc";
import type { Instance } from "../bindings/Instance";
import type { TaskSnapshot } from "../bindings/TaskSnapshot";
import { ToastRegion } from "../components/toast/ToastRegion";
import { clearToasts } from "../components/toast/useToast";
import { LAST_EXIT_KEY, RUNNING_KEY } from "../features/play/useGameSync";
import { TASKS_KEY } from "../features/tasks/useTasks";
import i18n from "../i18n";
import { META_MIN_WIDTH } from "../features/instances/visibleMeta";
import Instances from "./Instances";

vi.mock("../api/commands", () => ({
  listInstances: vi.fn(),
  listRunning: vi.fn(),
  listVersions: vi.fn(),
  createInstance: vi.fn(),
  deleteInstance: vi.fn(),
  duplicateInstance: vi.fn(),
  launchInstance: vi.fn(),
  openInstanceFolder: vi.fn(),
  cancelTask: vi.fn(),
  listAccounts: vi.fn(),
  getSkin: vi.fn(),
  beginLogin: vi.fn(),
}));
vi.mock("../api/events", () => ({
  onLoginProgress: vi.fn().mockResolvedValue(() => {}),
  onAccountsChanged: vi.fn().mockResolvedValue(() => {}),
}));

const instance = (id: string, name: string, patch: Partial<Instance> = {}): Instance => ({
  id, name, dirName: name, mcVersion: "1.21.5", state: "ready",
  createdAt: 0, lastPlayedAt: null, playTimeMs: 0,
  memoryMode: "auto", memoryMb: null, javaMode: "auto", javaPath: null, jvmArgs: "",
  loader: "vanilla", loaderVersion: null,
  ...patch,
});

const duplicateTask = (patch: Partial<TaskSnapshot> = {}): TaskSnapshot => ({
  id: "d1", kind: "duplicate", instanceId: "i1", state: "running", phase: null, progress: { current: 40, total: 100 },
  revision: 1, error: null, createdAt: 0, updatedAt: 0, ...patch,
});

let client: QueryClient;

/** 詳細の画面の代わり。開いたパスと、頼まれたタブを出す */
function DetailProbe() {
  const location = useLocation();
  const tab = (location.state as { tab?: string } | null)?.tab ?? "default";
  return <p>{`detail ${location.pathname} tab=${tab}`}</p>;
}

function mount(list: Instance[], tasks: TaskSnapshot[] = []) {
  vi.mocked(listInstances).mockResolvedValue(list);
  vi.mocked(listRunning).mockResolvedValue([]);
  client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  client.setQueryData(TASKS_KEY, Object.fromEntries(tasks.map((t) => [t.id, t])));
  client.setQueryData(RUNNING_KEY, []);
  client.setQueryData(LAST_EXIT_KEY, {});
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <Routes>
          <Route path="/" element={<Instances />} />
          <Route path="/instances/:id" element={<DetailProbe />} />
        </Routes>
      </MemoryRouter>
      <ToastRegion />
    </QueryClientProvider>,
  );
}

const row = (name: string) => screen.findByRole("listitem", { name });
const toasts = () => screen.getByRole("region", { name: "Notifications" });

const launchTask = (patch: Partial<TaskSnapshot> = {}): TaskSnapshot => ({
  id: "l1", kind: "launch", instanceId: "i1", state: "running", phase: null, progress: { current: 10, total: 100 },
  revision: 1, error: null, createdAt: 0, updatedAt: 0, ...patch,
});

async function press(el: HTMLElement) {
  await act(async () => {
    fireEvent.click(el);
    await Promise.resolve();
  });
}

async function openMenu(name: string) {
  await press(within(await row(name)).getByRole("button", { name: "More" }));
  return screen.findByRole("menu");
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
});

afterEach(() => {
  act(() => clearToasts());
  cleanup();
  vi.resetAllMocks();
});

describe("Instances", () => {
  it("lists_instances_in_backend_order", async () => {
    mount([
      instance("i2", "Survival", { lastPlayedAt: 1_700_000_000_000, playTimeMs: 3 * 3_600_000 + 12 * 60_000 }),
      instance("i1", "Creative", { mcVersion: "1.8.9", state: "broken" }),
      instance("i3", "Modded", { state: "installing" }),
    ]);
    await row("Survival");
    const names = screen.getAllByRole("listitem").map((li) => li.getAttribute("aria-label") ?? li.textContent);
    expect(screen.getAllByRole("listitem")).toHaveLength(3);
    expect(names[0]).toContain("Survival");
    expect(names[1]).toContain("Creative");
    expect(names[2]).toContain("Modded");

    const survival = await row("Survival");
    expect(within(survival).getByText("1.21.5")).toBeTruthy();
    // 使える状態の印は出さない
    expect(within(survival).queryByText("Ready")).toBeNull();
    expect(within(survival).getByText("3h 12m")).toBeTruthy();
    const played = new Intl.DateTimeFormat("en", { dateStyle: "medium", timeStyle: "short" }).format(1_700_000_000_000);
    expect(within(survival).getByText(played)).toBeTruthy();
    // 名前、「コンテンツ」、「設定」のどれからも、そのインスタンスの詳細を開ける
    expect(within(survival).getByRole("link", { name: "Survival" }).getAttribute("href")).toBe("/instances/i2");
    expect(within(survival).getByRole("link", { name: "Content" }).getAttribute("href")).toBe("/instances/i2");
    expect(within(survival).getByRole("link", { name: "Settings" }).getAttribute("href")).toBe("/instances/i2");

    const creative = await row("Creative");
    expect(within(creative).getByText("1.8.9")).toBeTruthy();
    expect(within(creative).getByText("Broken")).toBeTruthy();
    expect(within(creative).getByText("Not played yet")).toBeTruthy();
    expect(within(await row("Modded")).getByText("Preparing")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Create instance" })).toBeTruthy();
  });

  it("the name and the Content link open the instance's content, the Settings link its settings", async () => {
    const cases = [
      ["Survival", "detail /instances/i2 tab=default"],
      ["Content", "detail /instances/i2 tab=default"],
      ["Settings", "detail /instances/i2 tab=settings"],
    ] as const;
    for (const [name, expected] of cases) {
      mount([instance("i2", "Survival")]);
      await press(within(await row("Survival")).getByRole("link", { name }));
      expect(await screen.findByText(expected)).toBeTruthy();
      cleanup();
    }
  });

  it("the name link is not nested in another control", async () => {
    mount([instance("i2", "Survival")]);
    const link = within(await row("Survival")).getByRole("link", { name: "Survival" });
    expect(link.parentElement?.closest("a, button")).toBeNull();
    expect(link.querySelector("a, button")).toBeNull();
  });

  it("shows_empty_state_with_create_button", async () => {
    mount([]);
    expect(await screen.findByText("No instances yet. Pick a version to create one.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Create instance" })).toBeTruthy();
    // トーストの並び（空）は数えない
    expect(screen.queryAllByRole("list").filter((l) => !toasts().contains(l))).toHaveLength(0);
  });

  it("delete_asks_for_confirmation_and_cancel_is_default", async () => {
    mount([instance("i1", "World")]);
    const menu = await openMenu("World");
    await press(within(menu).getByRole("menuitem", { name: "Delete" }));
    const dialog = await screen.findByRole("alertdialog", { name: "Delete World?" });
    expect(within(dialog).getByText("All data in this instance, including worlds, will be deleted. This can't be undone.")).toBeTruthy();
    const cancel = within(dialog).getByRole("button", { name: "Cancel" });
    await waitFor(() => expect(document.activeElement).toBe(cancel));
    await press(cancel);
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(deleteInstance).not.toHaveBeenCalled();
  });

  it("confirming_delete_calls_command_with_the_id", async () => {
    vi.mocked(deleteInstance).mockResolvedValue(null);
    mount([instance("i0", "Other"), instance("i1", "World")]);
    const menu = await openMenu("World");
    await press(within(menu).getByRole("menuitem", { name: "Delete" }));
    const dialog = await screen.findByRole("alertdialog");
    vi.mocked(listInstances).mockResolvedValue([instance("i0", "Other")]);
    await press(within(dialog).getByRole("button", { name: "Delete" }));
    expect(deleteInstance).toHaveBeenCalledWith("i1");
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    await waitFor(() => expect(screen.queryByRole("listitem", { name: "World" })).toBeNull());
  });

  it("duplicate_row_shows_progress_and_disables_actions", async () => {
    mount([instance("i1", "World"), instance("i2", "Other")], [duplicateTask()]);
    const world = await row("World");
    expect(within(world).getByText("Duplicating 40%")).toBeTruthy();
    const play = await within(world).findByRole("button", { name: "Play" });
    expect((play as HTMLButtonElement).disabled).toBe(true);
    expect((within(world).getByRole("button", { name: "More" }) as HTMLButtonElement).disabled).toBe(true);

    // 他の行はそのまま使える
    const other = await row("Other");
    expect(within(other).queryByText(/Duplicating/)).toBeNull();
    expect((within(other).getByRole("button", { name: "More" }) as HTMLButtonElement).disabled).toBe(false);
    expect(((await within(other).findByRole("button", { name: "Play" })) as HTMLButtonElement).disabled).toBe(false);
  });

  it("duplicate_from_the_menu_calls_command_with_the_id", async () => {
    vi.mocked(duplicateInstance).mockResolvedValue("d1");
    mount([instance("i1", "World")]);
    const menu = await openMenu("World");
    await press(within(menu).getByRole("menuitem", { name: "Duplicate" }));
    expect(duplicateInstance).toHaveBeenCalledWith("i1");
  });

  it("duplicate_refused_while_sending_never_shows_progress", async () => {
    let refuse: (e: unknown) => void = () => {};
    vi.mocked(duplicateInstance).mockReturnValue(new Promise((_, reject) => (refuse = reject)));
    mount([instance("i1", "World")]);
    const menu = await openMenu("World");
    await press(within(menu).getByRole("menuitem", { name: "Duplicate" }));
    const world = await row("World");
    // 応答待ちの間は操作を止めるが、複製中の表示は出さない
    await waitFor(() => expect((within(world).getByRole("button", { name: "More" }) as HTMLButtonElement).disabled).toBe(true));
    expect(within(world).queryByText(/Duplicating/)).toBeNull();
    await act(async () => refuse(new IpcError({ code: "instance.busy", params: {}, detail: null })));
    await within(toasts()).findByRole("alert");
    expect(within(world).queryByText(/Duplicating/)).toBeNull();
    expect((within(world).getByRole("button", { name: "More" }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("refetches_the_list_when_a_duplicate_task_starts_and_finishes", async () => {
    mount([instance("i1", "World")]);
    await row("World");
    await waitFor(() => expect(listInstances).toHaveBeenCalledTimes(1));

    vi.mocked(listInstances).mockResolvedValue([instance("i1", "World"), instance("i9", "World (2)")]);
    act(() => client.setQueryData(TASKS_KEY, { d1: duplicateTask() }));
    await waitFor(() => expect(listInstances).toHaveBeenCalledTimes(2));

    act(() => client.setQueryData(TASKS_KEY, { d1: duplicateTask({ state: "succeeded", revision: 2 }) }));
    await waitFor(() => expect(listInstances).toHaveBeenCalledTimes(3));
    expect(await row("World (2)")).toBeTruthy();
    expect(within(await row("World")).queryByText(/Duplicating/)).toBeNull();
  });

  it("delete_failure_is_a_toast", async () => {
    vi.mocked(deleteInstance).mockRejectedValue(new IpcError({ code: "instance.busy", params: {}, detail: null }));
    mount([instance("i1", "World")]);
    const menu = await openMenu("World");
    await press(within(menu).getByRole("menuitem", { name: "Delete" }));
    await press(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Delete" }));
    const alert = await within(toasts()).findByRole("alert");
    expect(within(alert).getByText("This instance is running or being installed. Try again after it finishes.")).toBeTruthy();
    // 一覧の上には出さない
    expect(screen.getAllByRole("alert")).toHaveLength(1);
  });

  it("row_menu_opens_the_game_folder", async () => {
    vi.mocked(openInstanceFolder).mockResolvedValue(null);
    mount([instance("i1", "World")]);
    const menu = await openMenu("World");
    await press(within(menu).getByRole("menuitem", { name: "Open folder" }));
    expect(openInstanceFolder).toHaveBeenCalledWith("i1", "game");
  });

  it("duplicate_failure_is_a_toast", async () => {
    vi.mocked(duplicateInstance).mockRejectedValue(new IpcError({ code: "instance.busy", params: {}, detail: null }));
    mount([instance("i1", "World")]);
    const menu = await openMenu("World");
    await press(within(menu).getByRole("menuitem", { name: "Duplicate" }));
    const alert = await within(toasts()).findByRole("alert");
    expect(within(alert).getByText("This instance is running or being installed. Try again after it finishes.")).toBeTruthy();
  });

  it("launch_failure_is_a_toast", async () => {
    vi.mocked(launchInstance).mockRejectedValue(new IpcError({ code: "app.not_ready", params: {}, detail: null }));
    mount([instance("i1", "World")]);
    await press(within(await row("World")).getByRole("button", { name: "Play" }));
    const alert = await within(toasts()).findByRole("alert");
    expect(within(alert).getByText("Startup has not finished yet")).toBeTruthy();
    expect(within(await row("World")).queryByText("Startup has not finished yet")).toBeNull();
  });

  it("launch_task_failure_is_a_toast_once", async () => {
    mount([instance("i1", "World")], [launchTask({ state: "running" })]);
    await row("World");
    expect(within(toasts()).queryByRole("alert")).toBeNull();
    act(() =>
      client.setQueryData(TASKS_KEY, {
        l1: launchTask({ state: "failed", revision: 2, updatedAt: 1, error: { code: "app.not_ready", params: {}, detail: null } }),
      }),
    );
    const alert = await within(toasts()).findByRole("alert");
    expect(within(alert).getByText("Startup has not finished yet")).toBeTruthy();
    expect(within(toasts()).getAllByRole("alert")).toHaveLength(1);
  });

  it("an_existing_failure_is_not_toasted_on_open", async () => {
    mount([instance("i1", "World")], [
      launchTask({ state: "failed", updatedAt: 1, error: { code: "app.not_ready", params: {}, detail: null } }),
    ]);
    await row("World");
    await act(async () => { await Promise.resolve(); });
    expect(within(toasts()).queryByRole("alert")).toBeNull();
  });

  it("delete_io_failure_says_the_folder_may_be_partly_removed", async () => {
    vi.mocked(deleteInstance).mockRejectedValue(
      new IpcError({ code: "io.failed", params: { path: "C:/x" }, detail: null }),
    );
    mount([instance("i1", "World")]);
    const menu = await openMenu("World");
    await press(within(menu).getByRole("menuitem", { name: "Delete" }));
    await press(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Delete" }));
    const alert = await within(toasts()).findByRole("alert");
    expect(alert.textContent).toContain("Could not read or write a file: C:/x");
    expect(alert.textContent).toContain("The folder may have been partly removed. Try deleting it again.");
  });

  describe("meta line", () => {
    const played = 1_700_000_000_000;
    const full = () => instance("i1", "Survival", { lastPlayedAt: played, playTimeMs: 3 * 3_600_000 + 12 * 60_000 });

    it("keeps each label and its value together on one line", async () => {
      mount([full()]);
      const li = await row("Survival");
      for (const label of ["Last played", "Play time"]) {
        const pair = within(li).getByText(label).parentElement as HTMLElement;
        expect(pair.className).toContain("whitespace-nowrap");
      }
      const meta = li.querySelector("[data-meta]") as HTMLElement;
      expect(meta.className).toContain("whitespace-nowrap");
      expect(meta.className).not.toContain("flex-wrap");
    });

    it("puts everything, hidden or not, in the row tooltip", async () => {
      mount([full()]);
      const meta = (await row("Survival")).querySelector("[data-meta]") as HTMLElement;
      const played = new Intl.DateTimeFormat("en", { dateStyle: "medium", timeStyle: "short" }).format(1_700_000_000_000);
      expect(meta.getAttribute("title")).toBe(`1.21.5 · Last played ${played} · Play time 3h 12m`);
    });

    it("also gives the full meta text to screen readers, not only the tooltip", async () => {
      mount([full()]);
      const li = await row("Survival");
      const played = new Intl.DateTimeFormat("en", { dateStyle: "medium", timeStyle: "short" }).format(1_700_000_000_000);
      const full_text = `1.21.5 · Last played ${played} · Play time 3h 12m`;
      const sr = Array.from(li.querySelectorAll(".sr-only")).find((el) => el.textContent === full_text);
      expect(sr).toBeTruthy();
      expect((li.querySelector("[data-meta]") as HTMLElement).getAttribute("aria-hidden")).toBe("true");
    });

    it("hides play time, then last played, then version as the row narrows, keeping the tooltip", async () => {
      const widths: number[] = [];
      let callback: ResizeObserverCallback = () => {};
      vi.stubGlobal(
        "ResizeObserver",
        class {
          constructor(cb: ResizeObserverCallback) {
            callback = cb;
          }
          observe() {}
          disconnect() {}
          unobserve() {}
        },
      );
      const spy = vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(() => widths.at(-1) ?? 0);
      try {
        mount([full()]);
        const li = await row("Survival");
        const resize = async (w: number) => {
          widths.push(w);
          await act(async () => callback([], {} as ResizeObserver));
        };
        const text = () => (li.querySelector("[data-meta]") as HTMLElement).textContent ?? "";
        await resize(10_000);
        expect(text()).toContain("Play time");
        await resize(META_MIN_WIDTH.playTime - 1);
        expect(text()).not.toContain("Play time");
        expect(text()).toContain("Last played");
        await resize(META_MIN_WIDTH.lastPlayed - 1);
        expect(text()).not.toContain("Last played");
        expect(text()).toContain("1.21.5");
        await resize(META_MIN_WIDTH.version - 1);
        expect(text()).toBe("");
        expect((li.querySelector("[data-meta]") as HTMLElement).getAttribute("title")).toContain("Play time 3h 12m");
        expect(within(li).getByRole("link", { name: "Survival" })).toBeTruthy();
      } finally {
        spy.mockRestore();
        vi.unstubAllGlobals();
      }
    });

    it("shows a state label beside the name only when the instance is not usable", async () => {
      mount([
        instance("i1", "Fine"),
        instance("i2", "Hurt", { state: "broken" }),
        instance("i3", "Fresh", { state: "installing" }),
      ]);
      const fine = await row("Fine");
      expect(fine.querySelector("[data-state-label]")).toBeNull();
      for (const [name, label] of [["Hurt", "Broken"], ["Fresh", "Preparing"]] as const) {
        const li = await row(name);
        const badge = li.querySelector("[data-state-label]") as HTMLElement;
        expect(badge.textContent).toBe(label);
        expect(badge.className).toContain("shrink-0");
        // 名前と同じ行、名前のリンクの隣
        expect(badge.parentElement).toBe(within(li).getByRole("link", { name }).parentElement);
        // 重複して読み上げない（全文の方に含まれる）
        expect(li.querySelector("[data-meta]")?.textContent).not.toContain(label);
        expect(Array.from(li.querySelectorAll(".sr-only")).some((el) => el.textContent?.includes(label))).toBe(true);
      }
    });

    it("caps the name link to its column so it truncates instead of covering the buttons", async () => {
      mount([full()]);
      const link = within(await row("Survival")).getByRole("link", { name: "Survival" });
      expect(link.className).toContain("max-w-full");
      expect(link.className).toContain("truncate");
    });
  });
});
