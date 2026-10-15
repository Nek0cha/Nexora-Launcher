import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { getTask, listTasks } from "../../api/commands";
import { onTaskChanged } from "../../api/events";
import { type TaskTable, mergeList, mergeSnapshot, removeTask } from "./taskTable";

export const TASKS_KEY = ["tasks"] as const;

/** タスクの表を読む。データの取得はしない（useTaskSync が行う）。 */
export function useTasks(): TaskTable {
  const { data } = useQuery<TaskTable>({
    queryKey: TASKS_KEY,
    queryFn: async () => mergeList({}, await listTasks()),
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

    const update = (fn: (table: TaskTable) => TaskTable) =>
      queryClient.setQueryData<TaskTable>(TASKS_KEY, (table) => fn(table ?? {}));

    const refreshList = async () => {
      const list = await listTasks();
      if (!disposed) update((table) => mergeList(table, list));
    };

    void (async () => {
      unlisten = await onTaskChanged((event) => {
        if (event.change === "removed") {
          update((table) => removeTask(table, event.taskId));
          return;
        }
        void getTask(event.taskId)
          .then((snapshot) => {
            if (!disposed) update((table) => mergeSnapshot(table, snapshot));
          })
          .catch(() => {
            // 取得までの間に削除された場合などは、一覧を取り直す
            void refreshList();
          });
      });
      if (disposed) {
        unlisten();
        return;
      }
      await refreshList();
    })();

    const onVisible = () => {
      if (document.visibilityState === "visible") void refreshList();
    };
    document.addEventListener("visibilitychange", onVisible);

    return () => {
      disposed = true;
      unlisten?.();
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [queryClient]);
}
