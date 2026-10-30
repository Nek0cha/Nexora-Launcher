import { QueryClient, QueryClientProvider, useQuery } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { CurrentSkin } from "../../bindings/CurrentSkin";
import type { SavedSkin } from "../../bindings/SavedSkin";
import { getSkin } from "../../api/commands";
import { IpcError } from "../../api/ipc";
import { SKIN_KEY } from "../accounts/useAccounts";
import {
  CHANGE_COOLDOWN_MS,
  RATE_LIMITED_COOLDOWN_MS,
  useCurrentSkin,
  useDefaultSkins,
  useImportCurrentSkin,
  useSavedSkinImage,
  useSavedSkins,
  useSkinActions,
} from "./useSkins";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

afterEach(() => {
  vi.useRealTimers();
  cleanup();
  vi.resetAllMocks();
});

const current: CurrentSkin = { image: null, sha256: null, variant: "classic", capes: [] };
const saved: SavedSkin = { id: "s1", name: "A", variant: "classic", sha256: "h1", addedAtMs: 1 };

function wrapperFor() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
}

const calls = (cmd: string) => vi.mocked(invoke).mock.calls.filter((c) => c[0] === cmd).length;

describe("queries", () => {
  it("does not read the current skin without an account", () => {
    renderHook(() => useCurrentSkin(null), { wrapper: wrapperFor() });
    expect(invoke).not.toHaveBeenCalled();
  });

  it("reads the current skin, saved skins and a saved skin image", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_current_skin") return current;
      if (cmd === "list_saved_skins") return [saved];
      if (cmd === "get_saved_skin_image") return "data:image/png;base64,AA";
      return null;
    });
    const wrapper = wrapperFor();
    const a = renderHook(() => useCurrentSkin("a1"), { wrapper });
    const b = renderHook(() => useSavedSkins(), { wrapper });
    const c = renderHook(() => useSavedSkinImage(saved), { wrapper });
    await waitFor(() => expect(a.result.current.data).toEqual(current));
    await waitFor(() => expect(b.result.current.data).toEqual([saved]));
    await waitFor(() => expect(c.result.current.data).toBe("data:image/png;base64,AA"));
    expect(invoke).toHaveBeenCalledWith("get_current_skin", { accountId: "a1" });
    expect(invoke).toHaveBeenCalledWith("get_saved_skin_image", { skinId: "s1" });
  });

  it("does not retry the default skins on failure", async () => {
    vi.mocked(invoke).mockRejectedValue(new Error("offline"));
    const hook = renderHook(() => useDefaultSkins(), { wrapper: wrapperFor() });
    await waitFor(() => expect(hook.result.current.isError).toBe(true));
    expect(calls("list_default_skins")).toBe(1);
  });
});

