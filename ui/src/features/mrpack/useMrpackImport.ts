import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useEffect, useSyncExternalStore } from "react";
import { importMrpack } from "../../api/commands";
import { useToast } from "../../components/toast/useToast";
import { INSTANCES_KEY } from "../instances/useInstances";
import { isTerminal } from "../tasks/taskTable";
import { useTasks } from "../tasks/useTasks";

export const MRPACK_TASK_KIND = "mrpack-import";

// 読み込む `.mrpack` のパス。ドロップとボタンのどちらからでも開けるよう、React の外に置く
let pending: string | null = null;
const listeners = new Set<() => void>();

function emit() {
  for (const fn of listeners) fn();
}

function subscribe(fn: () => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

export function openMrpackImport(path: string): void {
  pending = path;
  emit();
}

export function closeMrpackImport(): void {
  if (pending === null) return;
  pending = null;
  emit();
}

/** 読み込みの確認ダイアログの状態。`path` があるあいだ開く。 */
export function useMrpackImport(): { path: string | null; open(path: string): void; close(): void } {
  const path = useSyncExternalStore(subscribe, () => pending);
  return { path, open: openMrpackImport, close: closeMrpackImport };
}

// 始めたタスク。終わりを見届けて、失敗を知らせるために覚えておく
const watched = new Set<string>();

/** `.mrpack` から新しいインスタンスを作るタスクを始める。 */
export function useImportMrpack() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: async ({ path, name }: { path: string; name: string }) => {
      const created = await importMrpack(path, name);
      watched.add(created.taskId);
      return created;
    },
    onSuccess: () => client.invalidateQueries({ queryKey: INSTANCES_KEY }),
  });
}

/**
 * 読み込みのタスクが終わったら、一覧を取り直し、失敗なら理由を知らせる。
 * 失敗したインポートはインスタンスごと消えるため、知らせないと何も起きなかったように見える。
 * アプリで 1 回だけ呼ぶ。
 */
export function useMrpackTaskWatch(): void {
  const tasks = useTasks();
  const client = useQueryClient();
  const toast = useToast();
  useEffect(() => {
    for (const task of Object.values(tasks)) {
      if (task.kind !== MRPACK_TASK_KIND) continue;
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
