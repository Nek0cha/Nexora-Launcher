import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { beginLogin, cancelTask, listAccounts } from "../../api/commands";
import { onLoginProgress } from "../../api/events";
import type { LoginProgressEvent } from "../../bindings/LoginProgressEvent";
import type { TaskSnapshot } from "../../bindings/TaskSnapshot";
import { TASKS_KEY } from "../tasks/useTasks";
import { useLogin } from "./useLogin";

vi.mock("../../api/commands", () => ({ beginLogin: vi.fn(), cancelTask: vi.fn(), listAccounts: vi.fn() }));
vi.mock("../../api/events", () => ({ onLoginProgress: vi.fn() }));

let emit: (event: LoginProgressEvent) => void;
let client: QueryClient;

function mount() {
  client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  client.setQueryData(TASKS_KEY, {});
  return renderHook(() => useLogin(), {
    wrapper: ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    ),
  });
}

const snapshot = (id: string, state: TaskSnapshot["state"], error: TaskSnapshot["error"] = null): TaskSnapshot => ({
  id, kind: "login", instanceId: null, state, progress: { current: 0, total: 0 }, revision: 1, error,
  createdAt: 0, updatedAt: 0,
});

function setTask(task: TaskSnapshot) {
  act(() => {
    client.setQueryData(TASKS_KEY, (t: Record<string, TaskSnapshot> | undefined) => ({ ...t, [task.id]: task }));
  });
}

beforeEach(() => {
  vi.mocked(onLoginProgress).mockImplementation(async (h) => {
    emit = h;
    return () => {};
  });
  vi.mocked(cancelTask).mockResolvedValue(null);
});
afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

