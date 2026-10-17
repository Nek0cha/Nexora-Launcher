import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { launchInstance, listAccounts, listInstances, listRunning } from "../api/commands";
import { IpcError } from "../api/ipc";
import type { Instance } from "../bindings/Instance";
import type { RunningSession } from "../bindings/RunningSession";
import { INSTANCES_KEY } from "../features/instances/useInstances";
import { RUNNING_KEY } from "../features/play/useGameSync";
import { TASKS_KEY } from "../features/tasks/useTasks";
import i18n from "../i18n";
import { CommandPalette } from "./CommandPalette";

vi.mock("../api/commands", () => ({
  listInstances: vi.fn(),
  listRunning: vi.fn(),
  launchInstance: vi.fn(),
  setSetting: vi.fn(),
  listAccounts: vi.fn(),
  getSkin: vi.fn(),
  beginLogin: vi.fn(),
}));
vi.mock("../api/events", () => ({
  onLoginProgress: vi.fn().mockResolvedValue(() => {}),
  onAccountsChanged: vi.fn().mockResolvedValue(() => {}),
}));

const world: Instance = {
  id: "i1", name: "World", dirName: "World", mcVersion: "1.21.5", state: "ready",
  createdAt: 0, lastPlayedAt: null, playTimeMs: 0,
};

async function mount(instances: Instance[], sessions: RunningSession[] = []) {
  vi.mocked(listInstances).mockResolvedValue(instances);
  vi.mocked(listRunning).mockResolvedValue(sessions);
  const client = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
  client.setQueryData(INSTANCES_KEY, instances);
  client.setQueryData(RUNNING_KEY, sessions);
  client.setQueryData(TASKS_KEY, {});
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <CommandPalette />
      </MemoryRouter>
    </QueryClientProvider>,
  );
  await act(async () => {
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
  });
  await screen.findByRole("dialog");
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("CommandPalette", () => {
  it("launches the home instance", async () => {
    vi.mocked(launchInstance).mockResolvedValue("t1");
    await mount([world]);
    await act(async () => fireEvent.click(await screen.findByRole("menuitem", { name: "Launch World" })));
    expect(launchInstance).toHaveBeenCalledWith("i1", false);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("has no launch item without an instance", async () => {
    await mount([]);
    expect(screen.getAllByRole("menuitem").length).toBeGreaterThan(0);
    expect(screen.queryByRole("menuitem", { name: /^Launch/ })).toBeNull();
  });

  it("has no launch item while the instance is already running", async () => {
    await mount([world], [{ instanceId: "i1", pid: 1, launchedAt: 0 }]);
    expect(screen.queryByRole("menuitem", { name: "Launch World" })).toBeNull();
  });

  it("command palette uses the same dialog", async () => {
    vi.mocked(listAccounts).mockResolvedValue({ accounts: [], devOffline: null });
    vi.mocked(launchInstance).mockRejectedValueOnce(new IpcError({ code: "auth.no_account", params: {}, detail: null }));
    await mount([world]);
    await act(async () => fireEvent.click(await screen.findByRole("menuitem", { name: "Launch World" })));
    expect(await screen.findByRole("heading", { name: "Sign in with a Microsoft account" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Sign in with Microsoft and play" })).toBeTruthy();
  });
});
