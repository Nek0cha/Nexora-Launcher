import { defaultScheduler, notifyManager, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, renderHook } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { stopInstance } from "../../api/commands";
import { onGameExited, onGameStarted } from "../../api/events";
import { IpcError } from "../../api/ipc";
import type { GameExitedEvent } from "../../bindings/GameExitedEvent";
import { clearToasts, useToastList } from "../../components/toast/useToast";
import { RUNNING_KEY, useGameSync } from "./useGameSync";
import { STOP_WAIT_MS, useStopFlow } from "./useStopFlow";

vi.mock("../../api/commands", () => ({
  stopInstance: vi.fn(),
  listRunning: vi.fn().mockResolvedValue([]),
  listInstances: vi.fn().mockResolvedValue([]),
}));
vi.mock("../../api/events", () => ({ onGameStarted: vi.fn(), onGameExited: vi.fn() }));

let exited: (e: GameExitedEvent) => void;

beforeEach(() => {
  vi.useFakeTimers();
  // 購読者への通知を、偽の時計に頼らずその場で届ける
  notifyManager.setScheduler((cb) => cb());
  vi.mocked(stopInstance).mockResolvedValue(null);
  vi.mocked(onGameStarted).mockResolvedValue(() => {});
  vi.mocked(onGameExited).mockImplementation(async (h) => {
    exited = h;
    return () => {};
  });
});

afterEach(() => {
  cleanup();
  clearToasts();
  vi.useRealTimers();
  notifyManager.setScheduler(defaultScheduler);
  vi.clearAllMocks();
});

function setup() {
  const client = new QueryClient();
  client.setQueryData(RUNNING_KEY, [
    { instanceId: "i1", pid: 1, launchedAt: 0 },
    { instanceId: "i2", pid: 2, launchedAt: 0 },
  ]);
  vi.spyOn(client, "invalidateQueries").mockResolvedValue();
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  const view = renderHook(
    () => {
      useGameSync();
      return { flow: useStopFlow(), toasts: useToastList() };
    },
    { wrapper },
  );
  return { client, view, flow: () => view.result.current.flow };
}

const flush = () =>
  act(async () => {
    await Promise.resolve();
  });

const run = (fn: () => void) =>
  act(async () => {
    fn();
    await Promise.resolve();
  });

const advance = (ms: number) =>
  act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });

describe("useStopFlow", () => {
  it("request_calls_graceful_stop_and_marks_waiting", async () => {
    const { flow } = setup();
    expect(flow().stateOf("i1")).toBe("idle");
    await run(() => flow().requestStop("i1"));
    expect(stopInstance).toHaveBeenCalledWith("i1", false);
    expect(flow().stateOf("i1")).toBe("waiting");
    expect(flow().confirmingFor).toBeNull();
    // 待っている間に押し直しても、重ねて頼まない
    await run(() => flow().requestStop("i1"));
    expect(stopInstance).toHaveBeenCalledTimes(1);
  });

  it("after_10s_still_running_asks_for_confirmation_once", async () => {
    const { flow } = setup();
    await run(() => flow().requestStop("i1"));
    await advance(STOP_WAIT_MS - 1);
    expect(flow().confirmingFor).toBeNull();
    await advance(1);
    expect(flow().confirmingFor).toBe("i1");
    await advance(STOP_WAIT_MS * 3);
    expect(flow().confirmingFor).toBe("i1");
    expect(stopInstance).toHaveBeenCalledTimes(1);
  });

  it("keep_waiting_switches_to_force_ready_without_asking_again", async () => {
    const { flow } = setup();
    await run(() => flow().requestStop("i1"));
    await advance(STOP_WAIT_MS);
    await run(() => flow().keepWaiting());
    expect(flow().confirmingFor).toBeNull();
    expect(flow().stateOf("i1")).toBe("forceReady");
    await advance(STOP_WAIT_MS * 3);
    expect(flow().confirmingFor).toBeNull();
    expect(stopInstance).not.toHaveBeenCalledWith("i1", true);
  });

  it("force_stop_calls_with_force_true", async () => {
    const { flow } = setup();
    await run(() => flow().requestStop("i1"));
    await advance(STOP_WAIT_MS);
    await run(() => flow().forceStop("i1"));
    expect(stopInstance).toHaveBeenLastCalledWith("i1", true);
    expect(flow().confirmingFor).toBeNull();
  });

  it("game_exit_clears_state_and_closes_dialog_without_forcing", async () => {
    const { flow } = setup();
    await flush();
    await run(() => flow().requestStop("i1"));
    await advance(STOP_WAIT_MS);
    expect(flow().confirmingFor).toBe("i1");
    await run(() => exited({ instanceId: "i1", exitCode: 0 }));
    expect(flow().confirmingFor).toBeNull();
    expect(flow().stateOf("i1")).toBe("idle");
    expect(stopInstance).not.toHaveBeenCalledWith("i1", true);
  });

  it("two instances reaching 10s together are asked one after the other, each once", async () => {
    const { flow } = setup();
    await run(() => flow().requestStop("i1"));
    await run(() => flow().requestStop("i2"));
    await advance(STOP_WAIT_MS);
    expect(flow().confirmingFor).toBe("i1");
    await run(() => flow().keepWaiting());
    expect(flow().confirmingFor).toBe("i2");
    expect(flow().stateOf("i1")).toBe("forceReady");
    await run(() => flow().keepWaiting());
    expect(flow().confirmingFor).toBeNull();
    await advance(STOP_WAIT_MS * 3);
    expect(flow().confirmingFor).toBeNull();
    expect(stopInstance).not.toHaveBeenCalledWith(expect.anything(), true);
  });

  it("a late failure of the graceful request does not undo a force stop", async () => {
    let reject!: (e: unknown) => void;
    vi.mocked(stopInstance).mockReturnValueOnce(new Promise((_, no) => (reject = no)));
    const { flow } = setup();
    await run(() => flow().requestStop("i1"));
    await run(() => flow().forceStop("i1"));
    await run(() => reject(new IpcError({ code: "unknown", params: {}, detail: null })));
    expect(flow().stateOf("i1")).toBe("forceReady");
  });

  it("game exit before 10s cancels the confirmation", async () => {
    const { flow } = setup();
    await flush();
    await run(() => flow().requestStop("i1"));
    await run(() => exited({ instanceId: "i1", exitCode: 0 }));
    await advance(STOP_WAIT_MS);
    expect(flow().confirmingFor).toBeNull();
    expect(flow().stateOf("i1")).toBe("idle");
  });

  it("a failed stop request returns to idle and shows a toast", async () => {
    vi.mocked(stopInstance).mockRejectedValueOnce(new IpcError({ code: "unknown", params: {}, detail: null }));
    const { flow, view } = setup();
    await run(() => flow().requestStop("i1"));
    await flush();
    expect(flow().stateOf("i1")).toBe("idle");
    expect(view.result.current.toasts).toHaveLength(1);
    expect(view.result.current.toasts[0]?.kind).toBe("error");
  });
});
