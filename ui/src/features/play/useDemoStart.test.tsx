import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { startDemoTask } from "../../api/commands";
import { IpcError } from "../../api/ipc";
import type { TaskSnapshot } from "../../bindings/TaskSnapshot";
import { TASKS_KEY } from "../tasks/useTasks";
import { useDemoStart } from "./useDemoStart";

vi.mock("../../api/commands", () => ({ startDemoTask: vi.fn() }));

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
  // Home と パレットに見立てた 2 つの呼び出し元
  const home = renderHook(() => useDemoStart(), { wrapper });
  const palette = renderHook(() => useDemoStart(), { wrapper });
  return { home, palette };
}

const demo = (state: TaskSnapshot["state"]): TaskSnapshot => ({
  id: "t1", kind: "demo", state, progress: { current: 1, total: 10 }, revision: 1,
  error: null, createdAt: 0, updatedAt: 0,
});

describe("useDemoStart", () => {
  it("starts only one task when two callers start in the same tick", async () => {
    vi.mocked(startDemoTask).mockReturnValue(new Promise(() => {}));
    const { home, palette } = setup();
    await act(async () => {
      home.result.current.start();
      palette.result.current.start();
    });
    expect(startDemoTask).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(palette.result.current.starting).toBe(true));
  });

  it("does not start while a demo task is unfinished", async () => {
    const { palette } = setup([demo("running")]);
    await act(async () => palette.result.current.start());
    expect(startDemoTask).not.toHaveBeenCalled();
  });

  it("starts again after the previous demo task has finished", async () => {
    vi.mocked(startDemoTask).mockResolvedValue("t2");
    const { palette } = setup([demo("succeeded")]);
    await act(async () => palette.result.current.start());
    expect(startDemoTask).toHaveBeenCalledTimes(1);
  });

  it("shares a start failure with another hook instance and clears it on a later success", async () => {
    vi.mocked(startDemoTask).mockRejectedValueOnce(new IpcError({ code: "app.unexpected", params: {}, detail: "d" }));
    const { home, palette } = setup();
    await act(async () => palette.result.current.start());
    await waitFor(() => expect(home.result.current.error?.code).toBe("app.unexpected"));
    expect(palette.result.current.error?.code).toBe("app.unexpected");

    vi.mocked(startDemoTask).mockResolvedValueOnce("t2");
    await act(async () => home.result.current.start());
    await waitFor(() => expect(palette.result.current.error).toBeNull());
    expect(home.result.current.error).toBeNull();
    expect(startDemoTask).toHaveBeenCalledTimes(2);
  });

  it("can retry after a failed start", async () => {
    vi.mocked(startDemoTask).mockRejectedValueOnce(new IpcError({ code: "app.unexpected", params: {}, detail: null }));
    const { home } = setup();
    await act(async () => home.result.current.start());
    await waitFor(() => expect(home.result.current.error).not.toBeNull());
    vi.mocked(startDemoTask).mockResolvedValueOnce("t2");
    await act(async () => home.result.current.start());
    expect(startDemoTask).toHaveBeenCalledTimes(2);
  });
});