describe("useLogin", () => {
  it("uses only the progress of its own task, including one that arrives before the id", async () => {
    vi.mocked(beginLogin).mockResolvedValue("L1");
    const { result } = mount();
    await waitFor(() => expect(onLoginProgress).toHaveBeenCalled());
    act(() => emit({ taskId: "L1", progress: { stage: "waitingForBrowser", url: "https://login.example/a" } }));
    await act(async () => result.current.start("browser"));
    await waitFor(() => expect(result.current.taskId).toBe("L1"));
    expect(result.current.progress).toEqual({ stage: "waitingForBrowser", url: "https://login.example/a" });
    act(() => emit({ taskId: "other", progress: { stage: "checkingXbox" } }));
    expect(result.current.progress?.stage).toBe("waitingForBrowser");
    act(() => emit({ taskId: "L1", progress: { stage: "checkingMinecraft" } }));
    expect(result.current.progress?.stage).toBe("checkingMinecraft");
  });

  it("reports the active account after the task succeeds", async () => {
    vi.mocked(beginLogin).mockResolvedValue("L1");
    vi.mocked(listAccounts).mockResolvedValue({
      accounts: [{ id: "acc", username: "Steve", isActive: true, entitlement: "verified", needsLogin: false, loginSaved: true, hasSkin: false }],
      devOffline: null,
    });
    const { result } = mount();
    await act(async () => result.current.start("browser"));
    await waitFor(() => expect(result.current.taskId).toBe("L1"));
    setTask(snapshot("L1", "succeeded"));
    await waitFor(() => expect(result.current.result).toEqual({ accountId: "acc" }));
    expect(result.current.error).toBeNull();
  });

  it("reports a failed task as an error, and a cancelled one as nothing", async () => {
    vi.mocked(beginLogin).mockResolvedValueOnce("L1").mockResolvedValueOnce("L2");
    const { result } = mount();
    await act(async () => result.current.start("browser"));
    await waitFor(() => expect(result.current.taskId).toBe("L1"));
    setTask(snapshot("L1", "failed", { code: "auth.denied", params: {}, detail: null }));
    await waitFor(() => expect(result.current.error?.code).toBe("auth.denied"));

    await act(async () => result.current.start("browser"));
    await waitFor(() => expect(result.current.taskId).toBe("L2"));
    expect(result.current.error).toBeNull();
    setTask(snapshot("L2", "cancelled"));
    await act(async () => { await Promise.resolve(); });
    expect(result.current.error).toBeNull();
    expect(result.current.result).toBeNull();
  });

  it("waits for the browser task to finish cancelling before starting the device code", async () => {
    vi.mocked(beginLogin).mockResolvedValueOnce("L1").mockResolvedValueOnce("L2");
    const { result } = mount();
    await act(async () => result.current.start("browser"));
    await waitFor(() => expect(result.current.taskId).toBe("L1"));
    setTask(snapshot("L1", "running"));

    await act(async () => result.current.start("deviceCode"));
    expect(cancelTask).toHaveBeenCalledWith("L1");
    expect(beginLogin).toHaveBeenCalledTimes(1);
    setTask(snapshot("L1", "cancelling"));
    await act(async () => { await Promise.resolve(); });
    expect(beginLogin).toHaveBeenCalledTimes(1);

    setTask(snapshot("L1", "cancelled"));
    await waitFor(() => expect(beginLogin).toHaveBeenLastCalledWith("deviceCode"));
    await waitFor(() => expect(result.current.taskId).toBe("L2"));
    expect(result.current.error).toBeNull();
  });

  it("cancel sends cancel_task and clears the state", async () => {
    vi.mocked(beginLogin).mockResolvedValue("L1");
    const { result } = mount();
    await act(async () => result.current.start("browser"));
    await waitFor(() => expect(result.current.taskId).toBe("L1"));
    act(() => result.current.cancel());
    expect(cancelTask).toHaveBeenCalledWith("L1");
    expect(result.current.taskId).toBeNull();
    expect(result.current.running).toBe(false);
  });

  it("cancels the task when cancel runs before begin_login answers", async () => {
    let resolve!: (id: string) => void;
    vi.mocked(beginLogin).mockReturnValue(new Promise((r) => { resolve = r; }));
    const { result } = mount();
    act(() => result.current.start("browser"));
    act(() => result.current.cancel());
    expect(cancelTask).not.toHaveBeenCalled();
    await act(async () => { resolve("L1"); await Promise.resolve(); });
    expect(cancelTask).toHaveBeenCalledWith("L1");
    expect(result.current.taskId).toBeNull();
  });

  it("cancels a running login when unmounted", async () => {
    vi.mocked(beginLogin).mockResolvedValue("L1");
    const { result, unmount } = mount();
    await act(async () => result.current.start("browser"));
    await waitFor(() => expect(result.current.taskId).toBe("L1"));
    unmount();
    expect(cancelTask).toHaveBeenCalledWith("L1");
  });

  it("cancels a login that is still being started when unmounted", async () => {
    let resolve!: (id: string) => void;
    vi.mocked(beginLogin).mockReturnValue(new Promise((r) => { resolve = r; }));
    const { result, unmount } = mount();
    act(() => result.current.start("browser"));
    unmount();
    await act(async () => { resolve("L1"); await Promise.resolve(); });
    expect(cancelTask).toHaveBeenCalledWith("L1");
  });

  it("does not cancel a finished login when unmounted", async () => {
    vi.mocked(beginLogin).mockResolvedValue("L1");
    vi.mocked(listAccounts).mockResolvedValue({ accounts: [], devOffline: null });
    const { result, unmount } = mount();
    await act(async () => result.current.start("browser"));
    await waitFor(() => expect(result.current.taskId).toBe("L1"));
    setTask(snapshot("L1", "failed", { code: "auth.denied", params: {}, detail: null }));
    await waitFor(() => expect(result.current.error).not.toBeNull());
    unmount();
    expect(cancelTask).not.toHaveBeenCalled();
  });

  it("starts again once an old task that is still cancelling ends", async () => {
    vi.mocked(beginLogin).mockResolvedValueOnce("OLD").mockResolvedValueOnce("L2");
    const { result } = mount();
    setTask(snapshot("OLD", "cancelling"));
    await act(async () => result.current.start("browser"));
    await waitFor(() => expect(result.current.taskId).toBe("OLD"));
    expect(result.current.running).toBe(true);
    setTask(snapshot("OLD", "cancelled"));
    await waitFor(() => expect(result.current.taskId).toBe("L2"));
    expect(beginLogin).toHaveBeenLastCalledWith("browser");
    expect(beginLogin).toHaveBeenCalledTimes(2);
    expect(result.current.running).toBe(true);
  });

  it("ignores early progress of other tasks once its own id is known", async () => {
    vi.mocked(beginLogin).mockResolvedValueOnce("L1").mockResolvedValueOnce("L2");
    const { result } = mount();
    await waitFor(() => expect(onLoginProgress).toHaveBeenCalled());
    await act(async () => result.current.start("browser"));
    await waitFor(() => expect(result.current.taskId).toBe("L1"));
    // L1 が動いている間の別のタスクの進み具合は、取っておかない
    act(() => emit({ taskId: "L2", progress: { stage: "checkingXbox" } }));
    setTask(snapshot("L1", "failed", { code: "auth.denied", params: {}, detail: null }));
    await waitFor(() => expect(result.current.error).not.toBeNull());
    await act(async () => result.current.start("browser"));
    await waitFor(() => expect(result.current.taskId).toBe("L2"));
    expect(result.current.progress).toBeNull();
  });
});
