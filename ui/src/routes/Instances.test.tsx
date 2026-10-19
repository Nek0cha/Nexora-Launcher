import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { deleteInstance, duplicateInstance, listInstances, listRunning } from "../api/commands";
import { IpcError } from "../api/ipc";
import type { Instance } from "../bindings/Instance";
import type { TaskSnapshot } from "../bindings/TaskSnapshot";
import { LAST_EXIT_KEY, RUNNING_KEY } from "../features/play/useGameSync";
import { TASKS_KEY } from "../features/tasks/useTasks";
import i18n from "../i18n";
import Instances from "./Instances";

vi.mock("../api/commands", () => ({
  listInstances: vi.fn(),
  listRunning: vi.fn(),
  listVersions: vi.fn(),
  createInstance: vi.fn(),
  deleteInstance: vi.fn(),
  duplicateInstance: vi.fn(),
  launchInstance: vi.fn(),
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
  ...patch,
});

const duplicateTask = (patch: Partial<TaskSnapshot> = {}): TaskSnapshot => ({
  id: "d1", kind: "duplicate", instanceId: "i1", state: "running", progress: { current: 40, total: 100 },
  revision: 1, error: null, createdAt: 0, updatedAt: 0, ...patch,
});

let client: QueryClient;

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
        <Instances />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

const row = (name: string) => screen.findByRole("listitem", { name });

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
    expect(within(survival).getByText("Ready")).toBeTruthy();
    expect(within(survival).getByText("3 hr 12 min")).toBeTruthy();
    const played = new Intl.DateTimeFormat("en", { dateStyle: "medium", timeStyle: "short" }).format(1_700_000_000_000);
    expect(within(survival).getByText(played)).toBeTruthy();
    expect(within(survival).getByRole("link", { name: "Details" }).getAttribute("href")).toBe("/instances/i2");

    const creative = await row("Creative");
    expect(within(creative).getByText("1.8.9")).toBeTruthy();
    expect(within(creative).getByText("Broken")).toBeTruthy();
    expect(within(creative).getByText("Not played yet")).toBeTruthy();
    expect(within(await row("Modded")).getByText("Preparing")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Create instance" })).toBeTruthy();
  });

  it("shows_empty_state_with_create_button", async () => {
    mount([]);
    expect(await screen.findByText("No instances yet. Pick a version to create one.")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Create instance" })).toBeTruthy();
    expect(screen.queryByRole("list")).toBeNull();
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

  it("failure_is_shown_with_failure_notice", async () => {
    vi.mocked(deleteInstance).mockRejectedValue(new IpcError({ code: "instance.busy", params: {}, detail: null }));
    mount([instance("i1", "World")]);
    const menu = await openMenu("World");
    await press(within(menu).getByRole("menuitem", { name: "Delete" }));
    await press(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Delete" }));
    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText("This instance can't be deleted while it is running or installing")).toBeTruthy();
  });

  it("delete_io_failure_says_the_folder_may_be_partly_removed", async () => {
    vi.mocked(deleteInstance).mockRejectedValue(
      new IpcError({ code: "io.failed", params: { path: "C:/x" }, detail: null }),
    );
    mount([instance("i1", "World")]);
    const menu = await openMenu("World");
    await press(within(menu).getByRole("menuitem", { name: "Delete" }));
    await press(within(await screen.findByRole("alertdialog")).getByRole("button", { name: "Delete" }));
    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText("Could not read or write a file: C:/x")).toBeTruthy();
    expect(
      within(alert).getByText("The folder may have been partly removed. Try deleting it again."),
    ).toBeTruthy();
  });
});
