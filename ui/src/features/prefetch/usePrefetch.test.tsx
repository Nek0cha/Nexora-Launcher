import { QueryClient, QueryClientProvider, QueryObserver } from "@tanstack/react-query";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { getStartupStatus, listLoaderGameVersions, listVersions } from "../../api/commands";
import { IpcError } from "../../api/ipc";
import type { StartupStatus } from "../../bindings/StartupStatus";
import { usePrefetch } from "./usePrefetch";

vi.mock("../../api/commands", () => ({
  getStartupStatus: vi.fn(),
  listVersions: vi.fn(),
  listLoaderGameVersions: vi.fn(),
}));

type Deferred = { resolve: () => void; reject: () => void };

/** 呼ばれた順に並べ、テストから終わらせられる保留中の問い合わせ */
function hold() {
  const pending: { name: string; done: Deferred }[] = [];
  let inFlight = 0;
  let peak = 0;
  const start = (name: string) =>
    new Promise<never[]>((resolve, reject) => {
      inFlight += 1;
      peak = Math.max(peak, inFlight);
      pending.push({
        name,
        done: {
          resolve: () => {
            inFlight -= 1;
            resolve([]);
          },
          reject: () => {
            inFlight -= 1;
            reject(new IpcError({ code: "net.offline", params: {}, detail: null }));
          },
        },
      });
    });
  vi.mocked(listVersions).mockImplementation(() => start("versions"));
  vi.mocked(listLoaderGameVersions).mockImplementation((loader) => start(loader));
  return { pending, peak: () => peak };
}

function setup(status: StartupStatus) {
  vi.mocked(getStartupStatus).mockResolvedValue(status);
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  return { client, wrapper };
}

afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

describe("usePrefetch", () => {
  it("does nothing until the app is ready", async () => {
    const h = hold();
    const { wrapper } = setup({ state: "starting" });
    renderHook(() => usePrefetch(), { wrapper });
    await waitFor(() => expect(getStartupStatus).toHaveBeenCalled());
    await new Promise((r) => setTimeout(r, 30));
    expect(h.pending).toEqual([]);
  });

  it("fetches versions and the four loader lists, two at a time", async () => {
    const h = hold();
    const { client, wrapper } = setup({ state: "ready" });
    renderHook(() => usePrefetch(), { wrapper });
    await waitFor(() => expect(h.pending.length).toBe(2));
    expect(h.pending.map((p) => p.name)).toEqual(["versions", "fabric"]);
    await new Promise((r) => setTimeout(r, 20));
    expect(h.pending.length).toBe(2);

    // 1 件終わるごとに次が始まる
    for (let i = 0; i < 5; i++) {
      await waitFor(() => expect(h.pending.length).toBe(Math.min(i + 2, 5)));
      act(() => h.pending[i]?.done.resolve());
    }
    await waitFor(() => expect(client.getQueryData(["loader-game-versions", "neoforge"])).toEqual([]));
    expect(h.pending.map((p) => p.name)).toEqual(["versions", "fabric", "quilt", "forge", "neoforge"]);
    expect(h.peak()).toBe(2);
  });

  it("leaves a failed list to the dialog: it is not cached as an error", async () => {
    const h = hold();
    const { client, wrapper } = setup({ state: "ready" });
    renderHook(() => usePrefetch(), { wrapper });
    await waitFor(() => expect(h.pending.length).toBe(2));
    act(() => h.pending[1]?.done.reject());
    await waitFor(() => expect(h.pending.length).toBe(3));
    expect(client.getQueryState(["loader-game-versions", "fabric"])?.status).toBe("pending");
    expect(client.getQueryState(["loader-game-versions", "fabric"])?.error).toBeNull();
  });

  it("does not reset a failed list that an open dialog is watching", async () => {
    const h = hold();
    const { client, wrapper } = setup({ state: "ready" });
    const key = ["loader-game-versions", "fabric"];
    const observer = new QueryObserver(client, { queryKey: key, enabled: false });
    const unsubscribe = observer.subscribe(() => {});
    renderHook(() => usePrefetch(), { wrapper });
    await waitFor(() => expect(h.pending.length).toBe(2));
    act(() => h.pending[1]?.done.reject());
    await waitFor(() => expect(h.pending.length).toBe(3));
    expect(client.getQueryState(key)?.status).toBe("error");
    unsubscribe();
  });
});
