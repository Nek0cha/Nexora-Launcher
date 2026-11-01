import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { useToast } from "../../components/toast/useToast";
import { INSTANCES_KEY } from "../instances/useInstances";
import { isTerminal } from "../tasks/taskTable";
import { useTasks } from "../tasks/useTasks";

/** バージョンを上げるタスク（core の UPGRADE_KIND）。 */
export const UPGRADE_TASK_KIND = "instance-upgrade";

// 動いているのを見たタスク。終わりを見届けて、失敗を知らせるために覚えておく
const watched = new Set<string>();

/**
 * バージョンを上げるタスクが終わったら、インスタンスの一覧を取り直し、失敗なら理由を知らせる。
 * 失敗した上げ直しは元のバージョンに戻るため、知らせないと何も起きなかったように見える。
 * アプリで 1 回だけ呼ぶ（useGameSync から）。
 */
export function useUpgradeTaskWatch(): void {
  const tasks = useTasks();
  const client = useQueryClient();
  const toast = useToast();
  useEffect(() => {
    for (const task of Object.values(tasks)) {
      if (task.kind !== UPGRADE_TASK_KIND) continue;
      if (!isTerminal(task)) {
        watched.add(task.id);
        continue;
      }
      if (!watched.delete(task.id)) continue;
      void client.invalidateQueries({ queryKey: INSTANCES_KEY });
      if (task.state === "failed" && task.error) toast.showError(task.error);
    }
  }, [tasks, client, toast]);
}
