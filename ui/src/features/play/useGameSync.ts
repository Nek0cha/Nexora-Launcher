import { type QueryClient, skipToken, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { listRunning } from "../../api/commands";
import { onGameExited, onGameStarted } from "../../api/events";
import { toErrorPayload } from "../../api/ipc";
import type { RunningSession } from "../../bindings/RunningSession";
import { INSTANCES_KEY } from "../instances/useInstances";
import { isTerminal } from "../tasks/taskTable";
import { useTasks } from "../tasks/useTasks";

export const RUNNING_KEY = ["running"] as const;
export const LAST_EXIT_KEY = ["lastExit"] as const;

export type LastExit = { instanceId: string; exitCode: number | null; at: number };

/** 起動中のセッション。最初の応答が届くまでは undefined（遊んでいるか不明）。 */
export function useRunning(): RunningSession[] | undefined {
  return useQuery({ queryKey: RUNNING_KEY, queryFn: listRunning }).data;
}

export function useLastExits(): Record<string, LastExit> {
  const { data } = useQuery<Record<string, LastExit>>({
    queryKey: LAST_EXIT_KEY,
    queryFn: skipToken,
    enabled: false,
    initialData: {},
    staleTime: Number.POSITIVE_INFINITY,
  });
  return data ?? {};
}

export function clearLastExit(client: QueryClient, instanceId: string): void {
  client.setQueryData<Record<string, LastExit>>(LAST_EXIT_KEY, (prev) => {
    if (!prev?.[instanceId]) return prev;
    const { [instanceId]: _removed, ...rest } = prev;
    return rest;
  });
}

/** アプリで 1 回だけ呼ぶ。ゲームのイベントと、インストールと起動のタスクの終了で、一覧を取り直す。 */
export function useGameSync(): void {
  const client = useQueryClient();
  const tasks = useTasks();
  // 終了したインストールと起動のタスクの組。変わったらインスタンスの state を取り直す
  const finished = Object.values(tasks)
    .filter((t) => (t.kind === "install" || t.kind === "launch") && isTerminal(t))
    .map((t) => `${t.id}:${t.state}`)
    .sort()
    .join(",");

  useEffect(() => {
    void client.invalidateQueries({ queryKey: INSTANCES_KEY });
  }, [client, finished]);

  useEffect(() => {
    let disposed = false;
    const stops: Array<() => void> = [];
    const refresh = () => {
      void client.invalidateQueries({ queryKey: RUNNING_KEY });
      void client.invalidateQueries({ queryKey: INSTANCES_KEY });
    };
    const keep = (stop: () => void) => (disposed ? stop() : stops.push(stop));
    const warn = (error: unknown) => console.warn(toErrorPayload(error).code);
    onGameStarted(refresh).then(keep, warn);
    onGameExited((event) => {
      client.setQueryData<Record<string, LastExit>>(LAST_EXIT_KEY, (prev) => ({
        ...prev,
        [event.instanceId]: { instanceId: event.instanceId, exitCode: event.exitCode, at: Date.now() },
      }));
      refresh();
    }).then(keep, warn);
    return () => {
      disposed = true;
      for (const stop of stops) stop();
    };
  }, [client]);
}
