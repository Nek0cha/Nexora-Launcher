import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { setSetting } from "../../api/commands";
import { IpcError } from "../../api/ipc";
import type { Settings } from "../../bindings/Settings";
import { SETTINGS_KEY, useSettingsWriteError, useUpdateSetting } from "./useSettings";

vi.mock("../../api/commands", () => ({ getSettings: vi.fn(), setSetting: vi.fn() }));

afterEach(() => {
  cleanup();
  vi.resetAllMocks();
});

const base: Settings = { theme: "dark", language: "en", maxConcurrentDownloads: 8 };

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

function setup() {
  const client = new QueryClient({ defaultOptions: { mutations: { retry: false } } });
  client.setQueryData(SETTINGS_KEY, base);
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  const hook = renderHook(() => ({ update: useUpdateSetting(), error: useSettingsWriteError() }), { wrapper });
  const current = () => client.getQueryData<Settings>(SETTINGS_KEY)!.maxConcurrentDownloads;
  return { client, hook, current };
}

describe("useUpdateSetting", () => {
  it("applies the change optimistically so a second step starts from the new value", async () => {
    const first = deferred<Settings>();
    const second = deferred<Settings>();
    vi.mocked(setSetting).mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
    const { hook, current } = setup();

    await act(async () => hook.result.current.update.mutate({ key: "maxConcurrentDownloads", value: current() + 1 }));
    expect(current()).toBe(9);
    await act(async () => hook.result.current.update.mutate({ key: "maxConcurrentDownloads", value: current() + 1 }));
    expect(current()).toBe(10);

    await act(async () => first.resolve({ ...base, maxConcurrentDownloads: 9 }));
    // 先の応答で、後の楽観的な値が戻らない
    expect(current()).toBe(10);
    await act(async () => second.resolve({ ...base, maxConcurrentDownloads: 10 }));
    expect(current()).toBe(10);
  });

  it("rolls back the cache on a rejected write and exposes the error", async () => {
    vi.mocked(setSetting).mockRejectedValueOnce(new IpcError({ code: "settings.invalid_value", params: {}, detail: "d" }));
    const { hook, current } = setup();
    await act(async () => hook.result.current.update.mutate({ key: "maxConcurrentDownloads", value: 9 }));
    await waitFor(() => expect(hook.result.current.error?.code).toBe("settings.invalid_value"));
    expect(current()).toBe(8);
  });

  it("clears the error after the next successful write", async () => {
    vi.mocked(setSetting)
      .mockRejectedValueOnce(new IpcError({ code: "app.unexpected", params: {}, detail: null }))
      .mockResolvedValueOnce({ ...base, maxConcurrentDownloads: 9 });
    const { hook } = setup();
    await act(async () => hook.result.current.update.mutate({ key: "maxConcurrentDownloads", value: 9 }));
    await waitFor(() => expect(hook.result.current.error).not.toBeNull());
    await act(async () => hook.result.current.update.mutate({ key: "maxConcurrentDownloads", value: 9 }));
    await waitFor(() => expect(hook.result.current.error).toBeNull());
  });

  it("runs writes serially: the second starts only after the first resolves", async () => {
    const first = deferred<Settings>();
    vi.mocked(setSetting)
      .mockReturnValueOnce(first.promise)
      .mockResolvedValueOnce({ ...base, maxConcurrentDownloads: 10 });
    const { hook } = setup();
    await act(async () => {
      hook.result.current.update.mutate({ key: "maxConcurrentDownloads", value: 9 });
      hook.result.current.update.mutate({ key: "maxConcurrentDownloads", value: 10 });
    });
    expect(setSetting).toHaveBeenCalledTimes(1);
    await act(async () => first.resolve({ ...base, maxConcurrentDownloads: 9 }));
    await waitFor(() => expect(setSetting).toHaveBeenCalledTimes(2));
  });
});
