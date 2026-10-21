import { QueryClient } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getTask } from "../../api/commands";
import { IpcError } from "../../api/ipc";
import type { TaskSnapshot } from "../../bindings/TaskSnapshot";
import type { TaskTable } from "../tasks/taskTable";
import { TASKS_KEY } from "../tasks/useTasks";
import { WAIT_MAX_FAILURES, WAIT_POLL_MS, waitForTask } from "./useModrinth";

vi.mock("../../api/commands", () => ({ getTask: vi.fn() }));

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
