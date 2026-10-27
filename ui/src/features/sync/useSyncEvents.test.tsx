import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { onGameExited, onSyncApplyFinished } from "../../api/events";
import type { GameExitedEvent } from "../../bindings/GameExitedEvent";
import type { Instance } from "../../bindings/Instance";
import type { SyncApplyFinishedEvent } from "../../bindings/SyncApplyFinishedEvent";
import { clearToasts, useToastList } from "../../components/toast/useToast";
import i18n from "../../i18n";
import { INSTANCES_KEY } from "../instances/useInstances";
import { SYNC_KEY, instanceSyncKey } from "./useInstanceSync";
import { useSyncEvents } from "./useSyncEvents";

vi.mock("../../api/events", () => ({ onSyncApplyFinished: vi.fn(), onGameExited: vi.fn() }));
vi.mock("../../api/commands", () => ({ listInstances: vi.fn(), getInstanceSync: vi.fn() }));

let finished: (e: SyncApplyFinishedEvent) => void;
let exited: (e: GameExitedEvent) => void;
const unlisten = vi.fn();
const unlistenExit = vi.fn();

const instance: Instance = {
  id: "i1", name: "Survival", dirName: "Survival", mcVersion: "1.21.5", state: "ready",
  createdAt: 0, lastPlayedAt: null, playTimeMs: 0,
  memoryMode: "auto", memoryMb: null, javaMode: "auto", javaPath: null, jvmArgs: "",
  loader: "vanilla", loaderVersion: null,
};

beforeEach(async () => {
  await i18n.changeLanguage("ja");
  clearToasts();
  vi.mocked(onSyncApplyFinished).mockImplementation(async (h) => {
    finished = h;
    return unlisten;
  });
  vi.mocked(onGameExited).mockImplementation(async (h) => {
    exited = h;
    return unlistenExit;
  });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  vi.restoreAllMocks();
});

function setup() {
  const client = new QueryClient();
  client.setQueryData(INSTANCES_KEY, [instance]);
  const invalidate = vi.spyOn(client, "invalidateQueries").mockResolvedValue();
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  const view = renderHook(
    () => {
      useSyncEvents();
      return useToastList();
    },
    { wrapper },
  );
  return { view, invalidate };
}

const flush = () =>
  act(async () => {
    await Promise.resolve();
  });

describe("useSyncEvents", () => {
  it("warns with the reason only when applying failed", async () => {
    const { view } = setup();
    await flush();
    act(() => finished({ instanceId: "i1", outcome: "skipped", reason: "noOptionsFile" }));
    act(() => finished({ instanceId: "i1", outcome: "applied", reason: null }));
    expect(view.result.current).toEqual([]);

    act(() => finished({ instanceId: "i1", outcome: "failed", reason: "writeFailed" }));
    expect(view.result.current).toHaveLength(1);
    expect(view.result.current[0]).toMatchObject({
      kind: "error",
      message: "Survival に共通設定を反映できませんでした。設定ファイルに書き込めませんでした。",
    });
  });

  it("reads the sync state of the instance again after every outcome", async () => {
    const { invalidate } = setup();
    await flush();
    invalidate.mockClear();
    act(() => finished({ instanceId: "i1", outcome: "applied", reason: null }));
    expect(invalidate).toHaveBeenCalledWith({ queryKey: instanceSyncKey("i1") });
  });

  it("reads the proposals again when a game exits", async () => {
    const { invalidate } = setup();
    await flush();
    invalidate.mockClear();
    act(() => exited({ instanceId: "i1", exitCode: 0 }));
    expect(invalidate).toHaveBeenCalledWith({ queryKey: SYNC_KEY });
  });

  it("stops listening on unmount", async () => {
    const { view } = setup();
    await flush();
    view.unmount();
    expect(unlisten).toHaveBeenCalled();
    expect(unlistenExit).toHaveBeenCalled();
  });
});
