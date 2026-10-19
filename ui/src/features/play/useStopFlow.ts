import { type QueryClient, skipToken, useQuery, useQueryClient } from "@tanstack/react-query";
import { useMemo } from "react";
import { stopInstance } from "../../api/commands";
import type { RunningSession } from "../../bindings/RunningSession";
import { useToast } from "../../components/toast/useToast";
import { RUNNING_KEY } from "./useGameSync";

export const STOP_KEY = ["stopFlow"] as const;
/** 「閉じて」と頼んでから、強制終了するかを聞くまでの時間 */
export const STOP_WAIT_MS = 10_000;

export type StopState = "idle" | "waiting" | "forceReady";

type StopTable = {
  /** idle のインスタンスは持たない */
  states: Record<string, Exclude<StopState, "idle">>;
  /** 強制終了するかを聞く順。先頭がダイアログに出ている */
  confirm: string[];
};

const EMPTY: StopTable = { states: {}, confirm: [] };

/** backend から取り直せない状態なので、見ている部品がなくても消さず、取り直しもしない */
const STOP_OPTIONS = {
  gcTime: Number.POSITIVE_INFINITY,
  staleTime: Number.POSITIVE_INFINITY,
} as const;

// 10 秒の待ちの時計。QueryClient ごとに持つ（テストでクライアントを作り直しても混ざらない）
const timers = new WeakMap<QueryClient, Map<string, ReturnType<typeof setTimeout>>>();

function timersOf(client: QueryClient): Map<string, ReturnType<typeof setTimeout>> {
  let map = timers.get(client);
  if (!map) {
    map = new Map();
    timers.set(client, map);
  }
  return map;
}

function clearTimer(client: QueryClient, instanceId: string): void {
  const map = timersOf(client);
  const timer = map.get(instanceId);
  if (timer !== undefined) clearTimeout(timer);
  map.delete(instanceId);
}

function update(client: QueryClient, fn: (prev: StopTable) => StopTable): void {
  client.setQueryDefaults(STOP_KEY, STOP_OPTIONS);
  client.setQueryData<StopTable>(STOP_KEY, (prev) => fn(prev ?? EMPTY));
}

function setState(client: QueryClient, instanceId: string, state: StopState): void {
  update(client, (prev) => {
    const { [instanceId]: _removed, ...rest } = prev.states;
    return {
      states: state === "idle" ? rest : { ...rest, [instanceId]: state },
      confirm: state === "idle" ? prev.confirm.filter((id) => id !== instanceId) : prev.confirm,
    };
  });
}

/** ゲームが終了したら呼ぶ。待ちの表示を消し、そのインスタンスの確認のダイアログも閉じる（強制終了は呼ばない）。 */
export function clearStop(client: QueryClient, instanceId: string): void {
  clearTimer(client, instanceId);
  const table = client.getQueryData<StopTable>(STOP_KEY);
  if (!table?.states[instanceId] && !table?.confirm.includes(instanceId)) return;
  setState(client, instanceId, "idle");
}

function isRunning(client: QueryClient, instanceId: string): boolean {
  const sessions = client.getQueryData<RunningSession[]>(RUNNING_KEY);
  // 一覧がまだ無いときは、動いているものとして扱う（確認を出し損ねない）
  return sessions === undefined || sessions.some((s) => s.instanceId === instanceId);
}

export type StopFlow = {
  stateOf(instanceId: string): StopState;
  /** 停止を頼んで終了を待っているインスタンス（waiting と forceReady） */
  stopping: string[];
  /** 「閉じて」と頼む。10 秒たってもまだ動いていれば、1 度だけ確認のダイアログを出す */
  requestStop(instanceId: string): void;
  forceStop(instanceId: string): void;
  /** 確認のダイアログに出ているインスタンス。無ければ null */
  confirmingFor: string | null;
  /** ダイアログを閉じて待ち続ける。停止のボタンは「強制終了」に変わる */
  keepWaiting(): void;
};

/** インスタンスごとの停止の状態。ピル、ポップアップの行、PlayButton で共有する（spec 3.2）。 */
export function useStopFlow(): StopFlow {
  const client = useQueryClient();
  const toast = useToast();
  const { data } = useQuery<StopTable>({
    queryKey: STOP_KEY,
    queryFn: skipToken,
    enabled: false,
    initialData: EMPTY,
    ...STOP_OPTIONS,
  });
  const table = data ?? EMPTY;

  return useMemo<StopFlow>(() => {
    const failed = (instanceId: string) => (error: unknown) => {
      // 頼めなかったので、もう一度押せるように戻す
      clearStop(client, instanceId);
      toast.showError(error);
    };

    return {
      stateOf: (instanceId) => table.states[instanceId] ?? "idle",
      stopping: Object.keys(table.states),
      requestStop: (instanceId) => {
        const current = client.getQueryData<StopTable>(STOP_KEY)?.states[instanceId];
        if (current) return;
        setState(client, instanceId, "waiting");
        clearTimer(client, instanceId);
        timersOf(client).set(
          instanceId,
          setTimeout(() => {
            timersOf(client).delete(instanceId);
            if (client.getQueryData<StopTable>(STOP_KEY)?.states[instanceId] !== "waiting") return;
            if (!isRunning(client, instanceId)) {
              // 終了のイベントより先に一覧から消えた。もう待つものがない
              clearStop(client, instanceId);
              return;
            }
            update(client, (prev) =>
              prev.confirm.includes(instanceId) ? prev : { ...prev, confirm: [...prev.confirm, instanceId] },
            );
          }, STOP_WAIT_MS),
        );
        stopInstance(instanceId, false).catch(failed(instanceId));
      },
      forceStop: (instanceId) => {
        clearTimer(client, instanceId);
        update(client, (prev) => ({
          states: { ...prev.states, [instanceId]: "forceReady" },
          confirm: prev.confirm.filter((id) => id !== instanceId),
        }));
        stopInstance(instanceId, true).catch(failed(instanceId));
      },
      confirmingFor: table.confirm[0] ?? null,
      keepWaiting: () => {
        const head = client.getQueryData<StopTable>(STOP_KEY)?.confirm[0];
        if (!head) return;
        update(client, (prev) => ({
          states: { ...prev.states, [head]: "forceReady" },
          confirm: prev.confirm.slice(1),
        }));
      },
    };
  }, [client, table, toast]);
}
