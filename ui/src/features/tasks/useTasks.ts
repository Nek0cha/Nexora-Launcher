import { skipToken, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { getTask, listTasks } from "../../api/commands";
import { onTaskChanged } from "../../api/events";
import { toErrorPayload } from "../../api/ipc";
import { type TaskTable, mergeList, mergeSnapshot, removeTask } from "./taskTable";

export const TASKS_KEY = ["tasks"] as const;

/** タスクの表を読む。データの取得はしない（useTaskSync が行う）。 */
export function useTasks(): TaskTable {
  const { data } = useQuery<TaskTable>({
    queryKey: TASKS_KEY,
    queryFn: skipToken,
    enabled: false,
    initialData: {},
    staleTime: Number.POSITIVE_INFINITY,
  });
  return data ?? {};
}

/** アプリで 1 回だけ呼ぶ。イベントを購読してから一覧を取得し、表を最新に保つ。 */
export function useTaskSync(): void {
  const queryClient = useQueryClient();

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let listRequest = 0;
    let eventSequence = 0;
    const changedAt = new Map<string, number>();
    // Keep removal generations after confirmation so pre-removal get_task calls stay invalid.
    const removedAt = new Map<string, number>();
    const removed = new Set<string>();
    const timers = new Set<ReturnType<typeof setTimeout>>();

    const update = (fn: (table: TaskTable) => TaskTable) =>
      queryClient.setQueryData<TaskTable>(TASKS_KEY, (table) => fn(table ?? {}));

    const warn = (error: unknown) => console.warn(toErrorPayload(error).code);
    const retry = (action: () => void) => {
      const timer = setTimeout(() => {
        timers.delete(timer);
        if (!disposed) action();
      }, 1000);
      timers.add(timer);
    };

    const refreshList = async (canRetry = true) => {
      if (disposed || !unlisten) return;
      const request = ++listRequest;
      const startedAt = eventSequence;
      try {
        const list = await listTasks();
        if (disposed || request !== listRequest) return;
        const preserve = new Set<string>();
        for (const [id, sequence] of changedAt) {
          if (sequence > startedAt) preserve.add(id);
        }
        // Only a list requested after a removal may confirm its new state.
        const confirmedList = list.filter((snapshot) =>
          !removed.has(snapshot.id) || removedAt.get(snapshot.id)! <= startedAt);
        update((table) => mergeList(table, confirmedList, preserve));
        for (const id of removed) {
          if (removedAt.get(id)! <= startedAt) removed.delete(id);
        }
      } catch (error) {
        if (disposed) return;
        warn(error);
        if (canRetry && request === listRequest) {
          retry(() => {
            if (request === listRequest) void refreshList(false);
          });
        }
      }
    };

    const subscribe = async (canRetry = true) => {
      try {
        const stop = await onTaskChanged((event) => {
          if (disposed) return;
          changedAt.set(event.taskId, ++eventSequence);
          if (event.change === "removed") {
            removed.add(event.taskId);
            removedAt.set(event.taskId, eventSequence);
            update((table) => removeTask(table, event.taskId));
            return;
          }
          const removalGeneration = removedAt.get(event.taskId);
          void (async () => {
            try {
              const snapshot = await getTask(event.taskId);
              if (disposed || removed.has(event.taskId)
                || removedAt.get(event.taskId) !== removalGeneration) return;
              update((table) => {
                const next = mergeSnapshot(table, snapshot);
                if (next !== table) changedAt.set(event.taskId, ++eventSequence);
                return next;
              });
            } catch (error) {
              if (disposed) return;
              warn(error);
              await refreshList();
            }
          })();
        });
        if (disposed) {
          stop();
          return;
        }
        unlisten = stop;
        await refreshList();
      } catch (error) {
        if (disposed) return;
        warn(error);
        if (canRetry) retry(() => { void subscribe(false); });
      }
    };
    void subscribe();

    const onVisible = () => {
      if (document.visibilityState === "visible") void refreshList();
    };
    document.addEventListener("visibilitychange", onVisible);

    return () => {
      disposed = true;
      unlisten?.();
      for (const timer of timers) clearTimeout(timer);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [queryClient]);
}
