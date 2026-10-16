import { act, cleanup, renderHook } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createElement, type ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { getTask, listTasks } from "../../api/commands";
import { onTaskChanged } from "../../api/events";
import type { TaskChangedEvent } from "../../bindings/TaskChangedEvent";
import type { TaskSnapshot } from "../../bindings/TaskSnapshot";
import { TASKS_KEY, useTasks, useTaskSync } from "./useTasks";
import type { TaskTable } from "./taskTable";

vi.mock("../../api/commands", () => ({ getTask: vi.fn(), listTasks: vi.fn() }));
vi.mock("../../api/events", () => ({ onTaskChanged: vi.fn() }));

function snap(id: string, revision: number): TaskSnapshot {
  return { id, revision, kind: "install", instanceId: null, state: "running", progress: { current: revision, total: 100 }, error: null, createdAt: 1, updatedAt: revision };
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
async function flush(fn: () => void = () => {}) {
  await act(async () => { fn(); await Promise.resolve(); });
}

let client: QueryClient;
let handler: (event: TaskChangedEvent) => void;
let unlisten: ReturnType<typeof vi.fn<() => void>>;
function mount(sync = true) {
  return renderHook(() => { if (sync) useTaskSync(); return useTasks(); }, {
    wrapper: ({ children }: { children: ReactNode }) => createElement(QueryClientProvider, { client }, children),
  });
}
function table() { return client.getQueryData<TaskTable>(TASKS_KEY); }
function event(id = "a", revision = 1, change: TaskChangedEvent["change"] = "progress") {
  handler({ taskId: id, revision, change });
}
function visible() { document.dispatchEvent(new Event("visibilitychange")); }

beforeEach(() => {
  vi.useFakeTimers();
  vi.resetAllMocks();
  vi.spyOn(console, "warn").mockImplementation(() => {});
  vi.spyOn(document, "visibilityState", "get").mockReturnValue("visible");
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  unlisten = vi.fn();
  vi.mocked(listTasks).mockResolvedValue([]);
  vi.mocked(onTaskChanged).mockImplementation(async (callback) => { handler = callback; return unlisten; });
});
afterEach(() => { cleanup(); client.clear(); vi.useRealTimers(); vi.restoreAllMocks(); });

describe("task cache synchronization", () => {
  it("reads the shared cache without fetching, including invalidation", async () => {
    const hook = mount(false);
    await flush();
    expect(hook.result.current).toEqual({});
    await flush(() => { client.setQueryData(TASKS_KEY, { a: snap("a", 9) }); });
    await act(async () => { await vi.advanceTimersByTimeAsync(1); });
    expect(hook.result.current).toEqual({ a: snap("a", 9) });
    await client.invalidateQueries({ queryKey: TASKS_KEY });
    expect(listTasks).not.toHaveBeenCalled();
  });

  it("waits for subscription before fetching even on visibility regain", async () => {
    const subscription = deferred<() => void>();
    vi.mocked(onTaskChanged).mockReturnValue(subscription.promise);
    mount();
    await flush(visible);
    expect(listTasks).not.toHaveBeenCalled();
    await flush(() => subscription.resolve(unlisten));
    expect(listTasks).toHaveBeenCalledTimes(1);
    expect(vi.mocked(onTaskChanged).mock.invocationCallOrder[0]).toBeLessThan(vi.mocked(listTasks).mock.invocationCallOrder[0]!);
  });

  it("does not regress out-of-order get_task snapshots", async () => {
    mount(); await flush();
    const old = deferred<TaskSnapshot>(); const newer = deferred<TaskSnapshot>();
    vi.mocked(getTask).mockReturnValueOnce(old.promise).mockReturnValueOnce(newer.promise);
    await flush(() => { event("a", 2); event("a", 3, "state"); });
    await flush(() => newer.resolve(snap("a", 3)));
    await flush(() => old.resolve(snap("a", 2)));
    expect(table()).toEqual({ a: snap("a", 3) });
  });

  it("does not resurrect removed tasks from late snapshots, even after list confirmation", async () => {
    mount(); await flush();
    const pending = deferred<TaskSnapshot>();
    vi.mocked(getTask).mockReturnValueOnce(pending.promise);
    await flush(() => event());
    await flush(() => event("a", 2, "removed"));
    await flush(visible);
    await flush(() => pending.resolve(snap("a", 1)));
    expect(table()).toEqual({});
    expect(getTask).toHaveBeenCalledTimes(1);
  });

  it("preserves event-added and event-updated tasks missing from an in-flight list", async () => {
    const pending = deferred<TaskSnapshot[]>();
    vi.mocked(listTasks).mockReturnValueOnce(pending.promise);
    client.setQueryData(TASKS_KEY, { b: snap("b", 1), c: snap("c", 1) });
    mount(); await flush();
    vi.mocked(getTask).mockResolvedValueOnce(snap("a", 2)).mockResolvedValueOnce(snap("b", 2));
    await flush(() => { event("a", 2); event("b", 2); });
    await flush(() => pending.resolve([]));
    expect(table()).toEqual({ a: snap("a", 2), b: snap("b", 2) });
  });

  it("applies only the latest list response", async () => {
    const old = deferred<TaskSnapshot[]>(); const latest = deferred<TaskSnapshot[]>();
    vi.mocked(listTasks).mockReturnValueOnce(old.promise).mockReturnValueOnce(latest.promise);
    mount(); await flush();
    await flush(visible);
    await flush(() => latest.resolve([snap("a", 3)]));
    await flush(() => old.resolve([]));
    expect(table()).toEqual({ a: snap("a", 3) });
  });

  it("preserves a snapshot applied during a list even if its event preceded the request", async () => {
    mount(); await flush();
    const snapshot = deferred<TaskSnapshot>();
    const list = deferred<TaskSnapshot[]>();
    vi.mocked(getTask).mockReturnValueOnce(snapshot.promise);
    await flush(() => event());
    vi.mocked(listTasks).mockReturnValueOnce(list.promise);
    await flush(visible);
    await flush(() => snapshot.resolve(snap("a", 1)));
    await flush(() => list.resolve([]));
    expect(table()).toEqual({ a: snap("a", 1) });
  });

  it("blocks removed ids until list confirmation and re-fetches on re-subscription", async () => {
    const hook = mount(); await flush();
    await flush(() => event("a", 2, "removed"));
    vi.mocked(getTask).mockResolvedValue(snap("a", 1));
    await flush(() => event("a", 1));
    expect(table()).toEqual({});
    await flush(visible);
    await flush(() => event("a", 3));
    expect(table()).toEqual({ a: snap("a", 1) });
    hook.unmount();
    vi.mocked(listTasks).mockResolvedValue([snap("a", 4)]);
    mount(); await flush();
    expect(onTaskChanged).toHaveBeenCalledTimes(2);
    expect(listTasks).toHaveBeenCalledTimes(3);
    expect(table()).toEqual({ a: snap("a", 4) });
  });

  it("does not resurrect a removal from a list started before that event", async () => {
    const pending = deferred<TaskSnapshot[]>();
    vi.mocked(listTasks).mockReturnValueOnce(pending.promise);
    mount(); await flush();
    await flush(() => event("a", 2, "removed"));
    await flush(() => pending.resolve([snap("a", 1)]));
    expect(table()).toEqual({});
  });

  it("catches list failures, keeps data and retries only once after 1 second", async () => {
    const failure = { code: "task.list_failed", params: {}, detail: "secret" };
    vi.mocked(listTasks).mockRejectedValue(failure);
    client.setQueryData(TASKS_KEY, { a: snap("a", 4) });
    mount(); await flush();
    expect(table()).toEqual({ a: snap("a", 4) });
    expect(console.warn).toHaveBeenCalledWith("task.list_failed");
    await act(async () => { await vi.advanceTimersByTimeAsync(999); });
    expect(listTasks).toHaveBeenCalledTimes(1);
    await act(async () => { await vi.advanceTimersByTimeAsync(1); });
    expect(listTasks).toHaveBeenCalledTimes(2);
    await act(async () => { await vi.advanceTimersByTimeAsync(2000); });
    expect(listTasks).toHaveBeenCalledTimes(2);
  });

  it("retries a failed subscription once and lists after re-subscription", async () => {
    vi.mocked(onTaskChanged).mockRejectedValueOnce({ code: "task.listen_failed", detail: "secret", params: {} });
    mount(); await flush();
    expect(listTasks).not.toHaveBeenCalled();
    expect(console.warn).toHaveBeenCalledWith("task.listen_failed");
    await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
    expect(onTaskChanged).toHaveBeenCalledTimes(2);
    expect(listTasks).toHaveBeenCalledTimes(1);
  });

  it("catches getTask and fallback list failures without leaking details", async () => {
    mount(); await flush();
    vi.mocked(getTask).mockRejectedValue({ code: "task.get_failed", detail: "secret", params: {} });
    vi.mocked(listTasks).mockRejectedValue({ code: "task.list_failed", detail: "secret", params: {} });
    await flush(() => event());
    expect(console.warn).toHaveBeenCalledWith("task.get_failed");
    expect(console.warn).toHaveBeenCalledWith("task.list_failed");
    await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
    expect(listTasks).toHaveBeenCalledTimes(3);
  });

  it("cancels retries and ignores late events after cleanup", async () => {
    vi.mocked(listTasks).mockRejectedValue({ code: "task.list_failed", params: {}, detail: null });
    const hook = mount(); await flush();
    hook.unmount();
    await flush(() => event());
    await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
    expect(listTasks).toHaveBeenCalledTimes(1);
    expect(getTask).not.toHaveBeenCalled();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });
});