describe("useSkinActions", () => {
  it("stays pending until the current skin is read again", async () => {
    let finishApply!: () => void;
    let finishRefetch!: (v: CurrentSkin) => void;
    let currentReads = 0;
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "apply_saved_skin") return new Promise((r) => (finishApply = () => r(null)));
      if (cmd === "get_current_skin") {
        currentReads += 1;
        return currentReads === 1 ? Promise.resolve(current) : new Promise((r) => (finishRefetch = r));
      }
      return Promise.resolve(null);
    });
    const hook = renderHook(() => ({ cur: useCurrentSkin("a1"), actions: useSkinActions("a1") }), {
      wrapper: wrapperFor(),
    });
    await waitFor(() => expect(hook.result.current.cur.data).toEqual(current));
    expect(hook.result.current.actions.busy).toBe(false);

    act(() => {
      hook.result.current.actions.apply.mutate({ skinId: "s1" });
    });
    await waitFor(() => expect(hook.result.current.actions.apply.isPending).toBe(true));
    expect(hook.result.current.actions.busy).toBe(true);

    act(() => finishApply());
    await waitFor(() => expect(currentReads).toBe(2));
    expect(hook.result.current.actions.apply.isPending).toBe(true);
    expect(hook.result.current.actions.busy).toBe(true);

    const next = { ...current, sha256: "new" };
    await act(async () => finishRefetch(next));
    await waitFor(() => expect(hook.result.current.actions.apply.isPending).toBe(false));
    expect(hook.result.current.cur.data).toEqual(next);
    // 進行中が終わっても、冷却の間は busy のまま（明けるのは下の冷却のテストで確かめる）
    expect(hook.result.current.actions.busy).toBe(true);
  });

  it("does not reread the sidebar face after a cape change, but does after a skin change", async () => {
    vi.mocked(invoke).mockImplementation(async () => null);
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    const face = renderHook(() => useQuery({ queryKey: [...SKIN_KEY, "a1"], queryFn: () => getSkin("a1") }), {
      wrapper,
    });
    const hook = renderHook(() => useSkinActions("a1"), { wrapper });
    await waitFor(() => expect(face.result.current.isSuccess).toBe(true));
    expect(calls("get_skin")).toBe(1);

    await act(async () => {
      await hook.result.current.setCape.mutateAsync({ capeId: "c1" });
    });
    expect(calls("get_skin")).toBe(1);

    await act(async () => {
      await hook.result.current.apply.mutateAsync({ skinId: "s1" });
    });
    expect(calls("get_skin")).toBe(2);
  });

  it("reads the saved list again after add and addFromPath", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => (cmd === "list_saved_skins" ? [saved] : saved));
    const hook = renderHook(() => ({ list: useSavedSkins(), actions: useSkinActions("a1") }), {
      wrapper: wrapperFor(),
    });
    await waitFor(() => expect(hook.result.current.list.data).toEqual([saved]));
    expect(calls("list_saved_skins")).toBe(1);

    await act(async () => {
      await hook.result.current.actions.add.mutateAsync({ name: "B", png: [1] });
    });
    expect(invoke).toHaveBeenCalledWith("add_saved_skin", { name: "B", png: [1] });
    expect(calls("list_saved_skins")).toBe(2);

    await act(async () => {
      await hook.result.current.actions.addFromPath.mutateAsync({ path: "C:\\a.png" });
    });
    expect(invoke).toHaveBeenCalledWith("add_saved_skin_from_path", { path: "C:\\a.png" });
    expect(calls("list_saved_skins")).toBe(3);
  });

  it("sends update, remove, applyDefault and setCape to the commands", async () => {
    vi.mocked(invoke).mockImplementation(async () => null);
    const hook = renderHook(() => useSkinActions("a1"), { wrapper: wrapperFor() });
    const patch = { name: "N", variant: null, png: null };
    await act(async () => {
      await hook.result.current.update.mutateAsync({ skinId: "s1", patch });
      await hook.result.current.remove.mutateAsync({ skinId: "s1" });
      await hook.result.current.applyDefault.mutateAsync({ key: "slim-alex" });
      await hook.result.current.setCape.mutateAsync({ capeId: null });
    });
    expect(invoke).toHaveBeenCalledWith("update_saved_skin", { skinId: "s1", patch });
    expect(invoke).toHaveBeenCalledWith("delete_saved_skin", { skinId: "s1" });
    expect(invoke).toHaveBeenCalledWith("apply_default_skin", { accountId: "a1", key: "slim-alex" });
    expect(invoke).toHaveBeenCalledWith("set_cape", { accountId: "a1", capeId: null });
  });
});

