import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { createMemoryRouter, RouterProvider } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { listInstances, listRunning, openInstanceFolder, stopInstance } from "../../api/commands";
import type { Instance } from "../../bindings/Instance";
import type { RunningSession } from "../../bindings/RunningSession";
import { RUNNING_KEY } from "../../features/play/useGameSync";
import { STOP_KEY } from "../../features/play/useStopFlow";
import { INSTANCES_KEY } from "../../features/instances/useInstances";
import i18n from "../../i18n";
import { RunningPill } from "./RunningPill";

vi.mock("../../api/commands", () => ({
  listInstances: vi.fn(),
  listRunning: vi.fn(),
  stopInstance: vi.fn(),
  openInstanceFolder: vi.fn(),
}));

const instance = (id: string, name: string): Instance => ({
  id, name, dirName: name, mcVersion: "1.21.5", state: "ready",
  createdAt: 0, lastPlayedAt: null, playTimeMs: 0,
  memoryMode: "auto", memoryMb: null, javaMode: "auto", javaPath: null, jvmArgs: "",
  loader: "vanilla", loaderVersion: null,
});

const instances = [instance("a", "Alpha"), instance("b", "Beta"), instance("c", "Gamma"), instance("d", "Delta")];
const session = (instanceId: string): RunningSession => ({ instanceId, pid: 1, launchedAt: 0 });

function mount(running: string[], stop: Record<string, "waiting" | "forceReady"> = {}) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const sessions = running.map(session);
  client.setQueryData(RUNNING_KEY, sessions);
  client.setQueryData(INSTANCES_KEY, instances);
  client.setQueryData(STOP_KEY, { states: stop, confirm: [] });
  vi.mocked(listRunning).mockResolvedValue(sessions);
  vi.mocked(listInstances).mockResolvedValue(instances);
  const router = createMemoryRouter(
    [
      { path: "/", element: <RunningPill /> },
      { path: "/instances/:id", element: <span>detail page</span> },
    ],
    { initialEntries: ["/"] },
  );
  render(
    <QueryClientProvider client={client}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return { router, client };
}

beforeEach(async () => {
  await i18n.changeLanguage("en");
  vi.mocked(stopInstance).mockResolvedValue(null);
  vi.mocked(openInstanceFolder).mockResolvedValue(null);
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("RunningPill", () => {
  it("hidden_with_no_games", () => {
    mount([]);
    expect(screen.queryByTestId("running-pill")).toBeNull();
  });

  it("single_game_shows_name_stop_and_log", async () => {
    const { router } = mount(["a"]);
    const pill = screen.getByTestId("running-pill");
    expect(within(pill).getByText("Alpha")).toBeTruthy();
    await act(async () => {
      fireEvent.click(within(pill).getByRole("button", { name: "Stop Alpha" }));
    });
    expect(stopInstance).toHaveBeenCalledWith("a", false);
    await act(async () => {
      fireEvent.click(within(pill).getByRole("button", { name: "Open logs for Alpha" }));
    });
    // フォルダではなく、詳細のログのタブへ移る
    expect(openInstanceFolder).not.toHaveBeenCalled();
    expect(router.state.location.pathname).toBe("/instances/a");
    expect(router.state.location.state).toEqual({ tab: "logs" });
  });

  it("two_games_show_two_dots", () => {
    mount(["a", "b"]);
    expect(screen.getAllByTestId("running-dot")).toHaveLength(2);
    expect(screen.getByRole("button", { name: "2 running games" })).toBeTruthy();
    expect(screen.queryByText("Alpha")).toBeNull();
  });

  it("four_games_show_three_dots_and_plus_one", () => {
    mount(["a", "b", "c", "d"]);
    expect(screen.getAllByTestId("running-dot")).toHaveLength(3);
    expect(screen.getByText("+1")).toBeTruthy();
  });

  it("waiting_instance_stays_listed_with_waiting_label", () => {
    // 一覧から消えても、終了のイベントが来るまでは残す
    mount([], { a: "waiting" });
    const pill = screen.getByTestId("running-pill");
    expect(within(pill).getByText("Alpha")).toBeTruthy();
    // 押した後もボタンのまま残し、フォーカスを失わない
    const waiting = within(pill).getByRole("button", { name: "Waiting for Alpha to close" });
    expect(within(waiting).getByText("Waiting for the game to close")).toBeTruthy();
    expect(waiting.getAttribute("aria-disabled")).toBe("true");
    expect(within(pill).queryByRole("button", { name: "Stop Alpha" })).toBeNull();
  });

  it("keyboard stop keeps focus on the same button while waiting", async () => {
    mount(["a"]);
    const stop = screen.getByRole("button", { name: "Stop Alpha" });
    await act(async () => {
      stop.focus();
      fireEvent.click(stop);
    });
    const waiting = await screen.findByRole("button", { name: "Waiting for Alpha to close" });
    expect(document.activeElement).toBe(waiting);
    await act(async () => {
      fireEvent.click(waiting);
    });
    expect(stopInstance).toHaveBeenCalledTimes(1);
  });

  it("multi pill shows the waiting label for force ready too", () => {
    mount(["a", "b"], { b: "forceReady" });
    expect(within(screen.getByTestId("running-pill")).getByText("Waiting for the game to close")).toBeTruthy();
  });

  it("force ready shows force stop", async () => {
    mount(["a"], { a: "forceReady" });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Force stop Alpha" }));
    });
    expect(stopInstance).toHaveBeenCalledWith("a", true);
  });

  it("popover_row_navigates_to_the_instance", async () => {
    const { router } = mount(["a", "b"], { b: "forceReady" });
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "2 running games" }));
    });
    const dialog = await screen.findByRole("dialog", { name: "Running games" });
    expect(within(dialog).getByRole("button", { name: "Stop Alpha" })).toBeTruthy();
    expect(within(dialog).getByRole("button", { name: "Force stop Beta" })).toBeTruthy();
    await act(async () => {
      fireEvent.click(within(dialog).getByRole("button", { name: "Open logs for Beta" }));
    });
    expect(router.state.location.pathname).toBe("/instances/b");
    expect(router.state.location.state).toEqual({ tab: "logs" });
    expect(openInstanceFolder).not.toHaveBeenCalled();
    await act(async () => {
      fireEvent.click(within(dialog).getByRole("button", { name: "Open details for Beta" }));
    });
    expect(router.state.location.pathname).toBe("/instances/b");
  });
});
