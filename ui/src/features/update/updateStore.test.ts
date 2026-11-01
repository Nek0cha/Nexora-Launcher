import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { checkUpdate, restartApp, type DownloadProgress } from "../../api/updater";
import { clearToasts, useToastList } from "../../components/toast/useToast";
import { checkForUpdate, installUpdate, resetUpdateStore, useUpdateState } from "./updateStore";

vi.mock("../../api/updater", () => ({ checkUpdate: vi.fn(), restartApp: vi.fn() }));

beforeEach(() => {
  resetUpdateStore();
  clearToasts();
  vi.mocked(restartApp).mockResolvedValue();
});
afterEach(() => vi.resetAllMocks());

function found(install: (onProgress: (p: DownloadProgress) => void) => Promise<void> = async () => {}) {
  vi.mocked(checkUpdate).mockResolvedValue({ version: "0.1.1", install });
}

describe("updateStore", () => {
  it("a found update becomes available", async () => {
    found();
    const { result } = renderHook(() => useUpdateState());
    await act(() => checkForUpdate({ manual: false }));
    expect(result.current).toEqual({ kind: "available", version: "0.1.1" });
  });

  it("no update after a manual check is upToDate, after a silent check stays idle", async () => {
    vi.mocked(checkUpdate).mockResolvedValue(null);
    const { result } = renderHook(() => useUpdateState());
    await act(() => checkForUpdate({ manual: false }));
    expect(result.current).toEqual({ kind: "idle" });
    await act(() => checkForUpdate({ manual: true }));
    expect(result.current).toEqual({ kind: "upToDate" });
  });

  it("a failed manual check is failed, a failed silent check stays idle", async () => {
    vi.mocked(checkUpdate).mockRejectedValue(new Error("offline"));
    const { result } = renderHook(() => useUpdateState());
    await act(() => checkForUpdate({ manual: false }));
    expect(result.current).toEqual({ kind: "idle" });
    await act(() => checkForUpdate({ manual: true }));
    expect(result.current).toEqual({ kind: "failed" });
  });

  it("install reports percent and restarts", async () => {
    let report!: (p: DownloadProgress) => void;
    let finish!: () => void;
    found((onProgress) => {
      report = onProgress;
      return new Promise<void>((resolve) => (finish = resolve));
    });
    const { result } = renderHook(() => useUpdateState());
    await act(() => checkForUpdate({ manual: false }));
    let done!: Promise<void>;
    act(() => {
      done = installUpdate();
    });
    expect(result.current).toEqual({ kind: "downloading", percent: null });
    act(() => report({ downloaded: 50, total: 100 }));
    expect(result.current).toEqual({ kind: "downloading", percent: 50 });
    act(() => report({ downloaded: 60, total: null }));
    expect(result.current).toEqual({ kind: "downloading", percent: null });
    await act(async () => {
      finish();
      await done;
    });
    expect(restartApp).toHaveBeenCalledTimes(1);
  });

  it("a failed install goes back to available and shows an error toast", async () => {
    found(() => Promise.reject(new Error("disk full")));
    const state = renderHook(() => useUpdateState());
    const toasts = renderHook(() => useToastList());
    await act(() => checkForUpdate({ manual: false }));
    await act(() => installUpdate());
    expect(state.result.current).toEqual({ kind: "available", version: "0.1.1" });
    expect(toasts.result.current).toHaveLength(1);
    expect(toasts.result.current[0]?.kind).toBe("error");
    expect(restartApp).not.toHaveBeenCalled();
  });

  it("a second check while one is running does nothing", async () => {
    let resolve!: (v: null) => void;
    vi.mocked(checkUpdate).mockReturnValue(new Promise((r) => (resolve = r)));
    let first!: Promise<void>;
    act(() => {
      first = checkForUpdate({ manual: true });
    });
    await act(() => checkForUpdate({ manual: true }));
    expect(checkUpdate).toHaveBeenCalledTimes(1);
    await act(async () => {
      resolve(null);
      await first;
    });
  });
});