describe("useSkinActions cooldown", () => {
  const rateLimited = () => new IpcError({ code: "net.http_status", params: { status: 429 }, detail: null });
  const settle = () => act(async () => void (await vi.advanceTimersByTimeAsync(0)));
  const wait = (ms: number) => act(async () => void (await vi.advanceTimersByTimeAsync(ms)));

  function setup(change: () => Promise<unknown>) {
    vi.useFakeTimers();
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "set_cape" || cmd === "apply_saved_skin" || cmd === "apply_default_skin") return change();
      return null;
    });
    const seen: boolean[] = [];
    const hook = renderHook(
      () => {
        const actions = useSkinActions("a1");
        seen.push(actions.busy);
        return actions;
      },
      { wrapper: wrapperFor() },
    );
    return { hook, seen };
  }

  it("keeps busy for the cooldown after a cape change, without a gap", async () => {
    const { hook, seen } = setup(async () => null);
    seen.length = 0;
    act(() => hook.result.current.setCape.mutate({ capeId: "c1" }));
    await settle();
    expect(hook.result.current.setCape.isPending).toBe(false);
    expect(hook.result.current.busy).toBe(true);
    // 押してから冷却が明けるまで、途中で操作できる状態に戻らない
    expect(seen).not.toContain(false);

    await wait(CHANGE_COOLDOWN_MS - 1);
    expect(hook.result.current.busy).toBe(true);
    await wait(1);
    expect(hook.result.current.busy).toBe(false);
  });

  it("starts the cooldown after applying a saved skin and a default skin", async () => {
    const { hook } = setup(async () => null);
    act(() => hook.result.current.apply.mutate({ skinId: "s1" }));
    await settle();
    expect(hook.result.current.busy).toBe(true);
    await wait(CHANGE_COOLDOWN_MS);
    expect(hook.result.current.busy).toBe(false);

    act(() => hook.result.current.applyDefault.mutate({ key: "slim-alex" }));
    await settle();
    expect(hook.result.current.busy).toBe(true);
    await wait(CHANGE_COOLDOWN_MS);
    expect(hook.result.current.busy).toBe(false);
  });

  it("holds the cooldown longer after HTTP 429", async () => {
    const { hook, seen } = setup(async () => {
      throw rateLimited();
    });
    seen.length = 0;
    act(() => hook.result.current.setCape.mutate({ capeId: "c1" }));
    await settle();
    expect(hook.result.current.setCape.isError).toBe(true);
    expect(hook.result.current.busy).toBe(true);
    expect(seen).not.toContain(false);

    await wait(CHANGE_COOLDOWN_MS);
    expect(hook.result.current.busy).toBe(true);
    await wait(RATE_LIMITED_COOLDOWN_MS - CHANGE_COOLDOWN_MS);
    expect(hook.result.current.busy).toBe(false);
  });

  it("does not start a cooldown for other failures or for library-only changes", async () => {
    const { hook } = setup(async () => {
      throw new IpcError({ code: "net.offline", params: {}, detail: null });
    });
    act(() => hook.result.current.setCape.mutate({ capeId: "c1" }));
    await settle();
    expect(hook.result.current.setCape.isError).toBe(true);
    expect(hook.result.current.busy).toBe(false);

    act(() => hook.result.current.remove.mutate({ skinId: "s1" }));
    await settle();
    expect(hook.result.current.busy).toBe(false);
  });

  it("restarts the cooldown when another change succeeds during it", async () => {
    const { hook } = setup(async () => null);
    act(() => hook.result.current.setCape.mutate({ capeId: "c1" }));
    await settle();
    await wait(CHANGE_COOLDOWN_MS - 1);
    act(() => hook.result.current.setCape.mutate({ capeId: "c2" }));
    await settle();
    await wait(CHANGE_COOLDOWN_MS - 1);
    expect(hook.result.current.busy).toBe(true);
    await wait(1);
    expect(hook.result.current.busy).toBe(false);
  });
});

describe("useImportCurrentSkin", () => {
  it("reads the saved list again only when a skin was added", async () => {
    let imported: SavedSkin | null = saved;
    vi.mocked(invoke).mockImplementation(async (cmd: string) => (cmd === "list_saved_skins" ? [saved] : imported));
    const hook = renderHook(() => ({ list: useSavedSkins(), imp: useImportCurrentSkin() }), { wrapper: wrapperFor() });
    await waitFor(() => expect(hook.result.current.list.data).toEqual([saved]));

    await act(async () => {
      await hook.result.current.imp.mutateAsync({ accountId: "a1" });
    });
    expect(invoke).toHaveBeenCalledWith("import_current_skin", { accountId: "a1" });
    expect(calls("list_saved_skins")).toBe(2);

    imported = null;
    await act(async () => {
      await hook.result.current.imp.mutateAsync({ accountId: "a1" });
    });
    expect(calls("list_saved_skins")).toBe(2);
  });

  it("only warns when the import fails", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    vi.mocked(invoke).mockRejectedValue(new Error("offline"));
    const hook = renderHook(() => useImportCurrentSkin(), { wrapper: wrapperFor() });
    act(() => hook.result.current.mutate({ accountId: "a1" }));
    await waitFor(() => expect(hook.result.current.isError).toBe(true));
    expect(warn).toHaveBeenCalledTimes(1);
    warn.mockRestore();
  });
});
