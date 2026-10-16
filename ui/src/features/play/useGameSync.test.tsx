import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { onGameExited, onGameStarted } from "../../api/events";
import type { GameExitedEvent } from "../../bindings/GameExitedEvent";
import type { RunningSession } from "../../bindings/RunningSession";
import type { TaskSnapshot } from "../../bindings/TaskSnapshot";
import { INSTANCES_KEY } from "../instances/useInstances";
import { TASKS_KEY } from "../tasks/useTasks";
import { LAST_EXIT_KEY, RUNNING_KEY, clearLastExit, useGameSync, type LastExit } from "./useGameSync";

vi.mock("../../api/events", () => ({ onGameStarted: vi.fn(), onGameExited: vi.fn() }));
vi.mock("../../api/commands", () => ({ listRunning: vi.fn(), listInstances: vi.fn() }));

let started: (e: RunningSession) => void;
let exited: (e: GameExitedEvent) => void;
const unlistenStarted = vi.fn();
const unlistenExited = vi.fn();

beforeEach(() => {
  vi.mocked(onGameStarted).mockImplementation(async (h) => {
    started = h;
    return unlistenStarted;
  });
  vi.mocked(onGameExited).mockImplementation(async (h) => {
    exited = h;
    return unlistenExited;
  });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  vi.restoreAllMocks();
});

function setup(tasks: TaskSnapshot[] = []) {
  const client = new QueryClient();
  client.setQueryData(TASKS_KEY, Object.fromEntries(tasks.map((t) => [t.id, t])));
  const invalidate = vi.spyOn(client, "invalidateQueries").mockResolvedValue();
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  const view = renderHook(() => useGameSync(), { wrapper });
  const invalidatedKeys = () => invalidate.mock.calls.map(([f]) => JSON.stringify(f?.queryKey));
  return { client, view, invalidate, invalidatedKeys };
}

const flush = () =>
  act(async () => {
    await Promise.resolve();
  });

describe("useGameSync", () => {
  it("refetches running sessions and instances on game://started", async () => {
    const { invalidate, invalidatedKeys } = setup();
    await flush();
    invalidate.mockClear();
    act(() => started({ instanceId: "i1", pid: 1, launchedAt: 0 }));
    expect(invalidatedKeys()).toEqual([JSON.stringify(RUNNING_KEY), JSON.stringify(INSTANCES_KEY)]);
  });

  it("records the exit and refetches on game://exited", async () => {
    vi.spyOn(Date, "now").mockReturnValue(1234);
    const { client, invalidate, invalidatedKeys } = setup();
    await flush();
    invalidate.mockClear();
    act(() => exited({ instanceId: "i1", exitCode: 1 }));
    act(() => exited({ instanceId: "i2", exitCode: null }));
    expect(client.getQueryData(LAST_EXIT_KEY)).toEqual({
      i1: { instanceId: "i1", exitCode: 1, at: 1234 },
      i2: { instanceId: "i2", exitCode: null, at: 1234 },
    });
    expect(invalidatedKeys()).toContain(JSON.stringify(RUNNING_KEY));
    expect(invalidatedKeys()).toContain(JSON.stringify(INSTANCES_KEY));
  });

  it("keeps the last exit while nothing observes it", async () => {
    vi.useFakeTimers();
    try {
      const { client } = setup();
      await flush();
      act(() => exited({ instanceId: "i1", exitCode: 1 }));
      // 既定の gcTime（5 分）を大きく超えても、ホームに戻ったときに通知を出せるよう残す
      await act(async () => {
        vi.advanceTimersByTime(60 * 60 * 1000);
      });
      expect(client.getQueryData<Record<string, LastExit>>(LAST_EXIT_KEY)?.i1?.exitCode).toBe(1);
    } finally {
      vi.useRealTimers();
    }
  });

  it("clearLastExit removes only the given instance", () => {
    const client = new QueryClient();
    const exit = (id: string): LastExit => ({ instanceId: id, exitCode: 1, at: 1 });
    client.setQueryData(LAST_EXIT_KEY, { a: exit("a"), b: exit("b") });
    clearLastExit(client, "a");
    expect(client.getQueryData(LAST_EXIT_KEY)).toEqual({ b: exit("b") });
    const before = client.getQueryData(LAST_EXIT_KEY);
    clearLastExit(client, "missing");
    expect(client.getQueryData(LAST_EXIT_KEY)).toBe(before);
  });

  it("refetches instances when an install or launch task finishes", async () => {
    const { client, invalidate, invalidatedKeys } = setup();
    await flush();
    invalidate.mockClear();
    const done: TaskSnapshot = {
      id: "t1", kind: "install", instanceId: "i1", state: "succeeded", progress: { current: 1, total: 1 },
      revision: 1, error: null, createdAt: 0, updatedAt: 1,
    };
    await act(async () => {
      client.setQueryData(TASKS_KEY, { t1: done });
      await new Promise((r) => setTimeout(r, 10));
    });
    expect(invalidatedKeys()).toEqual([JSON.stringify(INSTANCES_KEY)]);
  });

  it("stops listening on unmount, even when unmounted before the listeners resolve", async () => {
    const mounted = setup();
    await flush();
    mounted.view.unmount();
    expect(unlistenStarted).toHaveBeenCalledTimes(1);
    expect(unlistenExited).toHaveBeenCalledTimes(1);

    unlistenStarted.mockClear();
    unlistenExited.mockClear();
    const early = setup();
    early.view.unmount();
    await flush();
    expect(unlistenStarted).toHaveBeenCalledTimes(1);
    expect(unlistenExited).toHaveBeenCalledTimes(1);
  });
});
