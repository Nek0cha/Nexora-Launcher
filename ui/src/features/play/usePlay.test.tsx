import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { launchInstance } from "../../api/commands";
import { IpcError } from "../../api/ipc";
import type { TaskSnapshot } from "../../bindings/TaskSnapshot";
import { TASKS_KEY } from "../tasks/useTasks";
import { LAST_EXIT_KEY, type LastExit } from "./useGameSync";
import { usePlay } from "./usePlay";

vi.mock("../../api/commands", () => ({ launchInstance: vi.fn() }));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

function setup(tasks: TaskSnapshot[] = []) {
  const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } });
  client.setQueryData(TASKS_KEY, Object.fromEntries(tasks.map((t) => [t.id, t])));
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  // Home とパレットに見立てた 2 つの呼び出し元
  const home = renderHook(() => usePlay("i1"), { wrapper });
  const palette = renderHook(() => usePlay("i1"), { wrapper });
  return { client, home, palette };
}

const task = (kind: string, state: TaskSnapshot["state"], instanceId = "i1"): TaskSnapshot => ({
  id: `${kind}-${instanceId}`, kind, instanceId, state, progress: { current: 1, total: 10 }, revision: 1,
  error: null, createdAt: 0, updatedAt: 0,
});

describe("usePlay", () => {
  it("launches once when two callers press in the same tick", async () => {
    vi.mocked(launchInstance).mockReturnValue(new Promise(() => {}));
    const { home, palette } = setup();
    await act(async () => {
      home.result.current.press();
      palette.result.current.press();
      home.result.current.press();
    });
    expect(launchInstance).toHaveBeenCalledTimes(1);
    expect(launchInstance).toHaveBeenCalledWith("i1");
    await waitFor(() => expect(palette.result.current.starting).toBe(true));
  });

  it.each([
    ["install", "running"],
    ["launch", "queued"],
    ["launch", "cancelling"],
  ] as const)("does not launch while a %s task is %s", async (kind, state) => {
    const { palette } = setup([task(kind, state)]);
    await act(async () => palette.result.current.press());
    expect(launchInstance).not.toHaveBeenCalled();
  });

  it("launches when only another instance has an unfinished task, or the task is finished", async () => {
    vi.mocked(launchInstance).mockResolvedValue("t2");
    const { palette } = setup([task("launch", "running", "other"), task("install", "succeeded")]);
    await act(async () => palette.result.current.press());
    expect(launchInstance).toHaveBeenCalledTimes(1);
  });

  it("shares a failure with another hook instance and clears it on a later success", async () => {
    vi.mocked(launchInstance).mockRejectedValueOnce(new IpcError({ code: "launch.no_account", params: {}, detail: "d" }));
    const { home, palette } = setup();
    await act(async () => palette.result.current.press());
    await waitFor(() => expect(home.result.current.error?.code).toBe("launch.no_account"));
    expect(palette.result.current.error?.code).toBe("launch.no_account");

    vi.mocked(launchInstance).mockResolvedValueOnce("t2");
    await act(async () => home.result.current.press());
    await waitFor(() => expect(palette.result.current.error).toBeNull());
    expect(home.result.current.error).toBeNull();
    expect(launchInstance).toHaveBeenCalledTimes(2);
  });

  it("can retry after a failed start", async () => {
    vi.mocked(launchInstance).mockRejectedValueOnce(new IpcError({ code: "launch.spawn_failed", params: {}, detail: null }));
    const { home } = setup();
    await act(async () => home.result.current.press());
    await waitFor(() => expect(home.result.current.error).not.toBeNull());
    vi.mocked(launchInstance).mockResolvedValueOnce("t2");
    await act(async () => home.result.current.press());
    expect(launchInstance).toHaveBeenCalledTimes(2);
  });

  it("clears the last exit of the instance when pressed", async () => {
    vi.mocked(launchInstance).mockResolvedValue("t2");
    const { client, home } = setup();
    const exit = (id: string): LastExit => ({ instanceId: id, exitCode: 1, at: 1 });
    client.setQueryData(LAST_EXIT_KEY, { i1: exit("i1"), other: exit("other") });
    await act(async () => home.result.current.press());
    expect(client.getQueryData(LAST_EXIT_KEY)).toEqual({ other: exit("other") });
  });
});
