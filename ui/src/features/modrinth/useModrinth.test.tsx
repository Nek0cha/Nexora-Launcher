import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { changeContentVersion, contentTaskReport, getTask } from "../../api/commands";
import { IpcError } from "../../api/ipc";
import type { TaskSnapshot } from "../../bindings/TaskSnapshot";
import type { TaskTable } from "../tasks/taskTable";
import { TASKS_KEY } from "../tasks/useTasks";
import { useChangeContentVersion, WAIT_MAX_FAILURES, WAIT_POLL_MS, waitForTask } from "./useModrinth";

vi.mock("../../api/commands", () => ({ getTask: vi.fn(), changeContentVersion: vi.fn(), contentTaskReport: vi.fn() }));

function snapshot(state: TaskSnapshot["state"]): TaskSnapshot {
  return {
    id: "t1", kind: "content", instanceId: "i1", state, phase: null, progress: { current: 0, total: 0 },
    revision: 1, error: null, createdAt: 0, updatedAt: 0,
  };
}

const notFound = new IpcError({ code: "task.not_found", params: {}, detail: null });

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
  vi.resetAllMocks();
});

describe("waitForTask", () => {
  it("settles with the error when the task can never be read", async () => {
    vi.mocked(getTask).mockRejectedValue(notFound);
    const client = new QueryClient();
    const result = waitForTask(client, "t1").then(
      () => "resolved",
      (error: unknown) => error,
    );
    await vi.advanceTimersByTimeAsync(WAIT_POLL_MS * WAIT_MAX_FAILURES);
    expect(await result).toBe(notFound);
    // 終わったら問い合わせをやめる
    const calls = vi.mocked(getTask).mock.calls.length;
    await vi.advanceTimersByTimeAsync(WAIT_POLL_MS * 3);
    expect(vi.mocked(getTask).mock.calls.length).toBe(calls);
  });

  it("a passing read failure is retried and the task still resolves", async () => {
    vi.mocked(getTask).mockRejectedValueOnce(notFound).mockResolvedValue(snapshot("running"));
    const client = new QueryClient();
    const result = waitForTask(client, "t1");
    await vi.advanceTimersByTimeAsync(WAIT_POLL_MS * 2);
    vi.mocked(getTask).mockResolvedValue(snapshot("succeeded"));
    await vi.advanceTimersByTimeAsync(WAIT_POLL_MS);
    expect((await result).state).toBe("succeeded");
  });

  it("resolves from the task table without waiting for a poll", async () => {
    vi.mocked(getTask).mockResolvedValue(snapshot("running"));
    const client = new QueryClient();
    const result = waitForTask(client, "t1");
    await vi.advanceTimersByTimeAsync(0);
    client.setQueryData<TaskTable>(TASKS_KEY, { t1: snapshot("failed") });
    await vi.advanceTimersByTimeAsync(0);
    expect((await result).state).toBe("failed");
  });
});

describe("useChangeContentVersion", () => {
  it("runs the change as a task and refreshes the list, the update check and the version lists", async () => {
    vi.useRealTimers();
    vi.mocked(changeContentVersion).mockResolvedValue("t1");
    vi.mocked(getTask).mockResolvedValue(snapshot("succeeded"));
    const report = { installed: 0, dependenciesAdded: 0, updated: 1, failures: [] };
    vi.mocked(contentTaskReport).mockResolvedValue(report);
    const client = new QueryClient();
    const invalidate = vi.spyOn(client, "invalidateQueries");
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    const { result } = renderHook(() => useChangeContentVersion("i1"), { wrapper });

    const outcome = await result.current.mutateAsync({ relPath: "mods/a.jar", versionId: "v2" });

    expect(changeContentVersion).toHaveBeenCalledWith("i1", "mods/a.jar", "v2");
    expect(outcome.state).toBe("succeeded");
    expect(outcome.report).toEqual(report);
    await waitFor(() => {
      const keys = invalidate.mock.calls.map(([filters]) => filters?.queryKey);
      expect(keys).toEqual(
        expect.arrayContaining([["content", "i1"], ["content-updates", "i1"], ["content-versions", "i1"]]),
      );
    });
  });
});
